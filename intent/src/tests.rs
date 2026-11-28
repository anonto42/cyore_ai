// End-to-end tests on a tiny synthetic dataset (no real dataset or saved model needed):
// the overfit test proves forward, loss, backward and the optimizer are wired correctly
// (design.md, decision 12), and the round trip proves saving and loading lose nothing.

use std::collections::BTreeSet;

use crate::batch::encode;
use crate::data::tests::example;
use crate::data::{Example, IntentInfo};
use crate::model::ModelConfig;
use crate::predict::Predictor;
use crate::store::{self, Meta, TrainingSummary};
use crate::train::{TrainConfig, train_joint};
use crate::vocab::{Labels, Vocab};

fn tiny_dataset() -> Vec<Example> {
    vec![
        example("show files in src", "list_files", Some(("dir", "src"))),
        example("list files in docs", "list_files", Some(("dir", "docs"))),
        example("show the files", "list_files", None),
        example("show notes.txt", "show_file", Some(("file", "notes.txt"))),
        example("print main.rs", "show_file", Some(("file", "main.rs"))),
        example(
            "read the file todo.md",
            "show_file",
            Some(("file", "todo.md")),
        ),
        example("hello there", "unknown", None),
        example("write a poem", "unknown", None),
    ]
}

fn intents() -> Vec<IntentInfo> {
    [
        ("list_files", Some("ls -la {dir?}")),
        ("show_file", Some("cat {file}")),
        ("unknown", None),
    ]
    .into_iter()
    .map(|(name, command)| IntentInfo {
        name: name.to_string(),
        command: command.map(String::from),
    })
    .collect()
}

/// Train on the tiny set until it is memorised; returns a ready Predictor.
fn trained_predictor() -> Predictor {
    let data = tiny_dataset();
    let vocab = Vocab::build(&data, 1);
    let labels = Labels::new(
        intents().into_iter().map(|i| i.name).collect(),
        ["dir".to_string(), "file".to_string()],
    );
    let encoded = encode(&data, &vocab, &labels).unwrap();
    let config = ModelConfig {
        vocab_size: vocab.len(),
        n_intents: labels.intents.len(),
        n_tags: labels.tags.len(),
        d_embed: 16,
        d_hidden: 16,
    };
    let tc = TrainConfig {
        epochs: 80,
        batch_size: 8,
        learning_rate: 1e-2,
        word_dropout: 0.0,
        patience: 80,
        seed: 1,
    };
    let trained = train_joint(&config, &encoded, &encoded, &encoded, &tc, |_| {});
    let meta = Meta {
        config,
        vocab,
        labels,
        intents: intents(),
        training: TrainingSummary::default(),
    };
    Predictor::new(trained.model, meta).unwrap()
}

#[test]
fn test_overfits_tiny_dataset_and_round_trips() {
    let predictor = trained_predictor();
    for e in tiny_dataset() {
        let p = predictor.predict(&e.text, 0.0);
        assert_eq!(p.intent, e.intent, "{}", e.text);
        assert_eq!(p.slots, e.slots, "{}", e.text);
    }
    let p = predictor.predict("show files in src", 0.0);
    assert_eq!(p.command.as_deref(), Some("ls -la src"));

    // Save, load, and compare predictions.
    let dir = std::env::temp_dir().join(format!("cyore-intent-test-{}", std::process::id()));
    store::save(&dir, &predictor_model(&predictor), predictor.meta()).unwrap();
    let loaded = Predictor::load(&dir).unwrap();
    for text in [
        "show files in src",
        "print main.rs",
        "hello there",
        "something new.txt",
    ] {
        let (a, b) = (predictor.predict(text, 0.0), loaded.predict(text, 0.0));
        assert_eq!(a.intent, b.intent, "{text}");
        assert_eq!(a.slots, b.slots, "{text}");
        assert!((a.confidence - b.confidence).abs() < 1e-6, "{text}");
    }
    assert_eq!(loaded.meta(), predictor.meta());
    std::fs::remove_dir_all(&dir).ok();
}

fn predictor_model(p: &Predictor) -> crate::model::JointModel {
    p.model().clone()
}

#[test]
fn test_robust_inputs() {
    let predictor = trained_predictor();
    let empty = predictor.predict("", 0.7);
    assert_eq!(empty.command, None);
    assert_eq!(empty.reason.as_deref(), Some("empty request"));
    assert_eq!(
        predictor.predict("?! ,", 0.7).reason.as_deref(),
        Some("empty request")
    );
    let long = "show ".repeat(200);
    let p = predictor.predict(&long, 0.7);
    assert_eq!(p.tokens.len(), crate::predict::MAX_TOKENS);
    // Never-seen words of every shape still work.
    let p = predictor.predict("zzz ~/x *.q 1.2.3.4 a.b", 0.0);
    let labels: BTreeSet<&str> = ["list_files", "show_file", "unknown"].into();
    assert!(labels.contains(p.intent.as_str()));
}

#[test]
fn test_missing_model_hint() {
    let err = Predictor::load(std::path::Path::new("/nonexistent/cyore-model"))
        .err()
        .unwrap();
    assert!(err.contains("make intent-train"), "{err}");
}

#[test]
fn test_int8_model_matches_and_loads_from_static_bytes() {
    let predictor = trained_predictor();
    let device = burn::prelude::Device::flex();
    let q8 = store::quantize(predictor.model(), &device);
    let dir = std::env::temp_dir().join(format!("cyore-intent-q8-{}", std::process::id()));
    store::save(&dir, predictor.model(), predictor.meta()).unwrap();
    store::save_q8(&dir, &q8).unwrap();

    // Smaller than f32. (Only modestly here: this tiny model is mostly file metadata. The real
    // model shrinks to about 27%, checked by `intent quantize`.)
    let f32_size = std::fs::metadata(dir.join(store::MODEL_FILE))
        .unwrap()
        .len();
    let q8_size = std::fs::metadata(dir.join(store::MODEL_Q8_FILE))
        .unwrap()
        .len();
    assert!(
        q8_size < f32_size,
        "int8 {q8_size} bytes vs f32 {f32_size} bytes"
    );

    // Loaded from disk and from static bytes, it predicts like the f32 model.
    let from_disk = Predictor::load_q8(&dir).unwrap();
    let bytes: &'static [u8] = Box::leak(
        std::fs::read(dir.join(store::MODEL_Q8_FILE))
            .unwrap()
            .into_boxed_slice(),
    );
    let meta_json = std::fs::read_to_string(dir.join(store::META_FILE)).unwrap();
    let from_bytes = Predictor::from_static(bytes, &meta_json).unwrap();
    for e in tiny_dataset() {
        let f32_prediction = predictor.predict(&e.text, 0.0);
        for q in [&from_disk, &from_bytes] {
            let p = q.predict(&e.text, 0.0);
            assert_eq!(p.intent, f32_prediction.intent, "{}", e.text);
            assert_eq!(p.slots, f32_prediction.slots, "{}", e.text);
        }
    }
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn test_missing_q8_hint() {
    let err = Predictor::load_q8(std::path::Path::new("/nonexistent/cyore-model"))
        .err()
        .unwrap();
    assert!(err.contains("make intent-train"), "{err}");
}
