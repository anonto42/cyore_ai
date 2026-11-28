# Spec Delta

## Purpose

The `cyore` agent: it reads plain-English requests, lets the trained model propose terminal commands, enforces the project safety rules in code, and runs a command only after showing it and getting explicit confirmation. This is the Stage 3 milestone of the learning plan.

## ADDED Requirements

### Requirement: Confirmation before every execution
The agent SHALL print the exact command and ask `Run this? (y/n)` before running it, and SHALL run it only if the answer is `y` or `yes` (case-insensitive, surrounding spaces ignored). Any other answer, including an empty line or end of input, SHALL skip the command.

#### Scenario: Confirmed
- **WHEN** the model proposes `ls -la Pictures` and the user answers `y`
- **THEN** the command runs and its output is shown

#### Scenario: Declined
- **WHEN** the user answers `n`, anything other than `y`/`yes`, or input ends
- **THEN** the command is not run and the agent says it was skipped

#### Scenario: Dry run
- **WHEN** the agent runs with `--dry-run`
- **THEN** commands are shown and checked but never run, and no confirmation is asked

### Requirement: Allowlist
The agent SHALL run only commands whose program is on its allowlist, and every command template in the dataset catalog SHALL use an allowlisted program.

#### Scenario: Program not allowed
- **WHEN** a proposed command's program is not on the allowlist
- **THEN** the agent refuses it, says why, and does not ask for confirmation

### Requirement: Blocklist and argument safety
Whatever the model's confidence, the agent SHALL refuse a command that matches the safety blocklist: the programs `rm`, `mkfs` (and `mkfs.*`), `dd`, `shutdown`, `reboot`, `halt`, `poweroff`, `sudo`, `su`; any argument containing `/dev/sd`; a fork bomb `:(){`; `chmod -R 777 /`; or `rm -rf /`. It SHALL also refuse a command whose slot value starts with `-`, because such a value would be read as an option (for example `find -delete …`).

#### Scenario: Option injection through a slot
- **WHEN** the request supplies a slot value such as `-delete`
- **THEN** the command is refused before confirmation, with a reason naming the value

#### Scenario: Blocklisted pattern
- **WHEN** a proposed command contains `/dev/sda` as an argument
- **THEN** the command is refused before confirmation

### Requirement: No shell
The agent SHALL execute a command by passing its program and arguments directly to the operating system, never through a shell. It SHALL expand a leading `~` or `~/` in an argument to the user's home directory, and SHALL pass every other character literally.

#### Scenario: Glob passed literally
- **WHEN** the confirmed command is `find . -name *.txt`
- **THEN** the program `find` receives the argument `*.txt` unchanged

### Requirement: Uncertain and unsupported requests
The agent SHALL NOT propose a command when the model's confidence is below the threshold (default 0.7), when the intent is `unknown`, or when a required slot is missing. Instead it SHALL explain why and, when unsure, show the top guesses and ask the user to rephrase.

#### Scenario: Low confidence
- **WHEN** the model's confidence for a request is 0.4
- **THEN** no command is shown or run, and the agent asks the user to rephrase, listing its top guesses

### Requirement: Timeouts and graceful failure
Every executed command SHALL have a timeout (default 10 seconds) after which the process is killed and the agent reports the timeout. A missing program, a non-zero exit status, or output that is not valid UTF-8 SHALL produce a message, not a crash. Output shown SHALL be capped (default 200 lines), with a note saying how much was cut.

#### Scenario: Timeout
- **WHEN** a confirmed command is still running when the timeout expires
- **THEN** it is killed and the agent reports that it timed out, then continues

#### Scenario: Failure
- **WHEN** a confirmed command exits with a non-zero status
- **THEN** the agent shows its error output and exit status and continues with the next request

### Requirement: Multi-step requests
A request SHALL be split into steps at `;` and at the word `then` (an `and` or a comma right before it is dropped). Each step is decided and confirmed on its own, in order. If a step's command fails or times out, the remaining steps SHALL be skipped with a message. A declined step SHALL NOT stop the steps after it.

#### Scenario: Two steps
- **WHEN** the request is `show disk space and then list the files`
- **THEN** the agent proposes and confirms `df -h`, then proposes and confirms `ls -la`

### Requirement: Memory and built-in tools
The agent SHALL remember the last 20 commands it was asked to run, with their outcomes (ran with exit status, timed out, failed to start, declined). Built-in tools SHALL be recognised before the model is consulted: `help` lists the tools; `history` shows remembered commands; `again` proposes the most recent command again, which still needs confirmation; `quit` or `exit` ends interactive mode.

#### Scenario: Again
- **WHEN** the user types `again` after a command ran
- **THEN** the same command is shown and must be confirmed again before it runs

### Requirement: Modes and robustness
Run with no request, the agent SHALL loop: show a prompt, handle each line, and end on `quit`, `exit`, or end of input. Run with a request as arguments, it SHALL handle that request once and exit. A missing model SHALL produce a message naming `make intent-train` and a non-zero exit status. No input SHALL make the agent panic.

#### Scenario: One-shot
- **WHEN** `cyore show my files` runs and the user answers `y`
- **THEN** `ls -la` runs, its output is shown, and the program exits with status 0

### Requirement: Footprint
The release `cyore` binary SHALL be at most 30 MB, and an interactive session that loads the model and handles a request SHALL use less than 100 MB of RAM at peak.

#### Scenario: Measure
- **WHEN** the release binary is built and a one-shot dry run is measured
- **THEN** the file size is ≤ 30 MB and the peak resident memory is < 100 MB
