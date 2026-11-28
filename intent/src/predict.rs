// From one English request to an intent, slots and a command (design.md, decision 9).
//
// `decode` holds all the rules and works on plain probabilities, so it is tested without a
// model. `Predictor` runs the network and hands its probabilities to `decode`.

use std::collections::BTreeMap;
use std::path::Path;

use burn::prelude::*;
use burn::tensor::activation::softmax;
use dataset::template::{SlotKind, Template};
use dataset::tokenize::tokenize;
use serde::Serialize;

use crate::model::JointModel;
use crate::store::{self, Meta};
use crate::vocab::Labels;

pub const UNKNOWN: &str = "unknown";
/// Longer requests are cut to this many tokens.
pub const MAX_TOKENS: usize = 64;
pub const DEFAULT_THRESHOLD: f32 = 0.7;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Prediction {
    pub tokens: Vec<String>,
    /// Predicted tag per token.
    pub tags: Vec<String>,
    pub intent: String,
    /// Probability of `intent`, in [0, 1].
    pub confidence: f32,
    /// The three most likely intents with their probabilities.
    pub top: Vec<(String, f32)>,
    /// Slot values, copied verbatim from the request.
    pub slots: BTreeMap<String, String>,
    /// The command to suggest, or None (see `reason`).
    pub command: Option<String>,
    pub reason: Option<String>,
}

fn argmax(values: &[f32]) -> usize {
    let mut best = 0;
    for (i, &v) in values.iter().enumerate() {
        if v > values[best] {
            best = i;
        }
    }
    best
}

/// Apply the prediction rules to the model's probabilities.
/// `templates[i]` is the command template of `labels.intents[i]` (None for `unknown`).
pub fn decode(
    tokens: Vec<String>,
    intent_probs: &[f32],
    tag_probs: &[Vec<f32>],
    labels: &Labels,
    templates: &[Option<Template>],
    threshold: f32,
) -> Prediction {
    let mut ranking: Vec<usize> = (0..intent_probs.len()).collect();
    ranking.sort_by(|&a, &b| intent_probs[b].total_cmp(&intent_probs[a]).then(a.cmp(&b)));
    let best = ranking[0];
    let intent = labels.intents[best].clone();
    let confidence = intent_probs[best];
    let top: Vec<(String, f32)> = ranking
        .iter()
        .take(3)
        .map(|&i| (labels.intents[i].clone(), intent_probs[i]))
        .collect();
    let tag_ids: Vec<usize> = tag_probs.iter().map(|p| argmax(p)).collect();
    let tags = tag_ids.iter().map(|&t| labels.tags[t].clone()).collect();

    // For each slot the intent's command has, take the token tagged B-<slot> with the
    // highest probability for that tag. Tags for other slots are ignored.
    let template = templates[best].as_ref();
    let mut slots = BTreeMap::new();
    let mut missing = Vec::new();
    for (name, kind) in template.map(|t| t.slots()).unwrap_or_default() {
        let found = labels.tag_id(&format!("B-{name}")).and_then(|tag| {
            (0..tokens.len())
                .filter(|&t| tag_ids[t] == tag)
                .max_by(|&a, &b| {
                    tag_probs[a][tag]
                        .total_cmp(&tag_probs[b][tag])
                        .then(b.cmp(&a))
                })
        });
        match found {
            Some(t) => {
                slots.insert(name.to_string(), tokens[t].clone());
            }
            None if *kind == SlotKind::Required => missing.push(name.to_string()),
            None => {}
        }
    }

    let (command, reason) = if intent == UNKNOWN {
        (None, Some("this request is not supported".to_string()))
    } else if confidence < threshold {
        let guesses: Vec<String> = top.iter().map(|(n, p)| format!("{n} {p:.2}")).collect();
        let why = format!("not sure (top guesses: {})", guesses.join(", "));
        (None, Some(why))
    } else if !missing.is_empty() {
        (
            None,
            Some(format!("missing a value for {}", missing.join(", "))),
        )
    } else {
        match template.map(|t| t.fill(&slots)) {
            Some(Ok(command)) => (Some(command), None),
            Some(Err(e)) => (None, Some(e)),
            None => (None, Some(format!("{intent} has no command"))),
        }
    };

    Prediction {
        tokens,
        tags,
        intent,
        confidence,
        top,
        slots,
        command,
        reason,
    }
}

/// Parse the command template for each intent, in label order.
fn templates_for(meta: &Meta) -> Result<Vec<Option<Template>>, String> {
    meta.labels
        .intents
        .iter()
        .map(|name| {
            let info = meta
                .intents
                .iter()
                .find(|i| &i.name == name)
                .ok_or_else(|| format!("meta.json: no template entry for intent {name}"))?;
            info.command.as_deref().map(Template::parse).transpose()
        })
        .collect()
}

pub struct Predictor {
    model: JointModel,
    meta: Meta,
    templates: Vec<Option<Template>>,
    device: Device,
}

impl Predictor {
    /// Wrap a model that is already in inference mode (e.g. straight after training).
    pub fn new(model: JointModel, meta: Meta) -> Result<Self, String> {
        let templates = templates_for(&meta)?;
        Ok(Predictor {
            model,
            meta,
            templates,
            device: Device::flex(),
        })
    }

    /// Load a saved model from `dir` (model.bpk + meta.json).
    pub fn load(dir: &Path) -> Result<Self, String> {
        let device = Device::flex();
        let (model, meta) = store::load(dir, &device)?;
        Self::new(model, meta)
    }

    /// Load the int8 model from `dir` (model-q8.bpk + meta.json).
    pub fn load_q8(dir: &Path) -> Result<Self, String> {
        let device = Device::flex();
        let (model, meta) = store::load_file(dir, store::MODEL_Q8_FILE, &device)?;
        Self::new(model, meta)
    }

    /// Load from memory that lives for the whole program (e.g. `include_bytes!`).
    pub fn from_static(weights: &'static [u8], meta_json: &str) -> Result<Self, String> {
        let device = Device::flex();
        let (model, meta) = store::load_static(weights, meta_json, &device)?;
        Self::new(model, meta)
    }

    pub fn meta(&self) -> &Meta {
        &self.meta
    }

    pub fn model(&self) -> &JointModel {
        &self.model
    }

    pub fn predict(&self, text: &str, threshold: f32) -> Prediction {
        let mut tokens = tokenize(text);
        tokens.truncate(MAX_TOKENS);
        if tokens.is_empty() {
            return Prediction {
                tokens,
                tags: Vec::new(),
                intent: UNKNOWN.to_string(),
                confidence: 0.0,
                top: Vec::new(),
                slots: BTreeMap::new(),
                command: None,
                reason: Some("empty request".to_string()),
            };
        }
        let ids: Vec<i64> = self
            .meta
            .vocab
            .encode(&tokens)
            .into_iter()
            .map(|i| i as i64)
            .collect();
        let n = ids.len();
        let ids = Tensor::<2, Int>::from_data(TensorData::new(ids, [1, n]), &self.device);
        let (intent_logits, slot_logits) = self.model.forward(ids);
        let intent_probs: Vec<f32> = softmax(intent_logits, 1)
            .into_data()
            .iter::<f32>()
            .collect();
        let n_tags = self.meta.labels.tags.len();
        let flat: Vec<f32> = softmax(slot_logits, 2).into_data().iter::<f32>().collect();
        let tag_probs: Vec<Vec<f32>> = flat.chunks(n_tags).map(|c| c.to_vec()).collect();
        decode(
            tokens,
            &intent_probs,
            &tag_probs,
            &self.meta.labels,
            &self.templates,
            threshold,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labels() -> Labels {
        Labels::new(
            vec!["list_files".into(), "show_file".into(), "unknown".into()],
            ["dir".to_string(), "file".to_string()],
        ) // tags: O, B-dir, B-file
    }

    fn templates() -> Vec<Option<Template>> {
        vec![
            Some(Template::parse("ls -la {dir?}").unwrap()),
            Some(Template::parse("cat {file}").unwrap()),
            None,
        ]
    }

    fn toks(text: &str) -> Vec<String> {
        tokenize(text)
    }

    /// Tag probabilities: one row per token, putting `p` on the given tag and the rest on O.
    fn tag_rows(spec: &[(usize, f32)]) -> Vec<Vec<f32>> {
        spec.iter()
            .map(|&(tag, p)| {
                let mut row = vec![0.0; 3];
                row[tag] = p;
                row[0] += 1.0 - p;
                row
            })
            .collect()
    }

    #[test]
    fn test_confident_with_slot_keeps_case() {
        let p = decode(
            toks("show me the files in Pictures"),
            &[0.9, 0.05, 0.05],
            &tag_rows(&[(0, 1.0), (0, 1.0), (0, 1.0), (0, 1.0), (0, 1.0), (1, 0.95)]),
            &labels(),
            &templates(),
            0.7,
        );
        assert_eq!(p.intent, "list_files");
        assert_eq!(p.slots["dir"], "Pictures");
        assert_eq!(p.command.as_deref(), Some("ls -la Pictures"));
        assert_eq!(p.reason, None);
        assert_eq!(p.tags[5], "B-dir");
    }

    #[test]
    fn test_highest_probability_token_wins() {
        let p = decode(
            toks("cat a.txt b.txt"),
            &[0.0, 1.0, 0.0],
            &tag_rows(&[(0, 1.0), (2, 0.6), (2, 0.9)]),
            &labels(),
            &templates(),
            0.7,
        );
        assert_eq!(p.command.as_deref(), Some("cat b.txt"));
    }

    #[test]
    fn test_ignores_tags_for_other_slots() {
        // A B-file tag on a list_files request is not a dir.
        let p = decode(
            toks("list notes.txt"),
            &[1.0, 0.0, 0.0],
            &tag_rows(&[(0, 1.0), (2, 0.9)]),
            &labels(),
            &templates(),
            0.7,
        );
        assert!(p.slots.is_empty());
        assert_eq!(p.command.as_deref(), Some("ls -la"));
    }

    #[test]
    fn test_missing_required_slot() {
        let p = decode(
            toks("show the file"),
            &[0.0, 1.0, 0.0],
            &tag_rows(&[(0, 1.0), (0, 1.0), (0, 1.0)]),
            &labels(),
            &templates(),
            0.7,
        );
        assert_eq!(p.command, None);
        assert!(p.reason.unwrap().contains("missing a value for file"));
    }

    #[test]
    fn test_below_threshold_lists_guesses() {
        let p = decode(
            toks("files"),
            &[0.5, 0.3, 0.2],
            &tag_rows(&[(0, 1.0)]),
            &labels(),
            &templates(),
            0.7,
        );
        assert_eq!(p.command, None);
        let reason = p.reason.unwrap();
        assert!(reason.starts_with("not sure"), "{reason}");
        assert!(reason.contains("list_files 0.50") && reason.contains("unknown 0.20"));
        assert_eq!(p.top.len(), 3);
    }

    #[test]
    fn test_unknown_intent() {
        let p = decode(
            toks("delete everything"),
            &[0.0, 0.0, 1.0],
            &tag_rows(&[(0, 1.0), (0, 1.0)]),
            &labels(),
            &templates(),
            0.7,
        );
        assert_eq!(p.intent, "unknown");
        assert_eq!(p.command, None);
        assert!(p.reason.unwrap().contains("not supported"));
    }
}
