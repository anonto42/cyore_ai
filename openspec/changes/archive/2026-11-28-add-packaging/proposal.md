# Proposal

## Why

Stage 4 of the learning plan (README.md) packages Cyore for minimal PCs: quantise the weights to int8, use Burn's CPU backend, embed the weights in the binary with `include_bytes!`, keep the release settings small, and cross-compile for Linux, Windows and ARM. Today `cyore` (4.1 MiB, 10.7 MiB peak RAM) already meets the size and memory targets, but it is not one file: it needs `data/models/intent/` on disk at run time, which the release workflow neither builds nor ships. A prototype showed that Burn 0.22 can quantize this model to int8 (356 KB → 97 KB) with **no change** in development or test accuracy, and can load weights from a `&'static [u8]`. So the remaining step is packaging, not research.

## What Changes

- **int8 quantisation** in `intent`: a new `intent quantize` command reads the trained f32 model and writes `model-q8.bpk` (8-bit symmetric, per-tensor, min/max calibration). It prints both file sizes and the f32 and int8 accuracy on the development set and, as a report only, the test set. A `make intent-quantize` target runs it.
- **Embedded model** in `agent`: a build script copies `model-q8.bpk` and `meta.json` into the build when they exist (from `CYORE_MODEL_DIR`, default `data/models/intent`), and `cyore` loads them with `include_bytes!` and `BurnpackStore::from_static`. The binary then runs anywhere with no model files. `--model DIR` still loads from disk. Without a trained model the build still succeeds and `cyore` falls back to loading from disk, so `make check` keeps working on a fresh clone.
- `intent::predict::Predictor::from_static(weights, meta_json)` for loading from memory.
- **Release builds**: the release workflow generates the dataset, trains and quantizes the model, then builds and ships `cyore` with the embedded model for every target it already builds (Linux x86_64 and ARM64 static musl, Windows x86_64, macOS ARM64). `make release` does the same locally (dataset → train → quantize → release build), and `make cross-arm64` cross-compiles the static Linux ARM64 binary when the target is installed.
- **Measurements** recorded in `project-status.md`: binary size (with the model embedded), peak RAM, start-up time, and size with `opt-level = "z"` versus `3`, keeping whichever is better overall.
- Docs: README (Stage 4 status, the single-file binary), `agent/README.md`, `intent/README.md`, `docs/ARCHITECTURE.md`, `docs/RELEASES.md`, `AGENTS.md`, `.agents/shared/agent-startup.md`.

### Non-goals

- No quantisation-aware training, int4, or calibration on data. Per-tensor min/max int8 already matches f32 accuracy.
- No change to the model architecture, dataset, safety rules, or agent behaviour.
- No container image for `cyore`. It runs commands like `ls` and `df` on the host, and the `FROM scratch` image has none of them, so the existing Dockerfile stays for the Stage 1 crates.
- No Windows command mapping. The Windows build compiles and runs, but the 37 allowlisted programs are Unix tools, so on Windows most commands report "could not start". Documented as a limit.
- Raspberry Pi is covered by the Linux ARM64 build, verified by cross-compilation and the binary format, not on hardware.

### Invariants

- Learning-plan stage: Stage 4.
- int8 accuracy on the development and test sets stays within 1 percentage point of f32 (intent and command).
- `make check` passes on a fresh clone with no dataset or model; the embedded model is optional at build time.
- An embedded model is only ever the int8 copy of a model produced by `intent train` and `intent quantize` from the seeded dataset, so release binaries are reproducible from source.

## Capabilities

### New Capabilities
- `model-packaging`: producing the int8 model, embedding it in `cyore`, and building self-contained release binaries for the supported targets within the size and memory budget.

### Modified Capabilities

None. The model and agent behaviour is unchanged. The `command-model` and `command-agent` specs belong to changes that are not archived yet, so the new behaviour is specified here.

## Impact

- Changed: `intent/` (quantize command, `Predictor::from_static`), `agent/` (build script, embedded loading), `Makefile`, `.github/workflows/release.yml`, docs listed above.
- New dependencies: none. Quantisation and static loading come from Burn features already in use (`store`, `flex`).
- Release workflow: about 30 s more per job (dataset generation, training, quantisation).
