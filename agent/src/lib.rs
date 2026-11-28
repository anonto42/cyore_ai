// Cyore's Stage 3 agent: turns plain-English requests into commands with the `intent` model,
// and runs each one only after a safety check and the user's explicit confirmation.
// See agent/README.md and the add-agent design.

pub mod agent;
pub mod memory;
pub mod runner;
pub mod safety;
pub mod steps;
pub mod tools;
