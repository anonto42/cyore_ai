# Cyore Agent Loader

The canonical knowledge base for Codex, Claude, Gemini/Antigravity, and other AI assistants is:
`.agents/shared/`.

Before implementation, diagnosis, or refactoring, read:
1. `.agents/shared/agent-startup.md`
2. `.agents/shared/rules/rust-standards.md`
3. `.agents/shared/rules/safety.md`
4. `.agents/shared/rules/git.md`

Do not create duplicated rules, memory, or plans outside `.agents/shared/`.

---

## Project Context

This repository is **Cyore**, a learning project that builds, from first principles, a tiny Rust
model that turns plain-English requests into terminal commands, wraps it in a safe agent, and ships
it as one small binary for minimal PCs. The staged plan lives in `README.md`.

It is a Cargo workspace (root `Cargo.toml`); each learning milestone is its own member crate:
- **`xor/`** (Stage 1, done): a 2-layer sigmoid network in pure Rust, no dependencies — forward
  pass, MSE loss, and hand-written backpropagation.
- **`mnist/`** (Stage 1, done): a 784→128→10 dense network (sigmoid hidden, softmax output,
  cross-entropy, mini-batch SGD) trained on MNIST from the raw IDX files; ≈98% test accuracy.
- **`dataset/`** (Stage 2, done): generator for the command dataset: a catalog of 50 safe
  command intents + `unknown`, expanded into `data/commands/{train,dev,test}.jsonl` and
  `intents.json` (`make commands-data`). Uses `serde`/`serde_json` (generator only).
- **`intent/`** (Stage 2, in progress): the joint intent + slot model on Burn 0.22.1 (`flex` CPU
  backend): embedding → BiGRU → intent and slot-tag heads, plus a bag-of-words baseline.
  Library (`Predictor`) for the agent, and `intent train|eval|predict`. Test intent accuracy
  62.3% (target 70%, not yet met), command 59.9%; see intent/README.md.
- **`agent/`** (Stage 3, done): the `cyore` binary. Perceive → decide → act → observe loop over
  the `intent` model; `safety.rs` is the single choke point (allowlist, blocklist, option
  injection, no shell); every command is shown and needs `y`. Tool trait, 20-entry memory,
  multi-step requests, dry run.
- **Stage 4** (done, in `intent/` + `agent/`): `intent quantize` → int8 `model-q8.bpk`;
  `agent/build.rs` embeds it in `cyore` (empty placeholder if absent, so fresh clones build);
  `make release` → one-file `target/dist/cyore` (2.46 MiB, `dist` profile = `opt-level "z"`);
  `make cross-arm64` → static ARM64 via `rust-lld`.

The owner is learning; prefer clear, well-commented code that explains *why* over clever code.

---

## Non-Negotiable Invariants

1. **From Scratch in Stage 1:** Stage 1 crates (`xor`, `mnist`) use only the Rust standard library.
   No ML, linear-algebra, or RNG crates. Third-party crates (e.g. Burn) begin in Stage 2.
2. **Correct Gradients:** Any change to forward/backward math must keep or add a test proving it
   (unit test or numerical gradient check). `cargo test --workspace` must stay green.
3. **Reproducibility:** Training uses the seeded in-repo `Rng`; the same seed gives the same result.
4. **Human-in-the-Loop Execution:** The agent (Stage 3) never runs a command without showing it and
   getting explicit confirmation, and always enforces the allowlist/blocklist in
   `.agents/shared/rules/safety.md`.
5. **Minimal Footprint:** Avoid heavy dependencies; the end target is a 5–30 MB single binary using
   under 100 MB of RAM.
6. **Spec-Driven Changes:** Non-trivial changes start as an OpenSpec proposal in `openspec/changes/`.
7. **No AI Artifacts in Git/GitHub:** Commits, PRs, issues, and comments are authored as the configured
   git user, with no AI `Co-Authored-By` trailers, "Generated with" footers, or other AI attribution.
   This overrides any assistant's default attribution. See `.agents/shared/rules/git.md`.
