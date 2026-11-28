# Changelog

All notable changes to Cyore. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and versions follow [Semantic Versioning](https://semver.org/) as described in [docs/RELEASES.md](docs/RELEASES.md).

## [Unreleased]

## [0.5.0] - 2026-11-28

The first release: all four stages of the learning plan, from a hand-written XOR network to
a single-file agent.

### Stage 1: how models are built (`xor/`, `mnist/`, standard library only)
- XOR network: 2 → 4 → 1 sigmoid network with hand-written backpropagation. It converges to an
  MSE of about 3e-5. Tests cover seeding, the backward pass, a numerical gradient check, and
  convergence.
- MNIST digit classifier: 784 → 128 → 10 with softmax, cross-entropy and mini-batch SGD, read
  straight from the IDX files. 97.96% test accuracy in 10 epochs. `make mnist-data`, `make mnist`.

### Stage 2: our own model (`dataset/`, `intent/`)
- Command dataset generator: a catalog of 50 safe command intents plus `unknown`, a phrasing
  grammar, everyday variants, command-name phrasings, and validation. It writes 7,650 training,
  267 development and 162 test examples (`make commands-data`).
- Joint intent and slot model on Burn 0.22 (`flex` CPU backend): embedding → BiGRU → intent
  and slot-tag heads, with a bag-of-words baseline. Test results: intent 62.3%, command 59.9%,
  slot tags 97.8%. **Known limitation:** the 70% test intent-accuracy target is not met (see
  `intent/README.md`).
- Human-written test set format (`dataset/human_test.txt`) with report-only scoring
  (`make intent-eval-human`), plus evaluation integrity rules for contributors and assistants.

### Stage 3: the agent (`agent/`, binary `cyore`)
- Perceive → decide → act → observe loop. Every command is shown and runs only after an
  explicit `y`.
- A single safety choke point: allowlist, blocklist, option-injection check, no shell, `~`
  expansion, timeouts, output caps.
- Multi-step requests, memory of the last 20 commands, a `Tool` trait with `help`, `history`,
  `again` and `quit`, and interactive, one-shot and dry-run modes.

### Stage 4: one small binary (`make release`, `make cross-arm64`)
- int8 weights with one scale per row: 99 KB, matching f32 accuracy on the test set.
- The model is embedded in `cyore` with `include_bytes!`. The 2.46 MiB single file peaks at
  10.7 MiB of RAM and answers a request in about 6 ms.
- Static musl builds for x86_64 and ARM64 (1.98 MiB, Raspberry Pi), and a release workflow for
  Linux x86_64/ARM64, Windows and macOS that trains, quantizes and ships `cyore`.

### Known limitations
- `cyore` understands only the 50 commands in the catalog, and misreads about a third of
  unfamiliar phrasings. That is why every command needs confirmation.
- Slot values are single words (no paths with spaces).
- On Windows the build runs, but its commands are Unix tools.

[Unreleased]: https://github.com/anonto42/cyore_ai/compare/v0.5.0...HEAD
[0.5.0]: https://github.com/anonto42/cyore_ai/releases/tag/v0.5.0
