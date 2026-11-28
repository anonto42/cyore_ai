// Turning the validated catalog into examples.
//
// Training: for each intent, draw `per_intent` examples — a random phrasing, random slot values
// from the TRAIN pools, and random wrappers ("please", "can you", "?") — skipping duplicates.
// Test: every hand-written test sentence once, with slot values from the TEST pools.
//
// Labels come from the placeholders, never from searching for the value in the text: the
// sentence is tokenized while it still says `{dir}`, so we know exactly which token is the slot.

use std::collections::{BTreeMap, HashSet};

use serde::Serialize;

use crate::catalog::{ASK_PREFIXES, DO_PREFIXES, DO_SUFFIXES, SlotPool};
use crate::random::Rng;
use crate::template::SlotKind;
use crate::tokenize::tokenize;
use crate::validate::{Kind, Prepared, placeholders};

/// One labelled example, as written to the JSONL files.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Example {
    pub text: String,
    pub tokens: Vec<String>,
    /// One per token: `O`, or `B-<slot>` for the token holding that slot's value.
    pub tags: Vec<String>,
    pub intent: String,
    /// BTreeMap so the JSON key order is fixed (reproducible output).
    pub slots: BTreeMap<String, String>,
    pub command: Option<String>,
}

/// Which side of the slot pools to draw from.
#[derive(Clone, Copy)]
pub enum Split {
    Train,
    Test,
}

/// Probabilities for the wrappers.
const P_DO_PREFIX: f64 = 0.35;
const P_DO_SUFFIX: f64 = 0.25;
const P_ASK_PREFIX: f64 = 0.2;
const P_QUESTION_MARK: f64 = 0.5;
const P_FULL_STOP: f64 = 0.1;
const P_CAPITALISE: f64 = 0.5;
/// Chance of contracting each eligible word pair ("what is" → "what's").
const P_CONTRACT: f64 = 0.4;

/// Everyday contractions. People type "what's" (or "whats") far more often than the grammar's
/// "what is", so the model should see both.
const CONTRACTIONS: &[(&str, &str, &[&str])] = &[
    ("what", "is", &["what's", "whats"]),
    ("where", "is", &["where's"]),
    ("who", "is", &["who's"]),
    ("how", "is", &["how's"]),
    ("it", "is", &["it's"]),
    ("that", "is", &["that's"]),
    ("there", "is", &["there's"]),
    ("i", "am", &["i'm", "im"]),
    ("do", "not", &["don't"]),
];

/// Randomly contract word pairs listed in CONTRACTIONS (case-insensitive match).
pub fn contract(text: &str, rng: &mut Rng) -> String {
    let words: Vec<&str> = text.split(' ').collect();
    let mut out: Vec<String> = Vec::with_capacity(words.len());
    let mut i = 0;
    while i < words.len() {
        let pair = words.get(i + 1).and_then(|next| {
            CONTRACTIONS
                .iter()
                .find(|(a, b, _)| words[i].eq_ignore_ascii_case(a) && next.eq_ignore_ascii_case(b))
        });
        match pair {
            Some((_, _, forms)) if rng.chance(P_CONTRACT) => {
                out.push(rng.choose(forms).to_string());
                i += 2;
            }
            _ => {
                out.push(words[i].to_string());
                i += 1;
            }
        }
    }
    out.join(" ")
}

/// Give up on an intent after this many draws per wanted example (all duplicates).
const MAX_ATTEMPTS_PER_EXAMPLE: usize = 50;

fn pool<'a>(pools: &'a [SlotPool], name: &str) -> &'a SlotPool {
    pools
        .iter()
        .find(|p| p.name == name)
        .expect("validation guarantees a pool for every slot")
}

/// Pick a value for every placeholder in `text`.
fn draw_values(
    text: &str,
    pools: &[SlotPool],
    split: Split,
    rng: &mut Rng,
) -> BTreeMap<String, String> {
    let names = placeholders(text).expect("validation checked placeholders");
    names
        .into_iter()
        .map(|name| {
            let p = pool(pools, &name);
            let values = match split {
                Split::Train => p.train,
                Split::Test => p.test,
            };
            let value = rng.choose(values).to_string();
            (name, value)
        })
        .collect()
}

/// Add natural variety around a phrasing. Works on the placeholder form, so `{dir}?` still
/// tokenizes to `{dir}`.
pub fn wrap(kind: Kind, text: &str, rng: &mut Rng) -> String {
    let mut out = contract(text, rng);
    let mut ends_with_question = false;
    match kind {
        Kind::Do => {
            if rng.chance(P_DO_PREFIX) {
                let prefix = *rng.choose(DO_PREFIXES);
                ends_with_question = prefix.ends_with("you");
                out = format!("{prefix} {out}");
            }
            if rng.chance(P_DO_SUFFIX) {
                out = format!("{out} {}", rng.choose(DO_SUFFIXES));
            }
        }
        Kind::Ask => {
            if rng.chance(P_ASK_PREFIX) {
                out = format!("{} {out}", rng.choose(ASK_PREFIXES));
            }
            ends_with_question = true;
        }
        Kind::Plain => ends_with_question = true,
    }
    if ends_with_question {
        if rng.chance(P_QUESTION_MARK) {
            out.push('?');
        }
    } else if rng.chance(P_FULL_STOP) {
        out.push('.');
    }
    if rng.chance(P_CAPITALISE) {
        let mut chars = out.chars();
        if let Some(first) = chars.next() {
            out = first.to_uppercase().chain(chars).collect();
        }
    }
    out
}

/// Fill a placeholder sentence with `values` and label it.
pub fn build_example(
    intent: &Prepared,
    placeholder_text: &str,
    values: &BTreeMap<String, String>,
) -> Result<Example, String> {
    let mut tokens = Vec::new();
    let mut tags = Vec::new();
    for token in tokenize(placeholder_text) {
        match token.strip_prefix('{').and_then(|t| t.strip_suffix('}')) {
            Some(name) => {
                let value = values
                    .get(name)
                    .ok_or_else(|| format!("{}: no value for {{{name}}}", intent.name))?;
                tokens.push(value.clone());
                tags.push(format!("B-{name}"));
            }
            None => {
                tokens.push(token);
                tags.push("O".to_string());
            }
        }
    }

    let mut text = placeholder_text.to_string();
    for (name, value) in values {
        text = text.replace(&format!("{{{name}}}"), value);
    }
    // Tokenizing the final text must give the same tokens we labelled.
    if tokenize(&text) != tokens {
        return Err(format!(
            "{}: \"{text}\" does not tokenize to its labelled tokens",
            intent.name
        ));
    }

    let command = match &intent.template {
        Some(template) => Some(template.fill(values)?),
        None => None,
    };
    Ok(Example {
        text,
        tokens,
        tags,
        intent: intent.name.to_string(),
        slots: values.clone(),
        command,
    })
}

/// Training examples: `per_intent` per intent where possible, no duplicate texts, shuffled.
/// `held_out` holds the development and test texts: a generated sentence that happens to equal
/// one of them word for word (compared lowercased) is skipped, so held-out sets stay unseen.
pub fn generate_train(
    intents: &[Prepared],
    pools: &[SlotPool],
    per_intent: usize,
    held_out: &[Example],
    rng: &mut Rng,
) -> Result<Vec<Example>, String> {
    let mut seen: HashSet<String> = held_out.iter().map(|e| e.text.to_lowercase()).collect();
    let mut examples = Vec::new();
    for intent in intents {
        let mut made = 0;
        let mut attempts = 0;
        while made < per_intent && attempts < per_intent * MAX_ATTEMPTS_PER_EXAMPLE {
            attempts += 1;
            // Line first, then one of its expansions (see Prepared::lines).
            let line = rng.choose(&intent.lines);
            let phrasing = &intent.phrasings[*rng.choose(line)];
            let values = draw_values(&phrasing.text, pools, Split::Train, rng);
            let wrapped = wrap(phrasing.kind, &phrasing.text, rng);
            let example = build_example(intent, &wrapped, &values)?;
            if seen.insert(example.text.to_lowercase()) {
                examples.push(example);
                made += 1;
            }
        }
    }
    rng.shuffle(&mut examples);
    Ok(examples)
}

/// Test examples: each hand-written sentence once, with values from the test pools.
pub fn generate_test(
    intents: &[Prepared],
    pools: &[SlotPool],
    rng: &mut Rng,
) -> Result<Vec<Example>, String> {
    held_out(intents, pools, |i| &i.tests, rng)
}

/// Development examples: each hand-written development sentence once, with values from the
/// test pools (values never seen in training).
pub fn generate_dev(
    intents: &[Prepared],
    pools: &[SlotPool],
    rng: &mut Rng,
) -> Result<Vec<Example>, String> {
    held_out(intents, pools, |i| &i.dev, rng)
}

fn held_out(
    intents: &[Prepared],
    pools: &[SlotPool],
    sentences: impl Fn(&Prepared) -> &[String],
    rng: &mut Rng,
) -> Result<Vec<Example>, String> {
    let mut examples = Vec::new();
    for intent in intents {
        for sentence in sentences(intent) {
            let values = draw_values(sentence, pools, Split::Test, rng);
            examples.push(build_example(intent, sentence, &values)?);
        }
    }
    Ok(examples)
}

/// The label list written to intents.json.
#[derive(Debug, Serialize)]
pub struct IntentInfo {
    pub name: String,
    pub command: Option<String>,
    pub slots: Vec<SlotInfo>,
}

#[derive(Debug, Serialize)]
pub struct SlotInfo {
    pub name: String,
    pub required: bool,
    pub default: Option<String>,
}

pub fn intent_infos(intents: &[Prepared]) -> Vec<IntentInfo> {
    intents
        .iter()
        .map(|intent| IntentInfo {
            name: intent.name.to_string(),
            command: intent.template.as_ref().map(|t| t.source.clone()),
            slots: intent
                .template
                .as_ref()
                .map(|t| {
                    t.slots()
                        .into_iter()
                        .map(|(name, kind)| SlotInfo {
                            name: name.to_string(),
                            required: *kind == SlotKind::Required,
                            default: match kind {
                                SlotKind::Default(d) => Some(d.clone()),
                                _ => None,
                            },
                        })
                        .collect()
                })
                .unwrap_or_default(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::catalog;
    use crate::template::Template;
    use crate::validate::prepare;

    fn real() -> (Vec<Prepared>, &'static [SlotPool]) {
        let c = catalog();
        (prepare(&c).expect("catalog is valid"), c.pools)
    }

    fn list_files() -> Prepared {
        Prepared {
            name: "list_files",
            template: Some(Template::parse("ls -la {dir?}").unwrap()),
            phrasings: Vec::new(),
            lines: Vec::new(),
            tests: Vec::new(),
            dev: Vec::new(),
        }
    }

    #[test]
    fn test_build_example_labels_slot_token() {
        let values = BTreeMap::from([("dir".to_string(), "src".to_string())]);
        let e = build_example(&list_files(), "Can you list the files in {dir}?", &values).unwrap();
        assert_eq!(e.text, "Can you list the files in src?");
        assert_eq!(
            e.tokens,
            ["Can", "you", "list", "the", "files", "in", "src"]
        );
        assert_eq!(e.tags, ["O", "O", "O", "O", "O", "O", "B-dir"]);
        assert_eq!(e.command.as_deref(), Some("ls -la src"));
    }

    #[test]
    fn test_build_example_without_optional_slot() {
        let e = build_example(&list_files(), "show the files", &BTreeMap::new()).unwrap();
        assert_eq!(e.command.as_deref(), Some("ls -la"));
        assert!(e.tags.iter().all(|t| t == "O"));
    }

    #[test]
    fn test_tags_align_and_commands_reconstruct() {
        let (intents, pools) = real();
        let train = generate_train(&intents, pools, 20, &[], &mut Rng::new(1)).unwrap();
        let test = generate_test(&intents, pools, &mut Rng::new(2)).unwrap();
        for e in train.iter().chain(&test) {
            assert_eq!(e.tokens, tokenize(&e.text), "{}", e.text);
            assert_eq!(e.tags.len(), e.tokens.len(), "{}", e.text);
            // Tokens tagged B-<slot> are exactly the slot values.
            let tagged: BTreeMap<String, String> = e
                .tags
                .iter()
                .zip(&e.tokens)
                .filter_map(|(tag, tok)| tag.strip_prefix("B-").map(|s| (s.into(), tok.clone())))
                .collect();
            assert_eq!(tagged, e.slots, "{}", e.text);
            // The command is the template filled with the slots.
            let intent = intents.iter().find(|i| i.name == e.intent).unwrap();
            let expected = intent.template.as_ref().map(|t| t.fill(&e.slots).unwrap());
            assert_eq!(e.command, expected, "{}", e.text);
        }
    }

    #[test]
    fn test_same_seed_same_examples() {
        let (intents, pools) = real();
        let a = generate_train(&intents, pools, 10, &[], &mut Rng::new(42)).unwrap();
        let b = generate_train(&intents, pools, 10, &[], &mut Rng::new(42)).unwrap();
        assert_eq!(a, b);
        let c = generate_train(&intents, pools, 10, &[], &mut Rng::new(43)).unwrap();
        assert_ne!(a, c);
    }

    #[test]
    fn test_per_intent_count_and_no_duplicates() {
        let (intents, pools) = real();
        let train = generate_train(&intents, pools, 30, &[], &mut Rng::new(7)).unwrap();
        assert_eq!(train.len(), 30 * intents.len());
        let unique: HashSet<String> = train.iter().map(|e| e.text.to_lowercase()).collect();
        assert_eq!(unique.len(), train.len());
    }

    #[test]
    fn test_unknown_has_no_command_or_slots() {
        let (intents, pools) = real();
        let test = generate_test(&intents, pools, &mut Rng::new(1)).unwrap();
        let unknown: Vec<_> = test.iter().filter(|e| e.intent == "unknown").collect();
        assert!(unknown.len() >= 10);
        assert!(
            unknown
                .iter()
                .all(|e| e.command.is_none() && e.slots.is_empty())
        );
    }

    #[test]
    fn test_contractions() {
        let mut rng = Rng::new(3);
        let mut seen = HashSet::new();
        for _ in 0..200 {
            seen.insert(contract("What is in {dir}", &mut rng));
        }
        assert!(seen.contains("What is in {dir}"));
        assert!(seen.contains("what's in {dir}"));
        assert!(seen.contains("whats in {dir}"));
        // Words that only look similar are left alone.
        assert_eq!(contract("whatis in it", &mut rng), "whatis in it");
    }

    #[test]
    fn test_every_line_gets_sampled() {
        // ping_host has a one-expansion line ("ping {host}"); line-first sampling must use it.
        let (intents, pools) = real();
        let train = generate_train(&intents, pools, 80, &[], &mut Rng::new(42)).unwrap();
        let pings = train
            .iter()
            .filter(|e| {
                e.intent == "ping_host" && e.tokens.iter().any(|t| t.eq_ignore_ascii_case("ping"))
            })
            .count();
        assert!(pings >= 5, "only {pings} examples use the word ping");
    }

    #[test]
    fn test_wrap_keeps_placeholder_token() {
        let mut rng = Rng::new(9);
        for _ in 0..200 {
            let w = wrap(Kind::Do, "show {dir}", &mut rng);
            assert!(tokenize(&w).contains(&"{dir}".to_string()), "{w}");
        }
    }

    #[test]
    fn test_intent_infos() {
        let (intents, _) = real();
        let infos = intent_infos(&intents);
        let find = infos.iter().find(|i| i.name == "find_by_name").unwrap();
        assert_eq!(
            find.command.as_deref(),
            Some("find {dir=.} -name {pattern}")
        );
        assert_eq!(find.slots.len(), 2);
        assert_eq!(find.slots[0].default.as_deref(), Some("."));
        assert!(find.slots[1].required);
        let unknown = infos.iter().find(|i| i.name == "unknown").unwrap();
        assert!(unknown.command.is_none());
    }
}
