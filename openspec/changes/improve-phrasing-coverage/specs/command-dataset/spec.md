# Spec Delta

## ADDED Requirements

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
