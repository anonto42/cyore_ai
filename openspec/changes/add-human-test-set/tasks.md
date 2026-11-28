# Tasks

## 1. Format and validation

- [x] 1.1 `dataset/src/human.rs`: parse lines into examples (text, tokens, tags, intent, slots, command) using the catalog and `Template`; every error with its line number; verify unit tests for a valid line with and without slots, comments and blank lines, and each error in the spec (unknown intent, bad brackets, wrong slot, missing required slot, multi-token value, shell syntax, leading `-`, slot on `unknown`, duplicate)
- [x] 1.2 Generator: read `dataset/human_test.txt` (option `--human FILE`), add the sentences to the held-out filter, write `human.jsonl` when there are sentences and delete a stale one otherwise, and show the count in the summary; verify that with the template as shipped `train`, `dev` and `test` are byte-identical to before and no `human.jsonl` exists

## 2. Template and scoring

- [x] 2.1 Write `dataset/human_test.txt`: format instructions, two examples, one commented line per intent with its command and slots, no sentences; verify validation accepts it and finds 0 sentences
- [x] 2.2 `intent eval --set human` with the age warning, overlap counts, and a "no sentences yet" message (exit 0); `make intent-eval-human`; verify with a temporary human file of a few lines that the output contains none of its sentences, and that the age warning appears after touching the file

## 3. Rule and docs

- [x] 3.1 `.agents/shared/rules/evaluation.md`; reference it from `AGENTS.md` and `.agents/shared/agent-startup.md`
- [x] 3.2 Update `dataset/README.md` and `intent/README.md` (how to write and score a human test set), `project-status.md`; run `make check` and `make spec`; verify both pass

## Workflow follow-up

- Archive after review.
- The owner writes the sentences, retrains (`make intent-train`), and runs `make intent-eval-human`. Those numbers are the independent measure for `add-intent-model`'s targets.
