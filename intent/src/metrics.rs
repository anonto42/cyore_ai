// How good is the model? One function computes every number, so validation, the training
// report and `intent eval` always agree (design.md, decision 10).

use std::collections::BTreeMap;
use std::fmt;

use crate::data::Example;
use crate::predict::{Prediction, UNKNOWN};

pub const THRESHOLDS: [f32; 3] = [0.5, 0.7, 0.9];

#[derive(Debug, Clone, PartialEq)]
pub struct ThresholdRow {
    pub threshold: f32,
    /// Fraction of requests the model would answer (confidence ≥ threshold).
    pub coverage: f64,
    /// Intent accuracy on those requests (None if none are answered).
    pub accuracy: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Report {
    pub n: usize,
    pub intent_accuracy: f64,
    pub slot_accuracy: f64,
    pub command_accuracy: f64,
    pub unknown_precision: Option<f64>,
    pub unknown_recall: Option<f64>,
    /// (expected, predicted, count), most frequent first, at most 5.
    pub confusions: Vec<(String, String, usize)>,
    pub thresholds: Vec<ThresholdRow>,
}

fn ratio(a: usize, b: usize) -> Option<f64> {
    (b > 0).then(|| a as f64 / b as f64)
}

/// `predictions[i]` is the prediction for `examples[i]`, made with threshold 0 so every
/// confident-enough command is filled in (the threshold table is computed here instead).
pub fn evaluate(examples: &[Example], predictions: &[Prediction]) -> Report {
    assert_eq!(
        examples.len(),
        predictions.len(),
        "one prediction per example"
    );
    let n = examples.len();
    let mut intent_right = 0;
    let mut command_right = 0;
    let (mut tag_right, mut tag_total) = (0, 0);
    let (mut unknown_true_pos, mut predicted_unknown, mut actual_unknown) = (0, 0, 0);
    let mut confused: BTreeMap<(String, String), usize> = BTreeMap::new();

    for (e, p) in examples.iter().zip(predictions) {
        if p.intent == e.intent {
            intent_right += 1;
        } else {
            *confused
                .entry((e.intent.clone(), p.intent.clone()))
                .or_insert(0) += 1;
        }
        if p.command == e.command {
            command_right += 1;
        }
        for (i, gold) in e.tags.iter().enumerate() {
            tag_total += 1;
            if p.tags.get(i) == Some(gold) {
                tag_right += 1;
            }
        }
        let (is, said) = (e.intent == UNKNOWN, p.intent == UNKNOWN);
        actual_unknown += is as usize;
        predicted_unknown += said as usize;
        unknown_true_pos += (is && said) as usize;
    }

    let mut confusions: Vec<(String, String, usize)> =
        confused.into_iter().map(|((a, b), c)| (a, b, c)).collect();
    confusions.sort_by(|a, b| {
        b.2.cmp(&a.2)
            .then_with(|| a.0.cmp(&b.0))
            .then_with(|| a.1.cmp(&b.1))
    });
    confusions.truncate(5);

    let thresholds = THRESHOLDS
        .iter()
        .map(|&threshold| {
            let answered: Vec<_> = examples
                .iter()
                .zip(predictions)
                .filter(|(_, p)| p.confidence >= threshold)
                .collect();
            let right = answered
                .iter()
                .filter(|(e, p)| p.intent == e.intent)
                .count();
            ThresholdRow {
                threshold,
                coverage: ratio(answered.len(), n).unwrap_or(0.0),
                accuracy: ratio(right, answered.len()),
            }
        })
        .collect();

    Report {
        n,
        intent_accuracy: ratio(intent_right, n).unwrap_or(0.0),
        slot_accuracy: ratio(tag_right, tag_total).unwrap_or(0.0),
        command_accuracy: ratio(command_right, n).unwrap_or(0.0),
        unknown_precision: ratio(unknown_true_pos, predicted_unknown),
        unknown_recall: ratio(unknown_true_pos, actual_unknown),
        confusions,
        thresholds,
    }
}

fn pct(x: Option<f64>) -> String {
    x.map_or("n/a".to_string(), |v| format!("{:.1}%", 100.0 * v))
}

impl fmt::Display for Report {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "  examples            {}", self.n)?;
        writeln!(
            f,
            "  intent accuracy     {}",
            pct(Some(self.intent_accuracy))
        )?;
        writeln!(f, "  slot tag accuracy   {}", pct(Some(self.slot_accuracy)))?;
        writeln!(
            f,
            "  command accuracy    {}",
            pct(Some(self.command_accuracy))
        )?;
        writeln!(
            f,
            "  unknown precision   {}   recall {}",
            pct(self.unknown_precision),
            pct(self.unknown_recall)
        )?;
        writeln!(
            f,
            "  confidence threshold → coverage / accuracy of answered:"
        )?;
        for row in &self.thresholds {
            writeln!(
                f,
                "    ≥ {:.1}   {:>6}  /  {}",
                row.threshold,
                pct(Some(row.coverage)),
                pct(row.accuracy)
            )?;
        }
        if !self.confusions.is_empty() {
            writeln!(f, "  most confused (expected → predicted):")?;
            for (expected, predicted, count) in &self.confusions {
                writeln!(f, "    {count} × {expected} → {predicted}")?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::tests::example;

    fn prediction(
        intent: &str,
        confidence: f32,
        tags: &[&str],
        command: Option<&str>,
    ) -> Prediction {
        Prediction {
            tokens: Vec::new(),
            tags: tags.iter().map(|t| t.to_string()).collect(),
            intent: intent.to_string(),
            confidence,
            top: Vec::new(),
            slots: BTreeMap::new(),
            command: command.map(String::from),
            reason: None,
        }
    }

    #[test]
    fn test_report_numbers() {
        let mut a = example("show src", "list_files", Some(("dir", "src")));
        a.command = Some("ls -la src".into());
        let mut b = example("show files", "list_files", None);
        b.command = Some("ls -la".into());
        let c = example("hello there", "unknown", None);
        let d = example("hi", "unknown", None);
        let examples = [a, b, c, d];
        let predictions = [
            prediction("list_files", 0.95, &["O", "B-dir"], Some("ls -la src")), // all right
            prediction("disk_space", 0.6, &["O", "O"], Some("df -h")),           // wrong intent
            prediction("unknown", 0.8, &["O", "O"], None),                       // right
            prediction("list_files", 0.4, &["O"], Some("ls -la")),               // missed unknown
        ];
        let r = evaluate(&examples, &predictions);
        assert_eq!(r.n, 4);
        assert!((r.intent_accuracy - 0.5).abs() < 1e-9);
        assert!((r.command_accuracy - 0.5).abs() < 1e-9);
        assert!((r.slot_accuracy - 1.0).abs() < 1e-9); // all 7 tags match
        assert_eq!(r.unknown_precision, Some(1.0));
        assert_eq!(r.unknown_recall, Some(0.5));
        assert_eq!(r.confusions.len(), 2);
        // ≥ 0.5: three answered (0.95, 0.6, 0.8), two right.
        assert!((r.thresholds[0].coverage - 0.75).abs() < 1e-9);
        assert!((r.thresholds[0].accuracy.unwrap() - 2.0 / 3.0).abs() < 1e-9);
        // ≥ 0.9: one answered, right.
        assert_eq!(r.thresholds[2].accuracy, Some(1.0));
        let text = r.to_string();
        assert!(text.contains("intent accuracy     50.0%"), "{text}");
    }

    /// The human test set is report-only: a report must never contain the sentences it scored.
    #[test]
    fn test_report_never_contains_sentences() {
        let examples = [
            example("zebra quantum folder please", "list_files", None),
            example("flamingo disk check", "unknown", None),
        ];
        let predictions = [
            prediction("list_files", 0.9, &["O"; 4], None),
            prediction("disk_space", 0.6, &["O"; 3], None),
        ];
        let text = evaluate(&examples, &predictions).to_string();
        for word in ["zebra", "quantum", "flamingo"] {
            assert!(!text.contains(word), "report leaked \"{word}\":\n{text}");
        }
    }

    #[test]
    fn test_no_unknown_examples() {
        let r = evaluate(
            &[example("a", "x", None)],
            &[prediction("x", 1.0, &["O"], None)],
        );
        assert_eq!(r.unknown_recall, None);
        assert_eq!(r.unknown_precision, None);
    }
}
