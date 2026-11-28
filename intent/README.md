# intent: the Stage 2 model

Turns one plain-English request into a terminal command. A small neural network, trained with
[Burn](https://burn.dev) on the CPU, reads the request and predicts:

- the **intent**: which of 51 command templates is meant (e.g. `list_files` → `ls -la {dir?}`)
- the **slots**: which words are values to put into the template (e.g. `dir` = `Pictures`)
- a **confidence**: how sure it is; below the threshold it says so instead of guessing

```text
$ make intent-predict ARGS="find all *.mp4 files in ~/Videos"
request:     find all *.mp4 files in ~/Videos
intent:      find_by_name (confidence 1.00)
slots:       dir = ~/Videos, pattern = *.mp4
command:     find ~/Videos -name *.mp4
```

The crate is a library (`Predictor::load(dir)?.predict(text, threshold)`), which the Stage 3
agent will use, plus the `intent` command for training and evaluation.

## Run it

```sh
make commands-data                        # the dataset (see dataset/README.md)
make intent-train                         # ~20 s on a laptop; saves data/models/intent/
make intent-eval                          # final report on the hand-written test set
make intent-eval ARGS="--set dev"         # the same report on the development set
make intent-predict ARGS="how much disk space is left"
```

| Command | Options |
|---|---|
| `intent train` | `--data DIR` (`data/commands`), `--out DIR` (`data/models/intent`), `--epochs N` (40), `--seed N` (42), `--show-test` |
| `intent eval` | `--data DIR`, `--model DIR`, `--set test\|dev` (test) |
| `intent predict` | `--model DIR`, `--threshold P` (0.7), then the request |

Usage errors exit with status 2. A missing dataset or model exits with 1 and names the `make`
target to run.

## The model

```text
tokens → embedding (64) → bidirectional GRU (64 + 64) ─┬─ mean over words → linear → softmax → intent
                                                        └─ every word     → linear → softmax → slot tag
```

- **Embedding**: each known word id selects a learned 64-number vector. This is a row lookup,
  the same as multiplying a one-hot vector by a matrix.
- **Bidirectional GRU**: one recurrent layer reads left to right and another right to left, so
  each word's vector depends on the whole sentence. That is how "in X" and "called X" mark X as
  a folder or a file whatever X is. Gates (sigmoids between 0 and 1) decide what to keep from
  the previous step.
- **Two heads, one loss**: L = −ln p(intent) + mean over words of −ln q(tag). This is the same
  cross-entropy as `mnist`, applied once for the sentence and once per word.
- **Training**: Burn computes every gradient (backpropagation through time) and Adam updates
  the weights.
- **Size**: about 83k parameters, a 356 KB weight file and a 4.8 MB release binary.

**Words the model has never seen** become *shape tokens* (`<path>`, `<file>`, `<glob>`,
`<num>`, `<word>`). The vocabulary holds only context words from training, never slot values.
So `budget.xlsx`, which never appears in training, looks to the model exactly like
`notes.txt`. That is why slot tagging carries over so well to new sentences (98% on unseen
values).

**Baseline**: training also fits a bag-of-words dense network (word counts → 64 → 51), the
`mnist` network with a different input. It ignores word order, so it cannot tag slots, and it
is the yardstick the joint model is compared against.

## How a prediction is decided

1. Tokenize with the dataset's rule (shared code, `dataset::tokenize`), keeping at most 64
   tokens. Empty input → `unknown`, "empty request".
2. The intent is the most likely one, and the confidence is its probability.
3. For each slot in that intent's template, take the token tagged `B-<slot>` with the highest
   probability. Its text is copied verbatim, so case is kept.
4. No command if the intent is `unknown` ("not supported"), if the confidence is below the
   threshold ("not sure", with the top 3 guesses), or if a required slot was not found.
   Otherwise the template is filled in.

## Three kinds of held-out data

| Set | Source | Used for |
|---|---|---|
| validation | 10% of the generated training sentences | does the model fit the grammar? |
| development | 162 hand-written sentences | choosing the saved epoch, and every catalog or model change |
| test | 162 other hand-written sentences | the final report only |

## Results (seed 42)

| | validation | development | test |
|---|---|---|---|
| joint model: intent | 99.6% | 96.9% | **62.3%** |
| joint model: command | 99.6% | 95.7% | **60.5%** |
| joint model: slot tags | 100% | 99.4% | 97.6% |
| bag-of-words baseline: intent | 99.9% | 98.8% | 67.9% |

At the default threshold of 0.7, the model answers 71% of the test requests and gets 77% of
those right. 10 of the 12 out-of-scope test requests get no command. The two that slip through
map to `pwd` and `lsblk`, which are harmless because the dataset contains no destructive
commands.

**What this taught us.** The first run scored 99.8% on validation and 48.8% on test.
Validation sentences come from the same grammar as training, so they cannot reveal that a
quarter of the words people actually use never appear in the grammar. Adding a development set
showed where the gaps were. Widening the grammar and fixing the sampler raised the test score to
62%, which is still short of the 70% target. The development set is now optimistic (96.9%),
because the grammar was widened to fix its errors. And the bag-of-words baseline does better on
test than the joint model: with this little data, the vocabulary matters more than the
architecture. More varied, human-written training sentences are the next lever, not a bigger
network.

## Example predictions

Requests that are in none of the dataset files:

| Request | Result |
|---|---|
| show me the files in Pictures | `ls -la Pictures` (0.99) |
| how much space is left on my disk | `df -h` (1.00) |
| find all *.mp4 files in ~/Videos | `find ~/Videos -name *.mp4` (1.00) |
| delete everything in my home folder | no command: not sure |
| banana spaceship purple | no command: not supported |

## Files

| File | What it does |
|---|---|
| `data.rs` | read `train/dev/test.jsonl` and `intents.json`; seeded validation split |
| `vocab.rs` | vocabulary, shape tokens, intent and tag labels |
| `batch.rs` | equal-length batches (no padding), word dropout, tensors |
| `model.rs` | `JointModel` (embedding → BiGRU → two heads), `BagOfWords` |
| `train.rs` | training loop, development-based epoch selection, early stopping |
| `store.rs` | save and load `model.bpk` + `meta.json` |
| `predict.rs` | `Predictor` and the decision rules (`decode`) |
| `metrics.rs` | accuracy, slot and command accuracy, unknown precision/recall, threshold table |
| `main.rs` | the `intent` command |
