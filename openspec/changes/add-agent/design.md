# Design

## Context

- `intent::predict::Predictor::load(dir)?.predict(text, threshold) -> Prediction` already applies the threshold, `unknown` and missing-slot rules. It returns `command: Option<String>` (whitespace-separated arguments; slot values never contain whitespace), `slots`, `confidence`, `top`, and `reason`.
- The dataset's commands are 50 read-only or harmless templates over 37 programs, with no shell syntax. The catalog lives in the `dataset` binary, so other crates cannot see it yet.
- Safety rules: `.agents/shared/rules/safety.md` and SECURITY.md. Model output is untrusted input.

## Goals / Non-Goals

**Goals:**
- The four agent steps (perceive, decide, act, observe) are visible in the code as four functions.
- Every safety rule is enforced at one choke point that every command passes through, and each rule has a test.
- The whole loop is testable without a model, a terminal, or real processes.

**Non-Goals:**
- Fancy terminal UI (colours, line editing, history keys). Plain `stdin`/`stdout` keeps the binary small and the tests simple.

## Decisions

1. **Crate `agent/`, binary `cyore`.** Library modules plus a thin `main.rs`:
   - `steps.rs`: split a request into steps
   - `safety.rs`: allowlist, blocklist, option-injection check, `~` expansion
   - `runner.rs`: the `Runner` trait and `ProcessRunner` (spawn, timeout, capture, cap)
   - `memory.rs`: recent commands and outcomes
   - `tools.rs`: the `Tool` trait and built-ins
   - `agent.rs`: the loop
   - `main.rs`: the CLI

   The program is called `cyore` because it is the product; `agent` names the milestone.

2. **Seams for testing: an `Interpreter` and a `Runner`.** `trait Interpreter { fn interpret(&self, text: &str) -> Prediction }` is implemented for the real `Predictor` (with its threshold), and tests implement it with a lookup table. `trait Runner { fn run(&self, cmd: &CheckedCommand, timeout: Duration) -> RunOutcome }` is implemented by `ProcessRunner`, and tests record calls instead of spawning. The `Agent` takes `&mut dyn BufRead` for input and `&mut dyn Write` for output. So the key safety property, "nothing runs without a `y`", is a unit test that counts runner calls.

3. **One choke point: `CheckedCommand`.** `safety::check(command: &str, slots) -> Result<CheckedCommand, Refusal>` is the only constructor, and `Runner::run` only accepts a `CheckedCommand`. So the type system guarantees that no unchecked string can be executed. The checks run in order:
   1. split into arguments
   2. the program is on the allowlist
   3. no blocklisted program or pattern appears in any argument
   4. no shell metacharacters (`|`, `>`, `<`, `;`, `&`, `$(`, backtick) remain, as a defence even though there is no shell
   5. no slot value starts with `-`
   6. expand a leading `~` with `$HOME` (refused if `HOME` is unset)

4. **Allowlist owned by the agent and tested against the catalog.** The agent keeps its own `ALLOWLIST`: the safety layer must not trust data files. A test iterates over every catalog template and asserts that its program is allowed and that a filled example passes `check`. That needs the catalog to be visible, so `dataset/src/catalog.rs` moves into the dataset library (`pub mod catalog`), unchanged. The proof is that the dataset's tests pass and its generated files are byte-identical.

5. **Timeouts with the standard library only.** `ProcessRunner` spawns with piped stdout and stderr and stdin set to null (commands cannot wait for input). Two threads drain the pipes so a chatty process never blocks on a full pipe, while the main thread polls `try_wait` every 10 ms until the deadline, then calls `kill` and `wait`. Output is decoded with `String::from_utf8_lossy` and capped at 200 lines and 64 KB per stream, with a "… N more lines" note. A spawn error (for example, program not installed) becomes `RunOutcome::FailedToStart(message)`.

6. **Step splitting on tokens.** Split on `;` first, then on the whole word `then`, case-insensitive, dropping a directly preceding `and` and a trailing comma on the word before. Empty steps are dropped. "Strengthen" or "athena" are not split because only whole words match. The rule is simple on purpose; the model never sees the conjunctions.

7. **Tools.** The trait:

   ```rust
   trait Tool {
       fn name(&self) -> &str;
       fn help(&self) -> &str;
       /// Claim the step before the model sees it (e.g. the exact word "history").
       fn claims(&self, step: &str) -> bool;
       fn decide(&self, step: &str, memory: &Memory) -> Decision;
   }
   enum Decision { Say(String), Propose { command: String, slots: Slots }, Quit }
   ```

   Built-ins match exact lowercased words: `help`, `history`, `again`, `quit`/`exit`. The model is the fallback tool (`ModelTool`), which claims everything. A *proposed* command always goes through the same check, show, confirm and run path, so a tool cannot bypass safety: tools decide, only the agent acts.

8. **Memory:** a ring of 20 `Entry { command, outcome }`, where outcome is `Ran(exit code)`, `TimedOut`, `FailedToStart`, or `Declined`. Refused commands are not remembered, because they were never proposed to the user. `again` re-proposes the newest entry's command with its original slots, so it is checked again.

9. **Observe → continue or stop.** A non-zero exit, a timeout or a start failure marks the step failed. Remaining steps of the same request are skipped with "skipped the remaining N step(s) because the last command failed". A decline only skips that step: the user chose not to run it, which is not a failure.

10. **CLI:** `cyore [--model DIR] [--threshold P] [--timeout SECS] [--dry-run] [request…]`, hand-parsed like the other crates. Exit status: 2 for usage, 1 if the model cannot be loaded, otherwise 0 (one-shot returns 1 if its command failed). The prompt is `cyore> ` and is printed only when stdin is a terminal, detected with `std::io::IsTerminal`, so piped input gives clean output for tests and scripts.

## Risks / Trade-offs

- [The model is wrong about a third of the time on unseen phrasings (`intent` test accuracy 62.3%)] → this is exactly why confirmation is mandatory and below-threshold requests are handed back. The worst a wrong guess can do is propose a different harmless, read-only command, which the user then declines.
- [`mkdir -p` and `touch` change the file system] → harmless by construction (no overwrite or delete), still confirmed, and the option-injection check stops `-`-prefixed values.
- [Paths with spaces cannot be expressed: slot values are single tokens] → accepted for now; documented.
- [Polling `try_wait` adds up to 10 ms latency] → negligible for interactive use.
- [Killing a timed-out process does not kill its children] → the allowlisted programs do not spawn children (except `git`, which runs to completion quickly). Process groups need platform APIs or `unsafe`; deferred.

## Migration Plan

Additive. Rollback is removing `agent` from the workspace members; the catalog move is behaviour-neutral.
