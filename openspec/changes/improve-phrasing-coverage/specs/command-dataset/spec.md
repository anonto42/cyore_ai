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
