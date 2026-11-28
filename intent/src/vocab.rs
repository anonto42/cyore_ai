// From words to numbers.
//
// The vocabulary only contains *context* words (tokens tagged `O`) seen at least twice in
// training. Slot values are never added. Every other token becomes one of five *shape* tokens,
// so an unseen file name still looks like a file name to the model (design.md, decision 4).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::data::Example;

pub const PAD: &str = "<pad>";
pub const WORD: &str = "<word>";
pub const PATH: &str = "<path>";
pub const GLOB: &str = "<glob>";
pub const NUM: &str = "<num>";
pub const FILE: &str = "<file>";
/// Special tokens, in id order: 0 = <pad>, then the shapes.
pub const SPECIALS: [&str; 6] = [PAD, WORD, PATH, GLOB, NUM, FILE];

/// Which shape token stands in for a word the vocabulary does not know.
pub fn shape(token: &str) -> &'static str {
    if token.contains('/') || token.starts_with('~') {
        PATH
    } else if token.contains('*') {
        GLOB
    } else if !token.is_empty()
        && token
            .chars()
            .all(|c| c.is_ascii_digit() || c == '.' || c == ':')
    {
        NUM
    } else if token
        .rsplit_once('.')
        .is_some_and(|(_, ext)| ext.chars().next().is_some_and(|c| c.is_alphabetic()))
    {
        FILE
    } else {
        WORD
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Vocab {
    /// Id → token. `SPECIALS` first, then the words in sorted order (so lookup is a binary search).
    pub tokens: Vec<String>,
}

impl Vocab {
    pub fn build(examples: &[Example], min_count: usize) -> Self {
        let mut counts: BTreeMap<String, usize> = BTreeMap::new();
        for e in examples {
            for (token, tag) in e.tokens.iter().zip(&e.tags) {
                if tag == "O" {
                    *counts.entry(token.to_lowercase()).or_insert(0) += 1;
                }
            }
        }
        let mut tokens: Vec<String> = SPECIALS.iter().map(|s| s.to_string()).collect();
        tokens.extend(
            counts
                .into_iter()
                .filter(|(w, c)| *c >= min_count && !SPECIALS.contains(&w.as_str()))
                .map(|(w, _)| w),
        );
        Vocab { tokens }
    }

    pub fn len(&self) -> usize {
        self.tokens.len()
    }

    /// Never true in practice: the special tokens are always present.
    pub fn is_empty(&self) -> bool {
        self.tokens.is_empty()
    }

    /// Id of the first real word; ids below are specials.
    pub fn first_word_id() -> usize {
        SPECIALS.len()
    }

    fn special_id(name: &str) -> usize {
        SPECIALS
            .iter()
            .position(|s| *s == name)
            .expect("name is one of SPECIALS")
    }

    pub fn word_id() -> usize {
        Self::special_id(WORD)
    }

    /// The id for a token: the lowercased word if known, otherwise its shape.
    pub fn id(&self, token: &str) -> usize {
        let lower = token.to_lowercase();
        let words = &self.tokens[Self::first_word_id()..];
        match words.binary_search_by(|w| w.as_str().cmp(&lower)) {
            Ok(i) => Self::first_word_id() + i,
            Err(_) => Self::special_id(shape(token)),
        }
    }

    pub fn encode(&self, tokens: &[String]) -> Vec<usize> {
        tokens.iter().map(|t| self.id(t)).collect()
    }
}

/// The output classes: intent names (in intents.json order) and slot tags (`O` first).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Labels {
    pub intents: Vec<String>,
    pub tags: Vec<String>,
}

impl Labels {
    /// `slot_names` are all slot names any template uses.
    pub fn new(intents: Vec<String>, slot_names: impl IntoIterator<Item = String>) -> Self {
        let mut tags: Vec<String> = slot_names.into_iter().map(|s| format!("B-{s}")).collect();
        tags.sort();
        tags.dedup();
        tags.insert(0, "O".to_string());
        Labels { intents, tags }
    }

    pub fn intent_id(&self, name: &str) -> Option<usize> {
        self.intents.iter().position(|i| i == name)
    }

    pub fn tag_id(&self, tag: &str) -> Option<usize> {
        self.tags.iter().position(|t| t == tag)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::tests::example;

    #[test]
    fn test_shapes() {
        assert_eq!(shape("~/notes"), PATH);
        assert_eq!(shape("/etc"), PATH);
        assert_eq!(shape("*.go"), GLOB);
        assert_eq!(shape("10.0.0.1"), NUM);
        assert_eq!(shape("42"), NUM);
        assert_eq!(shape("budget.xlsx"), FILE);
        assert_eq!(shape(".vimrc"), FILE);
        assert_eq!(shape("debian.org"), FILE);
        assert_eq!(shape("heaviest"), WORD);
        assert_eq!(shape("sha-256"), WORD);
    }

    #[test]
    fn test_build_ignores_slot_values_and_rare_words() {
        let examples = [
            example("show files in src", "list_files", Some(("dir", "src"))),
            example("show files in src", "list_files", Some(("dir", "src"))),
            example("show rare", "list_files", None),
        ];
        let vocab = Vocab::build(&examples, 2);
        assert!(vocab.tokens.contains(&"show".to_string()));
        assert!(vocab.tokens.contains(&"files".to_string()));
        assert!(!vocab.tokens.contains(&"src".to_string()), "slot value");
        assert!(!vocab.tokens.contains(&"rare".to_string()), "seen once");
        assert_eq!(&vocab.tokens[..6], &SPECIALS);
    }

    #[test]
    fn test_encode() {
        let examples = [
            example("Show files", "a", None),
            example("show files", "a", None),
        ];
        let vocab = Vocab::build(&examples, 2);
        let ids = vocab.encode(&[
            "SHOW".into(),
            "notes.txt".into(),
            "~/x".into(),
            "zzz".into(),
        ]);
        assert_eq!(vocab.tokens[ids[0]], "show"); // case-insensitive
        assert_eq!(vocab.tokens[ids[1]], FILE);
        assert_eq!(vocab.tokens[ids[2]], PATH);
        assert_eq!(ids[3], Vocab::word_id());
    }

    #[test]
    fn test_json_round_trip() {
        let vocab = Vocab::build(&[example("a b", "x", None), example("a b", "x", None)], 1);
        let back: Vocab = serde_json::from_str(&serde_json::to_string(&vocab).unwrap()).unwrap();
        assert_eq!(back, vocab);
    }

    #[test]
    fn test_labels() {
        let labels = Labels::new(
            vec!["list_files".into(), "unknown".into()],
            ["file".to_string(), "dir".to_string(), "dir".to_string()],
        );
        assert_eq!(labels.tags, ["O", "B-dir", "B-file"]);
        assert_eq!(labels.intent_id("unknown"), Some(1));
        assert_eq!(labels.tag_id("B-file"), Some(2));
        assert_eq!(labels.tag_id("B-host"), None);
    }
}
