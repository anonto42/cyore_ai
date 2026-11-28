# Design

## Context

- Held-out sets live in `dataset/src/catalog.rs` as `tests` and `dev` arrays with `{slot}` placeholders, filled from the test slot pools. That is convenient for the catalog's author but awkward for someone else: they would have to edit Rust and could not choose their own slot values.
- `dataset` already has the pieces needed: `tokenize`, `Template` (fill, slots), the catalog, and the held-out filter in `generate_train`.
- `intent eval --set test|dev` reads `<data>/<set>.jsonl` and prints a `Report`, whose confused-pairs section names intents, not sentences.

## Goals / Non-Goals

**Goals:** a format a non-programmer can write in a text editor; mistakes caught with line numbers; scores that can be shared without leaking the sentences.

**Non-Goals:** a UI; per-sentence error analysis on the human set (that is what the development set is for).

## Decisions

1. **Inline slot markup `[slot: value]`.** The writer picks real values ("[dir: Downloads]") and the tags follow from the markup, with no separate annotation step. Parsing: scan for `[`, read up to `]`, split at the first `:`, trim both sides. The value replaces the group in the text, and a `{slot}` placeholder replaces it in a parallel placeholder text, which is tokenized exactly like the generator does to produce positional tags. This reuses the generator's labelling idea, so tags are always aligned.

2. **`dataset::human` in the library**, returning `Vec<HumanExample>` (same fields as the JSONL examples) or every error with its line number. The generator binary calls it. Values must satisfy the same rules as the agent's safety checks (one token, no shell syntax, no leading `-`), so every expected command is one the agent would accept.

3. **Generation order:** parse the human file first, then generate the development and test examples, then generate training with all three held-out sets in the filter. An empty or missing file means no `human.jsonl` (a stale one is deleted). Because the filter set is then unchanged, the generated files stay byte-identical.

4. **Report-only output.** `intent eval --set human` reuses `metrics::evaluate` and prints only its `Display`, which never contains sentence text, plus three notes:
   - an age warning if `human_test.txt` is newer than `meta.json`
   - how many human sentences also occur in `dev.jsonl` or `test.jsonl` (lowercased)
   - the sentence count

   A unit test checks that the printed report contains none of the input sentences.

5. **The rule lives in `.agents/shared/rules/evaluation.md`**, the canonical place for rules (AGENTS.md forbids duplicating them elsewhere). AGENTS.md's reading list and the startup checklist point to it. It covers the existing test set too, which until now relied on convention.

6. **Template file** generated once from `intents.json` and committed: a header explaining the format with two examples, then one commented line per intent (`# count_lines: wc -l {file}`, slots: `file` required). Suggested size: at least 3 sentences per intent and 10 for `unknown`. These are suggestions, not enforced, because a partial set is still useful.

## Risks / Trade-offs

- [The writer sees the catalog's command list and may echo the grammar's wording] → the template lists only intents, commands and slots, never example phrasings, and the instructions ask for the writer's own everyday wording.
- [An assistant could still open the file] → the rule forbids it and scoring never prints sentences. This is a process safeguard, not a technical one.
- [Model trained before the sentences were added may have seen some of them] → the age warning; retrain (`make intent-train`) for a clean score.

## Migration Plan

Additive. With the template as shipped, nothing about generated data or models changes.
