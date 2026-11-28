// Saving and loading a trained model: `model.bpk` (weights, Burn's burnpack format) and
// `meta.json` (everything else needed to use them).

use std::path::Path;

use burn::prelude::*;
use burn::store::{BurnpackStore, ModuleSnapshot};
use serde::{Deserialize, Serialize};

use crate::data::IntentInfo;
use crate::model::{JointModel, ModelConfig};
use crate::vocab::{Labels, Vocab};

pub const MODEL_FILE: &str = "model.bpk";
pub const META_FILE: &str = "meta.json";

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct TrainingSummary {
    pub seed: u64,
    pub best_epoch: usize,
    pub validation_intent_accuracy: f64,
    pub baseline_validation_intent_accuracy: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Meta {
    pub config: ModelConfig,
    pub vocab: Vocab,
    pub labels: Labels,
    /// Command templates, by intent name.
    pub intents: Vec<IntentInfo>,
    pub training: TrainingSummary,
}

pub fn save(dir: &Path, model: &JointModel, meta: &Meta) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("could not create {}: {e}", dir.display()))?;
    // Burnpack builds the file under a scratch name and renames it into place, so a failed save
    // never leaves a half-written model behind.
    let mut store = BurnpackStore::from_file(dir.join(MODEL_FILE)).overwrite(true);
    model
        .save_into(&mut store)
        .map_err(|e| format!("could not save the model: {e:?}"))?;

    let json = serde_json::to_string_pretty(meta).map_err(|e| e.to_string())?;
    let tmp = dir.join(format!("{META_FILE}.tmp"));
    std::fs::write(&tmp, json + "\n")
        .and_then(|_| std::fs::rename(&tmp, dir.join(META_FILE)))
        .map_err(|e| format!("could not write {}: {e}", dir.join(META_FILE).display()))
}

pub fn load(dir: &Path, device: &Device) -> Result<(JointModel, Meta), String> {
    let missing = |name: &str| {
        format!(
            "{}: not found. Train a model first with `make intent-train`.",
            dir.join(name).display()
        )
    };
    let meta_path = dir.join(META_FILE);
    let json = std::fs::read_to_string(&meta_path).map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => missing(META_FILE),
        _ => format!("{}: {e}", meta_path.display()),
    })?;
    let meta: Meta =
        serde_json::from_str(&json).map_err(|e| format!("{}: {e}", meta_path.display()))?;
    let model_path = dir.join(MODEL_FILE);
    if !model_path.exists() {
        return Err(missing(MODEL_FILE));
    }
    let mut model = JointModel::new(&meta.config, device);
    let mut store = BurnpackStore::from_file(&model_path);
    model
        .load_from(&mut store)
        .map_err(|e| format!("{}: could not load weights: {e:?}", model_path.display()))?;
    Ok((model, meta))
}
