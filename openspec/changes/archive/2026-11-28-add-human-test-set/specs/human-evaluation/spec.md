# Spec Delta

## Purpose

Lets a person other than the catalog's author write an independent test set in plain text, and scores the model on it without exposing individual sentences, so the Stage 2 model's quality can be judged honestly.

## ADDED Requirements

### Requirement: Human test file format
The file `dataset/human_test.txt` SHALL hold one request per line: an intent name, whitespace, then the sentence, with each slot value written as `[slot: value]`. Lines that are empty or start with `#` SHALL be ignored.

#### Scenario: Line with a slot
- **WHEN** the line `count_lines  how many lines are in [file: notes.txt]` is read
- **THEN** it yields the text `how many lines are in notes.txt`, intent `count_lines`, slot `file` = `notes.txt`, tag `B-file` on that token, and command `wc -l notes.txt`

#### Scenario: Template only
- **WHEN** the file contains only comments
- **THEN** no human test set is written and the other dataset files are unchanged

### Requirement: Human test validation
Every line SHALL be checked, and all problems SHALL be reported with their line numbers before anything is written: unknown intent, malformed brackets, a slot the intent's command does not have, a missing required slot, a slot value that is not one token, contains shell syntax or starts with `-`, a slot on `unknown`, or a duplicate sentence.

#### Scenario: Wrong slot
- **WHEN** a line for `disk_space` contains `[file: a.txt]`
- **THEN** generation fails with an error naming the line number and the slot

### Requirement: Held out from training
Human test sentences SHALL be excluded from the generated training data in the same way as development and test sentences.

#### Scenario: Excluded
- **WHEN** a human sentence equals a sentence the generator would produce
- **THEN** that sentence is not in `train.jsonl`

### Requirement: Report-only scoring
Scoring the human set SHALL print only aggregate metrics (the standard report) and SHALL never print an individual human sentence. It SHALL warn when the model directory is older than the human test file, and SHALL report how many human sentences also occur in the development or test sets.

#### Scenario: Score
- **WHEN** `make intent-eval-human` runs with sentences in the file and a trained model
- **THEN** the aggregate report is printed, and no line of the output contains a human test sentence

#### Scenario: Stale model
- **WHEN** the human test file was changed after the model was trained
- **THEN** the report starts with a warning to retrain for a clean score

#### Scenario: No sentences yet
- **WHEN** the file has no sentences
- **THEN** the command says so, names `dataset/human_test.txt`, and exits with status 0 without scoring
