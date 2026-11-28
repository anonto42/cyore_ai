// The parts of the dataset crate other crates must share exactly: the tokenization rule and
// command templates. The `intent` model (and later the agent) depend on this library so they
// tokenize requests and fill commands precisely as the dataset did.

pub mod template;
pub mod tokenize;
