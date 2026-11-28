// The parts of the dataset crate other crates must share exactly: the tokenization rule,
// command templates, and the catalog itself. The `intent` model depends on this library so it
// tokenizes requests and fills commands precisely as the dataset did; the agent tests its
// allowlist against every command in the catalog.

pub mod catalog;
pub mod template;
pub mod tokenize;
