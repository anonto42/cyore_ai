// The agent loop: perceive → decide → act → observe.
//
//   perceive: split the request into steps                       (steps.rs)
//   decide:   the first tool that claims a step decides           (tools.rs)
//   act:      safety check → show → "Run this? (y/n)" → run       (safety.rs, runner.rs)
//   observe:  show the output, remember the outcome, stop the remaining steps on failure
//
// Input and output are injected (`BufRead`/`Write`), as are the model (`Interpreter`) and the
// process runner (`Runner`), so every rule here is unit-tested without a terminal, a trained
// model, or real processes.

use std::io::{self, BufRead, Write};
use std::time::Duration;

use crate::memory::{Entry, Memory, Outcome};
use crate::runner::{Captured, RunOutcome, Runner};
use crate::safety::check;
use crate::steps::split_steps;
use crate::tools::{
    AgainTool, Decision, HelpTool, HistoryTool, Interpreter, ModelTool, Proposal, QuitTool, Tool,
};

pub struct Settings {
    pub timeout: Duration,
    /// Show and check commands, never run them.
    pub dry_run: bool,
    /// Home directory for `~` expansion.
    pub home: Option<String>,
}

/// What happened to one request.
#[derive(Debug, Default, PartialEq)]
pub struct RequestReport {
    /// A tool asked to quit.
    pub quit: bool,
    /// A command failed, timed out, could not start, or was refused.
    pub failed: bool,
}

pub struct Agent<'r> {
    tools: Vec<Box<dyn Tool>>,
    runner: &'r dyn Runner,
    memory: Memory,
    settings: Settings,
}

impl<'r> Agent<'r> {
    pub fn new(
        interpreter: Box<dyn Interpreter>,
        runner: &'r dyn Runner,
        settings: Settings,
    ) -> Self {
        // Order matters: built-ins claim exact words first, the model takes everything else.
        let mut tools: Vec<Box<dyn Tool>> = vec![
            Box::new(HistoryTool),
            Box::new(AgainTool),
            Box::new(QuitTool),
            Box::new(ModelTool { interpreter }),
        ];
        let mut lines = vec!["help: show this list".to_string()];
        lines.extend(tools.iter().map(|t| t.help().to_string()));
        let text = format!(
            "Things I understand:\n{}\nJoin several requests with \"then\" or \";\". Every command is shown and needs your \"y\" before it runs.",
            lines
                .iter()
                .map(|l| format!("  - {l}"))
                .collect::<Vec<_>>()
                .join("\n")
        );
        tools.insert(0, Box::new(HelpTool { text }));
        Agent {
            tools,
            runner,
            memory: Memory::default(),
            settings,
        }
    }

    pub fn memory(&self) -> &Memory {
        &self.memory
    }

    /// Interactive mode: read requests until `quit` or end of input.
    pub fn run_interactive(
        &mut self,
        input: &mut dyn BufRead,
        out: &mut dyn Write,
        prompt: bool,
    ) -> io::Result<()> {
        loop {
            if prompt {
                write!(out, "cyore> ")?;
                out.flush()?;
            }
            let mut line = String::new();
            if input.read_line(&mut line)? == 0 {
                if prompt {
                    writeln!(out)?;
                }
                return Ok(());
            }
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            if self.handle(line, input, out)?.quit {
                return Ok(());
            }
        }
    }

    /// One request: perceive (split into steps), then decide → act → observe for each step.
    /// `input` is also where confirmations are read from.
    pub fn handle(
        &mut self,
        request: &str,
        input: &mut dyn BufRead,
        out: &mut dyn Write,
    ) -> io::Result<RequestReport> {
        let mut report = RequestReport::default();
        let steps = split_steps(request);
        for (i, step) in steps.iter().enumerate() {
            if steps.len() > 1 {
                writeln!(out, "Step {}/{}: {step}", i + 1, steps.len())?;
            }
            match self.decide(step) {
                Decision::Say(text) => writeln!(out, "{text}")?,
                Decision::Quit => {
                    report.quit = true;
                    return Ok(report);
                }
                Decision::Propose(proposal) => {
                    let ok = self.act(&proposal, input, out)?;
                    if !ok {
                        report.failed = true;
                        let remaining = steps.len() - i - 1;
                        if remaining > 0 {
                            writeln!(
                                out,
                                "Skipped the remaining {remaining} step(s) because this one did not succeed."
                            )?;
                        }
                        break;
                    }
                }
            }
        }
        Ok(report)
    }

    /// Decide: the first tool that claims the step. (The model tool claims everything.)
    fn decide(&self, step: &str) -> Decision {
        self.tools
            .iter()
            .find(|t| t.claims(step))
            .map(|t| t.decide(step, &self.memory))
            .unwrap_or_else(|| Decision::Say("I don't know how to do that.".to_string()))
    }

    /// Act on a proposal: safety check → show → confirm → run → observe.
    /// Returns false if the step should stop the rest of the request.
    fn act(
        &mut self,
        proposal: &Proposal,
        input: &mut dyn BufRead,
        out: &mut dyn Write,
    ) -> io::Result<bool> {
        let checked = match check(
            &proposal.command,
            &proposal.slots,
            self.settings.home.as_deref(),
        ) {
            Ok(checked) => checked,
            Err(refusal) => {
                writeln!(out, "Refused `{}`: {refusal}.", proposal.command)?;
                return Ok(false);
            }
        };
        writeln!(out, "  $ {checked}        ({})", proposal.why)?;

        let (outcome, ok) = if self.settings.dry_run {
            writeln!(out, "Dry run: not running it.")?;
            (Outcome::DryRun, true)
        } else if !confirm(input, out)? {
            writeln!(out, "Skipped.")?;
            // Declining is the user's choice, not a failure: later steps still run.
            (Outcome::Declined, true)
        } else {
            let result = self.runner.run(&checked, self.settings.timeout);
            observe(&result, self.settings.timeout, out)?;
            let outcome = match &result {
                RunOutcome::Finished { code, .. } => Outcome::Ran(*code),
                RunOutcome::TimedOut { .. } => Outcome::TimedOut,
                RunOutcome::FailedToStart(_) => Outcome::FailedToStart,
            };
            (outcome, result.succeeded())
        };
        self.memory.remember(Entry {
            command: proposal.command.clone(),
            slots: proposal.slots.clone(),
            outcome,
        });
        Ok(ok)
    }
}

/// Ask "Run this? (y/n)". Only `y`/`yes` (any case) counts; end of input means no.
fn confirm(input: &mut dyn BufRead, out: &mut dyn Write) -> io::Result<bool> {
    write!(out, "Run this? (y/n) ")?;
    out.flush()?;
    let mut answer = String::new();
    if input.read_line(&mut answer)? == 0 {
        writeln!(out)?;
        return Ok(false);
    }
    Ok(matches!(answer.trim().to_lowercase().as_str(), "y" | "yes"))
}

fn show(captured: &Captured, out: &mut dyn Write) -> io::Result<()> {
    write!(out, "{}", captured.text)?;
    if captured.omitted_lines > 0 {
        writeln!(out, "… {} more line(s) not shown", captured.omitted_lines)?;
    }
    if captured.truncated_bytes {
        writeln!(out, "… output was too long and was cut")?;
    }
    Ok(())
}

/// Observe: show what the command printed and how it ended.
fn observe(result: &RunOutcome, timeout: Duration, out: &mut dyn Write) -> io::Result<()> {
    match result {
        RunOutcome::Finished {
            code,
            stdout,
            stderr,
        } => {
            show(stdout, out)?;
            show(stderr, out)?;
            match code {
                Some(0) => writeln!(out, "[done]"),
                Some(code) => writeln!(out, "[failed: exit status {code}]"),
                None => writeln!(out, "[failed: ended by a signal]"),
            }
        }
        RunOutcome::TimedOut { stdout, stderr } => {
            show(stdout, out)?;
            show(stderr, out)?;
            writeln!(
                out,
                "[stopped: still running after {} s]",
                timeout.as_secs()
            )
        }
        RunOutcome::FailedToStart(message) => writeln!(out, "[failed: {message}]"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::collections::HashMap;

    use intent::predict::Prediction;

    use crate::safety::CheckedCommand;

    /// A fake model: known requests map to fixed predictions; anything else is `unknown`.
    struct FakeModel(HashMap<String, Prediction>);

    impl Interpreter for FakeModel {
        fn interpret(&self, text: &str) -> Prediction {
            self.0.get(text).cloned().unwrap_or_else(|| {
                prediction(
                    "unknown",
                    0.9,
                    None,
                    &[],
                    Some("this request is not supported"),
                )
            })
        }
    }

    fn prediction(
        intent: &str,
        confidence: f32,
        command: Option<&str>,
        slots: &[(&str, &str)],
        reason: Option<&str>,
    ) -> Prediction {
        Prediction {
            tokens: Vec::new(),
            tags: Vec::new(),
            intent: intent.to_string(),
            confidence,
            top: vec![(intent.to_string(), confidence), ("other".to_string(), 0.1)],
            slots: slots
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            command: command.map(String::from),
            reason: reason.map(String::from),
        }
    }

    fn model() -> FakeModel {
        let mut m = HashMap::new();
        m.insert(
            "show files".into(),
            prediction("list_files", 0.95, Some("ls -la"), &[], None),
        );
        m.insert(
            "disk space".into(),
            prediction("disk_space", 0.99, Some("df -h"), &[], None),
        );
        m.insert(
            "where am i".into(),
            prediction("current_dir", 0.9, Some("pwd"), &[], None),
        );
        m.insert(
            "show the folder -delete".into(),
            prediction(
                "find_by_name",
                0.9,
                Some("find -delete -name *.txt"),
                &[("dir", "-delete"), ("pattern", "*.txt")],
                None,
            ),
        );
        m.insert(
            "something vague".into(),
            prediction(
                "list_files",
                0.4,
                None,
                &[],
                Some("not sure (top guesses: list_files 0.40)"),
            ),
        );
        FakeModel(m)
    }

    /// Records every command it is asked to run; programs in `failing` exit with status 1.
    #[derive(Default)]
    struct RecordingRunner {
        ran: RefCell<Vec<String>>,
        failing: Vec<&'static str>,
    }

    impl Runner for RecordingRunner {
        fn run(&self, command: &CheckedCommand, _: Duration) -> RunOutcome {
            self.ran.borrow_mut().push(command.to_string());
            let code = if self.failing.contains(&command.program()) {
                1
            } else {
                0
            };
            RunOutcome::Finished {
                code: Some(code),
                stdout: Captured {
                    text: format!("output of {command}\n"),
                    ..Captured::default()
                },
                stderr: Captured::default(),
            }
        }
    }

    fn settings(dry_run: bool) -> Settings {
        Settings {
            timeout: Duration::from_secs(10),
            dry_run,
            home: Some("/home/u".into()),
        }
    }

    /// Run a whole interactive session; returns (transcript, commands that ran).
    fn session(runner: &RecordingRunner, dry_run: bool, input: &str) -> (String, Vec<String>) {
        let mut agent = Agent::new(Box::new(model()), runner, settings(dry_run));
        let mut out = Vec::new();
        agent
            .run_interactive(&mut input.as_bytes(), &mut out, false)
            .unwrap();
        (String::from_utf8(out).unwrap(), runner.ran.borrow().clone())
    }

    #[test]
    fn test_runs_only_after_yes() {
        let runner = RecordingRunner::default();
        let (text, ran) = session(&runner, false, "show files\ny\n");
        assert_eq!(ran, ["ls -la"]);
        // Shown before the question, and the question before the output.
        let shown = text.find("$ ls -la").unwrap();
        let asked = text.find("Run this? (y/n)").unwrap();
        let output = text.find("output of ls -la").unwrap();
        assert!(shown < asked && asked < output, "{text}");
    }

    #[test]
    fn test_yes_variants() {
        for answer in ["y", "Y", "yes", "YES", "  yes  "] {
            let runner = RecordingRunner::default();
            let (_, ran) = session(&runner, false, &format!("show files\n{answer}\n"));
            assert_eq!(ran.len(), 1, "answer {answer:?}");
        }
    }

    #[test]
    fn test_nothing_runs_without_yes() {
        for answer in ["n", "no", "", "yep", "sure", "y y", "ok"] {
            let runner = RecordingRunner::default();
            let (text, ran) = session(&runner, false, &format!("show files\n{answer}\n"));
            assert!(ran.is_empty(), "answer {answer:?} ran {ran:?}");
            assert!(text.contains("Skipped."));
        }
        // End of input while asking counts as no.
        let runner = RecordingRunner::default();
        let (_, ran) = session(&runner, false, "show files\n");
        assert!(ran.is_empty());
    }

    #[test]
    fn test_refused_commands_are_never_asked_about() {
        let runner = RecordingRunner::default();
        let (text, ran) = session(&runner, false, "show the folder -delete\ny\n");
        assert!(ran.is_empty());
        assert!(text.contains("Refused"), "{text}");
        assert!(!text.contains("Run this?"), "{text}");
    }

    #[test]
    fn test_low_confidence_asks_to_rephrase() {
        let runner = RecordingRunner::default();
        let (text, ran) = session(&runner, false, "something vague\n");
        assert!(ran.is_empty());
        assert!(
            text.contains("not sure") && text.contains("rephrase"),
            "{text}"
        );
        assert!(!text.contains("Run this?"));
    }

    #[test]
    fn test_unknown_is_explained() {
        let runner = RecordingRunner::default();
        let (text, ran) = session(&runner, false, "delete everything\n");
        assert!(ran.is_empty());
        assert!(text.contains("can't do that"), "{text}");
    }

    #[test]
    fn test_multi_step_and_failure_stops_the_rest() {
        let runner = RecordingRunner {
            failing: vec!["df"],
            ..Default::default()
        };
        let (text, ran) = session(
            &runner,
            false,
            "disk space then show files then where am i\ny\n",
        );
        assert_eq!(ran, ["df -h"]);
        assert!(text.contains("Skipped the remaining 2 step(s)"), "{text}");
    }

    #[test]
    fn test_declined_step_does_not_stop_the_rest() {
        let runner = RecordingRunner::default();
        let (_, ran) = session(&runner, false, "disk space and then show files\nn\ny\n");
        assert_eq!(ran, ["ls -la"]);
    }

    #[test]
    fn test_dry_run_never_runs_or_asks() {
        let runner = RecordingRunner::default();
        let (text, ran) = session(&runner, true, "show files; disk space\n");
        assert!(ran.is_empty());
        assert!(!text.contains("Run this?"));
        assert_eq!(text.matches("Dry run").count(), 2);
    }

    #[test]
    fn test_again_needs_confirmation_and_history_records() {
        let runner = RecordingRunner::default();
        let (text, ran) = session(
            &runner,
            false,
            "show files\ny\nagain\nn\nagain\ny\nhistory\nquit\nshow files\ny\n",
        );
        assert_eq!(ran, ["ls -la", "ls -la"]); // second "again" confirmed; nothing after quit
        assert_eq!(text.matches("Run this?").count(), 3);
        assert!(text.contains("1. ls -la   (ok)"), "{text}");
        assert!(text.contains("2. ls -la   (declined)"), "{text}");
    }

    #[test]
    fn test_help_lists_tools() {
        let runner = RecordingRunner::default();
        let (text, _) = session(&runner, false, "help\n");
        for word in ["help", "history", "again", "quit"] {
            assert!(text.contains(&format!("- {word}")), "{text}");
        }
    }

    #[test]
    fn test_memory_keeps_unexpanded_command() {
        let mut m = model().0;
        m.insert(
            "list notes".into(),
            prediction(
                "list_files",
                0.9,
                Some("ls -la ~/notes"),
                &[("dir", "~/notes")],
                None,
            ),
        );
        let runner = RecordingRunner::default();
        let mut agent = Agent::new(Box::new(FakeModel(m)), &runner, settings(false));
        let mut out = Vec::new();
        agent
            .handle("list notes", &mut "y\n".as_bytes(), &mut out)
            .unwrap();
        assert_eq!(runner.ran.borrow().as_slice(), ["ls -la /home/u/notes"]);
        assert_eq!(agent.memory().last().unwrap().command, "ls -la ~/notes");
    }
}
