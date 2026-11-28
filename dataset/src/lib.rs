// The parts of the dataset crate other crates must share exactly: the tokenization rule,
// command templates, and the catalog itself. The `intent` model depends on this library so it
// tokenizes requests and fills commands precisely as the dataset did; the agent tests its
// allowlist against every command in the catalog. `human` reads a test set written by a person.

pub mod catalog;
pub mod human;
pub mod template;
pub mod tokenize;
