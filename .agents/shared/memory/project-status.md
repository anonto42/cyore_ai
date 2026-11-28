# Project Status & Design Decisions

Last updated: 2026-11-28

## Milestones (see README.md for the full plan)

| Stage | Milestone | Status |
|---|---|---|
| 1 | XOR network in pure Rust (`xor/`) | Done — converges to MSE ≈ 3e-5 in 20k epochs, seed 42 |
| 1 | MNIST digit recogniser (`mnist/`) | Done — 97.96% test accuracy after 10 epochs (defaults, seed 42), ≈6 s/epoch release on a 12-thread laptop, peak RAM ≈430 MB |
| 1 | Survey architectures (dense, CNN, RNN, transformer) | Next (study, no code) |
| 2 | Command dataset generator (`dataset/`) | Done — 51 intents, 7,650 train / 162 dev / 162 test examples (hand-written held-out sets), seed 42 |
| 2 | Burn intent + slot model (`intent/`) | Built; test intent 62.3% / command 60.5% (target 70% / 60%): intent target not met. ~18 s training, 4.8 MB binary, 356 KB weights |
| 2 | Tiny seq2seq transformer | Not started |
| 3 | Rust agent `cyore` (`agent/`) | Done — confirm-before-run loop, safety choke point, tools, memory, multi-step; 4.1 MiB binary, 10.7 MiB peak RSS (one-shot dry run, getrusage) |
| 4 | int8 quantisation, `include_bytes!` weights, cross-compile | Done — int8 97 KB (28% of f32, identical dev/test metrics); one-file `cyore` 2.46 MiB (`dist`, opt-level z), 10.2 MiB peak RSS, ~6 ms per request; static musl x86_64 4.31 MiB (release) and ARM64 1.98 MiB (dist) |

## `xor` design

- Architecture 2 → 4 → 1, sigmoid everywhere, MSE loss, per-example SGD, learning rate 1.0.
- `Layer::forward` caches inputs and outputs for backprop, so `Network::predict` takes `&mut self`.
- `Layer::backward` accumulates the previous layer's gradient using the weights *before* updating
  them (correct ordering — keep it that way).
- RNG: xorshift64 in `random.rs`; seed 0 is remapped to a non-zero constant.

- Tests cover every spec scenario: RNG seeding, layer init, pre-update-weight backprop, a numerical
  gradient check of the whole network, and a 20k-epoch convergence test.

## `mnist` design

- 784 → 128 (sigmoid) → 10 (softmax), cross-entropy, mini-batch SGD (B = 10, η = 0.5), 10 epochs.
- `Network` holds only parameters; `forward_into` fills `Activations`, `backward` *adds* into
  `Gradients`, `apply` steps and zeroes them. `predict(&self)` has no side effects.
- Weights uniform in ±1/√fan_in, biases 0 (XOR's ±1 would saturate 784-input sigmoids).
- Flat row-major `Matrix` (`matvec`, `matvec_transposed`, `add_outer`).
- Same seed → byte-identical per-epoch loss and accuracy (verified with two `--epochs 2` runs).
- Dataset: `make mnist-data` downloads from `storage.googleapis.com/cvdf-datasets/mnist` into
  git-ignored `data/mnist/`; the loader validates magic, length, 28×28 shape, counts, labels.

## `dataset` design

- Catalog in `dataset/src/catalog.rs`: 50 read-only/harmless command intents + `unknown` (which
  also absorbs destructive requests like "delete all my files"). No `rm`/`mv`/`kill`/`sudo`/pipes.
- Phrasing grammar `(a|b) [x] {slot}`; lines tagged `do:`/`ask:` get matching wrappers at
  sampling time. Each intent ≥ 20 distinct expansions (3,386 total).
- Slot pools split train/test; test sentences are hand-written with unseen values, so test
  accuracy measures generalisation. Validation runs before any file is written.
- Tokenization rule (shared with the future model and agent): whitespace split, strip trailing
  `?!,` from each token and `.` from the last.
- Hard cases on purpose: `list_by_size`/`dir_size`, `processes`/`top_cpu`, `whoami`/`logged_in_users`,
  `git_status`/`git_diff`, `os_info`/`kernel_version`.

## `intent` results and lessons (seed 42)

| Run | validation intent | dev intent / command | test intent / command |
|---|---|---|---|
| 1. first run (4,080 train, no dev set) | 99.8% | – | 48.8% / 45.7% |
| 2. + dev set for epoch selection | 99.3% | 76.5% / 71.6% | not looked at |
| 3. + line-first sampling, contractions, 150/intent | 99.9% | 77.8% / 75.9% | not looked at |
| 4. + widened grammar (general synonyms, from dev errors) | 99.6% | 96.9% / 95.7% | **62.3% / 60.5%** |

- Bag-of-words baseline on test: 47.5% (run 1) → 67.9% (run 4), better than the joint model's
  62.3%. With this little data, vocabulary coverage matters more than architecture.
- Validation (same grammar as training) is ~99% throughout and useless for selection. The dev
  set is now optimistic, because the grammar was widened to fix its errors. Test was looked at
  only in runs 1 and 4.
- Caveat: the same author wrote the dev and test sentences, so the test is not fully
  independent. A test set written by someone else is the stronger check.
- At threshold 0.7: 71% of test requests are answered, 77% of those correctly. 10/12
  out-of-scope test requests get no command; the 2 that slip through map to harmless `pwd`/`lsblk`.
- `add-agent` and `add-packaging` archived 2026-11-28. `add-intent-model` stays open by the
  owner's decision: the 70% test-intent target is kept, not lowered.
- `add-intent-model` is still open: its 70% test-intent target was missed, and lowering it needs
  the owner's explicit decision.
- Next lever: more varied *human-written* training sentences (or paraphrase augmentation), not
  a bigger network. Do not edit the catalog based on test errors.

## `agent` design

- `safety::check` is the only constructor of `CheckedCommand`; `Runner::run` only takes that type.
- Order: blocklist → allowlist → shell metacharacters → option injection (slot value starting
  with `-`) → `~` expansion. Allowlist is owned by the agent and tested against every catalog
  command filled with real slot values.
- `ProcessRunner`: stdin closed, stdout/stderr drained on threads, `try_wait` polling, kill at
  timeout, 200 lines / 64 KB per stream.
- A failed, timed-out, unstartable or refused step stops the rest of a multi-step request; a
  declined step does not.
- Tests inject a fake `Interpreter` and a recording `Runner`; "nothing runs without y" is tested
  with seven non-yes answers plus end of input.

## Stage 4 packaging

- `intent quantize`: Burn `Quantizer` (MinMax, `QuantValue::Q8S`, per tensor) → `model-q8.bpk`;
  deterministic (byte-identical on re-run).
- `agent/build.rs` copies `model-q8.bpk` + `meta.json` into OUT_DIR (empty placeholders if
  missing); `embedded.rs` uses `include_bytes!`/`include_str!` and `Predictor::from_static`.
  Selection: `--model DIR` > embedded > `data/models/intent`.
- `[profile.dist]` (inherits release, opt-level "z"): cyore 4.20 → 2.46 MiB, same speed (5.5 vs
  5.7 ms) and output. Training keeps opt-level 3.
- Cross: `rust-lld` links static musl for aarch64 and x86_64 without a C cross toolchain. The
  ARM64 binary is verified by `file` only (no emulator here); CI's ARM runner runs its tests.
- Release workflow: tests (no model) → dataset → train → quantize → build `BINS`
  (`package:binary:profile`).
- `make release` from scratch takes ~83 s.

## Known follow-ups

- `mnist` keeps all pixels as `f64` (≈430 MB). Storing `u8` and normalising per example would cut
  that to ≈60 MB if memory ever matters for training.
