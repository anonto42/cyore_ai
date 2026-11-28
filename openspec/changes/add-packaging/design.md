# Design

## Context

- `intent` saves `model.bpk` (f32, 356 KB) and `meta.json` (vocabulary, labels, templates, sizes; about 20 KB) in `data/models/intent/`, which is git-ignored and produced by `make commands-data && make intent-train` (deterministic, seed 42, about 20 s).
- Burn 0.22.1 offers `Module::quantize_weights(&mut Quantizer)` with `QuantValue::Q8S` (8-bit symmetric) on `flex`, and `BurnpackStore::from_static(&'static [u8])` for zero-copy loading. A prototype quantized the trained model to 97 KB with identical development and test metrics, and loaded the int8 file from static bytes into a freshly constructed model.
- `cyore` is 4.1 MiB, peaks at 10.7 MiB RSS, and already uses the size-oriented release profile (`lto`, `codegen-units = 1`, `strip`, `panic = "abort"`).
- The release workflow builds on native runners for four targets but ships only `xor` and `mnist`.

## Goals / Non-Goals

**Goals:**
- `cyore` becomes one file you can copy to another machine.
- The quantisation step is visible and measurable as its own command, not hidden inside training, because measuring the effect of int8 is the lesson.
- Fresh clones and CI without a model keep building and testing.

**Non-Goals:**
- Squeezing the binary further than the measured `opt-level` comparison; it is already 7× under budget.

## The Math (int8, symmetric, per tensor)

For a weight tensor W, let m = max |wᵢ| (min/max calibration). The scale is s = m / 127, and each weight is stored as qᵢ = round(wᵢ / s), clamped to [−127, 127], in one byte. One f32 scale is kept per tensor. Reading back gives ŵᵢ = s · qᵢ, so the error per weight is at most s/2 = m/254, under 0.4% of the largest weight. Storage drops from 4 bytes to 1 byte per weight, plus one scale per tensor: 356 KB → 97 KB, close to the 4× the plan predicts. Accuracy is unchanged because the decisions (argmax over intents and tags) have margins far larger than this noise.

## Decisions

1. **Quantize after training, as a separate command** (`intent quantize [--model DIR] [--data DIR]`). It loads `model.bpk`, applies `Quantizer::new(Calibration::MinMax, scheme.with_value(QuantValue::Q8S))`, saves `model-q8.bpk`, then evaluates both models with the existing `metrics::evaluate`. Test numbers are printed as a report; nothing is tuned on them.
   *Alternative:* quantize inside `intent train`. Rejected: it would hide the step being taught, and the f32 model is still needed for the comparison.

2. **`Predictor::from_static(weights: &'static [u8], meta_json: &str)`** builds a `JointModel` from `meta.config`, loads with `BurnpackStore::from_static`, and parses the meta. `Predictor::load(dir)` stays as it is. `Predictor::load_q8(dir)` loads `model-q8.bpk` so `intent eval` and `predict` can check the int8 model too (`--q8`).

3. **Embedding through a build script, not a Cargo feature.** `agent/build.rs` looks at `CYORE_MODEL_DIR` (default `<workspace>/data/models/intent`). If both `model-q8.bpk` and `meta.json` exist there, it copies them into `OUT_DIR`. Otherwise it writes empty files. It emits `rerun-if-changed` for both files and `rerun-if-env-changed=CYORE_MODEL_DIR`. `src/embedded.rs` does `include_bytes!(concat!(env!("OUT_DIR"), "/model-q8.bpk"))` and `include_str!` for the meta, and exposes `fn embedded() -> Option<(&'static [u8], &'static str)>`, which returns None when they are empty.
   - Why a build script: `include_bytes!` fails to compile when its file is missing. Copying into `OUT_DIR`, with empty placeholders, keeps a fresh clone building with no special flags. A Cargo feature would need the model whenever the feature is on, and `--all-features` builds would break on clean checkouts.
   - Run-time order in `cyore`: `--model DIR` wins; otherwise the embedded model if present; otherwise `data/models/intent` from disk.
   - Visibility: the interactive banner and `--help` say which model is in use ("embedded int8 model, 97 KB" or the directory).

4. **`make release`** = `commands-data` → `intent-train` → `intent-quantize` → `cargo build --release -p agent`, then prints the binary's size. **`make cross-arm64`** = `cargo build --release -p agent --target aarch64-unknown-linux-musl`, with `CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_LINKER=rust-lld` so no cross C toolchain is needed. The musl target ships its own C runtime objects and Rust's own linker can produce a static ELF, which `file` should confirm. If it is not possible, the target says so and exits non-zero; the CI ARM64 runner remains the guaranteed path.

5. **Release workflow:** add a "Train and quantize the model" step before the build: `make commands-data`, `cargo run --release -p intent -- train`, `cargo run --release -p intent -- quantize`. Replace the plain `BINS` list with package-to-binary pairs (`xor:xor mnist:mnist agent:cyore`), because the `agent` package builds the `cyore` binary. Archives then contain `cyore` with the model inside. Training runs per job: a few seconds, and it avoids passing artefacts between jobs. Each platform gets the same seeded data and training; tiny floating-point differences across CPUs are acceptable, and each job reports its own numbers in its log.

6. **`opt-level` comparison:** build `cyore` with `CARGO_PROFILE_RELEASE_OPT_LEVEL=z` and compare size, one-shot latency and test metrics with `opt-level = 3`. Keep 3 unless "z" saves at least 25% of the size with no visible slowdown. Record the numbers either way.

## Risks / Trade-offs

- [Burn's quantized kernels on `flex` may change between versions] → Burn is pinned to `=0.22.1`, and a unit test quantizes a small model and checks the predictions match f32.
- [A stale embedded model: the binary contains whatever was trained when it was built] → `rerun-if-changed` triggers a rebuild when the files change, and `cyore` reports which model it uses. Release binaries always train from the seeded dataset.
- [rust-lld cross-linking may not work for every target] → used only for the optional local ARM64 build; CI uses native runners.
- [Windows builds cannot run the Unix commands] → documented. The safety layer still applies, and failed starts are reported, not crashes.

## Migration Plan

Additive. Without a trained model nothing changes at build time. Rollback: delete `agent/build.rs` and `embedded.rs`, and revert the workflow step.
