# Tasks

## 1. Scaffolding

- [x] 1.1 Split `dataset` into library + binary: add `dataset/src/lib.rs` exporting `tokenize` and `template`; verify `cargo test -p dataset` passes unchanged and `make commands-data` output is byte-identical to before (`cmp`)
- [x] 1.2 Create the `intent/` crate (lib + bin) depending on `burn = "=0.22.1"` (`default-features = false`, features `std`, `flex`, `optim`, `store`), `dataset` (path), `serde`, `serde_json`; add it to workspace members; verify `cargo build -p intent` succeeds
- [x] 1.3 Add `intent-train`, `intent-eval`, `intent-predict` Makefile targets (release, options via `ARGS`) and list them in `make help`; verify `make help` shows them

## 2. Data and vocabulary

- [x] 2.1 `data.rs`: read `train.jsonl`/`test.jsonl` into `Example`s and `intents.json` into the intent list; a missing file returns an error mentioning `make commands-data`; verify tests on an in-memory JSONL string and a missing-directory error
- [x] 2.2 `vocab.rs`: shape classes, vocabulary from `O`-tagged tokens with count ≥ 2, encode tokens to ids, intent and tag lists, JSON save/load; verify tests for each shape (`~/notes`→`<path>`, `*.go`→`<glob>`, `10.0.0.1`→`<num>`, `budget.xlsx`→`<file>`, `heaviest`→`<word>`), that slot values never enter the vocabulary, and a save/load round trip
- [x] 2.3 Seeded per-intent validation split and word dropout; verify tests that the split is ~10% per intent, disjoint, identical for the same seed, and that dropout with p = 0 changes nothing
- [x] 2.4 `batch.rs`: length buckets and conversion to Burn tensors (`[batch, len]` ids, intent ids, flattened tag ids); verify tests that every batch has one length, every example appears once per epoch, and order is fixed by the seed

## 3. Models and training

- [x] 3.1 `model.rs`: `JointModel` (Embedding → BiGru → mean-pool intent head + per-token slot head) and `BagOfWords` baseline; verify a test that output shapes are `[b, 51]` and `[b, n, 8]` and that intent probabilities sum to 1
- [x] 3.2 `train.rs`: training loop (joint loss, Adam, per-epoch shuffle, validation accuracy, keep best epoch, early stopping) for both models; verify the overfit test from design.md decision 12 passes
- [x] 3.3 Save and load (`model.bpk` + `meta.json` via temp file and rename); verify a round-trip test that a loaded model's predictions equal the original's

## 4. Prediction and metrics

- [x] 4.1 `predict.rs`: `Predictor::load(dir)` and `predict(text) -> Prediction { intent, confidence, slots, command, reason, top }` with the rules in design.md decision 9; verify tests for slot decoding (highest-probability token, original case), missing required slot, threshold, `unknown`, empty and very long input
- [x] 4.2 `metrics.rs`: intent accuracy, slot token accuracy, command accuracy, `unknown` precision/recall, top-5 confused pairs, threshold table; verify tests on hand-built predictions with known answers

## 5. Command-line program

- [x] 5.1 `main.rs`: `train`, `eval`, `predict` subcommands with the options in design.md decision 11; usage errors exit 2, missing data/model exit 1 with the `make` hint; verify `intent predict` without a model and `intent train --data /nonexistent` both exit non-zero with the hint and no panic
- [x] 5.2 Add `intent/README.md` (what it does, the math summary, the commands, how prediction decides, the metrics); verify the documented commands run as written

## 6. Integration checks

- [ ] 6.1 Run `make commands-data && make intent-train && make intent-eval`; verify validation intent accuracy ≥ 95%, test intent accuracy ≥ 70%, test command accuracy ≥ 60%, and record all numbers, training time and model size in `.agents/shared/memory/project-status.md` (NOT MET: validation 99.6% ✓, test command 60.5% ✓, test intent 62.3% ✗. After improve-phrasing-coverage: validation 99.1% ✓, test intent 62.3% ✗, test command 59.9% ✗; see project-status.md)
- [x] 6.2 Train twice with the same seed; verify identical per-epoch losses and metrics
- [x] 6.3 Run `make intent-predict` on 5 requests not in any dataset file (including one destructive and one nonsense request); verify sensible output and record it in `intent/README.md`
- [x] 6.4 Measure `ls -lh target/release/intent`; verify it is within 5–30 MB
- [x] 6.5 Update `AGENTS.md`, `.agents/shared/agent-startup.md` (module map), `project-status.md`, and the `ndarray` → `flex` mention in `README.md` and `docs/ARCHITECTURE.md`; run `make check` and `make spec` and verify both pass

## 7. Development set (added after the first run missed the test targets)

- [x] 7.1 Dataset: add `dev` sentences to `Intent` (≥ 3 per command intent, ≥ 10 for `unknown`), validate counts and slots, write `dev.jsonl` from the test slot pools, and check dev/train/test text disjointness; verify unit tests for the new checks and that `make commands-data` writes four files
- [x] 7.2 Write the development sentences without consulting `test.jsonl`
- [x] 7.3 Intent: load `dev.jsonl`, select the saved epoch by development exact match, and report development metrics next to validation and test (baseline included); `intent eval --set dev`; verify by a training run
- [x] 7.4 Widen the catalog phrasings (synonyms and everyday wording) based on development errors and general English only; record development accuracy before and after in `project-status.md`
- [x] 7.5 Evaluate on the test set once with the final settings and record the result honestly, met or not

## Workflow follow-up

- Archive with `openspec archive add-intent-model` after review, which creates `openspec/specs/command-model`.
- Next: Stage 3 agent (`add-agent`): read input → `Predictor` → show command → confirm → run with allowlist, blocklist and timeout.
