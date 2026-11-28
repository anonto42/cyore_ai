// The catalog: every intent the model can learn, with its command, the phrasing grammar for
// generated training sentences, and hand-written test sentences.
//
// Grammar syntax (see grammar.rs): `(a|b)` one of, `[x]` optional, `{slot}` placeholder.
// Each phrasing line may start with a kind, which decides which wrappers it can get:
//   "do: "  an instruction ("show the files") → may get "please", "can you", "for me", ...
//   "ask: " a question ("what is my ip")      → may get "hey,", "quick question,", "?"
//   (none)  a bare phrase ("disk space")      → only capitalisation / "?"
//
// To add an intent: add an entry to INTENTS, put any new program on ALLOWLIST, and run the
// generator; validation tells you if anything is inconsistent.

pub struct Intent {
    pub name: &'static str,
    /// Command template (see template.rs); None only for `unknown`.
    pub command: Option<&'static str>,
    pub phrasings: &'static [&'static str],
    /// Hand-written, natural test sentences. Slot placeholders are filled from TEST pools.
    /// Used only to report final quality.
    pub tests: &'static [&'static str],
    /// Hand-written development sentences (also filled from the TEST pools). The only held-out
    /// data allowed to guide changes to this catalog or to the model.
    pub dev: &'static [&'static str],
}

/// Values for one slot type, split so the test set only uses values never seen in training.
pub struct SlotPool {
    pub name: &'static str,
    pub train: &'static [&'static str],
    pub test: &'static [&'static str],
}

pub struct Catalog {
    pub intents: &'static [Intent],
    pub pools: &'static [SlotPool],
    /// Programs a command template may start with.
    pub allowlist: &'static [&'static str],
}

pub const UNKNOWN: &str = "unknown";

pub fn catalog() -> Catalog {
    Catalog {
        intents: INTENTS,
        pools: POOLS,
        allowlist: ALLOWLIST,
    }
}

// ---------------------------------------------------------------------------------------------
// Wrappers added at sampling time (see generate.rs). They add variety without being counted as
// separate phrasings.

pub const DO_PREFIXES: &[&str] = &[
    "please",
    "can you",
    "could you",
    "would you",
    "go ahead and",
    "hey,",
];
pub const DO_SUFFIXES: &[&str] = &["please", "for me", "now"];
pub const ASK_PREFIXES: &[&str] = &["hey,", "quick question,", "so", "ok,"];

// ---------------------------------------------------------------------------------------------
// Slot values. No whitespace, no shell metacharacters; train and test lists never overlap.
// A leading `~/` is left for the Stage 3 agent to expand, since commands run without a shell.

pub const POOLS: &[SlotPool] = &[
    SlotPool {
        name: "dir",
        train: &[
            "Downloads",
            "Documents",
            "Desktop",
            "src",
            "docs",
            "~/projects",
            "/var/log",
            "/tmp",
            "/usr/local/bin",
            "Music",
            "Videos",
            "backups",
            "~/code",
            "./data",
            "assets",
            "config",
            "/opt",
            "logs",
            "scripts",
            "tests",
        ],
        test: &[
            "Pictures", "build", "~/notes", "/etc", "homework", "~/work", "vendor", "/srv",
            "archive", "reports",
        ],
    },
    SlotPool {
        name: "file",
        train: &[
            "notes.txt",
            "README.md",
            "main.rs",
            "config.toml",
            "report.pdf",
            "data.csv",
            "todo.md",
            "index.html",
            "app.log",
            "Cargo.toml",
            "script.sh",
            "photo.jpg",
            "settings.json",
            "server.py",
            "letter.docx",
            ".bashrc",
            "error.log",
            "Makefile",
            "package.json",
            "style.css",
        ],
        test: &[
            "budget.xlsx",
            "essay.txt",
            "lib.rs",
            "docker-compose.yml",
            "syslog",
            "resume.pdf",
            "setup.py",
            ".vimrc",
            "invoice.csv",
            "CHANGELOG.md",
        ],
    },
    SlotPool {
        name: "pattern",
        train: &[
            "*.txt", "*.rs", "*.log", "*.md", "*.py", "*.jpg", "*.csv", "*.json", "*.sh", "*.toml",
            "*.html", "*.pdf",
        ],
        test: &["*.go", "*.png", "*.yml", "*.c", "*.mp3", "*.zip"],
    },
    SlotPool {
        name: "text",
        train: &[
            "TODO", "error", "warning", "main", "import", "fixme", "version", "debug", "timeout",
            "panic", "password", "deadline",
        ],
        test: &[
            "deprecated",
            "unsafe",
            "hello",
            "failed",
            "license",
            "refused",
        ],
    },
    SlotPool {
        name: "host",
        train: &[
            "google.com",
            "github.com",
            "8.8.8.8",
            "1.1.1.1",
            "example.com",
            "localhost",
            "router.local",
            "wikipedia.org",
            "rust-lang.org",
            "192.168.1.1",
        ],
        test: &[
            "cloudflare.com",
            "9.9.9.9",
            "debian.org",
            "10.0.0.1",
            "archlinux.org",
        ],
    },
    SlotPool {
        name: "process",
        train: &[
            "firefox", "nginx", "python", "sshd", "postgres", "docker", "chrome", "node", "bash",
            "systemd", "code", "java",
        ],
        test: &["redis", "mysql", "slack", "spotify", "postfix", "cron"],
    },
    SlotPool {
        name: "program",
        train: &[
            "git", "python3", "cargo", "docker", "curl", "vim", "node", "gcc", "make", "wget",
            "ssh", "rustc",
        ],
        test: &["htop", "ffmpeg", "go", "zip", "nano", "tmux"],
    },
];

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

// ---------------------------------------------------------------------------------------------

pub const INTENTS: &[Intent] = &[
    // ----- Files: reading -----
    Intent {
        name: "list_files",
        command: Some("ls -la {dir?}"),
        phrasings: &[
            "do: (show [me]|list|display) [all] the files [in {dir}]",
            "do: (show [me]|list) (everything|what is) in {dir}",
            "ask: what (files are|is) in {dir}",
            "ask: what is in this (folder|directory)",
            "directory listing [for {dir}]",
            "ask: what (is|do i have|have i got) (in|inside) (this (folder|directory)|here|{dir})",
            "do: (look|peek) (in|into|inside) {dir}",
            "do: (give me|get) [a list of] [the] (files|contents|stuff) (in|of|inside) {dir}",
            "[the] contents of (this (folder|directory)|the current (folder|directory)|{dir})",
        ],
        tests: &[
            "can you tell me which files live in {dir}?",
            "I'd like to see the contents of {dir}",
            "whats inside this folder",
        ],
        dev: &[
            "what have I got in {dir}",
            "peek into {dir} and tell me what's there",
            "contents of the current directory please",
        ],
    },
    Intent {
        name: "list_by_size",
        command: Some("ls -lhS {dir?}"),
        phrasings: &[
            "do: (show [me]|list|sort) [the] files [in {dir}] (sorted|ordered) by size",
            "do: list [the] (biggest|largest) files first [in {dir}]",
            "ask: which files are (biggest|largest) [in {dir}]",
            "do: (rank|order|sort) [the] files [in {dir}] (by size|from (biggest|largest) to smallest)",
            "ask: which files (take up|use|eat) the most (space|room|disk) [in {dir}]",
            "(big|large|huge|biggest|largest) files first [in {dir}]",
        ],
        tests: &[
            "what are the heaviest files in {dir}, biggest first",
            "order the files in {dir} from large to small",
            "list files by how much space they take",
        ],
        dev: &[
            "rank the files in {dir} by size",
            "which files here take up the most room",
            "show me big files first",
        ],
    },
    Intent {
        name: "list_by_time",
        command: Some("ls -lt {dir?}"),
        phrasings: &[
            "do: (show [me]|list|sort) [the] files [in {dir}] (sorted|ordered) by (date|time)",
            "do: (show [me]|list) the (newest|most recent|latest) files [in {dir}]",
            "ask: which files (changed|were modified) (recently|last) [in {dir}]",
            "do: (sort|order|list) [the] files [in {dir}] (newest|latest|most recent) first",
            "ask: what (was|got|has been) (changed|modified|edited|updated) (recently|lately|last) [in (this folder|{dir})]",
            "(recently|lately) (changed|modified|edited|updated) files [in {dir}]",
        ],
        tests: &[
            "what did I edit most recently in {dir}",
            "newest stuff first please",
            "list files with the latest edits at the top",
        ],
        dev: &[
            "sort {dir} so the newest files come first",
            "what was modified lately in this folder",
            "show recently changed files",
        ],
    },
    Intent {
        name: "current_dir",
        command: Some("pwd"),
        phrasings: &[
            "ask: (where am i|what is my current (directory|folder)|which (directory|folder) am i in)",
            "do: (show [me]|print|display|tell me) the (current|working|present) (directory|folder|path)",
            "pwd",
            "ask: (what|which) (directory|folder|path) (is this|am i (in|at)|is my (shell|terminal) in)",
            "do: (print|show [me]|tell me) where i (am|currently am|am right now)",
            "[the] (full|current|working) path [of (this|the current) (folder|directory)]",
        ],
        tests: &[
            "what folder is this",
            "remind me where I am in the filesystem",
            "which path am I sitting in right now",
        ],
        dev: &[
            "which directory is my shell in",
            "print where I currently am",
            "tell me the full path of this folder",
        ],
    },
    Intent {
        name: "show_file",
        command: Some("cat {file}"),
        phrasings: &[
            "do: (show [me]|print|display|open|read) [the (file|contents of)] {file}",
            "ask: what is (in|inside) {file}",
            "do: cat {file}",
            "do: (open [up]|view) {file} [so i can (read|see) it]",
            "ask: (can|could) i (see|read|view) [the (text|contents) of] {file}",
            "do: (display|show [me]|print) what is (in|inside|written in) {file}",
        ],
        tests: &[
            "let me see what {file} contains",
            "dump {file} to the screen",
            "I want to read {file}",
        ],
        dev: &[
            "open up {file} so I can read it",
            "display what's written in {file}",
            "can I see the text of {file}",
        ],
    },
    Intent {
        name: "show_file_numbered",
        command: Some("cat -n {file}"),
        phrasings: &[
            "do: (show [me]|print|display) {file} with line numbers",
            "do: (show [me]|print|display) [the] (contents of|file) {file} (with|including) line numbers",
            "do: number the lines (of|in) {file}",
            "{file} (numbered|with numbered lines|with line numbers)",
            "do: (print|show [me]|display) {file} (and number (the|every|each) line|numbered|with numbered lines)",
        ],
        tests: &[
            "I need {file} printed with numbered lines",
            "cat {file} but number each line",
            "show line numbers for {file}",
        ],
        dev: &[
            "print {file} and number every line",
            "I want line numbers next to the contents of {file}",
            "show {file} numbered",
        ],
    },
    Intent {
        name: "show_head",
        command: Some("head -n 20 {file}"),
        phrasings: &[
            "do: (show [me]|print|display) the (first|top) (lines|20 lines|few lines) of {file}",
            "do: (show|print) the (start|beginning) of {file}",
            "do: head {file}",
            "do: (preview|glance at) [the (start|beginning|top) of] {file}",
            "do: (only|just) (show|give me|print) the (first|top) (lines|part|bit) of {file}",
            "ask: what is at the (top|start|beginning) of {file}",
        ],
        tests: &[
            "peek at the opening of {file}",
            "just the first part of {file} please",
            "what does the top of {file} look like",
        ],
        dev: &[
            "only show the first lines of {file}",
            "give me the top of {file}",
            "preview the beginning of {file}",
        ],
    },
    Intent {
        name: "show_tail",
        command: Some("tail -n 20 {file}"),
        phrasings: &[
            "do: (show [me]|print|display) the (last|final) (lines|20 lines|few lines) of {file}",
            "do: (show|print) the end of {file}",
            "do: tail {file}",
            "do: (show|tell) me how {file} ends",
            "ask: what is at the end of {file}",
            "[the] (last|final) (lines|entries|part) (of|in) {file}",
        ],
        tests: &[
            "what's at the bottom of {file}",
            "show the most recent entries at the end of {file}",
            "give me the closing lines of {file}",
        ],
        dev: &[
            "only the last lines of {file} please",
            "show me how {file} ends",
            "what are the final entries in {file}",
        ],
    },
    Intent {
        name: "count_lines",
        command: Some("wc -l {file}"),
        phrasings: &[
            "ask: how many lines (are (in|inside) {file}|does {file} (have|contain))",
            "do: count [the] lines (in|of) {file}",
            "do: (show|tell me|give me) [the] (line count|number of lines) (of|for|in) {file}",
            "ask: how long is {file} [in lines]",
            "(number of lines|line count|lines) (in|of|for) {file}",
        ],
        tests: &[
            "what's the length of {file} in lines",
            "{file} line total?",
            "count how many rows {file} has",
        ],
        dev: &[
            "how long is {file} in lines",
            "number of lines in {file}?",
            "tell me how many lines {file} contains",
        ],
    },
    Intent {
        name: "file_type",
        command: Some("file {file}"),
        phrasings: &[
            "ask: what (kind|type|sort) of file is {file}",
            "do: (check|detect|identify|show [me]|tell me) the (file type|type|format) of {file}",
            "ask: what format is {file} [in]",
            "ask: is {file} (an image|a script|a program|a document|text)",
            "ask: what (kind|sort|type) of (data|thing|content) (is {file}|does {file} (hold|contain))",
        ],
        tests: &[
            "is {file} a text file or binary",
            "figure out what {file} actually is",
            "which file format does {file} use",
        ],
        dev: &[
            "is {file} an image, text or something else",
            "check what sort of data {file} holds",
            "what kind of thing is {file}",
        ],
    },
    Intent {
        name: "file_info",
        command: Some("stat {file}"),
        phrasings: &[
            "do: (show [me]|display|get) [the] (details|info|information|metadata|stats) (about|for|of) {file}",
            "ask: when was {file} (last modified|created|changed)",
            "ask: who owns {file}",
            "ask: what are the permissions (of|on) {file}",
            "ask: when did {file} [last] (change|get (changed|modified))",
            "do: (show [me]|get [me]|give me) [the] (owner|permissions|timestamps) (of|for) {file}",
            "metadata (of|for|about) {file}",
        ],
        tests: &[
            "give me the full stat output for {file}",
            "what's the size and modification time of {file}",
            "look up the permissions and owner of {file}",
        ],
        dev: &[
            "when did {file} last change",
            "show the owner and permissions of {file}",
            "metadata for {file} please",
        ],
    },
    Intent {
        name: "file_checksum",
        command: Some("sha256sum {file}"),
        phrasings: &[
            "do: (compute|calculate|get [me]|show [me]) the (checksum|hash|sha256|sha256 hash) (of|for) {file}",
            "do: (checksum|hash) {file}",
            "do: verify the integrity of {file}",
            "do: (fingerprint|hash) {file}",
            "(sha256|checksum|hash) (of|for) {file}",
            "ask: what is the (hash|checksum|sha256) of {file}",
        ],
        tests: &[
            "I need a sha-256 fingerprint of {file}",
            "what's the hash value of {file}",
            "generate a checksum so I can compare {file}",
        ],
        dev: &[
            "sha256 of {file}",
            "give me a hash of {file} to verify it",
            "fingerprint {file}",
        ],
    },
    Intent {
        name: "find_by_name",
        command: Some("find {dir=.} -name {pattern}"),
        phrasings: &[
            "do: (find|search for|locate|look for) [all] {pattern} files [in {dir}]",
            "do: (find|search for|locate) [all] files (named|called|matching) {pattern} [in {dir}]",
            "ask: where are [all] [the] {pattern} files [in {dir}]",
            "do: search (this folder|here|{dir}) for {pattern} files",
            "ask: where can i find {pattern} files [(in|inside) {dir}]",
            "do: (find|show [me]) (all|each) files with the extension {pattern} [in {dir}]",
        ],
        tests: &[
            "hunt down every {pattern} file under {dir}",
            "are there any {pattern} files around here",
            "list the paths of files matching {pattern}",
        ],
        dev: &[
            "search this folder for {pattern} files",
            "where can I find {pattern} files inside {dir}",
            "find every file ending in {pattern}",
        ],
    },
    Intent {
        name: "search_text",
        command: Some("grep -rn {text} {dir=.}"),
        phrasings: &[
            "do: (search|look) for [the (word|text|string)] {text} [in {dir}]",
            "do: (find|grep [for]) [all] (lines|files) (containing|that mention|with) {text} [in {dir}]",
            "ask: which files (contain|mention) {text} [in {dir}]",
            "do: look (inside|in|through) [the] files [in {dir}] for [the word] {text}",
            "ask: which files (mention|contain|include) {text} [at all]",
            "do: grep [for] {text} [in {dir}]",
        ],
        tests: &[
            "grep the code in {dir} for {text}",
            "where does {text} appear in my files",
            "show every line mentioning {text}",
        ],
        dev: &[
            "look inside files in {dir} for the word {text}",
            "which files mention {text} at all",
            "grep for {text}",
        ],
    },
    Intent {
        name: "dir_size",
        command: Some("du -sh {dir=.}"),
        phrasings: &[
            "ask: how (big|large|heavy) is (this (folder|directory)|{dir})",
            "ask: how much (space|disk space|storage) (does {dir} use|is {dir} using|does this folder use)",
            "do: (show [me]|get [me]|check) the (size|total size|disk usage) of {dir}",
            "ask: how much (space|room|disk) does {dir} (take [up]|use)",
            "[the] total size of (this folder|{dir})",
            "ask: is (this folder|{dir}) (big|large)",
        ],
        tests: &[
            "what's the total footprint of {dir}",
            "measure how much {dir} weighs on disk",
            "size of this directory?",
        ],
        dev: &[
            "how much space does {dir} take up",
            "total size of {dir}",
            "is this folder big",
        ],
    },
    // ----- Files: creating (non-destructive) -----
    Intent {
        name: "make_dir",
        command: Some("mkdir -p {dir}"),
        phrasings: &[
            "do: (create|make|add) [a] [new] (folder|directory) (called|named) {dir}",
            "do: (create|make) [the] (folder|directory) {dir}",
            "do: mkdir {dir}",
            "do: (make|create) [a] [new] (folder|directory|dir) {dir}",
            "[a] new (folder|directory|dir) (called|named) {dir}",
        ],
        tests: &[
            "set up an empty folder named {dir}",
            "I need a new dir {dir}",
            "make {dir} for me as a folder",
        ],
        dev: &[
            "make a new folder {dir}",
            "create directory {dir} please",
            "new folder called {dir}",
        ],
    },
    Intent {
        name: "create_file",
        command: Some("touch {file}"),
        phrasings: &[
            "do: (create|make|add) [a] [new] [empty] file (called|named) {file}",
            "do: (create|touch) [the file] {file}",
            "do: (create|make) [an] (empty|blank|new) {file}",
            "do: (make|create) a (blank|empty) file (named|called) {file}",
            "[a] new (file|empty file) {file}",
        ],
        tests: &[
            "start a blank file {file}",
            "I need an empty {file}",
            "make an empty file and call it {file}",
        ],
        dev: &[
            "create an empty {file}",
            "make a blank file named {file}",
            "new file {file}",
        ],
    },
    // ----- Disks and memory -----
    Intent {
        name: "disk_space",
        command: Some("df -h"),
        phrasings: &[
            "ask: how much (disk|storage) space (do i have [left]|is (left|available|free))",
            "ask: how much free space (do i have [left]|is (left|there))",
            "do: (show [me]|check) [the] (disk space|disk usage|free space|storage) [on (my|this) (computer|machine)]",
            "ask: is my disk (full|almost full)",
            "ask: how much (room|space) is (left|free) on (the|my) (disk|drive|hard drive)",
            "free (disk|drive) space",
            "do: check (if|whether) my (disk|drive) is (getting full|full|almost full)",
        ],
        tests: &[
            "am I running out of space on the drive",
            "how full are my disks",
            "what's free on each partition",
        ],
        dev: &[
            "how much room is left on the disk",
            "free disk space?",
            "check if my drive is getting full",
        ],
    },
    Intent {
        name: "disk_inodes",
        command: Some("df -i"),
        phrasings: &[
            "do: (show [me]|check) [the] inode (usage|count|counts) [on (my|the) (disks|drives|filesystems)]",
            "ask: (am i|are we) running out of inodes",
            "ask: how many inodes are (used|free|left)",
            "ask: how many inodes (do i have [left]|are (left|free|used))",
            "inode (usage|status) [(per|for each) filesystem]",
            "ask: are [the] inodes (running low|almost full|full)",
        ],
        tests: &[
            "are any filesystems low on inodes",
            "inode stats please",
            "display free and used inodes for every mount",
        ],
        dev: &[
            "how many inodes do I have left",
            "inode usage per filesystem",
            "are inodes running low",
        ],
    },
    Intent {
        name: "memory",
        command: Some("free -h"),
        phrasings: &[
            "ask: how much (memory|ram) (do i have|is (free|used|available)|is left)",
            "do: (show [me]|check) [the] (memory|ram) (usage|use|stats)",
            "(memory|ram) usage",
            "ask: is my (memory|ram) (almost full|full|running low)",
            "ask: how much (memory|ram) am i using",
            "(memory|ram) (status|info)",
        ],
        tests: &[
            "how much RAM is in use right now",
            "display free and used memory",
            "check whether I have enough memory free",
        ],
        dev: &[
            "is my memory almost full",
            "how much memory am I using",
            "ram status",
        ],
    },
    Intent {
        name: "block_devices",
        command: Some("lsblk"),
        phrasings: &[
            "do: (show [me]|list) [all] [the] (disks|drives|block devices|partitions) [(connected|attached) [to (this|the) (computer|machine)]]",
            "ask: what (disks|drives) (do i have|are (connected|plugged in))",
            "do: (list|show [me]) [all] [my] (storage devices|disks and partitions|drives and partitions)",
            "ask: what (drives|disks|devices) are (plugged in|plugged into this (machine|computer))",
        ],
        tests: &[
            "which storage devices are attached",
            "show my hard drives and their partitions",
            "is my usb stick detected as a disk",
        ],
        dev: &[
            "list all my storage devices",
            "what drives are plugged into this machine",
            "show disks and partitions",
        ],
    },
    Intent {
        name: "mounted_filesystems",
        command: Some("findmnt"),
        phrasings: &[
            "do: (show [me]|list) [all] [the] (mounted filesystems|mounts|mount points|mounted drives)",
            "ask: (what|which) (filesystems|drives) are mounted",
            "ask: where is (everything|each drive) mounted",
            "ask: what is mounted [right now]",
            "do: show [me] where [my] (drives|disks|filesystems) are mounted",
            "ask: (what|which) mount points (are there|do i have)",
        ],
        tests: &[
            "tell me what's currently mounted",
            "which mount points exist",
            "display the tree of mounted file systems",
        ],
        dev: &[
            "what is mounted right now",
            "show me where my drives are mounted",
            "what mount points does this system have",
        ],
    },
    // ----- System -----
    Intent {
        name: "os_info",
        command: Some("uname -a"),
        phrasings: &[
            "do: (show [me]|print|display) [the] (system|os|operating system) (info|information|details)",
            "ask: what (os|operating system|system) (am i running|is this [machine])",
            "do: (show|print|give me) [the] [full] details about (this|the) (os|operating system|system)",
            "ask: what (system|os|operating system) am i (on|using)",
            "uname [info]",
        ],
        tests: &[
            "tell me about this computer's system",
            "which linux is installed on this box",
            "full system identification please",
        ],
        dev: &[
            "full details about this OS",
            "what system am I on",
            "print uname info",
        ],
    },
    Intent {
        name: "kernel_version",
        command: Some("uname -r"),
        phrasings: &[
            "ask: what (kernel|linux kernel) (version|release) (is this|am i (running|using)|is (running|installed))",
            "do: (show [me]|print|display) the kernel (version|release)",
            "ask: which kernel [version] am i (running|on|using)",
            "kernel version",
            "ask: what (kernel|linux kernel) (do i have|is installed)",
        ],
        tests: &[
            "which kernel is this machine on",
            "kernel release number?",
            "tell me the version of the linux kernel",
        ],
        dev: &[
            "which kernel version am I running",
            "kernel version?",
            "what linux kernel do I have",
        ],
    },
    Intent {
        name: "hostname",
        command: Some("hostname"),
        phrasings: &[
            "ask: what is (the|my|this) (computer|machine|host)['s] (name|hostname)",
            "do: (show [me]|print) the hostname",
            "ask: what is the name of (this|my) (computer|machine)",
            "do: show [me] my hostname",
            "ask: what is (this|my) (computer|machine) called",
        ],
        tests: &[
            "what's this box called",
            "name of this machine?",
            "tell me the network name of my computer",
        ],
        dev: &[
            "what's the name of this computer",
            "show my hostname",
            "what is this machine called",
        ],
    },
    Intent {
        name: "uptime",
        command: Some("uptime"),
        phrasings: &[
            "ask: how long has (the|this|my) (computer|machine|system|server) been (running|up|on)",
            "ask: when did (the|this) (computer|machine|system) (boot|start)",
            "do: (show [me]|check) [the] (uptime|system load|load average)",
            "ask: how long since (the|my) (computer|machine|system) (started|booted)",
            "ask: when was the last (boot|restart)",
            "uptime",
        ],
        tests: &[
            "how long since the last reboot",
            "time since boot?",
            "is the system load high right now",
        ],
        dev: &[
            "how long since the computer started",
            "uptime please",
            "when was the last boot",
        ],
    },
    Intent {
        name: "whoami",
        command: Some("whoami"),
        phrasings: &[
            "ask: (who|which user) am i [logged in as]",
            "ask: what is my (username|user name|login|account name)",
            "do: (show [me]|print|tell me) [my] (username|current user|user name)",
            "ask: what user am i [logged in as]",
            "ask: which user is running this (shell|terminal)",
            "my (username|user name|login)",
        ],
        tests: &[
            "which account am I using",
            "remind me of my login name",
            "who is the current user here",
        ],
        dev: &[
            "what user am I logged in as",
            "which user is running this shell",
            "my username?",
        ],
    },
    Intent {
        name: "date",
        command: Some("date"),
        phrasings: &[
            "ask: what (time|date|day) is it [(now|today|right now)]",
            "ask: what is (the|today's) (date|time)",
            "do: (show [me]|tell me|print) the [current] (date|time|date and time)",
            "(today's|the current) (date|time)",
            "ask: what is the (date|time) [(today|now)]",
        ],
        tests: &[
            "what's today",
            "give me the clock time",
            "current day and hour please",
        ],
        dev: &["today's date please", "current time?", "what's the date"],
    },
    Intent {
        name: "calendar",
        command: Some("cal"),
        phrasings: &[
            "do: (show [me]|display|print) [the] (calendar|month calendar|calendar for this month)",
            "ask: what does this month look like",
            "do: (show|print|display) [me] (a|this month's) calendar",
            "calendar",
        ],
        tests: &[
            "I want to see this month's days laid out",
            "open a calendar view",
            "which weekday does the month start on",
        ],
        dev: &[
            "show this month's calendar",
            "print a calendar",
            "calendar please",
        ],
    },
    Intent {
        name: "cpu_info",
        command: Some("lscpu"),
        phrasings: &[
            "do: (show [me]|display|get) [the] (cpu|processor) (info|information|details|specs)",
            "ask: what (cpu|processor) (do i have|is (in|inside) this (computer|machine))",
            "(cpu|processor) (model|speed|model and speed|details)",
            "do: tell me about (my|the) (cpu|processor)",
            "ask: which (cpu|processor) does this (computer|machine) (use|have)",
        ],
        tests: &[
            "which processor model is installed",
            "tell me about my CPU architecture",
            "how fast is my processor",
        ],
        dev: &[
            "cpu model and speed",
            "tell me about the processor",
            "which cpu does this computer use",
        ],
    },
    Intent {
        name: "cpu_count",
        command: Some("nproc"),
        phrasings: &[
            "ask: how many (cpus|cpu cores|cores|processors|threads) (do i have|does this (machine|computer) have|are there|are available)",
            "do: (count|show [me]) the number of (cores|cpus|processors)",
            "number of (cores|cpus|processors|cpu cores)",
            "do: count [the] (cpu cores|cores|cpus|processors)",
            "ask: how many (processors|cores|cpus) are (in|on) this (machine|computer)",
        ],
        tests: &[
            "number of CPU threads available?",
            "core count please",
            "how parallel can this machine go",
        ],
        dev: &[
            "number of cores",
            "count the cpu cores",
            "how many processors are in this machine",
        ],
    },
    Intent {
        name: "processes",
        command: Some("ps aux"),
        phrasings: &[
            "do: (show [me]|list|display) [all] [the] (running processes|processes|programs that are running)",
            "ask: what (processes|programs) are running",
            "ask: what is running [on (this|my) (computer|machine)]",
            "do: (list|show [me]) [all] (running programs|everything that is running|running tasks)",
            "ask: what is running on (my|this) (computer|machine) right now",
        ],
        tests: &[
            "everything currently executing please",
            "give me the full process list",
            "which tasks are active on the system",
        ],
        dev: &[
            "list running programs",
            "what's running on my computer right now",
            "list everything that is running",
        ],
    },
    Intent {
        name: "top_cpu",
        command: Some("ps aux --sort=-%cpu"),
        phrasings: &[
            "ask: (what|which) (processes|programs|apps) (are using|use) the most (cpu|processor)",
            "do: (show [me]|list) [the] (processes|programs) (sorted|ordered) by cpu [usage]",
            "ask: what is (eating|hogging|using) [all] my cpu",
            "ask: which (process|program|app) uses the most (cpu|processor)",
            "ask: what is using so much (cpu|processor)",
            "do: show [me] the (cpu hogs|cpu heavy processes)",
        ],
        tests: &[
            "what's slowing my computer down",
            "which app is burning the processor",
            "top CPU consumers?",
        ],
        dev: &[
            "which process uses the most cpu",
            "what's using so much cpu",
            "show the cpu hogs",
        ],
    },
    Intent {
        name: "top_memory",
        command: Some("ps aux --sort=-%mem"),
        phrasings: &[
            "ask: (what|which) (processes|programs|apps) (are using|use) the most (memory|ram)",
            "do: (show [me]|list) [the] (processes|programs) (sorted|ordered) by (memory|ram) [usage]",
            "ask: what is (eating|hogging|using) [all] my (memory|ram)",
            "ask: which (process|program|app) uses the most (memory|ram)",
            "do: show [me] the (biggest|largest) memory hogs",
            "(processes|programs) sorted by (memory|ram) [(use|usage)]",
        ],
        tests: &[
            "which program is filling up my RAM",
            "biggest memory users right now",
            "who is consuming all the memory",
        ],
        dev: &[
            "which program uses the most ram",
            "show the biggest memory hogs",
            "processes sorted by memory use",
        ],
    },
    Intent {
        name: "find_process",
        command: Some("pgrep -a {process}"),
        phrasings: &[
            "ask: is {process} (running|still running|alive|open)",
            "do: (find|show [me]|check for|look for) [the] {process} (process|processes)",
            "ask: (what is|which is) the (pid|process id) of {process}",
            "ask: is {process} running [right now]",
            "do: show [me] the (pid|process id) (of|for) {process}",
            "do: check (if|whether) {process} is (alive|running|up)",
        ],
        tests: &[
            "do I have {process} open somewhere",
            "get me the PID for {process}",
            "check whether {process} has started",
        ],
        dev: &[
            "is {process} running right now",
            "show me the pid of {process}",
            "check if {process} is alive",
        ],
    },
    Intent {
        name: "logged_in_users",
        command: Some("who"),
        phrasings: &[
            "ask: who is (logged in|logged on|online|on this (machine|computer|system))",
            "do: (show [me]|list) [the] (logged in|connected|current) users",
            "ask: who else is (logged in|on this (machine|computer))",
            "do: list [the] users (on|logged into) this (machine|computer)",
            "ask: which users are (connected|logged in) [right now]",
        ],
        tests: &[
            "which people are signed in right now",
            "anyone else using this computer?",
            "display active user sessions",
        ],
        dev: &[
            "who else is logged in",
            "list users on this machine",
            "which users are connected right now",
        ],
    },
    Intent {
        name: "env_vars",
        command: Some("env"),
        phrasings: &[
            "do: (show [me]|list|print|display) [all] [the] environment variables",
            "do: (show [me]|print) [my] (env|environment)",
            "do: (show|list) [me] my environment variables",
            "do: list env",
            "ask: what environment variables are (set|defined)",
        ],
        tests: &[
            "what variables are set in my shell",
            "dump the environment",
            "which env vars exist",
        ],
        dev: &[
            "show my environment variables",
            "list env",
            "what environment variables are set",
        ],
    },
    Intent {
        name: "show_path",
        command: Some("printenv PATH"),
        phrasings: &[
            "do: (show [me]|print|display) [(my|the)] path [variable]",
            "ask: what is (in|on) my path",
            "ask: which (directories|folders) are (in|on) my path",
            "ask: what is in my path variable",
            "ask: which (folders|directories) does [my] path contain",
        ],
        tests: &[
            "where does my shell look for programs",
            "echo the PATH value",
            "list the PATH entries",
        ],
        dev: &[
            "what's in my PATH variable",
            "which folders does PATH contain",
            "echo $PATH",
        ],
    },
    Intent {
        name: "which_program",
        command: Some("which {program}"),
        phrasings: &[
            "ask: where is {program} (installed|located)",
            "ask: (is {program} installed|do i have {program} [installed])",
            "do: (find|locate|show [me]) the (path|location) (of|to) [the] {program} (program|binary|command|executable)",
            "which {program}",
            "[the] path (to|of) {program}",
            "ask: is {program} (available|on this (machine|computer))",
        ],
        tests: &[
            "where on disk does {program} live",
            "is the {program} command available here",
            "full path for {program}?",
        ],
        dev: &[
            "where is {program}",
            "path to {program}",
            "is {program} available",
        ],
    },
    // ----- Network -----
    Intent {
        name: "ip_addresses",
        command: Some("ip addr"),
        phrasings: &[
            "ask: what is my (ip|ip address|local ip|ip addresses)",
            "do: (show [me]|list|display) [my] [the] (ip addresses|network interfaces|network addresses|interfaces)",
            "ask: (which|what) ip [address] do i have",
            "my ip [address]",
            "do: list [the] ip addresses of this (computer|machine)",
        ],
        tests: &[
            "which address does this machine have on the network",
            "show interface addresses",
            "whats my IP",
        ],
        dev: &[
            "my ip?",
            "which ip do I have",
            "list ip addresses of this computer",
        ],
    },
    Intent {
        name: "routes",
        command: Some("ip route"),
        phrasings: &[
            "do: (show [me]|list|display) [the] (routing table|routes|network routes|default gateway)",
            "ask: what is my (default gateway|gateway|default route)",
            "do: show [me] the default (route|gateway)",
            "ask: what is my (gateway|router)",
            "do: print [the] routing table",
        ],
        tests: &[
            "how does traffic leave this computer",
            "where is my router on the routing table",
            "which gateway am I using",
        ],
        dev: &[
            "show the default route",
            "what's my gateway",
            "print routing table",
        ],
    },
    Intent {
        name: "ping_host",
        command: Some("ping -c 4 {host}"),
        phrasings: &[
            "do: ping {host}",
            "ask: (is {host} (up|reachable|online|down)|can i (reach|connect to) {host})",
            "do: (check|test|see) (if|whether) {host} is (up|reachable|online)",
            "do: test (my|the) connection to {host}",
            "do: ping {host} [a (few|couple of) times]",
            "ask: does {host} (respond|reply)",
        ],
        tests: &[
            "send a few pings to {host}",
            "is {host} answering",
            "check latency to {host}",
        ],
        dev: &[
            "ping {host} a few times",
            "is {host} reachable from here",
            "does {host} respond",
        ],
    },
    Intent {
        name: "open_ports",
        command: Some("ss -tuln"),
        phrasings: &[
            "do: (show [me]|list|display) [the] (open|listening) ports",
            "ask: (what|which) ports are (open|listening|in use)",
            "ask: what is listening on (this|my) (machine|computer)",
            "do: show [me] [the] listening (sockets|ports)",
            "ask: which ports is this (machine|computer) listening on",
            "ask: (are there|any) open ports [here]",
        ],
        tests: &[
            "which services accept connections here",
            "any sockets listening?",
            "display tcp and udp listeners",
        ],
        dev: &[
            "show listening sockets",
            "which ports is this machine listening on",
            "any open ports here",
        ],
    },
    Intent {
        name: "dns_lookup",
        command: Some("getent hosts {host}"),
        phrasings: &[
            "do: (look up|resolve|find) [the] (ip|ip address|address) (of|for) {host}",
            "ask: what (ip|address) does {host} (resolve to|point to|have)",
            "do: (resolve|look up) {host}",
            "do: look up {host} in dns",
            "do: get the (ip|address|ip address) (of|for) {host}",
            "(ip|address|ip address) (of|for) {host}",
        ],
        tests: &[
            "translate {host} into an IP",
            "which address is {host} at",
            "do a DNS lookup on {host}",
        ],
        dev: &[
            "look up {host} in dns",
            "get the address of {host}",
            "ip address for {host}?",
        ],
    },
    // ----- Git -----
    Intent {
        name: "git_status",
        command: Some("git status"),
        phrasings: &[
            "do: (show [me]|check|display) [the] (git|repo|repository) status",
            "ask: what (has changed|did i change|changed) [in (the|this) (repo|repository|project)]",
            "git status",
            "ask: what is the status of (my|the|this) (repo|repository|project)",
            "ask: (has anything changed|did anything change) in (the|this) (repo|repository)",
        ],
        tests: &[
            "any uncommitted changes in this repo?",
            "which files have I modified but not committed",
            "state of the working tree please",
        ],
        dev: &[
            "what's the status of my repo",
            "git status please",
            "anything changed in the repo",
        ],
    },
    Intent {
        name: "git_log",
        command: Some("git log --oneline -n 10"),
        phrasings: &[
            "do: (show [me]|list|display) [the] (recent|last|latest) (commits|git commits|commit history)",
            "ask: what (were|are) the (last|latest|recent) commits",
            "git log",
            "[the] last few commits",
            "[the] commit history",
            "ask: what (has been|was) committed (recently|lately)",
        ],
        tests: &[
            "what did we commit lately",
            "show the history of this project",
            "list the ten newest commits",
        ],
        dev: &[
            "last few commits",
            "commit history please",
            "what has been committed recently",
        ],
    },
    Intent {
        name: "git_diff",
        command: Some("git diff"),
        phrasings: &[
            "do: (show [me]|display) [the] (diff|changes|unstaged changes|code changes) [in (the|this) (repo|repository)]",
            "ask: what (lines|code) did i change",
            "ask: what exactly changed in the code",
            "do: show [me] what (changed|i changed)",
            "diff",
            "ask: what did i (edit|change) since the last commit",
        ],
        tests: &[
            "let me review my edits line by line",
            "show what I modified compared to the last commit",
            "diff of my working copy",
        ],
        dev: &[
            "show me what changed",
            "diff please",
            "what did I edit since the last commit",
        ],
    },
    Intent {
        name: "git_branches",
        command: Some("git branch"),
        phrasings: &[
            "do: (show [me]|list|display) [all] [the] [git] branches",
            "ask: what branches (are there|exist|do we have)",
            "do: show [me] (every|each) branch in (this|the) (repo|repository)",
            "ask: which branches are there",
        ],
        tests: &[
            "which branches does this repository have",
            "enumerate local branches",
            "branch list?",
        ],
        dev: &[
            "show every branch in this repo",
            "which branches are there",
            "all git branches",
        ],
    },
    Intent {
        name: "git_current_branch",
        command: Some("git branch --show-current"),
        phrasings: &[
            "ask: (what|which) branch (am i on|is this|am i (using|working on))",
            "ask: what is the current [git] branch",
            "do: (show [me]|print|tell me) [the] (current|active) branch [name]",
            "current branch",
            "do: tell me which branch i am (on|using)",
        ],
        tests: &[
            "name of the branch I'm on?",
            "which git branch is checked out",
            "remind me what branch this is",
        ],
        dev: &[
            "current branch?",
            "what branch is checked out",
            "tell me which branch I'm using",
        ],
    },
    Intent {
        name: "git_remotes",
        command: Some("git remote -v"),
        phrasings: &[
            "do: (show [me]|list|display) [the] [git] (remotes|remote urls|remote repositories)",
            "ask: where does (this|the) (repo|repository|project) push to",
            "ask: what is the (remote|origin) url",
            "ask: where does origin point [to]",
            "ask: what is origin",
            "[the] remote (repositories|urls) for this (project|repo)",
        ],
        tests: &[
            "which server is origin pointing at",
            "list the upstream urls",
            "where is this code hosted",
        ],
        dev: &[
            "where does origin point",
            "what is origin",
            "remote repositories for this project",
        ],
    },
    // ----- Out of scope -----
    // Requests the system must not map to a command: chit-chat, other apps, and destructive
    // actions deliberately left out of the catalog. The agent asks instead of guessing.
    Intent {
        name: UNKNOWN,
        command: None,
        phrasings: &[
            "do: (write|compose) [me] a (poem|story|song|joke)",
            "ask: what is the weather [(today|tomorrow|like)]",
            "do: (delete|remove|erase|wipe) (all|every one of) my files",
            "do: (format|wipe|erase) (the|my) (disk|hard drive)",
            "do: (shut down|restart|reboot) (the|my|this) (computer|machine|system)",
            "ask: who (won the game|is the president)",
            "do: (order|buy) [me] (a pizza|some food)",
            "do: (send|write) an email to (my boss|mom)",
            "do: kill all (processes|programs)",
            "do: (install|uninstall|update) (firefox|python|the system)",
            "(hello|hi|thanks|thank you)",
            "do: (play|stop) [some] music",
            "do: translate this (to|into) (french|spanish)",
            "do: (remove|delete) (the|my) (desktop|system|home|documents) (folder|files)",
            "do: format (the|my) (usb|usb drive|disk)",
            "do: (reboot|restart|shut down) [the computer] now",
            "do: kill (every|each) (process|program)",
            "do: (lock|unlock) the screen",
            "ask: (who|what) are you",
            "ask: what is the meaning of life",
            "do: (book|reserve) a (flight|hotel|table)",
            "do: send a (text|message) to my (friend|mom)",
            "do: (sing|play) me a song",
        ],
        tests: &[
            "please wipe my entire home folder",
            "tell me a funny joke",
            "turn off the computer now",
            "what's the capital of france",
            "make me a sandwich",
            "remove the downloads folder",
            "change my password",
            "how are you today",
            "open youtube in the browser",
            "give root access to everyone",
            "move all my photos to the trash",
            "set an alarm for 7am",
        ],
        dev: &[
            "remove all files from my desktop",
            "delete the system folder",
            "format the usb drive",
            "reboot now",
            "what's the meaning of life",
            "book a flight to paris",
            "sing me a song",
            "kill every process",
            "who are you",
            "lock the screen",
            "send a text to my friend",
            "uninstall everything",
        ],
    },
];
