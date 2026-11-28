# Tasks

## 1. int8 model

- [ ] 1.1 `intent`: `store::save_q8` / `Predictor::load_q8(dir)` / `Predictor::from_static(weights, meta_json)`; verify a unit test that quantizes the overfit test model, round-trips it through bytes with `from_static`, and gets the same intents and slots as the f32 model
- [ ] 1.2 `intent quantize [--model DIR] [--data DIR]`: write `model-q8.bpk`, print sizes and f32 vs int8 metrics on dev (and test as report only); missing model exits 1 with the `make intent-train` hint; add `--q8` to `eval` and `predict`; add `make intent-quantize`; verify by running it and checking the int8 file is ≤ 30% of f32 and accuracy is within 1 point
- [ ] 1.3 Quantize the same model twice; verify `cmp` reports identical files

## 2. Embedded model

- [ ] 2.1 `agent/build.rs` and `agent/src/embedded.rs` as in design.md decision 3; verify `cargo build -p agent` succeeds with `CYORE_MODEL_DIR=/nonexistent` (no model) and that `cyore` then falls back to the directory
- [ ] 2.2 `cyore`: `--model DIR` > embedded > `data/models/intent`, with the model in use shown in the banner and `--help`; verify a test of the selection order (via a function that takes the three sources)
- [ ] 2.3 Build with the quantized model present, copy `cyore` alone to an empty temporary directory, run one-shot dry runs there for 5 requests; verify the commands match the f32 `intent predict` output

## 3. Release builds

- [ ] 3.1 `make release` (dataset → train → quantize → release build, then size) and `make cross-arm64` (rust-lld); list both in `make help`; verify `make release` output and that `file` reports a static ARM64 executable for `make cross-arm64` (or record why it cannot link here)
- [ ] 3.2 Release workflow: train-and-quantize step, package:binary pairs including `agent:cyore`; verify with `actionlint` if available, otherwise by review against the existing job structure
- [ ] 3.3 Measure size, peak RSS (getrusage) and one-shot dry-run time of the embedded binary; build with `opt-level = "z"` and compare; verify ≤ 30 MB, < 100 MB, < 1 s, and record all numbers in `project-status.md`

## 4. Docs and checks

- [ ] 4.1 Update README (Stage 4 status, single-file usage), `agent/README.md`, `intent/README.md` (int8 section with the math and the numbers), `docs/ARCHITECTURE.md`, `docs/RELEASES.md`, `AGENTS.md`, `.agents/shared/agent-startup.md`, `project-status.md`; run `make check` and `make spec` on the current tree, and `make check` with `CYORE_MODEL_DIR=/nonexistent` to prove a fresh clone still builds; verify all pass

## Workflow follow-up

- Archive with `openspec archive add-packaging` after review (after `add-intent-model` and `add-agent`), which creates `openspec/specs/model-packaging`.
- Tag a release (`v0.x.0`) to exercise the workflow on all four targets.
