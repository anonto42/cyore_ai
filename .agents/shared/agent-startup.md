# Agent Startup Checklist

When starting any new task, bug fix, or feature in this repository, follow these steps:

1. **Check Environment:**
   Run `cargo --version`: it must report the version pinned in `rust-toolchain.toml`, which is
   the same rustc as the Nix shell (CI checks this). With Nix, `nix develop` (or direnv via
   `.envrc`) provides the full toolchain plus `make` and `node` for OpenSpec.
2. **Review OpenSpec:**
   Inspect `openspec/config.yaml`, run `openspec list` and `openspec list --specs`, and check
   `openspec/changes/` for active changes before starting.
   Workflow skills live in `.agents/skills/openspec-*` (propose → apply → archive). Any
   non-trivial change starts with a proposal under `openspec/changes/<name>/` and must pass
   `openspec validate <name> --strict` (or `make spec`).
3. **Know Where You Are in the Plan:**
   Read `README.md` (the 4-stage learning plan) and `.agents/shared/memory/project-status.md`
   (what is done, what is next). Keep the status memory current when a milestone lands.
4. **Understand Architectural Boundaries:**
   - Workspace root: `Cargo.toml` (members + shared release profile). One crate per milestone.
   - `xor/src/`: `activation.rs`, `loss.rs`, `layer.rs` (dense layer, forward/backward),
     `network.rs` (hidden + output layers, `train_step`), `random.rs` (xorshift RNG),
     `data.rs` (dataset), `main.rs` (training loop and evaluation).
   - `mnist/src/`: `random.rs` (Rng + shuffle), `matrix.rs` (flat matrix), `activation.rs`
     (sigmoid, softmax), `loss.rs` (cross-entropy), `network.rs` (params, forward, backward into
     `Gradients`, `apply`), `data.rs` (IDX loader), `metrics.rs`, `config.rs` (CLI), `main.rs`.
     The dataset lives in git-ignored `data/mnist/` (`make mnist-data`).
   - `dataset/src/`: `catalog.rs` (intents, slot pools, allowlist: edit this to add data),
     `grammar.rs` (phrasing grammar), `template.rs` (command templates), `tokenize.rs` (the shared
     tokenization rule), `validate.rs` (catalog checks), `generate.rs` (examples, JSON types),
     `lib.rs` (exports `tokenize` + `template` for other crates), `random.rs`, `main.rs` (CLI, atomic writes). Output: git-ignored `data/commands/`.
   - `intent/src/`: `data.rs`, `vocab.rs` (vocabulary + shape tokens), `batch.rs` (equal-length
     batches), `model.rs` (`JointModel`, `BagOfWords`), `train.rs` (loop, dev-based epoch
     selection), `store.rs` (`model.bpk` + `meta.json`), `predict.rs` (`Predictor`, `decode`),
     `metrics.rs`, `main.rs` (`train|eval|predict`). Burn 0.22 API: `Tensor<D>` has no backend
     parameter; `Device::flex()` (+ `.autodiff()` for training). Output: `data/models/intent/`.
     Never tune on `test.jsonl`; use `dev.jsonl`.
5. **Run Verification Before & After:**
   Always run `make check` (`cargo fmt --check`, `cargo clippy -D warnings`, `cargo test`,
   release build) before finishing any edit.
