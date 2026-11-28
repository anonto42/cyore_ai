// Catalog validation. Everything is checked *before* any file is written, and every problem is
// reported (not just the first), each naming the intent it is about.
//
// `prepare` also does the parsing the generator needs (templates, expanded phrasings), so the
// generator only ever sees a catalog that has passed every check.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::catalog::{Catalog, SlotPool, UNKNOWN};
use crate::generate::Example;
use crate::grammar;
use crate::template::{SlotKind, Template};
use crate::tokenize::tokenize;

pub const MIN_PHRASINGS: usize = 20;
pub const MIN_TESTS: usize = 3;
pub const MIN_UNKNOWN_TESTS: usize = 10;
pub const MIN_INTENTS: usize = 50;

/// Programs and patterns from .agents/shared/rules/safety.md that must never appear.
const BLOCKED_PROGRAMS: &[&str] = &[
    "rm", "mkfs", "dd", "shutdown", "reboot", "halt", "poweroff", "sudo", "su",
];
const BLOCKED_PATTERNS: &[&str] = &[":(){", "/dev/sd", "chmod -R 777 /", "mkfs."];
/// Shell syntax: commands run without a shell, so none of this may appear.
const SHELL_METACHARACTERS: &[&str] = &["|", ">", "<", ";", "&", "$(", "`"];

/// What kind of sentence a phrasing line describes (decides which wrappers it may get).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Kind {
    Do,
    Ask,
    Plain,
}

pub fn split_kind(line: &str) -> (Kind, &str) {
    if let Some(rest) = line.strip_prefix("do:") {
        (Kind::Do, rest)
    } else if let Some(rest) = line.strip_prefix("ask:") {
        (Kind::Ask, rest)
    } else {
        (Kind::Plain, line)
    }
}

/// One expanded phrasing, still with `{slot}` placeholders.
#[derive(Debug, Clone)]
pub struct Phrasing {
    pub kind: Kind,
    pub text: String,
}

/// An intent after parsing and validation.
pub struct Prepared {
    pub name: &'static str,
    pub template: Option<Template>,
    pub phrasings: Vec<Phrasing>,
    /// For each grammar line, the indices into `phrasings` it produced. Sampling picks a line
    /// first, so a short line ("ping {host}") is not drowned out by a line with 48 expansions.
    pub lines: Vec<Vec<usize>>,
    pub tests: Vec<String>,
    pub dev: Vec<String>,
}

/// The slot names used in a sentence, in order. Every token touching a brace must be exactly
/// `{name}`, so a placeholder always becomes exactly one token.
pub fn placeholders(text: &str) -> Result<Vec<String>, String> {
    let mut names = Vec::new();
    for token in tokenize(text) {
        if token.contains('{') || token.contains('}') {
            match token.strip_prefix('{').and_then(|t| t.strip_suffix('}')) {
                Some(name) if !name.contains(['{', '}']) => names.push(name.to_string()),
                _ => {
                    return Err(format!(
                        "placeholder must be a whole word, got \"{token}\" in \"{text}\""
                    ));
                }
            }
        }
    }
    Ok(names)
}

fn check_command(name: &str, template: &Template, allowlist: &[&str], errors: &mut Vec<String>) {
    let source = &template.source;
    if !allowlist.contains(&template.program()) {
        errors.push(format!(
            "{name}: program '{}' in \"{source}\" is not on the allowlist",
            template.program()
        ));
    }
    let words: Vec<&str> = source.split_whitespace().collect();
    if words.iter().any(|w| BLOCKED_PROGRAMS.contains(w))
        || BLOCKED_PATTERNS.iter().any(|p| source.contains(p))
    {
        errors.push(format!("{name}: \"{source}\" matches the safety blocklist"));
    }
    if SHELL_METACHARACTERS.iter().any(|m| source.contains(m)) {
        errors.push(format!(
            "{name}: \"{source}\" uses shell syntax (pipes, redirection, ;, &, $( or backticks)"
        ));
    }
}

/// Check one sentence's placeholders against the intent's template.
fn check_slots(
    name: &str,
    text: &str,
    template: Option<&Template>,
    pools: &[SlotPool],
    errors: &mut Vec<String>,
) {
    let found = match placeholders(text) {
        Ok(found) => found,
        Err(e) => {
            errors.push(format!("{name}: {e}"));
            return;
        }
    };
    let slots = template.map(|t| t.slots()).unwrap_or_default();
    let mut seen = BTreeSet::new();
    for slot in &found {
        if !seen.insert(slot) {
            errors.push(format!("{name}: \"{text}\" uses {{{slot}}} twice"));
        }
        if !slots.iter().any(|(s, _)| s == slot) {
            errors.push(format!(
                "{name}: \"{text}\" uses {{{slot}}}, which the command does not have"
            ));
        }
        if !pools.iter().any(|p| p.name == slot) {
            errors.push(format!("{name}: no slot pool for {{{slot}}}"));
        }
    }
    for (slot, kind) in slots {
        if *kind == SlotKind::Required && !found.iter().any(|f| f == slot) {
            errors.push(format!(
                "{name}: \"{text}\" is missing the required slot {{{slot}}}"
            ));
        }
    }
}

fn check_pools(pools: &[SlotPool], errors: &mut Vec<String>) {
    let mut train_values = BTreeSet::new();
    for pool in pools {
        if pool.train.is_empty() || pool.test.is_empty() {
            errors.push(format!(
                "slot pool '{}' needs both train and test values",
                pool.name
            ));
        }
        for value in pool.train.iter().chain(pool.test) {
            let clean = tokenize(value) == [value.to_string()];
            let shell = SHELL_METACHARACTERS.iter().any(|m| value.contains(m));
            if !clean || shell || value.contains(['{', '}']) {
                errors.push(format!(
                    "slot pool '{}': value \"{value}\" must be one token with no shell syntax",
                    pool.name
                ));
            }
        }
        train_values.extend(pool.train.iter().copied());
    }
    for pool in pools {
        for value in pool.test {
            if train_values.contains(value) {
                errors.push(format!(
                    "slot pool '{}': test value \"{value}\" is also a training value",
                    pool.name
                ));
            }
        }
    }
}

/// Validate the whole catalog and return the parsed intents, or every problem found.
pub fn prepare(catalog: &Catalog) -> Result<Vec<Prepared>, Vec<String>> {
    let mut errors = Vec::new();
    let mut prepared = Vec::new();

    check_pools(catalog.pools, &mut errors);

    let mut names = BTreeSet::new();
    for intent in catalog.intents {
        if !names.insert(intent.name) {
            errors.push(format!("{}: duplicate intent name", intent.name));
        }
    }
    let command_intents = catalog.intents.iter().filter(|i| i.name != UNKNOWN).count();
    if command_intents < MIN_INTENTS {
        errors.push(format!(
            "catalog has {command_intents} command intents, need at least {MIN_INTENTS}"
        ));
    }
    if !names.contains(UNKNOWN) {
        errors.push(format!("catalog has no '{UNKNOWN}' intent"));
    }

    for intent in catalog.intents {
        let name = intent.name;
        let is_unknown = name == UNKNOWN;

        // Command template
        let template = match (intent.command, is_unknown) {
            (None, true) => None,
            (Some(_), true) => {
                errors.push(format!("{name}: must not have a command"));
                None
            }
            (None, false) => {
                errors.push(format!("{name}: has no command"));
                None
            }
            (Some(source), false) => match Template::parse(source) {
                Ok(t) => {
                    check_command(name, &t, catalog.allowlist, &mut errors);
                    Some(t)
                }
                Err(e) => {
                    errors.push(format!("{name}: {e}"));
                    None
                }
            },
        };

        // Phrasings: expand every line, dedupe, check slots, count.
        let mut phrasings: Vec<Phrasing> = Vec::new();
        let mut lines: Vec<Vec<usize>> = Vec::new();
        let mut distinct = BTreeSet::new();
        for line in intent.phrasings {
            let (kind, body) = split_kind(line);
            match grammar::expand(body) {
                Ok(texts) => {
                    let mut indices = Vec::new();
                    for text in texts {
                        if distinct.insert(text.to_lowercase()) {
                            check_slots(name, &text, template.as_ref(), catalog.pools, &mut errors);
                            indices.push(phrasings.len());
                            phrasings.push(Phrasing { kind, text });
                        }
                    }
                    if !indices.is_empty() {
                        lines.push(indices);
                    }
                }
                Err(e) => errors.push(format!("{name}: {e}")),
            }
        }
        if distinct.len() < MIN_PHRASINGS {
            errors.push(format!(
                "{name}: only {} distinct phrasings, need at least {MIN_PHRASINGS}",
                distinct.len()
            ));
        }

        // Hand-written test and development sentences: same minimums, same slot rules.
        let min_sentences = if is_unknown {
            MIN_UNKNOWN_TESTS
        } else {
            MIN_TESTS
        };
        for (kind, sentences) in [("test", intent.tests), ("development", intent.dev)] {
            if sentences.len() < min_sentences {
                errors.push(format!(
                    "{name}: only {} {kind} sentences, need at least {min_sentences}",
                    sentences.len()
                ));
            }
            for sentence in sentences {
                check_slots(
                    name,
                    sentence,
                    template.as_ref(),
                    catalog.pools,
                    &mut errors,
                );
            }
        }

        prepared.push(Prepared {
            name,
            template,
            phrasings,
            lines,
            tests: intent.tests.iter().map(|t| t.to_string()).collect(),
            dev: intent.dev.iter().map(|t| t.to_string()).collect(),
        });
    }

    // The same sentence under two intents would teach the model contradictory labels.
    let mut owner: HashMap<String, &str> = HashMap::new();
    for intent in &prepared {
        for p in &intent.phrasings {
            if let Some(other) = owner.insert(p.text.to_lowercase(), intent.name)
                && other != intent.name
            {
                errors.push(format!(
                    "{}: phrasing \"{}\" is also a phrasing of {other}",
                    intent.name, p.text
                ));
            }
        }
    }

    if errors.is_empty() {
        Ok(prepared)
    } else {
        Err(errors)
    }
}

/// Held-out sets must measure generalisation: no held-out text in training, and no held-out
/// slot value seen as a slot value in training. `label` names the set in messages.
pub fn check_disjoint(
    train: &[Example],
    held_out: &[Example],
    label: &str,
) -> Result<(), Vec<String>> {
    let train_texts: BTreeSet<String> = train.iter().map(|e| e.text.to_lowercase()).collect();
    let train_values: BTreeSet<&String> = train.iter().flat_map(|e| e.slots.values()).collect();
    let mut errors = Vec::new();
    for example in held_out {
        if train_texts.contains(&example.text.to_lowercase()) {
            errors.push(format!(
                "{}: {label} sentence \"{}\" also appears in training",
                example.intent, example.text
            ));
        }
        for value in example.slots.values() {
            if train_values.contains(value) {
                errors.push(format!(
                    "{}: {label} slot value \"{value}\" also appears in training",
                    example.intent
                ));
            }
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

/// The development and test sets must not share a sentence (lowercased), or tuning on the
/// development set would also tune on the test set.
pub fn check_dev_test_disjoint(dev: &[Example], test: &[Example]) -> Result<(), Vec<String>> {
    let test_texts: BTreeSet<String> = test.iter().map(|e| e.text.to_lowercase()).collect();
    let errors: Vec<String> = dev
        .iter()
        .filter(|e| test_texts.contains(&e.text.to_lowercase()))
        .map(|e| {
            format!(
                "{}: \"{}\" is in both the development and test sets",
                e.intent, e.text
            )
        })
        .collect();
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

/// Count examples per intent (used by the summary).
pub fn counts_per_intent(examples: &[Example]) -> BTreeMap<&str, usize> {
    let mut counts = BTreeMap::new();
    for e in examples {
        *counts.entry(e.intent.as_str()).or_insert(0) += 1;
    }
    counts
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::{Intent, catalog};

    const POOLS: &[SlotPool] = &[SlotPool {
        name: "dir",
        train: &["src", "docs"],
        test: &["build"],
    }];

    /// 4 × 5 = 20 phrasings, just enough.
    const TWENTY: &str = "do: (show|list|display|print) (the files|files|everything|it all|stuff)";

    fn errors_for(intents: &'static [Intent]) -> Vec<String> {
        let catalog = Catalog {
            intents,
            pools: POOLS,
            allowlist: &["ls", "cat"],
        };
        match prepare(&catalog) {
            Ok(_) => Vec::new(),
            Err(errors) => errors,
        }
    }

    fn has(errors: &[String], needle: &str) -> bool {
        errors.iter().any(|e| e.contains(needle))
    }

    #[test]
    fn test_real_catalog_is_valid() {
        if let Err(errors) = prepare(&catalog()) {
            panic!("catalog errors:\n{}", errors.join("\n"));
        }
    }

    #[test]
    fn test_too_few_phrasings() {
        const INTENTS: &[Intent] = &[Intent {
            name: "a",
            command: Some("ls"),
            phrasings: &["(show|list) files"],
            tests: &["x", "y", "z"],
            dev: &["d1", "d2", "d3"],
        }];
        assert!(has(&errors_for(INTENTS), "a: only 2 distinct phrasings"));
    }

    #[test]
    fn test_program_not_allowlisted() {
        const INTENTS: &[Intent] = &[Intent {
            name: "a",
            command: Some("curl example.com"),
            phrasings: &[TWENTY],
            tests: &["x", "y", "z"],
            dev: &["d1", "d2", "d3"],
        }];
        let errors = errors_for(INTENTS);
        assert!(has(&errors, "a: program 'curl'"));
        assert!(has(&errors, "curl example.com"));
    }

    #[test]
    fn test_blocklist() {
        const INTENTS: &[Intent] = &[Intent {
            name: "a",
            command: Some("ls ; rm -rf /"),
            phrasings: &[TWENTY],
            tests: &["x", "y", "z"],
            dev: &["d1", "d2", "d3"],
        }];
        let errors = errors_for(INTENTS);
        assert!(has(&errors, "matches the safety blocklist"));
        assert!(has(&errors, "uses shell syntax"));
    }

    #[test]
    fn test_pipe_rejected() {
        const INTENTS: &[Intent] = &[Intent {
            name: "a",
            command: Some("ls | cat"),
            phrasings: &[TWENTY],
            tests: &["x", "y", "z"],
            dev: &["d1", "d2", "d3"],
        }];
        assert!(has(
            &errors_for(INTENTS),
            "a: \"ls | cat\" uses shell syntax"
        ));
    }

    #[test]
    fn test_missing_required_slot() {
        const INTENTS: &[Intent] = &[Intent {
            name: "a",
            command: Some("ls {dir}"),
            phrasings: &[TWENTY],
            tests: &["x {dir}", "y {dir}", "z {dir}"],
            dev: &["d1", "d2", "d3"],
        }];
        assert!(has(&errors_for(INTENTS), "missing the required slot {dir}"));
    }

    #[test]
    fn test_unknown_placeholder() {
        const INTENTS: &[Intent] = &[Intent {
            name: "a",
            command: Some("ls"),
            phrasings: &[TWENTY, "do: show {file}"],
            tests: &["x", "y", "z"],
            dev: &["d1", "d2", "d3"],
        }];
        let errors = errors_for(INTENTS);
        assert!(has(&errors, "which the command does not have"));
        assert!(has(&errors, "no slot pool for {file}"));
    }

    #[test]
    fn test_placeholder_must_be_whole_word() {
        const INTENTS: &[Intent] = &[Intent {
            name: "a",
            command: Some("ls {dir?}"),
            phrasings: &[TWENTY, "do: show {dir}'s files"],
            tests: &["x", "y", "z"],
            dev: &["d1", "d2", "d3"],
        }];
        assert!(has(&errors_for(INTENTS), "must be a whole word"));
    }

    #[test]
    fn test_too_few_tests_and_duplicates() {
        const INTENTS: &[Intent] = &[
            Intent {
                name: "a",
                command: Some("ls"),
                phrasings: &[TWENTY],
                tests: &["x"],
                dev: &["d1", "d2", "d3"],
            },
            Intent {
                name: "a",
                command: Some("cat"),
                phrasings: &[TWENTY],
                tests: &["x", "y", "z"],
                dev: &["d1", "d2", "d3"],
            },
        ];
        let errors = errors_for(INTENTS);
        assert!(has(&errors, "a: only 1 test sentences"));
        assert!(!has(&errors, "development sentences"));
        assert!(has(&errors, "a: duplicate intent name"));
    }

    #[test]
    fn test_too_few_dev_sentences() {
        const INTENTS: &[Intent] = &[Intent {
            name: "a",
            command: Some("ls"),
            phrasings: &[TWENTY],
            tests: &["x", "y", "z"],
            dev: &["d"],
        }];
        assert!(has(&errors_for(INTENTS), "a: only 1 development sentences"));
    }

    #[test]
    fn test_dev_and_test_must_not_share_sentences() {
        use crate::generate::Example;
        let example = |text: &str| Example {
            text: text.to_string(),
            tokens: Vec::new(),
            tags: Vec::new(),
            intent: "a".to_string(),
            slots: Default::default(),
            command: None,
        };
        let errors = check_dev_test_disjoint(&[example("Show Files")], &[example("show files")])
            .unwrap_err();
        assert!(has(&errors, "in both the development and test sets"));
        assert!(check_dev_test_disjoint(&[example("a")], &[example("b")]).is_ok());
    }

    #[test]
    fn test_shared_phrasing_between_intents() {
        const INTENTS: &[Intent] = &[
            Intent {
                name: "a",
                command: Some("ls"),
                phrasings: &[TWENTY],
                tests: &["x", "y", "z"],
                dev: &["d1", "d2", "d3"],
            },
            Intent {
                name: "b",
                command: Some("cat"),
                phrasings: &[TWENTY],
                tests: &["x", "y", "z"],
                dev: &["d1", "d2", "d3"],
            },
        ];
        assert!(has(&errors_for(INTENTS), "is also a phrasing of a"));
    }

    #[test]
    fn test_catalog_size_and_unknown_required() {
        const INTENTS: &[Intent] = &[Intent {
            name: "a",
            command: Some("ls"),
            phrasings: &[TWENTY],
            tests: &["x", "y", "z"],
            dev: &["d1", "d2", "d3"],
        }];
        let errors = errors_for(INTENTS);
        assert!(has(&errors, "need at least 50"));
        assert!(has(&errors, "no 'unknown' intent"));
    }

    #[test]
    fn test_bad_grammar_reported() {
        const INTENTS: &[Intent] = &[Intent {
            name: "a",
            command: Some("ls"),
            phrasings: &["do: (show|list files"],
            tests: &["x", "y", "z"],
            dev: &["d1", "d2", "d3"],
        }];
        assert!(has(&errors_for(INTENTS), "a: grammar error"));
    }

    #[test]
    fn test_overlapping_pools_rejected() {
        static BAD_POOLS: &[SlotPool] = &[
            SlotPool {
                name: "dir",
                train: &["src"],
                test: &["docs"],
            },
            SlotPool {
                name: "file",
                train: &["docs"],
                test: &["two words"],
            },
        ];
        let mut errors = Vec::new();
        check_pools(BAD_POOLS, &mut errors);
        assert!(has(&errors, "test value \"docs\" is also a training value"));
        assert!(has(&errors, "\"two words\" must be one token"));
    }

    #[test]
    fn test_split_kind() {
        assert_eq!(split_kind("do: x").0, Kind::Do);
        assert_eq!(split_kind("ask: x").0, Kind::Ask);
        assert_eq!(split_kind("x").0, Kind::Plain);
    }
}
