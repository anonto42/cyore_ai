// Saving and loading a trained model: `model.bpk` (f32 weights, Burn's burnpack format),
// `model-q8.bpk` (the same weights quantized to int8, Stage 4), and `meta.json` (everything
// else needed to use them). Weights can also be loaded from bytes compiled into a binary.

use std::path::Path;

use burn::module::{ParamGroup, Quantizer};
use burn::prelude::*;
use burn::store::{BurnpackStore, ModuleSnapshot};
use burn::tensor::quantization::{Calibration, QuantValue, ScaleDtype};
use serde::{Deserialize, Serialize};

use crate::data::IntentInfo;
use crate::model::{JointModel, ModelConfig};
use crate::vocab::{Labels, Vocab};

pub const MODEL_FILE: &str = "model.bpk";
pub const MODEL_Q8_FILE: &str = "model-q8.bpk";
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

/// Quantize every weight to int8, symmetric, with **one scale per row**: each row r of a weight
/// matrix gets scale s_r = max|w_r| / 127 and stores q = round(w / s_r) in one byte.
///
/// Why per row and not one scale per tensor: a single scale is set by the tensor's largest
/// weight, so rows of small weights are rounded coarsely. With the improve-phrasing-coverage
/// model, per-tensor int8 fell 1.2 points below f32 on the test set (the spec allows 1); per row
/// it matches f32 there, and the file is 29% of f32 (the spec allows 30%). Finer blocks did not
/// fit that size budget.
///
/// Row lengths are the last dimension of each weight: embedding and GRU rows have d_embed /
/// d_hidden values, the slot head rows n_tags, the intent head rows n_intents. A row longer than
/// 255 values (Burn's block-size limit) falls back to one scale per tensor.
pub fn quantize(model: &JointModel, config: &ModelConfig, device: &Device) -> JointModel {
    let int8 = device
        .settings()
        .quantization
        .scheme
        .with_value(QuantValue::Q8S);
    let per_row = |row: usize| match u8::try_from(row) {
        Ok(row) => int8.per_block([row], ScaleDtype::F32),
        Err(_) => int8,
    };
    let parts = [
        ("embed", per_row(config.d_embed)),
        ("encoder", per_row(config.d_hidden)),
        ("slot_head", per_row(config.n_tags)),
        ("intent_head", per_row(config.n_intents)),
    ];
    let mut quantized = model.clone();
    for (part, scheme) in parts {
        let mut quantizer = Quantizer::new(Calibration::MinMax, scheme);
        quantized =
            quantized.quantize_weights_group(&mut quantizer, ParamGroup::from_predicate(part));
    }
    quantized
}

/// Save an already-quantized model next to the f32 one as `model-q8.bpk`.
pub fn save_q8(dir: &Path, model: &JointModel) -> Result<(), String> {
    let mut store = BurnpackStore::from_file(dir.join(MODEL_Q8_FILE)).overwrite(true);
    model
        .save_into(&mut store)
        .map_err(|e| format!("could not save the int8 model: {e:?}"))
}

fn missing(dir: &Path, name: &str) -> String {
    format!(
        "{}: not found. Train a model first with `make intent-train`.",
        dir.join(name).display()
    )
}

pub fn parse_meta(json: &str, origin: &str) -> Result<Meta, String> {
    serde_json::from_str(json).map_err(|e| format!("{origin}: {e}"))
}

fn read_meta(dir: &Path) -> Result<Meta, String> {
    let meta_path = dir.join(META_FILE);
    let json = std::fs::read_to_string(&meta_path).map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => missing(dir, META_FILE),
        _ => format!("{}: {e}", meta_path.display()),
    })?;
    parse_meta(&json, &meta_path.display().to_string())
}

fn load_weights(
    model: &mut JointModel,
    mut store: BurnpackStore,
    origin: &str,
) -> Result<(), String> {
    model
        .load_from(&mut store)
        .map(|_| ())
        .map_err(|e| format!("{origin}: could not load weights: {e:?}"))
}

/// Load `meta.json` and the weights file `file` (MODEL_FILE or MODEL_Q8_FILE) from `dir`.
pub fn load_file(dir: &Path, file: &str, device: &Device) -> Result<(JointModel, Meta), String> {
    let meta = read_meta(dir)?;
    let model_path = dir.join(file);
    if !model_path.exists() {
        return Err(if file == MODEL_Q8_FILE {
            format!(
                "{}: not found. Quantize the model first with `make intent-quantize`.",
                model_path.display()
            )
        } else {
            missing(dir, file)
        });
    }
    let mut model = JointModel::new(&meta.config, device);
    let origin = model_path.display().to_string();
    load_weights(&mut model, BurnpackStore::from_file(&model_path), &origin)?;
    Ok((model, meta))
}

/// Load the f32 model.
pub fn load(dir: &Path, device: &Device) -> Result<(JointModel, Meta), String> {
    load_file(dir, MODEL_FILE, device)
}

/// Load weights and meta that live in memory for the whole program, e.g. compiled into the
/// binary with `include_bytes!`. Zero-copy: the weights are read straight from the binary.
pub fn load_static(
    weights: &'static [u8],
    meta_json: &str,
    device: &Device,
) -> Result<(JointModel, Meta), String> {
    let meta = parse_meta(meta_json, "embedded meta.json")?;
    let mut model = JointModel::new(&meta.config, device);
    load_weights(
        &mut model,
        BurnpackStore::from_static(weights),
        "embedded model",
    )?;
    Ok((model, meta))
}
