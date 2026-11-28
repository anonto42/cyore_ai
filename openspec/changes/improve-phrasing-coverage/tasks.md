# Tasks

## 1. Baseline

- [ ] 1.1 Add the second development batch to the catalog (2 per command intent, 5 for `unknown`), written without consulting `test.jsonl`; verify validation passes and `make commands-data` writes the larger `dev.jsonl` with `train.jsonl` and `test.jsonl` byte-identical to before
- [ ] 1.2 Score the current model on the full development set and on the new batch alone (`intent eval --set dev` plus a breakdown); record the baseline in `project-status.md`

## 2. Everyday variants

- [ ] 2.1 `catalog.rs`: `PERSONAL_NOUNS`, `DISPLAY_VERBS`; `generate.rs`: `vary` with the operations and probabilities in design.md decision 2, applied before `contract`; verify unit tests: each operation occurs over many samples (`show my files`, `list biggest files`, `show the files`, verb swap), placeholders are never altered, and output is reproducible
- [ ] 2.2 Test on the real catalog that no generated training text appears under two intents; verify it passes

## 3. Retrain and choose

- [ ] 3.1 Regenerate, retrain, and score on development; try word dropout 0.1 and 0.2; keep the best development exact match; verify by recording all runs in `project-status.md`
- [ ] 3.2 Evaluate the test set once with the final settings (`make intent-eval`) and record the result; requantize and confirm int8 matches f32

## 4. Docs and checks

- [ ] 4.1 Update `dataset/README.md` (variants), `intent/README.md` and the README result rows, `agent/README.md` if example outputs changed, and `project-status.md`; run `make check` and `make spec`; verify both pass

## Workflow follow-up

- Archive after review. If the test target is met, `add-intent-model` task 6.1 can be completed and archived as well.
