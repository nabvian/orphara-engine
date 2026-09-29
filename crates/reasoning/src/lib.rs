//! Deterministic diagnosis-support reasoning over the `KnowledgeProvider`
//! contract. No medical knowledge lives here; the same functions run over any
//! provider.
//!
//! Pipeline: candidate generation -> evidence verification -> ranking (an
//! ordering, not a probability) -> abstention. This is the rule half; the GNN is
//! the `gnn` crate and `engine` composes the two.

pub mod assess;
pub mod candidates;

pub use assess::{assess, Assessment, CandidateResult};
