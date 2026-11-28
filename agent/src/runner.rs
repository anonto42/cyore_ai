// Running a checked command: no shell, a timeout, and capped output (design.md, decision 5).
// Standard library only: threads drain stdout and stderr while the main thread waits.

use std::io::Read;
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use crate::safety::CheckedCommand;

/// Output of one stream, possibly shortened.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Captured {
    pub text: String,
    /// Lines (or bytes, if `truncated_bytes`) not shown.
    pub omitted_lines: usize,
    pub truncated_bytes: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum RunOutcome {
    /// The process ended by itself. `code` is None if a signal ended it.
    Finished {
        code: Option<i32>,
        stdout: Captured,
        stderr: Captured,
    },
    /// Still running at the deadline, so it was killed.
    TimedOut { stdout: Captured, stderr: Captured },
    /// The program could not be started (e.g. it is not installed).
    FailedToStart(String),
}

impl RunOutcome {
    pub fn succeeded(&self) -> bool {
        matches!(self, RunOutcome::Finished { code: Some(0), .. })
    }
}

/// Something that can run a checked command. The real one starts processes; tests use a fake.
pub trait Runner {
    fn run(&self, command: &CheckedCommand, timeout: Duration) -> RunOutcome;
}

pub struct ProcessRunner {
    pub max_lines: usize,
    pub max_bytes: usize,
}

impl Default for ProcessRunner {
    fn default() -> Self {
        ProcessRunner {
            max_lines: 200,
            max_bytes: 64 * 1024,
        }
    }
}

/// Read a pipe to the end on its own thread, keeping at most `max_bytes`. Reading continues
/// past the limit (and discards) so the process never blocks on a full pipe.
fn drain(
    mut pipe: impl Read + Send + 'static,
    max_bytes: usize,
) -> thread::JoinHandle<(Vec<u8>, bool)> {
    thread::spawn(move || {
        let mut kept = Vec::new();
        let mut truncated = false;
        let mut chunk = [0u8; 8192];
        loop {
            match pipe.read(&mut chunk) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    let room = max_bytes.saturating_sub(kept.len());
                    kept.extend_from_slice(&chunk[..n.min(room)]);
                    truncated |= n > room;
                }
            }
        }
        (kept, truncated)
    })
}

/// Decode (invalid UTF-8 becomes �, never an error) and keep at most `max_lines` lines.
fn cap(bytes: &[u8], truncated_bytes: bool, max_lines: usize) -> Captured {
    let text = String::from_utf8_lossy(bytes);
    let lines: Vec<&str> = text.lines().collect();
    let shown = lines.len().min(max_lines);
    let mut out = lines[..shown].join("\n");
    if !out.is_empty() {
        out.push('\n');
    }
    Captured {
        text: out,
        omitted_lines: lines.len() - shown,
        truncated_bytes,
    }
}

/// Wait for `child` until `deadline`; returns None if it is still running.
fn wait_until(child: &mut Child, deadline: Instant) -> Option<Option<i32>> {
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Some(status.code()),
            Ok(None) if Instant::now() >= deadline => return None,
            Ok(None) => thread::sleep(Duration::from_millis(10)),
            Err(_) => return Some(None),
        }
    }
}

impl Runner for ProcessRunner {
    fn run(&self, command: &CheckedCommand, timeout: Duration) -> RunOutcome {
        let spawned = Command::new(command.program())
            .args(command.args())
            .stdin(Stdio::null()) // commands may not wait for input
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn();
        let mut child = match spawned {
            Ok(child) => child,
            Err(e) => {
                return RunOutcome::FailedToStart(format!(
                    "could not start `{}`: {e}",
                    command.program()
                ));
            }
        };
        // Both pipes exist because we asked for them above.
        let out = drain(
            child.stdout.take().expect("stdout is piped"),
            self.max_bytes,
        );
        let err = drain(
            child.stderr.take().expect("stderr is piped"),
            self.max_bytes,
        );

        let finished = wait_until(&mut child, Instant::now() + timeout);
        if finished.is_none() {
            let _ = child.kill();
            let _ = child.wait();
        }
        let collect = |h: thread::JoinHandle<(Vec<u8>, bool)>| {
            let (bytes, truncated) = h.join().unwrap_or_default();
            cap(&bytes, truncated, self.max_lines)
        };
        let (stdout, stderr) = (collect(out), collect(err));
        match finished {
            Some(code) => RunOutcome::Finished {
                code,
                stdout,
                stderr,
            },
            None => RunOutcome::TimedOut { stdout, stderr },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    use crate::safety::check;

    fn checked(command: &str) -> CheckedCommand {
        check(command, &BTreeMap::new(), None).unwrap()
    }

    const SECOND: Duration = Duration::from_secs(1);

    #[test]
    fn test_success() {
        let outcome = ProcessRunner::default().run(&checked("date"), 5 * SECOND);
        assert!(outcome.succeeded(), "{outcome:?}");
        if let RunOutcome::Finished { stdout, .. } = outcome {
            assert!(!stdout.text.is_empty());
        }
    }

    #[test]
    fn test_failure_has_status_and_stderr() {
        let outcome =
            ProcessRunner::default().run(&checked("ls /nonexistent-cyore-test"), 5 * SECOND);
        match outcome {
            RunOutcome::Finished { code, stderr, .. } => {
                assert_ne!(code, Some(0));
                assert!(!stderr.text.is_empty());
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn test_timeout_kills() {
        let start = Instant::now();
        let outcome = ProcessRunner::default().run(&checked("tail -f /dev/null"), SECOND);
        assert!(
            matches!(outcome, RunOutcome::TimedOut { .. }),
            "{outcome:?}"
        );
        assert!(
            start.elapsed() < 5 * SECOND,
            "the runner waited for the killed process"
        );
    }

    #[test]
    fn test_missing_program() {
        let cmd = CheckedCommand::unchecked_for_tests("cyore-no-such-program", &[]);
        let outcome = ProcessRunner::default().run(&cmd, SECOND);
        assert!(
            matches!(outcome, RunOutcome::FailedToStart(ref m) if m.contains("could not start"))
        );
    }

    #[test]
    fn test_output_is_capped() {
        let path = std::env::temp_dir().join(format!("cyore-runner-{}.txt", std::process::id()));
        let lines: Vec<String> = (1..=500).map(|i| format!("line {i}")).collect();
        std::fs::write(&path, lines.join("\n")).unwrap();
        let runner = ProcessRunner {
            max_lines: 10,
            max_bytes: 64 * 1024,
        };
        let outcome = runner.run(&checked(&format!("cat {}", path.display())), 5 * SECOND);
        std::fs::remove_file(&path).ok();
        match outcome {
            RunOutcome::Finished { stdout, .. } => {
                assert_eq!(stdout.text.lines().count(), 10);
                assert_eq!(stdout.omitted_lines, 490);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn test_byte_cap_and_invalid_utf8() {
        let capped = cap(&[b'a', 0xff, b'\n', b'b'], true, 10);
        assert_eq!(capped.text, "a\u{fffd}\nb\n");
        assert!(capped.truncated_bytes);
    }
}
