// Cyore's Stage 2 model: an intent classifier and slot tagger (one joint network) trained on
// the command dataset. The binary (main.rs) trains and evaluates it; the Stage 3 agent will use
// `Predictor` from this library. See intent/README.md and the add-intent-model design.

pub mod batch;
pub mod data;
pub mod metrics;
pub mod model;
pub mod predict;
pub mod random;
pub mod store;
pub mod train;
pub mod vocab;

#[cfg(test)]
mod tests;
