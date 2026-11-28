# Spec Delta

## ADDED Requirements

### Requirement: Development set
The dataset SHALL include a second hand-written held-out file, `dev.jsonl`, with at least 3 phrasings per command intent and at least 10 for `unknown`, written independently of the test file. It is the only held-out data that may guide decisions about the model or the catalog. No development text SHALL appear in the training or test files, and development examples with slots SHALL use slot values that never appear in the training file.

#### Scenario: Disjoint from training and test
- **WHEN** the development file is compared with the training and test files after lowercasing
- **THEN** no text appears in more than one file

#### Scenario: Too few development sentences
- **WHEN** the catalog has a command intent with fewer than 3 development sentences
- **THEN** generation fails with an error naming the intent

## MODIFIED Requirements

### Requirement: Held-out test set
The test file SHALL contain hand-written phrasings, at least 3 per command intent and at least 10 for `unknown`. No test text SHALL appear in the training or development files, and test examples with slots SHALL use slot values that never appear in the training file. The test file is used only to report final quality, never to choose catalog content, model settings, or the saved epoch.

#### Scenario: No overlap
- **WHEN** the training and test files are compared after lowercasing
- **THEN** no text appears in both

#### Scenario: Unseen slot values
- **WHEN** a test example has a slot value
- **THEN** that value does not occur as a slot value anywhere in the training file

### Requirement: Generation command
The project SHALL provide one command that generates the dataset into `data/commands/` and prints a summary (number of intents, training, development and test examples). Generated files SHALL NOT be tracked by git.

#### Scenario: Generate
- **WHEN** the generation command runs
- **THEN** `train.jsonl`, `dev.jsonl`, `test.jsonl` and `intents.json` exist in `data/commands/`, the summary shows at least 3,000 training examples, and `git status` shows no new files under `data/`

#### Scenario: Invalid catalog
- **WHEN** catalog validation fails
- **THEN** the command prints the problem and exits with a non-zero status without writing partial files
