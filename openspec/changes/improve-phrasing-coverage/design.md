# Design

## Context

- `dataset/src/generate.rs` builds a training sentence as follows: pick a grammar line, pick an expansion, apply `contract` ("what is" → "what's"), apply the kind-based wrappers (`do:` prefixes and suffixes, `ask:` prefixes, `?`), then fill the slots. Labels come from placeholders, so word-level edits made *before* filling are safe as long as `{slot}` tokens are untouched.
- The model maps unseen words to shape tokens, and word dropout (0.05) replaces known context words with `<word>` during training.
- The development set (162) is saturated (96.9%). The test set (162) has been read twice (runs 1 and 4 of `add-intent-model`).

## Goals / Non-Goals

**Goals:** close general wording gaps (terse, possessive, varied verbs) with rules that apply to every intent, measured on held-out sentences written before the rules.

**Non-Goals:** per-intent fixes and the test set as a guide.

## Decisions

1. **Write the new development batch before changing anything**, and score it with the current model. That fixes the baseline, so an improvement cannot be imagined after the fact. The batch is about 105 sentences: 2 per command intent and 5 for `unknown`, in everyday style.

2. **Variants as one `vary` function**, called just before `contract`, each operation with its own probability:
   - articles dropped, each independently with p = 0.3
   - `me the` → `my` with p = 0.3; `the` → `my` with p = 0.2 before a noun in `PERSONAL_NOUNS`
   - `me` dropped after a word in `DISPLAY_VERBS` with p = 0.3
   - a leading word in `DISPLAY_VERBS` replaced by another from the group with p = 0.25

   Words are compared case-insensitively, and placeholders (`{…}`) are never edited or removed. The lists live in `catalog.rs` next to the other wrappers, so they read as data.
   *Alternative:* add the variants to each grammar line. Rejected: 50 intents × several lines is error-prone, and these are properties of English, not of any intent.

3. **Validation of variants:** the existing cross-intent collision check works on grammar expansions, not samples, so a variant could in principle turn one intent's sentence into another's. The generator already skips duplicate texts globally. A unit test checks, on the real catalog, that no generated training text appears under two intents.

4. **Word dropout** is chosen from {0.05, 0.1, 0.2} by development exact match. The default stays at 0.05 unless another value wins.

5. **Reporting:** a table in `project-status.md` with the development set (old 162, new batch, all) before and after, and the final test once.

## Risks / Trade-offs

- [Dropped articles make some sentences ambiguous between intents] → the global duplicate filter plus the cross-intent test; development accuracy shows any harm.
- [The same person wrote the rules and the new development sentences] → the sentences are written and scored first; the limitation is stated in the report. An owner-written test set is recommended.
- [Test accuracy may still miss 70%] → reported as is; `add-intent-model` stays open.

## Migration Plan

Regenerate the data and retrain (`make release`). Rollback: revert the commit and regenerate.
