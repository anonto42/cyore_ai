# model-packaging Specification

## Purpose
Turns the trained Stage 2 model and the Stage 3 agent into one small, self-contained `cyore` program for minimal PCs: int8 weights embedded in the binary, built for Linux (x86_64 and ARM64), Windows and macOS, within the 30 MB and 100 MB budgets. This is the Stage 4 milestone.

## Requirements

### Requirement: int8 model
The quantize command SHALL read a trained model and write an int8 copy of its weights (`model-q8.bpk`) next to it, at most 30% of the size of the f32 weights. It SHALL report both sizes and the intent and command accuracy of both models on the development set, and on the test set as a report only. The int8 model's accuracy SHALL be within 1 percentage point of the f32 model's on both sets.

#### Scenario: Quantize
- **WHEN** `make intent-quantize` runs after `make intent-train`
- **THEN** `model-q8.bpk` exists, is at most 30% of `model.bpk`'s size, and the printed int8 accuracies are within 1 point of the f32 ones

#### Scenario: No trained model
- **WHEN** the quantize command runs and the model directory has no trained model
- **THEN** it exits with a non-zero status and a message naming `make intent-train`

#### Scenario: Deterministic
- **WHEN** the same trained model is quantized twice
- **THEN** the two `model-q8.bpk` files are byte-identical

### Requirement: Embedded model
When `model-q8.bpk` and `meta.json` exist in the model directory at build time, the `cyore` binary SHALL contain them and SHALL run with no model files on disk. `--model DIR` SHALL load from that directory instead. When no model exists at build time, the build SHALL still succeed and `cyore` SHALL load from `data/models/intent` at run time, failing with the `make intent-train` hint if it is missing there too.

#### Scenario: One file
- **WHEN** `cyore` is built with a quantized model present, copied alone to an empty directory, and run there with a request
- **THEN** it proposes the same command as the f32 model does for that request

#### Scenario: Built without a model
- **WHEN** the workspace is built on a fresh clone with no dataset or model
- **THEN** the build and tests succeed

### Requirement: Self-contained release binary
The release `cyore` binary with the embedded model SHALL be at most 30 MB, use less than 100 MB of RAM at peak for a request, and start (load the model and answer a one-shot dry run) in under one second on the development machine. The static Linux musl builds SHALL have no dynamic library dependencies.

#### Scenario: Measure
- **WHEN** the release binary with the embedded model is measured
- **THEN** its size, peak RSS and one-shot dry-run time meet the limits, and are recorded in `project-status.md`

### Requirement: Release targets
The release workflow SHALL build `cyore` with the embedded int8 model for Linux x86_64 (static musl), Linux ARM64 (static musl), Windows x86_64, and macOS ARM64, and include it in each release archive. Locally, `make release` SHALL produce the same single-file binary for the host, and `make cross-arm64` SHALL cross-compile the static Linux ARM64 binary when that Rust target is installed.

#### Scenario: Cross-compiled ARM64 binary
- **WHEN** `make cross-arm64` runs with the `aarch64-unknown-linux-musl` target installed
- **THEN** it produces a statically linked ARM64 ELF executable named `cyore`
