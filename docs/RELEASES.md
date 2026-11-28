# Releases and Packaging

How Cyore is versioned, built for each platform, and published.

---

## 1. Versioning

Cyore follows **Semantic Versioning** (`vMAJOR.MINOR.PATCH`). Before `v1.0.0`, minor versions track the learning plan:

- `MINOR`: A new milestone crate or capability (for example, MNIST, the intent classifier, the agent).
- `PATCH`: Bug fixes, better tests, and documentation improvements.
- `MAJOR` (`v1.0.0`): The packaged Stage 4 agent: one binary with embedded int8 weights.

### Creating a Release Tag
```bash
# Make sure the working tree is clean and every check passes
make check
make spec

# Create an annotated git tag
git tag -a v0.2.0 -m "v0.2.0: MNIST digit classifier in pure Rust"

# Push the tag to GitHub
git push origin v0.2.0
```

Pushing a tag that matches `v*.*.*` starts `.github/workflows/release.yml`.

---

## 2. What the Release Workflow Builds

Each target runs on its own native GitHub runner. The workflow runs the full test suite (without a model, which proves a fresh clone builds), then generates the seeded dataset, trains and quantizes the model, and builds every binary listed in `BINS` (`package:binary:profile`). `cyore` uses the size-optimised `dist` profile and has the int8 model built in. Each archive also contains `README.md` and `LICENSE`:

| Target | Runner | Archive |
|---|---|---|
| `x86_64-unknown-linux-musl` | `ubuntu-latest` | `cyore-<tag>-x86_64-unknown-linux-musl.tar.gz` (static) |
| `aarch64-unknown-linux-musl` | `ubuntu-24.04-arm` | `cyore-<tag>-aarch64-unknown-linux-musl.tar.gz` (static; Raspberry Pi 3/4/5 on a 64-bit OS) |
| `x86_64-pc-windows-msvc` | `windows-latest` | `cyore-<tag>-x86_64-pc-windows-msvc.zip` |
| `aarch64-apple-darwin` | `macos-latest` | `cyore-<tag>-aarch64-apple-darwin.tar.gz` |

A final job collects every archive, writes `SHA256SUMS.txt`, and publishes a GitHub Release with auto-generated release notes.

To ship a new milestone binary, add a `package:binary:profile` entry to `BINS` at the top of the workflow.

To build the same single-file `cyore` locally: `make release` (host) or `make cross-arm64` (static Linux ARM64, linked by `rust-lld` with no cross C toolchain).

On Windows `cyore` builds and runs, but the commands it knows (`ls`, `df`, …) are Unix tools, so most report "could not start".

---

## 3. Container Image

The `Dockerfile` builds a fully static musl binary for one crate and copies it into an empty `FROM scratch` image that runs as an unprivileged user:

```bash
make docker-build CRATE=xor
make docker-run CRATE=xor
```

---

## 4. Footprint Budget

Stage 4 targets a **5–30 MB** binary that uses **under 100 MB of RAM**. The shipped `cyore` comes in well under it:

| Build | Size | Peak RAM | One request (load + answer) |
|---|---|---|---|
| `cyore`, `release` profile (`opt-level = 3`), x86_64 | 4.20 MiB | 10.7 MiB | 5.5 ms |
| `cyore`, `dist` profile (`opt-level = "z"`), x86_64 | **2.46 MiB** | **10.7 MiB** | 5.7 ms |
| `cyore`, `dist`, static musl ARM64 | 1.98 MiB | not measured (no ARM machine) | not measured |
| int8 model inside it (one scale per row) | 99 KB (f32: 342 KB) | | |

The release profile in the root `Cargo.toml` uses `lto = true`, `codegen-units = 1`, `strip = true` and `panic = "abort"`. The `dist` profile inherits it with `opt-level = "z"`: 41% smaller with the same speed, so it is used for `cyore`. Training crates keep `opt-level = 3`. Check any build with `make size CRATE=xor`.

---

## 5. Verifying Downloads

Every release includes `SHA256SUMS.txt`. Verify an archive before you use it:

```bash
sha256sum -c SHA256SUMS.txt --ignore-missing
```
