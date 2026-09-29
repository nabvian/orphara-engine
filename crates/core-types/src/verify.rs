//! Value types for deterministic evidence verification and for the
//! evidence-channel model.
//!
//! These live in the shared contract so the Core is channel-aware and
//! abstention-aware from the very first version. The live sidecar is OFF in v1,
//! but because the channel type already exists, turning it on later needs no
//! change to the Core or to this contract.

use crate::ids::NodeId;

/// Evidence channel. DSD edges are `VerifiedLocal`. The external channels are
/// reserved for the future live sidecar; they exist now so the Core never has to
/// change to accommodate them.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Channel {
    VerifiedLocal,
    AuthoritativeExternal,
    EmergingExternal,
    ConflictingExternal,
    UnverifiedExternal,
}

/// Deterministic evidence-verification outcome. Kept strictly
/// separate from any ranking score — ranking is not a diagnosis, and
/// `Abstain` is an intentional, first-class result.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EvidenceOutcome {
    StronglySupported,
    Supported,
    Possible,
    InsufficientEvidence,
    ConflictingEvidence,
    Contradicted,
    Abstain,
}

/// Component evidence for one candidate. Deliberately kept as separate
/// components rather than one opaque score so every result is explainable.
#[derive(Clone, Debug)]
pub struct CandidateEvidence {
    pub disease: NodeId,
    pub phenotype_matches: u32,
    pub strong_discriminators: u32,
    pub contradictions: u32,
    pub required_missing: u32,
    pub local_support: f32,
    pub local_contradiction: f32,
}

impl CandidateEvidence {
    pub fn new(disease: NodeId) -> Self {
        CandidateEvidence {
            disease,
            phenotype_matches: 0,
            strong_discriminators: 0,
            contradictions: 0,
            required_missing: 0,
            local_support: 0.0,
            local_contradiction: 0.0,
        }
    }
}
