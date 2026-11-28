// Stage 4: embed the int8 model in the `cyore` binary when one has been trained.
//
// `include_bytes!` refuses to compile if its file is missing, so this script always writes the
// two files into OUT_DIR: copies of `model-q8.bpk` and `meta.json` from the model directory if
// both exist, or empty placeholders otherwise. `src/embedded.rs` treats empty as "no embedded
// model", and `cyore` then loads from disk. A fresh clone therefore builds without a model.
//
// Model directory: $CYORE_MODEL_DIR, default <workspace>/data/models/intent.

use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    let out = PathBuf::from(env::var("OUT_DIR").expect("cargo sets OUT_DIR"));
    let manifest =
        PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR"));
    let dir = env::var("CYORE_MODEL_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| manifest.join("../data/models/intent"));
    let weights = dir.join("model-q8.bpk");
    let meta = dir.join("meta.json");

    println!("cargo:rerun-if-env-changed=CYORE_MODEL_DIR");
    println!("cargo:rerun-if-changed={}", weights.display());
    println!("cargo:rerun-if-changed={}", meta.display());

    let (weight_bytes, meta_bytes) = if weights.is_file() && meta.is_file() {
        (
            fs::read(&weights).expect("read model-q8.bpk"),
            fs::read(&meta).expect("read meta.json"),
        )
    } else {
        (Vec::new(), Vec::new())
    };
    fs::write(out.join("model-q8.bpk"), weight_bytes).expect("write embedded weights");
    fs::write(out.join("meta.json"), meta_bytes).expect("write embedded meta");
}
