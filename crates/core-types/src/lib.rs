//! `core-types` — the frozen, disease-blind contract shared by the DSD and the
//! Blind Core.
//!
//! # Invariant
//! This crate and the [`KnowledgeProvider`] trait are the stable boundary.
//! Knowledge lives behind the trait, in the DSD. The Core is built against the
//! trait and does not change when subfamilies are added.
//!
//! # Current scope (version 0.1.0)
//! The hybrid GNN (crate `gnn`) uses this same contract. The live evidence sidecar and
//! the cross-family router are reserved for later — behind optional DSD sections and
//! additional crates, never by altering this contract or the Core. See
//! `ENGINE_INVARIANTS.md`.

pub mod ids;
pub mod knowledge;
pub mod mem;
pub mod patient;
pub mod verify;

pub use ids::*;
pub use knowledge::*;
pub use mem::*;
pub use patient::*;
pub use verify::*;
