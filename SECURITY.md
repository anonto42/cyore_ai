# Security Policy

Cyore will execute shell commands suggested by a machine-learning model. That makes the safety of the execution path the most important security property of the project.

## Supported Versions

Only the current `main` branch and the latest tagged release receive security fixes.

| Version | Supported |
|---|---|
| `main` (latest commit) | :white_check_mark: |
| Latest tagged release | :white_check_mark: |
| Older releases | :x: |

---

## Security Architecture & Design Guarantees

Cyore treats every model output as untrusted input. The rules below are enforced in code, not left to the model (see [Safety Rules](.agents/shared/rules/safety.md)):

1. **Human-in-the-Loop Execution**:
   The agent shows each generated command and asks `Run this? (y/n)` before running it. There is no silent or auto-run mode.

2. **Allowlist & Blocklist**:
   Only programs on the allowlist may run. Destructive patterns are always refused, whatever the model's confidence: `rm -rf /`, `mkfs`, `dd`, `shutdown`, `reboot`, fork bombs, writes to `/dev/sd*`, and `chmod -R 777 /`.

3. **No Shell Interpolation of Model Output**:
   Commands run through `std::process::Command` with explicit arguments. Model output never goes through `sh -c` unless the command template was chosen from the allowlist.

4. **Confidence Threshold & Timeouts**:
   Below the confidence threshold, the agent asks you to clarify instead of guessing. Every executed command has a timeout, and failures return a message instead of crashing the agent.

5. **Memory-Safe Core**:
   No `unsafe` code. Stage 1 crates have no third-party dependencies at all, which keeps the supply-chain surface minimal.

6. **No Secrets in Data**:
   Training datasets must not contain real secrets, hostnames, or personal paths.

> [!NOTE]
> These guarantees are enforced by the `cyore` agent (Stage 3). Every check lives in [`agent/src/safety.rs`](agent/src/safety.rs) and each one has a test; the confirmation prompt is in [`agent/src/agent.rs`](agent/src/agent.rs). Any way to run a command without confirmation, or to get a blocklisted command past `check`, is a security bug.

---

## Reporting a Vulnerability

If you discover a security vulnerability or a way to bypass the safety rules:

1. **Do not open a public GitHub issue.**
2. Send an email to the project maintainer:
   **MD Sohidul Islam Ananto** &lt;[anontom90@gmail.com](mailto:anontom90@gmail.com)&gt;
3. Please include:
   - A description of the vulnerability and its potential impact.
   - Exact steps, inputs, or prompts to reproduce the issue (for example, a phrasing that produces a blocked command that still runs).
   - Relevant environment details (OS, architecture, Cyore version or git SHA).

### Response Timeline
- **Initial Acknowledgement**: Within 48 hours.
- **Assessment & Status Update**: Within 7 days.
- **Coordinated Disclosure**: A fix will be prepared and tested before public release.
