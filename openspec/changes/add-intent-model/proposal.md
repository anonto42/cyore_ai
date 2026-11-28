# Proposal

## Why

Stage 2 of the learning plan (README.md) is "build your own model in Rust" with Burn, starting simple: an intent classifier that picks one command template, plus a slot model that extracts values like file and folder names. The command dataset (`dataset/`, archived change `add-command-dataset`) now provides 4,080 labelled training sentences and 162 hand-written test sentences, and `docs/MODEL_SURVEY.md` recommends the architecture. This change trains that model, so English requests can become commands for the first time.

## What Changes

- Add a workspace crate `intent/` using Burn 0.22 on its pure-Rust CPU backend (`flex`). It is a library (vocabulary, model, prediction) plus a binary with three subcommands:
  - `intent train`: train on `data/commands/train.jsonl` (with a validation split held out from it), save the weights and vocabulary to `data/models/intent/`, and report metrics
  - `intent eval`: report metrics on the hand-written test set
  - `intent predict "<request>"`: print the intent, confidence, slots, and the filled-in command, or say it is not sure
- Two models, compared in the report:
  - **Baseline:** a bag-of-words dense network for the intent only (the `mnist` network with a different input), which shows how far plain word counting gets
  - **Joint model** (the one that is saved and used): word embeddings → bidirectional GRU → an intent head on the pooled sentence and a slot-tag head on every word
- A vocabulary built from training words plus *shape tokens* for words it has never seen (path, file name, glob, number or IP address, other word), so unseen file names and folders still look like file names and folders to the model.
- Metrics: intent accuracy, slot accuracy, end-to-end **command accuracy** (the predicted command equals the expected one), `unknown` precision and recall, the most-confused intent pairs, and accuracy versus coverage at several confidence thresholds (input for the Stage 3 agent's threshold).
- `make intent-train`, `make intent-eval`, `make intent-predict` targets; updates to `AGENTS.md`, `.agents/shared/memory/project-status.md` and the backend name in `README.md`/`docs/ARCHITECTURE.md` (`ndarray` → `flex`).

### Non-goals

- No seq2seq/transformer model (the plan's "advanced version"; a later change).
- No execution of commands, allowlist enforcement, or confirmation prompt; that is the Stage 3 agent, which will use this crate as a library.
- No int8 quantisation, weights embedded with `include_bytes!`, or cross-compilation (Stage 4).
- No GPU backends, Burn's `Learner`/training dashboard, or pretrained word embeddings.
- No changes to the dataset catalog driven by test-set errors (that would leak the test set into training).

### Invariants

- Learning-plan stage: Stage 2 (second change).
- `xor`, `mnist`, `dataset` and their specs are untouched; Stage 1 crates stay standard-library only.
- The same seed and data give identical metrics on the same machine.
- `make check` stays green without the dataset or a trained model (tests use small in-memory data).
- `predict` never panics on any input, including empty text, unknown words, or missing model files.

## Capabilities

### New Capabilities
- `command-model`: training, evaluating, saving and loading the joint intent and slot model, and turning one English request into an intent, slots, a confidence, and a command.

### Modified Capabilities

None.

## Impact

- New: `intent/` crate (workspace member); trained artefacts in git-ignored `data/models/intent/`.
- Changed: root `Cargo.toml` (members), `Cargo.lock`, `Makefile`, `AGENTS.md`, `.agents/shared/memory/project-status.md`, and one backend mention each in `README.md` and `docs/ARCHITECTURE.md`.
- New dependency: `burn = 0.22.1` with `default-features = false` and features `std`, `flex`, `optim`, `store` only. A prototype with this feature set built to a **4.7 MB** release binary (training included), within the 5–30 MB target. `serde`/`serde_json` (already in the lockfile) read the dataset.
- Build time: Burn adds roughly a minute to a clean `make check`.
