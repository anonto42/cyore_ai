# Architecture

One Cargo workspace with one crate per learning milestone. Each crate stands alone, so the project reads in order: every stage reuses the *ideas* of the stage before it, not its code.

```
xor/      Stage 1   2 → 4 → 1 sigmoid network, MSE, per-example SGD     (std only, done)
mnist/    Stage 1   784 → 128 → 10 dense classifier, softmax + CE      (std only, done)
dataset/  Stage 2   command catalog → train/test JSONL with intents + BIO slot tags (done)
intent/   Stage 2   Burn embedding → BiGRU → intent + slot heads        (done)
agent/    Stage 3   `cyore`: confirm-before-run agent with a safety layer (done)
          Stage 4   int8 weights embedded with include_bytes!, one 2.5 MiB binary (done)
```

The root `Cargo.toml` lists the members and holds the shared release profile (`opt-level = 3`, `lto = true`, `codegen-units = 1`, `strip = true`, `panic = "abort"`).

## Dependency rule
Stage 1 crates depend on **nothing but `std`**, including their own random number generator. There is deliberately no shared `nn` library crate: `mnist` re-implements what it needs from `xor`, written again at a larger scale. Third-party crates start in Stage 2, and each new dependency must be justified against the 5–30 MB binary target. The first ones are `serde` and `serde_json`, used only by the `dataset` generator, which is a build-time tool and never ships in the agent binary.

---

## `xor`: the smallest network that learns
```
xor/src/
  activation.rs   sigmoid and its derivative (takes the OUTPUT s, not x)
  loss.rs         mse and mse_derivative
  layer.rs        dense Layer: forward (caches inputs/outputs), backward (updates, returns input grads)
  network.rs      hidden + output Layer, predict, train_step
  random.rs       xorshift64 Rng (seed 0 is remapped to a non-zero constant)
  data.rs         the four XOR examples
  main.rs         training loop and truth-table evaluation
```

### One training step
```
 x ──► hidden.forward ──► h ──► output.forward ──► ŷ ──► L = mse(ŷ, t)
                                                         │
 hidden.backward ◄── dL/dh ◄── output.backward ◄── dL/dŷ ┘
```

For each neuron `j` of a layer:

| Step | Formula |
|---|---|
| Forward | `z_j = b_j + Σ_i w_ji · x_i`, `a_j = σ(z_j)` |
| Loss | `L = (1/n) Σ_j (a_j − t_j)²`, `∂L/∂a_j = 2(a_j − t_j)/n` |
| Chain rule through σ | `δ_j = ∂L/∂a_j · a_j(1 − a_j)` |
| Gradient for previous layer | `∂L/∂x_i = Σ_j δ_j · w_ji` (using the weights **before** the update) |
| Update | `w_ji −= η · δ_j · x_i`, `b_j −= η · δ_j` |

`Layer::backward` accumulates the previous layer's gradient *before* it changes a weight. Swapping that order is a classic bug, and `test_backward_uses_pre_update_weights` exists to catch it.

---

## `mnist`: training on real data
```
mnist/src/
  config.rs       hand-parsed CLI options (--epochs, --lr, --batch, --hidden, --seed, --data)
  data.rs         IDX loader with validation (magic, length, 28×28 shape, counts, labels)
  matrix.rs       flat row-major Matrix: matvec, matvec_transposed, add_outer
  activation.rs   sigmoid and softmax
  loss.rs         cross-entropy
  network.rs      784 → 128 → 10 network: forward_into, backward, apply, predict
  metrics.rs      accuracy and confusion matrix
  random.rs       the same seeded xorshift64 Rng
  main.rs         load → train (mini-batches, per-epoch shuffle) → evaluate
```

New ideas compared with `xor`:
- **Softmax + cross-entropy**: The output-layer gradient simplifies to `ŷ − t`.
- **Mini-batch SGD** (B = 10, η = 0.5): `backward` *adds* into `Gradients`, then `apply` takes one averaged step and zeroes them.
- **Separated state**: `Network` holds only parameters, `Activations` hold one example's forward pass, and `predict(&self)` has no side effects.
- **Scaled initialisation**: Weights are uniform in ±1/√fan_in and biases start at 0. XOR's ±1 range would saturate 784-input sigmoids.
- **Honest evaluation**: Accuracy and a confusion matrix on the 10,000-image test set, which is never used for training.

Result with the defaults and seed 42: **97.96% test accuracy after 10 epochs**.

---

## The agent (Stages 2–4)
```
request ──► model ──► (command, confidence) ──► safety layer ──► "Run this? (y/n)" ──► executor
             │                                    │                                    │
   intent classifier + slots,      allowlist, blocklist, threshold     std::process::Command,
   or a tiny seq2seq transformer   (unsure → ask instead of guessing)   explicit args, timeout
```

- **Tool trait**: Each capability (run a command, read a file, ...) is one implementation, so new tools are easy to add.
- **Memory**: A short history of recent commands for multi-step tasks.
- **Packaging**: int8 weights (`intent quantize`, one scale per row, 99 KB) embedded with `include_bytes!` by `agent/build.rs` and loaded zero-copy with `BurnpackStore::from_static`, Burn's pure-Rust CPU (`flex`) backend, loaded once at start-up. Built with the `dist` profile (`opt-level = "z"`): 2.46 MiB, 10.2 MiB peak RAM. Static musl builds for x86_64 and ARM64.

The rules the agent must enforce are in [Safety Rules](../.agents/shared/rules/safety.md) and [SECURITY.md](../SECURITY.md). They are implemented in `agent/src/safety.rs` (a single `check` that is the only way to build a runnable `CheckedCommand`) and `agent/src/runner.rs` (no shell, timeout, output caps); see [agent/README.md](../agent/README.md).

---

## Add a milestone
1. Propose it in `openspec/changes/<name>/` (purpose, non-goals, invariants, the math in `design.md`), then run `make spec`.
2. Create the crate and add it to `members` in the root `Cargo.toml`.
3. Write the tests first for every numerical component, including a gradient check for each backward pass.
4. Run `make check`, then update `.agents/shared/memory/project-status.md` and the README progress table.
5. Add the binary to `BINS` in `.github/workflows/release.yml` if it should ship in releases.
