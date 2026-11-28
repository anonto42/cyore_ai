# Proposal

## Why

The Stage 2 model reaches 99.6% on validation but 62.3% intent accuracy on the hand-written test set, short of the 70% target that the owner chose to keep. Even the plan's own example misses: "show my files" gets *not sure* (0.41), while "show me the files" gets `ls -la` (0.99). The grammar produces tidy, complete sentences ("show me the files"), but people type terse ("list biggest files"), possessive ("show my files") and varied ("view the files") requests. The development set can no longer guide this: it scores 96.9% because the grammar was widened to fix its errors.

## What Changes

- **Second development batch, written first.** Add 2 more hand-written development sentences per command intent and 5 more for `unknown` (105 in total), in everyday style. They are written before any grammar change and scored with the current model, which is the baseline. Same rules as before: never in training or test, slot values from the test pools.
- **Everyday variants at sampling time** in the dataset generator, applied to every intent like the existing contractions:
  - drop articles (`the`, `a`, `an`): "list the biggest files" → "list biggest files"
  - possessives: "me the files" → "my files", and "the" → "my" before personal nouns (files, folder, disk, memory, processes, branch, …)
  - drop "me" after a verb: "show me the files" → "show the files"
  - swap the leading display verb within one synonym group: show, list, display, print, view, get
  The variants never touch slot placeholders, and validation still guarantees that no training sentence equals a held-out one.
- **Model setting:** choose the word-dropout rate (0.05, 0.1 or 0.2) by development exact match.
- **Report** development numbers before and after, then evaluate the test set once with the final settings and record the result, met or not, in `add-intent-model`'s records and `project-status.md`.

### Non-goals

- No catalog edits based on test errors, no new intents, no model architecture changes.
- No spelling-error augmentation (unknown words already map to shape tokens; word dropout covers them).
- Not a replacement for a test set written by someone else, which remains the honest check and is recommended to the owner.

### Invariants

- Learning-plan stage: Stage 2 (follow-up to `add-intent-model`, which stays open).
- The test set is read only once, at the end (its third look overall; recorded).
- Generated files stay reproducible: same seed, same bytes.
- `make check` stays green without data; the agent's safety behaviour is untouched.

## Capabilities

### New Capabilities

None.

### Modified Capabilities
- `command-dataset`: adds the everyday-variant augmentation requirement.

## Impact

- Changed: `dataset/src/catalog.rs` (dev sentences, noun and verb lists), `dataset/src/generate.rs` (variants), `intent` default word dropout if the development set favours another value, docs (`dataset/README.md`, `intent/README.md`, `project-status.md`).
- Regenerated data and model, so the README result tables change. The embedded model in `cyore` changes at the next `make release`.
- No new dependencies.
