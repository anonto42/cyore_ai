# command-dataset Specification

## Purpose
Provides the labelled data that the Stage 2 intent classifier and slot model learn from: a catalog of safe terminal-command intents with natural-language phrasings, expanded into reproducible training and held-out test files.

## Requirements

### Requirement: Intent catalog
The dataset SHALL define at least 50 command intents plus one `unknown` intent. Each command intent SHALL have a unique name, a command template, and the names of the slots its template uses. The `unknown` intent SHALL have no command and stands for requests the system cannot handle.

#### Scenario: Catalog size
- **WHEN** the intent list file is read
- **THEN** it lists at least 51 intents with unique names, exactly one of which is `unknown`

#### Scenario: Template slots
- **WHEN** a command template contains a slot placeholder
- **THEN** that slot name is listed for the intent, marked as required or optional

### Requirement: Phrasing variety
Each command intent SHALL have at least 20 distinct training phrasings (texts that differ after slot values are replaced by their slot names).

#### Scenario: Too few phrasings
- **WHEN** the catalog contains an intent whose phrasings expand to fewer than 20 distinct texts
- **THEN** generation fails with an error naming the intent and how many phrasings it has

### Requirement: Example format
The training and test files SHALL be JSON Lines, one example per line, each with: `text` (the request), `tokens` (the request split by the documented tokenization rule), `tags` (one BIO slot tag per token, e.g. `O`, `B-dir`), `intent`, `slots` (slot name to value), and `command` (the template with slots filled in, or null for `unknown`).

#### Scenario: Tags align with tokens
- **WHEN** any example is read
- **THEN** `tags` has the same length as `tokens`, and the tokens tagged `B-<slot>` are exactly the values in `slots`

#### Scenario: Command is reconstructible
- **WHEN** an example's intent template is filled with its `slots` values (omitting absent optional slots, using defaults where defined)
- **THEN** the result equals the example's `command`

### Requirement: Tokenization rule
Tokens SHALL be produced by splitting the text on whitespace, removing trailing `?`, `!` and `,` from each token, and removing a trailing `.` from the last token. Empty tokens SHALL be dropped. Slot values SHALL never contain whitespace, so each value is exactly one token.

#### Scenario: Punctuation stripped
- **WHEN** the text is `can you list the files in Downloads?`
- **THEN** the tokens are `can`, `you`, `list`, `the`, `files`, `in`, `Downloads`

#### Scenario: File extensions kept
- **WHEN** the text is `show me notes.txt please.`
- **THEN** the tokens are `show`, `me`, `notes.txt`, `please`

### Requirement: Held-out test set
The test file SHALL contain hand-written phrasings, at least 3 per command intent and at least 10 for `unknown`. No test text SHALL appear in the training file, and test examples with slots SHALL use slot values that never appear in the training file.

#### Scenario: No overlap
- **WHEN** the training and test files are compared after lowercasing
- **THEN** no text appears in both

#### Scenario: Unseen slot values
- **WHEN** a test example has a slot value
- **THEN** that value does not occur as a slot value anywhere in the training file

### Requirement: Safe commands only
Every command in the dataset SHALL use a program on the dataset's allowlist and SHALL NOT match any blocklisted destructive pattern from the project safety rules (`rm`, `mkfs`, `dd`, `shutdown`, `reboot`, fork bombs, writes to `/dev/sd*`, `chmod -R 777 /`). Commands SHALL NOT use shell pipelines, redirection, or command substitution.

#### Scenario: Blocked program in catalog
- **WHEN** the catalog contains a template whose program is not on the allowlist, or that contains `|`, `>`, `<`, `;`, `&` or `$(`
- **THEN** generation fails with an error naming the intent and the offending template, and no files are written

### Requirement: Reproducible generation
Generating the dataset twice with the same seed SHALL produce byte-identical files.

#### Scenario: Same seed twice
- **WHEN** the generator runs twice with the same seed into two directories
- **THEN** every output file is identical between the two

### Requirement: Generation command
The project SHALL provide one command that generates the dataset into `data/commands/` and prints a summary (number of intents, training examples, test examples). Generated files SHALL NOT be tracked by git.

#### Scenario: Generate
- **WHEN** the generation command runs
- **THEN** `train.jsonl`, `test.jsonl` and `intents.json` exist in `data/commands/`, the summary shows at least 3,000 training examples, and `git status` shows no new files under `data/`

#### Scenario: Invalid catalog
- **WHEN** catalog validation fails
- **THEN** the command prints the problem and exits with a non-zero status without writing partial files

### Requirement: Everyday phrasing variants
When sampling training sentences, the generator SHALL randomly produce everyday variants of a phrasing: dropped articles, possessives (`me the` → `my`, and `the` → `my` before a listed personal noun), a dropped `me` after a verb, and a leading display verb swapped within its synonym group.

#### Scenario: Possessive variant
- **WHEN** the phrasing `show me the files` is sampled many times
- **THEN** some training sentences read `show my files`

#### Scenario: Terse variant
- **WHEN** the phrasing `list the biggest files` is sampled many times
- **THEN** some training sentences read `list biggest files`

### Requirement: Variants keep labels and held-out sets intact
Variants SHALL never change slot placeholders, SHALL be deduplicated against held-out sentences like every other training sentence, and SHALL never place the same sentence under two intents.

#### Scenario: Slots untouched
- **WHEN** a phrasing contains `{dir}` and a variant is applied
- **THEN** the variant still contains `{dir}` exactly once and its tags still mark the slot value

#### Scenario: Reproducible
- **WHEN** the generator runs twice with the same seed
- **THEN** the output files are byte-identical

### Requirement: The command itself as a phrasing
Each command intent SHALL also accept its own command typed as a request: the program and its plain-word arguments with slot placeholders, without options or numbers (`wc -l {file}` → `wc {file}`). Such a form SHALL be added only when no other intent's grammar or command produces the same words.

#### Scenario: Command name
- **WHEN** the training data is generated
- **THEN** `count_lines` has training sentences of the form `wc <file>`

#### Scenario: Ambiguous command name
- **WHEN** two intents' commands reduce to the same words (`df -h` and `df -i` both to `df`)
- **THEN** neither intent gets that form

### Requirement: Held-out sentences belong to one intent
A hand-written test or development sentence SHALL be listed under exactly one intent (compared lowercased); otherwise generation fails with an error naming both intents.

#### Scenario: Duplicate held-out sentence
- **WHEN** the same development sentence is listed under two intents
- **THEN** generation fails and names both intents
