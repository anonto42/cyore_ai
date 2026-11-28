// Memory: the most recent commands the user was asked about, and what happened
// (design.md, decision 8). `history` shows it; `again` re-proposes the newest command.

use std::collections::{BTreeMap, VecDeque};
use std::fmt;

pub const CAPACITY: usize = 20;

#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    /// Ran to completion with this exit code (None: ended by a signal).
    Ran(Option<i32>),
    TimedOut,
    FailedToStart,
    Declined,
    /// Shown in --dry-run mode, not run.
    DryRun,
}

impl fmt::Display for Outcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Outcome::Ran(Some(0)) => write!(f, "ok"),
            Outcome::Ran(Some(code)) => write!(f, "exit status {code}"),
            Outcome::Ran(None) => write!(f, "ended by a signal"),
            Outcome::TimedOut => write!(f, "timed out"),
            Outcome::FailedToStart => write!(f, "failed to start"),
            Outcome::Declined => write!(f, "declined"),
            Outcome::DryRun => write!(f, "dry run"),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    /// The command as proposed (before `~` expansion), so `again` re-checks it from scratch.
    pub command: String,
    pub slots: BTreeMap<String, String>,
    pub outcome: Outcome,
}

#[derive(Debug, Default)]
pub struct Memory {
    entries: VecDeque<Entry>,
}

impl Memory {
    pub fn remember(&mut self, entry: Entry) {
        if self.entries.len() == CAPACITY {
            self.entries.pop_front();
        }
        self.entries.push_back(entry);
    }

    /// The most recent entry.
    pub fn last(&self) -> Option<&Entry> {
        self.entries.back()
    }

    /// Oldest first.
    pub fn entries(&self) -> impl Iterator<Item = &Entry> {
        self.entries.iter()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(command: &str) -> Entry {
        Entry {
            command: command.to_string(),
            slots: BTreeMap::new(),
            outcome: Outcome::Ran(Some(0)),
        }
    }

    #[test]
    fn test_order_and_last() {
        let mut m = Memory::default();
        assert!(m.last().is_none());
        m.remember(entry("pwd"));
        m.remember(entry("date"));
        assert_eq!(m.last().unwrap().command, "date");
        let all: Vec<&str> = m.entries().map(|e| e.command.as_str()).collect();
        assert_eq!(all, ["pwd", "date"]);
    }

    #[test]
    fn test_capacity() {
        let mut m = Memory::default();
        for i in 0..CAPACITY + 5 {
            m.remember(entry(&format!("cmd {i}")));
        }
        assert_eq!(m.len(), CAPACITY);
        assert_eq!(m.entries().next().unwrap().command, "cmd 5");
    }

    #[test]
    fn test_outcome_text() {
        assert_eq!(Outcome::Ran(Some(2)).to_string(), "exit status 2");
        assert_eq!(Outcome::Declined.to_string(), "declined");
    }
}
