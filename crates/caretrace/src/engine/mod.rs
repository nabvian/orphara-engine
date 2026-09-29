//! The deterministic audit engines: change / conflict / claim-resolution / gap
//! detection. Each is a pure function over records and the active packs; none
//! selects a winning value or asserts a clinical judgement.

pub mod changes;
pub mod claims;
pub mod conflicts;
pub mod gaps;
