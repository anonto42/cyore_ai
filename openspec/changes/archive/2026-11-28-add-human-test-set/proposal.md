# Proposal

## Why

The only test set (162 sentences) was written by the same author as the catalog, the development set and the phrasing rules, and it has been read three times. It can no longer give an independent measure, and the `add-intent-model` targets (70% intent, 60% command) are judged against it. The honest check is a test set written by someone else, typically the owner. That person needs a simple format to write it in, validation that catches mistakes, and a command that scores the model without revealing individual sentences to whoever is tuning the model.

## What Changes

- **A plain-text file** `dataset/human_test.txt`, tracked in git, that a person fills in: one request per line, the intent name first, slot values marked inline:
  `count_lines  how many lines are in [file: notes.txt]`
  Comments (`#`) and blank lines are ignored. It ships as a template: instructions plus a commented list of all 51 intents with their commands and slots, and no sentences.
- **Validation** in the dataset generator, with line numbers: the intent exists; slot names belong to that intent's command; required slots are present; a slot value is one token, contains no shell syntax and does not start with `-`; no duplicate sentences; `unknown` has no slots.
- **Conversion**: `make commands-data` writes `data/commands/human.jsonl` in the same format as `test.jsonl` when the file has sentences, and excludes those sentences from the generated training data like the other held-out sets.
- **Scoring**: `intent eval --set human` and `make intent-eval-human` print the usual aggregate report (intent, slot and command accuracy, `unknown` precision and recall, threshold table, confused intent pairs) and never print individual sentences. They warn when the model is older than the file (retrain for a clean score) and report how many human sentences also occur in the development or test sets.
- **A rule for assistants**, `.agents/shared/rules/evaluation.md` (referenced from `AGENTS.md` and the startup checklist): test and human sets are report-only; never tune the catalog or the model on them; never read or print the human sentences; development errors are the only legitimate guide.

### Non-goals

- Writing the sentences. That is the point: they must come from someone else.
- Changing the model, the catalog, or the existing test set.
- Lowering or changing the `add-intent-model` targets.

### Invariants

- Learning-plan stage: Stage 2 evaluation tooling.
- With the template as shipped (no sentences), generated `train`, `dev` and `test` files are byte-identical to before, and no `human.jsonl` is written.
- `make check` stays green without data.

## Capabilities

### New Capabilities
- `human-evaluation`: the human-written test file format, its validation and conversion, and report-only scoring.

### Modified Capabilities

None.

## Impact

- New: `dataset/src/human.rs` (library), `dataset/human_test.txt` (template), `.agents/shared/rules/evaluation.md`.
- Changed: `dataset/src/main.rs` (conversion, held-out filter), `intent/src/main.rs` and `data.rs` (`--set human`, age warning), `Makefile`, `AGENTS.md`, `.agents/shared/agent-startup.md`, `dataset/README.md`, `intent/README.md`, `project-status.md`.
- No new dependencies.
