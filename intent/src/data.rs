// Reading the command dataset written by `make commands-data` (see dataset/README.md).

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::random::Rng;

/// One labelled request, exactly as stored in train.jsonl / test.jsonl.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Example {
    pub text: String,
    pub tokens: Vec<String>,
    pub tags: Vec<String>,
    pub intent: String,
    pub slots: BTreeMap<String, String>,
    pub command: Option<String>,
}

/// One entry of intents.json: the intent name and its command template.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IntentInfo {
    pub name: String,
    pub command: Option<String>,
}

pub struct CommandData {
    pub train: Vec<Example>,
    /// Hand-written development set: guides model selection and catalog changes.
    pub dev: Vec<Example>,
    /// Hand-written test set: report only.
    pub test: Vec<Example>,
    pub intents: Vec<IntentInfo>,
}

fn read(path: &Path) -> Result<String, String> {
    std::fs::read_to_string(path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            format!(
                "{}: not found. Generate the dataset first with `make commands-data`.",
                path.display()
            )
        } else {
            format!("{}: {e}", path.display())
        }
    })
}

/// Parse JSON Lines; `name` is only used in error messages.
pub fn parse_jsonl(name: &str, text: &str) -> Result<Vec<Example>, String> {
    text.lines()
        .enumerate()
        .filter(|(_, line)| !line.trim().is_empty())
        .map(|(i, line)| serde_json::from_str(line).map_err(|e| format!("{name}:{}: {e}", i + 1)))
        .collect()
}

pub fn read_examples(path: &Path) -> Result<Vec<Example>, String> {
    parse_jsonl(&path.display().to_string(), &read(path)?)
}

pub fn read_intents(path: &Path) -> Result<Vec<IntentInfo>, String> {
    serde_json::from_str(&read(path)?).map_err(|e| format!("{}: {e}", path.display()))
}

pub fn load(dir: &Path) -> Result<CommandData, String> {
    Ok(CommandData {
        train: read_examples(&dir.join("train.jsonl"))?,
        dev: read_examples(&dir.join("dev.jsonl"))?,
        test: read_examples(&dir.join("test.jsonl"))?,
        intents: read_intents(&dir.join("intents.json"))?,
    })
}

/// Hold out `fraction` of each intent's examples for validation (at least one per intent),
/// chosen by a seeded shuffle. Returns (fit, validation).
pub fn split_validation(
    examples: &[Example],
    fraction: f64,
    seed: u64,
) -> (Vec<Example>, Vec<Example>) {
    let mut by_intent: BTreeMap<&str, Vec<&Example>> = BTreeMap::new();
    for e in examples {
        by_intent.entry(&e.intent).or_default().push(e);
    }
    let mut rng = Rng::new(seed);
    let (mut fit, mut val) = (Vec::new(), Vec::new());
    for group in by_intent.values_mut() {
        rng.shuffle(group);
        let n_val = ((group.len() as f64 * fraction).round() as usize).max(1);
        for (i, e) in group.iter().enumerate() {
            if i < n_val { &mut val } else { &mut fit }.push((*e).clone());
        }
    }
    (fit, val)
}

#[cfg(test)]
pub mod tests {
    use super::*;

    /// A small example for tests elsewhere in the crate. `slot` is (name, value) for a single
    /// `B-` tag on the token equal to the value.
    pub fn example(text: &str, intent: &str, slot: Option<(&str, &str)>) -> Example {
        let tokens = dataset::tokenize::tokenize(text);
        let tags = tokens
            .iter()
            .map(|t| match slot {
                Some((name, value)) if t == value => format!("B-{name}"),
                _ => "O".to_string(),
            })
            .collect();
        Example {
            text: text.to_string(),
            tokens,
            tags,
            intent: intent.to_string(),
            slots: slot
                .map(|(n, v)| BTreeMap::from([(n.to_string(), v.to_string())]))
                .unwrap_or_default(),
            command: None,
        }
    }

    #[test]
    fn test_parse_jsonl() {
        let text = r#"{"text":"show src","tokens":["show","src"],"tags":["O","B-dir"],"intent":"list_files","slots":{"dir":"src"},"command":"ls -la src"}

{"text":"hi","tokens":["hi"],"tags":["O"],"intent":"unknown","slots":{},"command":null}"#;
        let examples = parse_jsonl("x", text).unwrap();
        assert_eq!(examples.len(), 2);
        assert_eq!(examples[0].slots["dir"], "src");
        assert_eq!(examples[1].command, None);
    }

    #[test]
    fn test_parse_error_names_line() {
        let err = parse_jsonl("train.jsonl", "{}\n").unwrap_err();
        assert!(err.starts_with("train.jsonl:1:"), "{err}");
    }

    #[test]
    fn test_missing_directory_hint() {
        let err = load(Path::new("/nonexistent/cyore-commands"))
            .err()
            .unwrap();
        assert!(err.contains("make commands-data"), "{err}");
    }

    #[test]
    fn test_split_validation() {
        let mut all = Vec::new();
        for i in 0..20 {
            all.push(example(&format!("a {i}"), "a", None));
            all.push(example(&format!("b {i}"), "b", None));
        }
        let (fit, val) = split_validation(&all, 0.1, 7);
        assert_eq!(val.len(), 4); // 2 per intent
        assert_eq!(fit.len() + val.len(), all.len());
        assert!(val.iter().all(|v| !fit.contains(v)));
        assert_eq!(val.iter().filter(|e| e.intent == "a").count(), 2);
        let (_, val2) = split_validation(&all, 0.1, 7);
        assert_eq!(val, val2);
    }
}
