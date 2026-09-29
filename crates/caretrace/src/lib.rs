//! Deterministic multi-report clinical extraction and audit.
//!
//! PDF/text ingestion, layout-agnostic UCUM-first extraction, fact/claim/
//! medication separation, and change/conflict/gap detection with provenance.
//! The text pipeline is deterministic and byte-reproducible.
//!
//! Layers:
//! - [`ingest`] — the format-aware stage (PDF / text / image);
//! - [`units`] / [`normalize`] / [`packs`] — vocabulary and grammar;
//! - `extract` / `extract_generic` — the rule extractor;
//! - [`engine`] — change / conflict / claim-resolution / gap detection;
//! - `pipeline` — the staged orchestrator;
//! - `terminology` — an optional, user-activated coding layer, advisory only.

use once_cell::sync::Lazy;
use regex::Regex;

/// Collapse whitespace runs to single spaces and trim — a common step.
pub fn normalize_ws(s: &str) -> String {
    static WS: Lazy<Regex> = Lazy::new(|| Regex::new(r"\s+").unwrap());
    WS.replace_all(s.trim(), " ").to_string()
}

pub mod date;
pub mod engine;
pub mod extract;
pub mod extract_generic;
pub mod hash;
pub mod ingest;
pub mod models;
pub mod normalize;
pub mod packs;
pub mod pipeline;
pub mod terminology;
pub mod text;
pub mod units;
