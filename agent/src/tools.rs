// Decide: tools look at one step and decide what to do (design.md, decision 7).
//
// Built-in tools claim exact words ("history", "again", …) before the model sees the step; the
// model is the last tool and claims everything else. A tool can only *propose* a command; the
// agent alone checks, shows, confirms and runs it, so no tool can bypass the safety layer.
// Adding a capability = one new `Tool` implementation.

use std::collections::BTreeMap;

use intent::predict::{Prediction, Predictor};

use crate::memory::Memory;

/// A command a tool wants to run, with the slot values it came from (for the safety check).
#[derive(Debug, Clone, PartialEq)]
pub struct Proposal {
    pub command: String,
    pub slots: BTreeMap<String, String>,
    /// Why this command, shown to the user (e.g. "list_files, confidence 0.97").
    pub why: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Decision {
    /// Just tell the user something.
    Say(String),
    Propose(Proposal),
    Quit,
}

pub trait Tool {
    fn name(&self) -> &str;
    /// One line for `help`.
    fn help(&self) -> &str;
    /// Does this tool handle the step (checked in order; the first claim wins)?
    fn claims(&self, step: &str) -> bool;
    fn decide(&self, step: &str, memory: &Memory) -> Decision;
}

/// Is the step exactly one of `words` (case-insensitive), with or without trailing `?!.`?
fn is_word(step: &str, words: &[&str]) -> bool {
    let raw = step.trim();
    let bare = raw.trim_end_matches(['?', '!', '.']);
    words
        .iter()
        .any(|w| raw.eq_ignore_ascii_case(w) || bare.eq_ignore_ascii_case(w))
}

pub struct HelpTool {
    /// Built by the agent from all tools' `help` lines.
    pub text: String,
}

impl Tool for HelpTool {
    fn name(&self) -> &str {
        "help"
    }
    fn help(&self) -> &str {
        "help: show this list"
    }
    fn claims(&self, step: &str) -> bool {
        is_word(step, &["help"])
    }
    fn decide(&self, _: &str, _: &Memory) -> Decision {
        Decision::Say(self.text.clone())
    }
}

pub struct HistoryTool;

impl Tool for HistoryTool {
    fn name(&self) -> &str {
        "history"
    }
    fn help(&self) -> &str {
        "history: list the recent commands and what happened to each"
    }
    fn claims(&self, step: &str) -> bool {
        is_word(step, &["history"])
    }
    fn decide(&self, _: &str, memory: &Memory) -> Decision {
        if memory.is_empty() {
            return Decision::Say("No commands yet.".to_string());
        }
        let lines: Vec<String> = memory
            .entries()
            .enumerate()
            .map(|(i, e)| format!("{:>3}. {}   ({})", i + 1, e.command, e.outcome))
            .collect();
        Decision::Say(lines.join("\n"))
    }
}

pub struct AgainTool;

impl Tool for AgainTool {
    fn name(&self) -> &str {
        "again"
    }
    fn help(&self) -> &str {
        "again: propose the last command again (you still confirm it)"
    }
    fn claims(&self, step: &str) -> bool {
        is_word(step, &["again", "repeat", "!!"])
    }
    fn decide(&self, _: &str, memory: &Memory) -> Decision {
        match memory.last() {
            Some(e) => Decision::Propose(Proposal {
                command: e.command.clone(),
                slots: e.slots.clone(),
                why: "repeating the last command".to_string(),
            }),
            None => Decision::Say("There is no previous command to repeat.".to_string()),
        }
    }
}

pub struct QuitTool;

impl Tool for QuitTool {
    fn name(&self) -> &str {
        "quit"
    }
    fn help(&self) -> &str {
        "quit (or exit): leave Cyore"
    }
    fn claims(&self, step: &str) -> bool {
        is_word(step, &["quit", "exit", "bye"])
    }
    fn decide(&self, _: &str, _: &Memory) -> Decision {
        Decision::Quit
    }
}

/// Anything that turns a request into a prediction. The real one wraps the trained model;
/// tests use a lookup table.
pub trait Interpreter {
    fn interpret(&self, text: &str) -> Prediction;
}

/// The trained `intent` model with its confidence threshold.
pub struct ModelInterpreter {
    pub predictor: Predictor,
    pub threshold: f32,
}

impl Interpreter for ModelInterpreter {
    fn interpret(&self, text: &str) -> Prediction {
        self.predictor.predict(text, self.threshold)
    }
}

/// The model as a tool: claims every step no built-in claimed.
pub struct ModelTool {
    pub interpreter: Box<dyn Interpreter>,
}

impl Tool for ModelTool {
    fn name(&self) -> &str {
        "model"
    }
    fn help(&self) -> &str {
        "anything else: describe what you want, e.g. \"show the files in Downloads\" or \"how much disk space is left\""
    }
    fn claims(&self, _: &str) -> bool {
        true
    }
    fn decide(&self, step: &str, _: &Memory) -> Decision {
        let p = self.interpreter.interpret(step);
        match (&p.command, p.reason.as_deref()) {
            (Some(command), _) => Decision::Propose(Proposal {
                command: command.clone(),
                slots: p.slots.clone(),
                why: format!("{}, confidence {:.2}", p.intent, p.confidence),
            }),
            (None, Some(reason)) if reason.starts_with("not sure") => {
                let guesses: Vec<String> = p
                    .top
                    .iter()
                    .map(|(intent, prob)| format!("{intent} ({prob:.2})"))
                    .collect();
                Decision::Say(format!(
                    "I'm not sure what you mean. My best guesses: {}. Could you rephrase?",
                    guesses.join(", ")
                ))
            }
            (None, Some(reason)) => Decision::Say(format!("I can't do that: {reason}.")),
            (None, None) => Decision::Say("I can't do that.".to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::{Entry, Outcome};

    #[test]
    fn test_builtins_claim_exact_words() {
        assert!(HistoryTool.claims("History"));
        assert!(!HistoryTool.claims("show my shell history file"));
        assert!(QuitTool.claims("exit"));
        assert!(QuitTool.claims("quit!"));
        assert!(AgainTool.claims("!!"));
        assert!(!AgainTool.claims("run it again please"));
    }

    #[test]
    fn test_history() {
        let mut memory = Memory::default();
        assert_eq!(
            HistoryTool.decide("history", &memory),
            Decision::Say("No commands yet.".into())
        );
        memory.remember(Entry {
            command: "pwd".into(),
            slots: BTreeMap::new(),
            outcome: Outcome::Declined,
        });
        match HistoryTool.decide("history", &memory) {
            Decision::Say(text) => assert!(text.contains("pwd") && text.contains("declined")),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn test_again() {
        let mut memory = Memory::default();
        assert!(
            matches!(AgainTool.decide("again", &memory), Decision::Say(t) if t.contains("no previous"))
        );
        memory.remember(Entry {
            command: "ls -la src".into(),
            slots: BTreeMap::from([("dir".into(), "src".into())]),
            outcome: Outcome::Ran(Some(0)),
        });
        match AgainTool.decide("again", &memory) {
            Decision::Propose(p) => {
                assert_eq!(p.command, "ls -la src");
                assert_eq!(p.slots["dir"], "src");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn test_quit_and_help() {
        assert_eq!(QuitTool.decide("quit", &Memory::default()), Decision::Quit);
        let help = HelpTool {
            text: "tools…".into(),
        };
        assert_eq!(
            help.decide("help", &Memory::default()),
            Decision::Say("tools…".into())
        );
    }
}
