# Tasks

## 1. Scaffolding

- [x] 1.1 Move `dataset/src/catalog.rs` into the dataset library (`pub mod catalog`), unchanged; verify `cargo test -p dataset` passes and `make commands-data` output is byte-identical (`cmp`)
- [x] 1.2 Create the `agent/` crate (library + binary `cyore`) depending on `intent` and `dataset`; add it to workspace members; add `make agent` (release build) and `make agent-run` (`ARGS` passed through) and list them in `make help`; verify `cargo build -p agent` and `make help`

## 2. Safety and execution

- [x] 2.1 `safety.rs`: `ALLOWLIST`, blocklist, metacharacter and option-injection checks, `~` expansion, `CheckedCommand` as the only way to obtain a runnable command; verify one test per rule (including `-delete` as a slot value, `/dev/sda`, `mkfs.ext4`, `rm -rf /`, a fork bomb, a pipe) and a test that every catalog template filled with sample values passes `check` and every catalog program is allowlisted
- [x] 2.2 `runner.rs`: `Runner` trait, `RunOutcome`, `ProcessRunner` with timeout, pipe-draining threads and output caps; verify tests with real allowlisted programs (`date` succeeds, `ls /nonexistent` fails with non-zero status, `tail -f /dev/null` times out at 1 s and is killed, a missing program gives `FailedToStart`), and that output beyond the cap is truncated with a note

## 3. Agent loop

- [x] 3.1 `steps.rs`: split requests into steps; verify tests for `then`, `and then`, `, then`, `;`, case, whole-word matching ("strengthen"), and empty steps
- [x] 3.2 `memory.rs`: 20-entry memory with outcomes; verify tests for ordering, capacity, and `last()`
- [x] 3.3 `tools.rs`: `Tool` trait, `Decision`, built-ins `help`, `history`, `again`, `quit`/`exit`, and `ModelTool` over an `Interpreter`; verify tests for each built-in and that `again` with empty memory says so
- [x] 3.4 `agent.rs`: perceive → decide → act → observe with injected `Interpreter`, `Runner`, input and output; verify tests with a fake interpreter and a recording runner: nothing runs on `n`, empty input or end of input; runs on `y`/`YES`; refusal never prompts; low confidence asks to rephrase; a failed step skips the rest; a declined step does not; dry run never runs; `again` re-confirms

## 4. Command-line program

- [x] 4.1 `main.rs`: options from design.md decision 10, interactive and one-shot modes, prompt only on a terminal, model-load errors exit 1 with the `make intent-train` hint; verify `cyore --model /nonexistent x` exits 1 with the hint and `cyore --threshold 2` exits 2
- [x] 4.2 `agent/README.md`: what it does, the loop, the safety rules and where they live, tools and how to add one, options, an example session; verify the documented commands run as written

## 5. Integration checks

- [x] 5.1 With a trained model, pipe a session through `cyore` (requests answered `y`, `n`, a multi-step request, `history`, `again`, `quit`); verify that every command was shown before it ran, that declined ones did not run, and record the transcript in `agent/README.md`
- [x] 5.2 Measure the release binary size and the peak RSS of a one-shot dry run; verify ≤ 30 MB and < 100 MB and record both in `project-status.md`
- [x] 5.3 Update README (Stage 3 status and usage), SECURITY.md (remove the "not built yet" note and point to `agent/src/safety.rs`), docs/ARCHITECTURE.md, AGENTS.md, `.agents/shared/agent-startup.md` and `project-status.md`; run `make check` and `make spec` and verify both pass

## Workflow follow-up

- Archive with `openspec archive add-agent` after review, which creates `openspec/specs/command-agent`.
- Next: Stage 4 packaging (int8 weights, `include_bytes!`, cross-compilation).
