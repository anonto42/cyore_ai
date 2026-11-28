# dataset: the command dataset generator

Stage 2, first step. Before a model can turn "show my files" into `ls -la`, it needs labelled
examples. This crate turns a hand-written catalog (`src/catalog.rs`) into training and test
files for the Stage 2 intent classifier and slot model.

## Run it

```sh
make commands-data                          # writes data/commands/ (git-ignored)
make commands-data ARGS="--per-intent 200"  # more examples per intent
cargo run -p dataset -- --help
```

| Option | Meaning | Default |
|---|---|---|
| `--out <DIR>` | output directory | `data/commands` |
| `--seed <N>` | random seed (same seed → byte-identical files) | 42 |
| `--per-intent <N>` | training examples per intent | 150 |

With the defaults: 51 intents (50 commands + `unknown`), 7,650 training examples, 162
hand-written development examples, and 162 hand-written test examples.

## Output files

`train.jsonl`, `dev.jsonl` and `test.jsonl` hold one JSON object per line:

```json
{"text": "Can you list the files in src?",
 "tokens": ["Can", "you", "list", "the", "files", "in", "src"],
 "tags":   ["O", "O", "O", "O", "O", "O", "B-dir"],
 "intent": "list_files",
 "slots":  {"dir": "src"},
 "command": "ls -la src"}
```

- `intent` is the label for the **intent classifier**.
- `tags` (one per token, BIO format) are the labels for the **slot model**: `B-dir` marks the
  token holding the `dir` value, and `O` marks everything else.
- `command` is the full answer, kept for the later seq2seq model; it is `null` for `unknown`.

`intents.json` lists every intent with its command template and slots: the label vocabulary.

**Tokenization rule** (the model and agent must use the same one): split on whitespace,
strip trailing `?`, `!` and `,` from every token, and strip a trailing `.` from the last token.

**Train, development, test:** training sentences come from the grammar. Development and
test sentences are written by hand, never appear in training (or in each other), and use slot
values (file names, folders, hosts…) that training never uses. Accuracy on them therefore
measures whether the model learned the *language*, not whether it memorised sentences.

- **Development** sentences are the only held-out data allowed to guide changes to the
  catalog or the model. Look at their errors, then widen the grammar in general terms.
- **Test** sentences are for the final report only. Never change the catalog because of a
  test error, or the test score stops meaning anything.

Why both exist: the first model scored 99.8% on validation (held out from the grammar) and
48.8% on test. A quarter of everyday words simply never appeared in the grammar, and only
hand-written sentences reveal that (see intent/README.md).

## The catalog

Each intent in `src/catalog.rs` has a command template, phrasing grammar lines, test
sentences, and development sentences.

**Command templates**: one slot per word.

| Form | Meaning | Example |
|---|---|---|
| `{file}` | required | `cat {file}` |
| `{dir?}` | optional; the word disappears if not given | `ls -la {dir?}` → `ls -la` |
| `{dir=.}` | optional with a default | `du -sh {dir=.}` → `du -sh .` |

Commands never use pipes, redirection or other shell syntax, so the Stage 3 agent can run
them with `std::process::Command` and no shell. (It must expand a leading `~/` itself.)

**Phrasing grammar**: one line describes many sentences.

| Syntax | Meaning |
|---|---|
| `(a\|b\|c)` | exactly one of the options |
| `[x]` | optional |
| `{slot}` | slot placeholder |

`(show [me]|list) the files [in {dir}]` gives 6 sentences. Start a line with `do:` for
instructions or `ask:` for questions; the generator then adds fitting wrappers ("can you…",
"…please", "hey, …", "?") at random, and contracts common pairs ("what is" → "what's" or
"whats", "i am" → "i'm").

**Sampling**: for each training example the generator first picks one of the intent's grammar
lines, then one sentence from that line. Picking among all sentences directly would let a line
with 48 variants drown out `ping {host}`, and words from short lines would never make it into
the vocabulary. A generated sentence that equals a development or test sentence word for word
is skipped.

## Adding an intent

1. Add an `Intent { name, command, phrasings, tests, dev }` entry to `INTENTS` (at least 3
   test and 3 development sentences, written independently of each other).
2. Put a new program on `ALLOWLIST`, and add new slot types to `POOLS` (train and test values
   must not overlap).
3. Run `make commands-data`. Validation reports every problem before writing anything: fewer
   than 20 distinct phrasings, a missing required slot, a sentence shared with another
   intent, a program not on the allowlist, shell syntax, or a blocklisted command.

Destructive actions (`rm`, `kill`, `shutdown`, …) are deliberately **not** intents. Requests
for them are examples of `unknown`, so the model learns to hand them back to the user.
