# agent: `cyore`, the Stage 3 agent

Type what you want in plain English. Cyore asks the Stage 2 model (`intent/`) for a command,
checks it against the safety rules, **shows it, and runs it only if you answer `y`**.

```text
$ cyore
Cyore: tell me what you want to do (`help` for tools, `quit` to leave).
cyore> show me the files in agent
  $ ls -la agent        (list_files, confidence 0.99)
Run this? (y/n) y
total 16
drwxrwxr-x  3 md md 4096 Nov 28 20:49 .
drwxrwxr-x 16 md md 4096 Nov 28 20:49 ..
-rw-rw-r--  1 md md  432 Nov 28 20:49 Cargo.toml
drwxrwxr-x  2 md md 4096 Nov 28 20:52 src
[done]
cyore> how much disk space is left
  $ df -h        (disk_space, confidence 1.00)
Run this? (y/n) n
Skipped.
cyore> where am i and then what time is it
Step 1/2: where am i
  $ pwd        (current_dir, confidence 1.00)
Run this? (y/n) y
/home/md/Code/Ananto/cyore-ai
[done]
Step 2/2: what time is it
  $ date        (date, confidence 1.00)
Run this? (y/n) y
Sat Nov 28 08:53:24 PM +06 2026
[done]
cyore> history
  1. ls -la agent   (ok)
  2. df -h   (declined)
  3. pwd   (ok)
  4. date   (ok)
cyore> again
  $ date        (repeating the last command)
Run this? (y/n) y
Sat Nov 28 08:53:24 PM +06 2026
[done]
cyore> delete everything in my home folder
I'm not sure what you mean. My best guesses: current_dir (0.43), unknown (0.35), dir_size (0.07). Could you rephrase?
cyore> count the lines in Cargo.toml
  $ wc -l Cargo.toml        (count_lines, confidence 1.00)
Run this? (y/n) y
11 Cargo.toml
[done]
cyore> quit
```

This is a real session with the trained model (seed 42). The answers are shown as typed.

## Run it

```sh
make commands-data && make intent-train       # the model (once)
make agent                                    # builds ./target/release/cyore
./target/release/cyore                        # interactive
./target/release/cyore show me the files          # one request, then exit
./target/release/cyore --dry-run show me the files  # show and check, never run
make agent-run ARGS="how much disk space is left"
```

| Option | Meaning | Default |
|---|---|---|
| `--model DIR` | trained model directory | `data/models/intent` |
| `--threshold P` | minimum model confidence to propose a command | 0.7 |
| `--timeout SECS` | kill a command still running after this long | 10 |
| `--dry-run` | show and check commands, never run them | off |

Exit status: 0 on success, 1 if the model is missing or a one-shot command failed, 2 for a
usage error. The `cyore>` prompt appears only when input is a terminal, so piping requests in
(`printf 'show me the files\ny\n' | cyore`) gives clean output.

## How it works: perceive → decide → act → observe

| Step | What happens | Code |
|---|---|---|
| perceive | split the request into steps at `then` / `and then` / `;` | `steps.rs` |
| decide | the first tool that claims a step decides; the model is the last tool | `tools.rs` |
| act | safety check → show the command → `Run this? (y/n)` → run it | `agent.rs`, `safety.rs`, `runner.rs` |
| observe | show output and exit status, remember it, stop the remaining steps if it failed | `agent.rs`, `memory.rs` |

This is the same loop large LLM agents run. The difference is that the "decide" step here is a
350 KB model that can only choose among 50 known commands.

## The safety rules and where they are enforced

All in `src/safety.rs`. `check` is the only way to create a `CheckedCommand`, and the runner
only accepts a `CheckedCommand`, so the compiler guarantees nothing unchecked ever runs.

1. **Confirmation.** Only `y` or `yes` runs a command. Anything else, an empty line, or end of
   input skips it. There is no "yes to all". (`agent.rs`, `confirm`)
2. **Allowlist.** 37 programs (`ls`, `df`, `git`, …). A test checks that every command in the
   dataset catalog, filled with real slot values, passes the check.
3. **Blocklist.** `rm`, `mkfs`/`mkfs.*`, `dd`, `shutdown`, `reboot`, `halt`, `poweroff`, `sudo`,
   `su`, `/dev/sd…`, fork bombs, `chmod -R 777 /`, `rm -rf /`. Refused whatever the model's
   confidence, and before you are ever asked.
4. **Option injection.** A value taken from your request that starts with `-` is refused, so
   "find files in -delete" cannot become `find -delete …`.
5. **No shell.** Programs get their arguments directly (`std::process::Command`), so `*.txt` is
   passed to `find` as is, and `|`, `>`, `;`, `&`, `$(` and backticks are refused outright. A
   leading `~/` is expanded to `$HOME` by Cyore itself.
6. **Uncertainty.** Below the confidence threshold, for `unknown` requests, or with a missing
   value, nothing is proposed. Cyore says why and lists its best guesses.
7. **Timeouts and failures.** Every command is killed after the timeout. Commands get no
   input (stdin is closed). Output is capped at 200 lines and 64 KB per stream. A failure,
   timeout or missing program is reported, never a crash, and it stops the remaining steps of
   that request. A *declined* step does not stop them, because you chose to skip it.

## Tools

```rust
pub trait Tool {
    fn name(&self) -> &str;
    fn help(&self) -> &str;                               // one line for `help`
    fn claims(&self, step: &str) -> bool;                 // first claim wins
    fn decide(&self, step: &str, memory: &Memory) -> Decision; // Say / Propose / Quit
}
```

Built in: `help`, `history` (last 20 commands and what happened), `again` (re-propose the last
command; you confirm again), `quit`/`exit`. Then `ModelTool`, which claims everything else.

**To add a tool**, implement `Tool` and add it to the list in `Agent::new` before `ModelTool`. A
tool can only *propose* a command: the agent still runs it through the safety check and asks
you, so a new tool cannot weaken the rules.

## Size

The release binary is 4.1 MiB, the model 356 KB, and peak memory for a one-shot request is
10.7 MiB. The Stage 4 targets are ≤ 30 MB and < 100 MB.

## Limits

- It understands the 50 commands in the dataset and nothing else, and about a third of
  unfamiliar phrasings are misread (the model's test accuracy is 62%). That is why every
  command is shown and confirmed.
- Even simple wording can fall outside the grammar: "show my files" gets *not sure*
  (list_files 0.41), while "show me the files" gets `ls -la` (0.99). Cyore then asks you to
  rephrase instead of guessing, which is the threshold doing its job. The fix belongs in the
  dataset's grammar, not here.
- Slot values are single words, so paths with spaces cannot be expressed yet.
- A timed-out command is killed, but programs it started are not (no process groups without
  platform-specific code).
