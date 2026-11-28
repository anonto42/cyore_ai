# Spec Delta

## Purpose

Turns one plain-English request into a terminal command: a trained model picks the intent (which command template), extracts slot values (files, folders, hosts…), reports its confidence, and fills in the command, as the Stage 2 milestone that the Stage 3 agent builds on.

## ADDED Requirements

### Requirement: Training from the command dataset
The training command SHALL read the generated command dataset, hold out a seeded validation split from the training file, train the joint intent and slot model, keep the epoch that does best on the hand-written development set, and save the weights together with everything needed to use them (vocabulary, intent list, tag list, model sizes) into the model directory. The hand-written test file SHALL NOT be used for training or for choosing hyperparameters or the saved epoch.

#### Scenario: Train with defaults
- **WHEN** the training command runs with the dataset present
- **THEN** it prints per-epoch training loss, validation intent accuracy and development accuracy, saves the model to the model directory, and prints the final metrics report

#### Scenario: Missing dataset
- **WHEN** the training command runs and the dataset files do not exist
- **THEN** it exits with a non-zero status and a message that includes `make commands-data`, without panicking

### Requirement: Baseline comparison
Training SHALL also train a bag-of-words dense intent classifier on the same split and report its validation, development and test intent accuracy next to the joint model's.

#### Scenario: Report shows both
- **WHEN** training completes
- **THEN** the report lists intent accuracy for both the baseline and the joint model

### Requirement: Unseen words
Words not in the vocabulary SHALL be mapped to shape classes, so unseen values that look like paths, file names, globs, numbers or IP addresses, or plain words are each represented by their own token rather than by one shared unknown token.

#### Scenario: Unseen file name
- **WHEN** a request contains a file name that never occurred in training, such as `budget.xlsx`
- **THEN** it is encoded as the file-name shape token, and the request is processed normally

### Requirement: Prediction
Given one request, prediction SHALL return the predicted intent, its confidence (the probability of the chosen intent, in [0, 1]), the slot values taken verbatim from the request's tokens, and the command filled from the intent's template. If the confidence is below the threshold, the intent is `unknown`, or a required slot has no value, prediction SHALL return no command and say why.

#### Scenario: Confident request
- **WHEN** prediction runs on `show me the files in Pictures` with a trained model
- **THEN** it returns intent `list_files`, slot `dir` = `Pictures` (original case kept), and command `ls -la Pictures`

#### Scenario: Low confidence
- **WHEN** the chosen intent's probability is below the threshold
- **THEN** no command is returned and the reason says the model is not sure, listing the top candidate intents

#### Scenario: Out-of-scope request
- **WHEN** the predicted intent is `unknown`
- **THEN** no command is returned and the reason says the request is not supported

#### Scenario: Robust input
- **WHEN** prediction runs on an empty string, only punctuation, or a 200-word input
- **THEN** it returns a result without panicking (empty input gives no command)

### Requirement: Saved model round trip
A saved model SHALL load into an identical model: predictions from the loaded model SHALL equal those from the model that was saved.

#### Scenario: Load and predict
- **WHEN** a trained model is saved, loaded again, and both predict on the same requests
- **THEN** intents, confidences (within 1e-6) and slots are identical

#### Scenario: Missing model
- **WHEN** evaluation or prediction runs and the model directory has no saved model
- **THEN** it exits with a non-zero status and a message that includes `make intent-train`

### Requirement: Metrics report
Evaluation SHALL report on the test set (and, on request, the development set): intent accuracy, slot token accuracy, command accuracy (predicted command string equals the expected command, where `unknown` counts as correct when both are null), `unknown` precision and recall, the five most frequent confused intent pairs, and accuracy and coverage at confidence thresholds 0.5, 0.7 and 0.9.

#### Scenario: Report contents
- **WHEN** evaluation runs on the test set with a trained model
- **THEN** every listed metric is printed, and the number of evaluated examples equals the number of lines in the test file

### Requirement: Quality targets
With default settings, the joint model SHALL reach at least 95% intent accuracy on the validation split, and at least 70% intent accuracy and 60% command accuracy on the hand-written test set.

#### Scenario: Default training run
- **WHEN** `make intent-train` and then `make intent-eval` run with the default dataset and settings
- **THEN** the reported numbers meet all three targets

### Requirement: Reproducible training
Two training runs with the same seed, data, and settings SHALL report identical per-epoch losses and metrics on the same machine.

#### Scenario: Same seed twice
- **WHEN** training runs twice with the same seed
- **THEN** the printed per-epoch losses and final metrics are identical
