// Command templates: whitespace-separated words, where a word may be a slot:
//
//   {dir}       required: the request must supply it
//   {dir?}      optional: the word disappears when no value is given
//   {dir=.}     optional with a default: `.` is used when no value is given
//
// `ls -la {dir?}` → `ls -la` or `ls -la Downloads`.
// Each word becomes one argument for std::process::Command in the Stage 3 agent, which is why
// slot values may never contain whitespace.

use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq)]
pub enum SlotKind {
    Required,
    Optional,
    Default(String),
}

#[derive(Debug, Clone, PartialEq)]
enum Word {
    Literal(String),
    Slot { name: String, kind: SlotKind },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Template {
    pub source: String,
    words: Vec<Word>,
}

impl Template {
    pub fn parse(source: &str) -> Result<Self, String> {
        let mut words = Vec::new();
        for word in source.split_whitespace() {
            let parsed = match word.strip_prefix('{').and_then(|w| w.strip_suffix('}')) {
                Some(inner) => {
                    let (name, kind) = if let Some(name) = inner.strip_suffix('?') {
                        (name, SlotKind::Optional)
                    } else if let Some((name, default)) = inner.split_once('=') {
                        if default.is_empty() {
                            return Err(format!("template \"{source}\": empty default in {word}"));
                        }
                        (name, SlotKind::Default(default.to_string()))
                    } else {
                        (inner, SlotKind::Required)
                    };
                    if name.is_empty() || !name.chars().all(|c| c.is_ascii_lowercase() || c == '_')
                    {
                        return Err(format!("template \"{source}\": bad slot name in {word}"));
                    }
                    Word::Slot {
                        name: name.to_string(),
                        kind,
                    }
                }
                None if word.contains('{') || word.contains('}') => {
                    return Err(format!(
                        "template \"{source}\": a slot must be a whole word, got {word}"
                    ));
                }
                None => Word::Literal(word.to_string()),
            };
            words.push(parsed);
        }
        match words.first() {
            Some(Word::Literal(_)) => Ok(Template {
                source: source.to_string(),
                words,
            }),
            _ => Err(format!(
                "template \"{source}\": must start with a program name"
            )),
        }
    }

    /// The program (first word), e.g. `ls`.
    pub fn program(&self) -> &str {
        match &self.words[0] {
            Word::Literal(p) => p,
            Word::Slot { .. } => unreachable!("parse guarantees a literal first word"),
        }
    }

    /// How someone might type the command itself as a request: the program, its plain-word
    /// arguments, and slot placeholders, without options or numbers. With every slot and with
    /// only the required ones: `grep -rn {text} {dir=.}` → ["grep {text} {dir}", "grep {text}"];
    /// `git remote -v` → ["git remote"].
    pub fn spoken_forms(&self) -> Vec<String> {
        let mut forms: Vec<String> = Vec::new();
        for all_slots in [true, false] {
            let words: Vec<String> =
                self.words
                    .iter()
                    .filter_map(|w| match w {
                        Word::Literal(text) if text.chars().all(|c| c.is_alphabetic()) => {
                            Some(text.clone())
                        }
                        Word::Literal(_) => None,
                        Word::Slot { name, kind } => (all_slots || *kind == SlotKind::Required)
                            .then(|| format!("{{{name}}}")),
                    })
                    .collect();
            let form = words.join(" ");
            if !forms.contains(&form) {
                forms.push(form);
            }
        }
        forms
    }

    /// The template's slots in order: (name, kind).
    pub fn slots(&self) -> Vec<(&str, &SlotKind)> {
        self.words
            .iter()
            .filter_map(|w| match w {
                Word::Slot { name, kind } => Some((name.as_str(), kind)),
                Word::Literal(_) => None,
            })
            .collect()
    }

    /// Fill in the slots. Every key in `values` must be a slot of this template.
    pub fn fill(&self, values: &BTreeMap<String, String>) -> Result<String, String> {
        for key in values.keys() {
            if !self.slots().iter().any(|(name, _)| name == key) {
                return Err(format!("template \"{}\" has no slot '{key}'", self.source));
            }
        }
        let mut out: Vec<&str> = Vec::new();
        for word in &self.words {
            match word {
                Word::Literal(text) => out.push(text),
                Word::Slot { name, kind } => match (values.get(name), kind) {
                    (Some(value), _) => out.push(value),
                    (None, SlotKind::Default(default)) => out.push(default),
                    (None, SlotKind::Optional) => {}
                    (None, SlotKind::Required) => {
                        return Err(format!(
                            "template \"{}\" needs a value for '{name}'",
                            self.source
                        ));
                    }
                },
            }
        }
        Ok(out.join(" "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn values(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn test_required_slot() {
        let t = Template::parse("cat {file}").unwrap();
        assert_eq!(t.program(), "cat");
        assert_eq!(t.fill(&values(&[("file", "a.txt")])).unwrap(), "cat a.txt");
        assert!(t.fill(&values(&[])).unwrap_err().contains("needs a value"));
    }

    #[test]
    fn test_optional_slot_disappears() {
        let t = Template::parse("ls -la {dir?}").unwrap();
        assert_eq!(t.fill(&values(&[])).unwrap(), "ls -la");
        assert_eq!(t.fill(&values(&[("dir", "src")])).unwrap(), "ls -la src");
    }

    #[test]
    fn test_default_slot() {
        let t = Template::parse("find {dir=.} -name {pattern}").unwrap();
        assert_eq!(
            t.fill(&values(&[("pattern", "*.rs")])).unwrap(),
            "find . -name *.rs"
        );
        assert_eq!(
            t.slots(),
            vec![
                ("dir", &SlotKind::Default(".".to_string())),
                ("pattern", &SlotKind::Required)
            ]
        );
    }

    #[test]
    fn test_no_double_spaces() {
        let t = Template::parse("grep -rn {text} {dir?}").unwrap();
        let filled = t.fill(&values(&[("text", "TODO")])).unwrap();
        assert_eq!(filled, "grep -rn TODO");
        assert!(!filled.contains("  "));
    }

    #[test]
    fn test_unknown_value_rejected() {
        let t = Template::parse("pwd").unwrap();
        assert!(t.fill(&values(&[("dir", "x")])).is_err());
    }

    #[test]
    fn test_spoken_forms() {
        let forms = |t: &str| Template::parse(t).unwrap().spoken_forms();
        assert_eq!(
            forms("grep -rn {text} {dir=.}"),
            ["grep {text} {dir}", "grep {text}"]
        );
        assert_eq!(forms("git remote -v"), ["git remote"]);
        assert_eq!(forms("head -n 20 {file}"), ["head {file}"]);
        assert_eq!(forms("ls -la {dir?}"), ["ls {dir}", "ls"]);
        assert_eq!(forms("printenv PATH"), ["printenv PATH"]);
    }

    #[test]
    fn test_bad_templates() {
        assert!(Template::parse("{dir} ls").is_err());
        assert!(Template::parse("ls {dir").is_err());
        assert!(Template::parse("ls x{dir}").is_err());
        assert!(Template::parse("ls {dir=}").is_err());
        assert!(Template::parse("").is_err());
    }
}
