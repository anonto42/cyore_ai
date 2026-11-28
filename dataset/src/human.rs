// A test set written by a person, in plain text (dataset/human_test.txt):
//
//   # comments and blank lines are ignored
//   count_lines   how many lines are in [file: notes.txt]
//   unknown       book me a taxi
//
// Each line: an intent name, whitespace, then the request, with every slot value written as
// `[slot: value]`. The value goes into the sentence; the markup tells us which token is the slot.
// Labels are built the same way the generator builds them: the sentence is tokenized with
// `{slot}` placeholders, so tags are positional and always aligned with tokens.

use std::collections::{BTreeMap, HashMap};

use serde::Serialize;

use crate::catalog::{Intent, UNKNOWN};
use crate::template::{SlotKind, Template};
use crate::tokenize::tokenize;

/// Shell syntax a slot value may not contain (the agent would refuse the command anyway).
const SHELL_SYNTAX: &[&str] = &["|", ">", "<", ";", "&", "$(", "`"];

/// One human-written example, in the same shape as the generated JSONL examples.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct HumanExample {
    pub text: String,
    pub tokens: Vec<String>,
    pub tags: Vec<String>,
    pub intent: String,
    pub slots: BTreeMap<String, String>,
    pub command: Option<String>,
}

/// The sentence with values filled in, the same sentence with `{slot}` placeholders, and the
/// slot values, from text like `how many lines in [file: notes.txt]`.
fn parse_markup(sentence: &str) -> Result<(String, String, BTreeMap<String, String>), String> {
    let (mut text, mut placeholders) = (String::new(), String::new());
    let mut slots = BTreeMap::new();
    let mut rest = sentence;
    while let Some(open) = rest.find(['[', ']']) {
        if rest[open..].starts_with(']') {
            return Err("a `]` without a matching `[`".to_string());
        }
        text.push_str(&rest[..open]);
        placeholders.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        let close = after
            .find(']')
            .ok_or_else(|| "a `[` without a matching `]`".to_string())?;
        let inner = &after[..close];
        if inner.contains('[') {
            return Err("brackets cannot be nested".to_string());
        }
        let (name, value) = inner
            .split_once(':')
            .ok_or_else(|| format!("expected [slot: value], got [{inner}]"))?;
        let (name, value) = (name.trim(), value.trim());
        if name.is_empty() || !name.chars().all(|c| c.is_ascii_lowercase() || c == '_') {
            return Err(format!("bad slot name \"{name}\""));
        }
        if tokenize(value) != [value.to_string()] || value.contains(['{', '}', '[', ']']) {
            return Err(format!(
                "the {name} value \"{value}\" must be one word with no trailing punctuation"
            ));
        }
        if let Some(m) = SHELL_SYNTAX.iter().find(|m| value.contains(*m)) {
            return Err(format!(
                "the {name} value \"{value}\" contains shell syntax `{m}`"
            ));
        }
        if value.starts_with('-') {
            return Err(format!(
                "the {name} value \"{value}\" starts with `-`, which a program would read as an option"
            ));
        }
        if slots.insert(name.to_string(), value.to_string()).is_some() {
            return Err(format!("the {name} slot appears twice"));
        }
        text.push_str(value);
        placeholders.push_str(&format!("{{{name}}}"));
        rest = &after[close + 1..];
    }
    text.push_str(rest);
    placeholders.push_str(rest);
    Ok((text, placeholders, slots))
}

/// Turn one sentence into a labelled example for `intent`.
fn build(intent: &Intent, sentence: &str) -> Result<HumanExample, String> {
    let (text, placeholders, slots) = parse_markup(sentence)?;
    let template = match intent.command {
        Some(source) => Some(Template::parse(source)?),
        None => None,
    };
    match &template {
        None if !slots.is_empty() => {
            return Err(format!("`{UNKNOWN}` requests have no slots"));
        }
        None => {}
        Some(t) => {
            let names: Vec<&str> = t.slots().iter().map(|(n, _)| *n).collect();
            if let Some(name) = slots.keys().find(|n| !names.contains(&n.as_str())) {
                return Err(format!(
                    "{} has no {name} slot (its command is `{}`)",
                    intent.name, t.source
                ));
            }
            if let Some((name, _)) = t
                .slots()
                .into_iter()
                .find(|(n, k)| **k == SlotKind::Required && !slots.contains_key(*n))
            {
                return Err(format!("{} needs a [{name}: …] value", intent.name));
            }
        }
    }
    let (mut tokens, mut tags) = (Vec::new(), Vec::new());
    for token in tokenize(&placeholders) {
        match token.strip_prefix('{').and_then(|t| t.strip_suffix('}')) {
            Some(name) if slots.contains_key(name) => {
                tokens.push(slots[name].clone());
                tags.push(format!("B-{name}"));
            }
            _ if token.contains(['{', '}']) => {
                return Err("put spaces around each [slot: value]".to_string());
            }
            _ => {
                tokens.push(token);
                tags.push("O".to_string());
            }
        }
    }
    if tokens.is_empty() {
        return Err("the request is empty".to_string());
    }
    if tokenize(&text) != tokens {
        return Err("put spaces around each [slot: value]".to_string());
    }
    let command = template.map(|t| t.fill(&slots)).transpose()?;
    Ok(HumanExample {
        text,
        tokens,
        tags,
        intent: intent.name.to_string(),
        slots,
        command,
    })
}

/// Parse the whole file. Returns every example, or every problem with its line number.
pub fn parse(file: &str, intents: &[Intent]) -> Result<Vec<HumanExample>, Vec<String>> {
    let mut examples = Vec::new();
    let mut errors = Vec::new();
    let mut seen: HashMap<String, usize> = HashMap::new();
    for (index, raw) in file.lines().enumerate() {
        let n = index + 1;
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (name, sentence) = line
            .split_once(char::is_whitespace)
            .map(|(a, b)| (a, b.trim()))
            .unwrap_or((line, ""));
        let Some(intent) = intents.iter().find(|i| i.name == name) else {
            errors.push(format!("line {n}: unknown intent \"{name}\""));
            continue;
        };
        match build(intent, sentence) {
            Ok(example) => {
                let key = example.text.to_lowercase();
                if let Some(first) = seen.insert(key, n) {
                    errors.push(format!("line {n}: the same request as line {first}"));
                } else {
                    examples.push(example);
                }
            }
            Err(e) => errors.push(format!("line {n}: {e}")),
        }
    }
    if errors.is_empty() {
        Ok(examples)
    } else {
        Err(errors)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::INTENTS;

    fn one(line: &str) -> Result<HumanExample, String> {
        parse(line, INTENTS)
            .map(|mut v| v.remove(0))
            .map_err(|e| e.join("; "))
    }

    fn error(line: &str) -> String {
        one(line).unwrap_err()
    }

    #[test]
    fn test_line_with_slot() {
        let e = one("count_lines  how many lines are in [file: notes.txt]").unwrap();
        assert_eq!(e.text, "how many lines are in notes.txt");
        assert_eq!(e.tokens.last().unwrap(), "notes.txt");
        assert_eq!(e.tags.last().unwrap(), "B-file");
        assert!(e.tags[..e.tags.len() - 1].iter().all(|t| t == "O"));
        assert_eq!(e.command.as_deref(), Some("wc -l notes.txt"));
    }

    #[test]
    fn test_optional_and_default_slots() {
        let e = one("list_files\tshow my files").unwrap();
        assert_eq!(e.command.as_deref(), Some("ls -la"));
        let e = one("search_text grep for [text: TODO]?").unwrap();
        assert_eq!(e.command.as_deref(), Some("grep -rn TODO ."));
        assert_eq!(e.text, "grep for TODO?");
    }

    #[test]
    fn test_unknown_and_comments() {
        let file = "# a comment\n\n   \nunknown  book me a taxi\n";
        let v = parse(file, INTENTS).unwrap();
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].command, None);
        assert!(parse("# only comments\n", INTENTS).unwrap().is_empty());
    }

    #[test]
    fn test_errors_name_the_line() {
        let errors = parse("unknown fine\nnope  hello\n", INTENTS).unwrap_err();
        assert_eq!(errors, ["line 2: unknown intent \"nope\""]);
    }

    #[test]
    fn test_each_mistake() {
        assert!(error("disk_space check [file: a.txt]").contains("has no file slot"));
        assert!(error("count_lines count the lines").contains("needs a [file: …] value"));
        assert!(error("count_lines lines in [file: two words]").contains("one word"));
        assert!(error("count_lines lines in [file: a|b]").contains("shell syntax"));
        assert!(error("count_lines lines in [file: -rf]").contains("starts with `-`"));
        assert!(error("unknown open [file: a.txt]").contains("have no slots"));
        assert!(error("count_lines lines in [file notes.txt]").contains("expected [slot: value]"));
        assert!(error("count_lines lines in [file: a.txt").contains("without a matching `]`"));
        assert!(error("count_lines lines ] in").contains("without a matching `[`"));
        assert!(error("count_lines lines in x[file: a.txt]").contains("spaces around"));
        assert!(error("count_lines").contains("needs a [file"));
    }

    #[test]
    fn test_duplicates() {
        let errors = parse("unknown Hi there\nunknown hi there\n", INTENTS).unwrap_err();
        assert_eq!(errors, ["line 2: the same request as line 1"]);
    }
}
