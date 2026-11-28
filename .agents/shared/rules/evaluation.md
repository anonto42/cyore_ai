# Evaluation Integrity Rules

These rules keep Cyore's accuracy numbers honest. They apply to every AI assistant and every
person tuning the dataset or the model.

1. **Three kinds of held-out data, three roles:**
   - **Development** (`dev` sentences in `dataset/src/catalog.rs`, `data/commands/dev.jsonl`):
     the only held-out data that may guide changes to the catalog, the generator, or the model.
     Read its errors, then fix them in general terms.
   - **Test** (`tests` in the catalog, `test.jsonl`): report only. Look at its numbers when a
     change is finished, never to decide what to change. Record each look in
     `.agents/shared/memory/project-status.md`.
   - **Human** (`dataset/human_test.txt`, `human.jsonl`): written by someone other than the
     catalog's author, and the independent measure. Report only.

2. **Never read the human test set.** Do not open, print, quote, or summarise the requests in
   `dataset/human_test.txt` or `data/commands/human.jsonl`. Score it only with
   `make intent-eval-human`, which prints totals and never sentences. Editing the file is the
   owner's job; an assistant may only fix its format when the owner asks and points to a line
   number from a validation error.

3. **Never tune on test or human results.** Do not add phrasings, synonyms, slot values,
   training settings, or quantisation choices because of a test or human error. If a test
   number is needed to check a stated requirement (for example "int8 within 1 point of f32"),
   use it for that check only, and say so in the record.

4. **Never copy held-out sentences into the grammar.** Generation already drops any training
   sentence equal to a held-out one; do not work around that with near-copies.

5. **Report honestly.** Record misses as misses (with the numbers), and never lower a target
   without the owner's explicit decision.
