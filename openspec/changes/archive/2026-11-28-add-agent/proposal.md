# Proposal

## Why

Stage 3 of the learning plan (README.md) connects the model to an agent: read a request, run the model, show the command, ask `Run this? (y/n)`, run it, and show the output, with a safety layer, timeouts, multi-step tasks, memory, and a `Tool` trait. The Stage 2 model (`intent/`) can already turn a request into a command, but nothing runs it yet. SECURITY.md lists guarantees the agent "must meet", and no code enforces them today. This change builds the agent that does.

## What Changes

- Add a workspace crate `agent/` that builds the `cyore` binary, the program Stage 4 will package:
  - interactive mode (`cyore`): a prompt loop that reads requests until `quit` or end of input
  - one-shot mode (`cyore show my files`): handle one request and exit
  - `--dry-run`: show what would run, never execute
  - options for the model directory, confidence threshold, and command timeout
- **Perceive → decide → act → observe** loop:
  - *perceive*: read a request, split it into steps on "then" or `;`
  - *decide*: a built-in tool claims the request, or the model predicts a command
  - *act*: the safety layer checks the command, it is shown, the user confirms, and it runs
  - *observe*: show its output and exit status, remember it, and stop the remaining steps if it failed
- A **safety layer** enforced in code: a program allowlist, the blocklist from `.agents/shared/rules/safety.md`, refusal of slot values that look like options (`-delete`), no shell (`std::process::Command` with explicit arguments, with `~` expanded by the agent), a timeout that kills the process, capped output, and the confidence threshold (below it, ask the user to rephrase and show the top guesses).
- A **`Tool` trait** with built-in tools: `help`, `history` (recent commands and outcomes), `again` (propose the last command again, which still needs confirmation) and `quit`. The model path is itself a tool, so a new capability is one new implementation.
- **Memory** of the last 20 commands with their outcomes.
- `make agent` / `make agent-run` targets; README, SECURITY.md (the "not built yet" note), ARCHITECTURE.md, AGENTS.md, and `project-status.md` updates.

### Non-goals

- No auto-run, "yes to all", or background execution: every command is confirmed individually.
- No shell pipelines, redirection, `sudo`, or commands outside the dataset's 50 intents.
- No LLM, network calls, or learning from use. The agent only uses the trained `intent` model.
- No embedded weights, quantisation, or cross-compilation (Stage 4). The model is loaded from `data/models/intent/`.
- No change to the model or the dataset's commands. Because the dataset's catalog moves into its library (see Impact), the allowlist can be tested against it.

### Invariants

- Learning-plan stage: Stage 3.
- No code path runs a command without printing it and reading an explicit `y`/`yes` from the user first. End of input counts as "no".
- The agent never panics on any input, model output, or command failure.
- `make check` stays green without a trained model (tests use a fake model and a fake process runner).
- Existing crates keep their behaviour; generated dataset files stay byte-identical.

## Capabilities

### New Capabilities
- `command-agent`: the `cyore` program. It turns requests into confirmed, safety-checked command executions, supports multi-step requests, keeps memory and built-in tools, and has interactive, one-shot and dry-run modes.

### Modified Capabilities

None.

## Impact

- New: `agent/` crate (binary `cyore`), workspace member.
- Changed: `dataset` (the catalog module moves from the binary into the library, unchanged, so the agent's allowlist can be tested against every catalog command), root `Cargo.toml`, `Cargo.lock`, `Makefile`, docs listed above.
- Dependencies: only the workspace's own `intent` and `dataset` crates (and Burn through `intent`). No new third-party crates; timeouts use `std::thread` and `std::process`.
- Footprint: the binary is expected to be about 5 MB (the `intent` binary is 4.8 MB). Peak RAM is measured against the < 100 MB target.
