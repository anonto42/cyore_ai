# Design

## Context

- The dataset (`openspec/specs/command-dataset`) is JSON Lines: `text`, `tokens`, `tags` (BIO), `intent`, `slots`, `command`. There are 51 intents, 8 tags (`O` plus `B-` for 7 slot types), sentences of 1–13 tokens, and about 425 distinct lowercased words. The test set is hand-written and uses slot values never seen in training.
- Burn 0.22 changed its API from older tutorials. Tensors are `Tensor<D>` with no backend type parameter, and the backend comes from a runtime `Device`. `Device::flex()` is the pure-Rust CPU backend and `device.autodiff()` turns on gradients. The `ndarray` backend that README.md names is **deprecated**. Modules (`Embedding`, `Linear`, `BiGru`) derive `Module` without generics, `AdamConfig::new().init()` gives a `ModuleOptimizer` with `step(lr, model, grads)`, and `burn::store::BurnpackStore` saves and loads weights but refuses to overwrite an existing file.
- A prototype (embedding → BiLSTM → two heads, Adam, flex, save and load) compiled to 4.7 MB in release, trained, and printed identical losses across two seeded runs.

## Goals / Non-Goals

**Goals:**
- Every piece of the model maps to math the learner has already written by hand (embedding lookup = one-hot × matrix, linear layers, softmax, cross-entropy), plus one new idea: a recurrent layer.
- Generalise to the hand-written test set: unseen phrasings and unseen slot values.
- A prediction API the Stage 3 agent can call: `Predictor::load(dir)?.predict(text) -> Prediction`.

**Non-Goals:**
- Squeezing out the last percent with hyperparameter search; a few sensible defaults chosen on the validation split are enough.
- Batching for inference throughput; the agent predicts one sentence at a time.

## The Math

For a request with tokens w₁…wₙ mapped to ids x₁…xₙ:

**Embedding**: eₜ = E[xₜ] ∈ ℝᵈ (d = 64). A row lookup, which is the same as multiplying a one-hot vector by E (V × d), so its gradient only touches the rows of the words that were used.

**Bidirectional GRU** (hidden size k = 64 per direction). One step reads eₜ and the previous state h:
- zₜ = σ(W_z eₜ + U_z h + b_z) (update gate: how much of the old state to keep)
- rₜ = σ(W_r eₜ + U_r h + b_r) (reset gate: how much of the old state to use for the candidate)
- ñₜ = tanh(W_n eₜ + rₜ ⊙ (U_n h + b_n))
- hₜ = (1 − zₜ) ⊙ ñₜ + zₜ ⊙ h

One GRU runs left to right and another right to left, and hₜ = [h→ₜ ; h←ₜ] ∈ ℝ²ᵏ. Every word's vector now depends on the whole sentence, so "in X" and "called X" can mark X as a slot whatever X is.

**Intent head**: s = (1/n) Σₜ hₜ (mean pooling), p = softmax(W_i s + b_i) ∈ ℝ⁵¹.

**Slot head** (per token): qₜ = softmax(W_s hₜ + b_s) ∈ ℝ⁸.

**Loss** for one sentence: L = −ln p[intent] + (1/n) Σₜ −ln qₜ[tagₜ]. This is the same cross-entropy as `mnist`, once for the sentence and averaged over its words. Burn's autodiff computes ∂L/∂θ for every parameter, including through the GRU's time steps (backpropagation through time). Adam updates the parameters (lr = 2·10⁻³).

**Baseline**: x = word counts over the vocabulary (bag of words), p = softmax(W₂ ReLU(W₁x + b₁) + b₂). Word order is lost, so it cannot tag slots, and "sort files by size" looks the same as "size files by sort".

**Size**: E 64·V ≈ 25k, BiGRU 2 · 3 · (64·64 + 64·64 + 2·64) ≈ 50k, heads 128·51 + 128·8 ≈ 7.5k. That is about 83k parameters, roughly 330 KB as f32.

## Decisions

1. **Crate layout: library plus binary.** `intent/src/`: `data.rs` (read JSONL), `vocab.rs` (vocabulary, shape tokens, label lists), `batch.rs` (bucketing, tensors), `model.rs` (`JointModel`, `BagOfWords`), `train.rs` (loop, validation, best epoch), `metrics.rs`, `predict.rs` (`Predictor`, `Prediction`), `lib.rs`; `main.rs` holds the CLI. The Stage 3 agent will depend on the library.

2. **Reuse the dataset's tokenizer and templates by making `dataset` a library too.** Model, dataset and agent must tokenize identically, and prediction must fill templates exactly as the dataset did. So `dataset/src/lib.rs` exports `tokenize` and `template` (moved, not changed), and `intent` depends on `dataset` by path. A copy could drift silently; this cannot.
   *Alternative:* copy both modules, as `mnist` copied `Rng`. Rejected because a mismatch would quietly lower accuracy.

3. **`flex` backend, not `ndarray`.** `ndarray` is deprecated in Burn 0.22, and `flex` is its pure-Rust replacement with the same goal (CPU, no system libraries). Training uses `Device::flex().autodiff()`; evaluation and prediction use `Device::flex()`. README.md and docs/ARCHITECTURE.md are updated to say `flex`.

4. **Vocabulary from context words only, with shape tokens for everything else.** The vocabulary holds lowercased tokens tagged `O` that occur at least twice in the training split. Slot values are **never** added, even when they repeat. Any other token maps to a shape token:
   - `<path>`: contains `/` or starts with `~`
   - `<glob>`: contains `*`
   - `<num>`: only digits, `.` and `:` (numbers, IPv4 addresses)
   - `<file>`: contains a `.` followed by a letter (`notes.txt`, `.bashrc`, `github.com`)
   - `<word>`: anything else

   Plus `<pad>` (id 0, reserved). Training and test values therefore look alike to the model ("budget.xlsx" and "notes.txt" are both `<file>`), so it has to learn slots from context and shape, which is exactly what generalises. **Word dropout:** during training, each in-vocabulary context word is replaced by `<word>` with probability 0.05. The test set has words the grammar never produced ("heaviest", "footprint"), and this teaches the model not to fall apart on them.
   *Alternative:* one `<unk>` token. Rejected because it would erase the difference between a folder and a file name.

5. **Length-bucketed batches, no padding.** Examples are grouped by token count and batched within each group (batch size up to 32). Every batch is rectangular, so there are no masks, mean pooling is exact, and the reverse GRU never reads padding. Each epoch shuffles examples within buckets and the order of batches, using the seeded xorshift `Rng` (copied once more; extracting a shared utility crate is still deferred).

6. **Validation split, development set, and model selection.** A seeded, per-intent 10% of `train.jsonl` is held out as *validation*: it shows whether the model fits the grammar. After each epoch, **exact match on the hand-written development set** decides which epoch is kept, since it is the only signal of generalisation that may be used. Exact match means the intent and every slot tag are right. Intent accuracy alone peaks before the slot tagger learns anything, and validation scores about 99% on every epoch, so neither can tell epochs apart. Ties go to the later epoch. Training stops after 8 epochs without improvement. The test set is only read by `eval` and by the end-of-training report; it never influences training.

7. **Defaults:** d = 64, k = 64, epochs 40, batch 32, Adam lr 2·10⁻³, word dropout 0.05, seed 42, prediction threshold 0.7. These come from the survey's size estimate and standard practice for small RNN taggers; they may change only by looking at validation numbers.

8. **Saved artefacts** in `data/models/intent/` (git-ignored via `data/`):
   - `model.bpk`: joint model weights (BurnpackStore)
   - `meta.json`: vocabulary, intent names with command templates, tag names, d, k, seed, and the training metrics

   Burnpack already writes through a scratch file and renames it, so `.overwrite(true)` is enough for the weights; `meta.json` is written to a temporary name and renamed. The baseline is trained for comparison only and is not saved.

13. **Why a development set (added after the first run).** The first run scored 99.8% on validation and 48.8% intent accuracy on test. The bag-of-words baseline scored 47.5%, so the architecture was not the limit. 25% of the test's context words, in 134 of 162 sentences, never occur in training, so the model sees them all as `<word>`. Fixing that means widening the catalog's wording, and that needs held-out sentences that may legitimately be looked at. The dataset therefore gains `dev.jsonl`: hand-written like the test set, written without consulting it, using the test slot pools. Only development errors guide catalog and model changes; the test set is evaluated once at the end. Limitation: the same author wrote both sets, so the test is not fully independent; a test set written by someone else would be the stronger check.

9. **Prediction rules.**
   - Tokenize with the shared rule and keep at most 64 tokens.
   - Empty input returns `unknown`, confidence 0, reason "empty request", without running the model.
   - The intent is the argmax of p, and the confidence is that probability.
   - For each slot of the intent's template, take the token whose predicted tag is `B-<slot>`, choosing the one with the highest probability for that tag if several are. The value is the original token text, so case is kept. Tags for slots the template does not have are ignored.
   - No command is returned if the confidence is below the threshold (the reason lists the top 3 intents), if the intent is `unknown`, or if a required slot is missing. Otherwise the template is filled as in the dataset.

10. **Metrics** are one function used for validation, the training report and `eval`, so the numbers are always computed the same way. Command accuracy compares `Prediction.command` with the example's `command` at threshold 0 (pure model quality); the threshold table separately shows how accuracy rises as coverage falls.

11. **CLI:** `intent train [--data DIR] [--out DIR] [--epochs N] [--seed N]`, `intent eval [--data DIR] [--model DIR]`, `intent predict [--model DIR] [--threshold P] <text…>`, hand-parsed like the other crates. Exit status 2 for usage errors, 1 for missing files, 0 otherwise.

12. **Tests without the real dataset:** vocabulary and shapes, bucketing, slot decoding and command filling, and metrics on hand-built inputs. Also a tiny overfit test: about 8 synthetic sentences and 3 intents, trained for a fixed number of steps, must all be predicted correctly with correct slots. And a save, load and predict round trip in a temporary directory. These replace a hand-written gradient check, because Burn's autodiff computes the gradients; the overfit test proves they are wired correctly.

## Risks / Trade-offs

- [The test set is small (162) and deliberately paraphrastic, so 70% may be hard] → shape tokens and word dropout target exactly this. If the target is missed, tune only on validation numbers. Never edit the catalog to fit specific test sentences (proposal non-goal). Record the honest result.
- [Burn is a large dependency (compile time, lockfile size)] → minimal features (no `train`, `tui`, GPU or `fusion`). The release binary stayed at 4.7 MB in the prototype.
- [Burn 0.22 is new and its API differs from most online material] → the decisions above were checked against the 0.22.1 sources and a working prototype. Pin `=0.22.1`.
- [Mean pooling dilutes long sentences] → sentences are short (≤ 13 tokens). Max or attention pooling is a possible later experiment.
- [Determinism depends on single-threaded CPU kernels] → verified on the prototype. If `flex` ever parallelises reductions, the reproducibility check in tasks will catch it.

## Migration Plan

Additive, apart from the `dataset` library split (a file move with no behaviour change, proven by its unchanged tests and byte-identical generated files). Rollback is removing `intent` from the workspace.
