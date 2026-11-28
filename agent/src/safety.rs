// The safety layer: the single choke point every command passes through before it can run.
//
// `check` is the only way to build a `CheckedCommand`, and the runner only accepts a
// `CheckedCommand`, so the type system guarantees nothing unchecked is ever executed
// (design.md, decision 3). The rules come from .agents/shared/rules/safety.md and SECURITY.md.
// Model output is untrusted input: these checks hold no matter how confident the model is.

use std::collections::BTreeMap;
use std::fmt;

/// Programs the agent may run. Owned here, not read from a data file, so a tampered dataset or
/// model cannot widen it. A test checks it covers every command in the dataset catalog.
pub const ALLOWLIST: &[&str] = &[
    "ls",
    "pwd",
    "cat",
    "head",
    "tail",
    "wc",
    "file",
    "stat",
    "sha256sum",
    "find",
    "grep",
    "du",
    "mkdir",
    "touch",
    "df",
    "free",
    "lsblk",
    "findmnt",
    "uname",
    "hostname",
    "uptime",
    "whoami",
    "date",
    "cal",
    "lscpu",
    "nproc",
    "ps",
    "pgrep",
    "who",
    "env",
    "printenv",
    "which",
    "ip",
    "ping",
    "ss",
    "getent",
    "git",
];

/// Always refused as a program, whatever the allowlist says (defence in depth).
const BLOCKED_PROGRAMS: &[&str] = &[
    "rm", "mkfs", "dd", "shutdown", "reboot", "halt", "poweroff", "sudo", "su",
];
/// Always refused anywhere in the command.
const BLOCKED_PATTERNS: &[&str] = &["/dev/sd", ":(){", "chmod -R 777 /", "rm -rf /"];
/// Shell syntax. There is no shell, but a command containing these was not produced by any
/// template we wrote, so something is wrong: refuse rather than guess.
const SHELL_METACHARACTERS: &[&str] = &["|", ">", "<", ";", "&", "$(", "`"];

/// Why a command was refused.
#[derive(Debug, Clone, PartialEq)]
pub struct Refusal(pub String);

impl fmt::Display for Refusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A command that has passed every safety check. Only `check` can create one.
#[derive(Debug, Clone, PartialEq)]
pub struct CheckedCommand {
    program: String,
    args: Vec<String>,
}

impl CheckedCommand {
    pub fn program(&self) -> &str {
        &self.program
    }

    pub fn args(&self) -> &[String] {
        &self.args
    }

    /// Test-only escape hatch so the runner can be tested with programs the allowlist refuses
    /// (e.g. one that does not exist). Not compiled into the real program.
    #[cfg(test)]
    pub fn unchecked_for_tests(program: &str, args: &[&str]) -> Self {
        CheckedCommand {
            program: program.to_string(),
            args: args.iter().map(|a| a.to_string()).collect(),
        }
    }
}

/// The exact command line that will run, for showing to the user.
impl fmt::Display for CheckedCommand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.program)?;
        for arg in &self.args {
            write!(f, " {arg}")?;
        }
        Ok(())
    }
}

/// Expand a leading `~` or `~/` to the home directory (no shell does it for us).
fn expand_home(arg: &str, home: Option<&str>) -> Result<String, Refusal> {
    let rest = match arg.strip_prefix('~') {
        Some(rest) if rest.is_empty() || rest.starts_with('/') => rest,
        _ => return Ok(arg.to_string()),
    };
    match home {
        Some(home) => Ok(format!("{home}{rest}")),
        None => Err(Refusal(format!(
            "cannot expand \"{arg}\": the HOME environment variable is not set"
        ))),
    }
}

/// Check a proposed command. `slots` are the values taken from the user's request; `home` is
/// the home directory used for `~` expansion.
pub fn check(
    command: &str,
    slots: &BTreeMap<String, String>,
    home: Option<&str>,
) -> Result<CheckedCommand, Refusal> {
    // 1. Split into program and arguments.
    let words: Vec<&str> = command.split_whitespace().collect();
    let Some((&program, args)) = words.split_first() else {
        return Err(Refusal("empty command".to_string()));
    };
    // 2. Blocklist first, so a blocked program is reported as dangerous, not just unknown.
    let base = program.rsplit('/').next().unwrap_or(program);
    if BLOCKED_PROGRAMS.contains(&base) || base.starts_with("mkfs.") {
        return Err(Refusal(format!(
            "`{base}` is blocked: it can destroy data or stop the system"
        )));
    }
    let joined = words.join(" ");
    if let Some(pattern) = BLOCKED_PATTERNS.iter().find(|p| joined.contains(*p)) {
        return Err(Refusal(format!(
            "the command contains the blocked pattern `{pattern}`"
        )));
    }
    // 3. Allowlist (by exact name; a path like /usr/bin/ls is not on the list).
    if !ALLOWLIST.contains(&program) {
        return Err(Refusal(format!(
            "`{program}` is not on the allowlist of programs Cyore may run"
        )));
    }
    // 4. No shell syntax.
    if let Some(m) = SHELL_METACHARACTERS.iter().find(|m| joined.contains(*m)) {
        return Err(Refusal(format!("the command contains shell syntax `{m}`")));
    }
    // 5. Option injection: a value from the request must not be read as an option.
    if let Some((name, value)) = slots.iter().find(|(_, v)| v.starts_with('-')) {
        return Err(Refusal(format!(
            "the {name} \"{value}\" starts with `-`, so the program would read it as an option"
        )));
    }
    // 6. Expand ~ in the arguments.
    let args = args
        .iter()
        .map(|a| expand_home(a, home))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(CheckedCommand {
        program: program.to_string(),
        args,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use dataset::catalog::{INTENTS, POOLS};
    use dataset::template::Template;

    fn no_slots() -> BTreeMap<String, String> {
        BTreeMap::new()
    }

    fn slots(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    fn refused(command: &str, slots: &BTreeMap<String, String>) -> String {
        check(command, slots, Some("/home/u")).unwrap_err().0
    }

    #[test]
    fn test_allowed_command() {
        let c = check("ls -la src", &slots(&[("dir", "src")]), Some("/home/u")).unwrap();
        assert_eq!(c.program(), "ls");
        assert_eq!(c.args(), ["-la", "src"]);
        assert_eq!(c.to_string(), "ls -la src");
    }

    #[test]
    fn test_not_allowlisted() {
        assert!(refused("curl example.com", &no_slots()).contains("not on the allowlist"));
        assert!(refused("/usr/bin/ls", &no_slots()).contains("not on the allowlist"));
    }

    #[test]
    fn test_blocklist() {
        assert!(refused("rm -rf /", &no_slots()).contains("`rm` is blocked"));
        assert!(refused("mkfs.ext4 x", &no_slots()).contains("`mkfs.ext4` is blocked"));
        assert!(refused("dd if=a of=b", &no_slots()).contains("blocked"));
        assert!(refused("sudo ls", &no_slots()).contains("blocked"));
        assert!(refused("/sbin/shutdown now", &no_slots()).contains("`shutdown` is blocked"));
        assert!(refused("ls /dev/sda", &no_slots()).contains("/dev/sd"));
        assert!(refused("cat :(){ :|:& };:", &no_slots()).contains("blocked pattern"));
    }

    #[test]
    fn test_shell_syntax() {
        assert!(refused("ls | cat", &no_slots()).contains("shell syntax `|`"));
        assert!(refused("cat a > b", &no_slots()).contains("shell syntax"));
        assert!(refused("ls $(whoami)", &no_slots()).contains("shell syntax"));
        assert!(refused("ls a;b", &no_slots()).contains("shell syntax"));
    }

    #[test]
    fn test_option_injection() {
        let err = refused("find -delete -name x", &slots(&[("dir", "-delete")]));
        assert!(err.contains("\"-delete\" starts with `-`"), "{err}");
    }

    #[test]
    fn test_home_expansion() {
        let c = check("ls -la ~/notes", &no_slots(), Some("/home/u")).unwrap();
        assert_eq!(c.args(), ["-la", "/home/u/notes"]);
        let c = check("du -sh ~", &no_slots(), Some("/home/u")).unwrap();
        assert_eq!(c.args(), ["-sh", "/home/u"]);
        // ~user and a ~ in the middle are left alone.
        let c = check("ls ~bob a~b", &no_slots(), Some("/home/u")).unwrap();
        assert_eq!(c.args(), ["~bob", "a~b"]);
        assert!(check("ls ~/x", &no_slots(), None).is_err());
    }

    #[test]
    fn test_globs_pass_literally() {
        let c = check("find . -name *.txt", &slots(&[("pattern", "*.txt")]), None).unwrap();
        assert_eq!(c.args(), [".", "-name", "*.txt"]);
    }

    #[test]
    fn test_empty() {
        assert!(refused("   ", &no_slots()).contains("empty"));
    }

    /// Every command the model can produce must pass the safety layer, with any slot values
    /// from the dataset (train and test pools).
    #[test]
    fn test_every_catalog_command_passes() {
        for intent in INTENTS {
            let Some(source) = intent.command else {
                continue;
            };
            let template = Template::parse(source).unwrap();
            assert!(
                ALLOWLIST.contains(&template.program()),
                "{}: program {} not allowlisted",
                intent.name,
                template.program()
            );
            for round in 0..3 {
                let values: BTreeMap<String, String> = template
                    .slots()
                    .into_iter()
                    .map(|(name, _)| {
                        let pool = POOLS.iter().find(|p| p.name == name).unwrap();
                        let all: Vec<&str> = pool.train.iter().chain(pool.test).copied().collect();
                        (name.to_string(), all[round % all.len()].to_string())
                    })
                    .collect();
                let command = template.fill(&values).unwrap();
                if let Err(e) = check(&command, &values, Some("/home/u")) {
                    panic!("{}: \"{command}\" refused: {e}", intent.name);
                }
            }
        }
    }
}
