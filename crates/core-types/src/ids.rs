//! Identifier and category types for the knowledge graph.
//!
//! `NodeId` is the graph identity; `Offset` and the evidence/source/qualifier
//! ids address sections of a compiled `.dsd`. NodeIds are allocated from one
//! global namespace so packages compiled separately compose without collision.

/// Global graph identity.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
pub struct NodeId(pub u32);

/// A byte offset / position inside a compiled `.dsd`. `u64` because the file can
/// exceed the addressable node count even when node count stays well below 2^32.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
pub struct Offset(pub u64);

/// Reference into the evidence sections.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct EvidenceId(pub u32);

/// Reference into the source registry.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct SourceId(pub u32);

/// Reference into the edge-qualifier section (age / stage / trigger / treatment state).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct QualifierId(pub u32);

/// The class of a graph node. Generic biomedical categories only — the Core
/// reasons over these without knowing any specific disease.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum NodeKind {
    Disease,
    Phenotype,
    Finding,
    Test,
    Gene,
    Variant,
    Protein,
    Pathway,
    Mechanism,
    Trigger,
    Pattern,
    Drug,
    Treatment,
    Anatomy,
    Evidence,
    Source,
}

/// Relationship type carried on an edge.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum EdgeType {
    HasPhenotype,
    HasFinding,
    HasPattern,
    AssociatedWith,
    CausedBy,
    ParticipatesIn,
    TriggeredBy,
    TreatedBy,
    SupportedBy,
    ContradictedBy,
    Requires,
    Excludes,
    Discriminates,
    Mimics,
    IsA,
    /// `result -> confounding state`: the source result is unreliable when the
    /// target state co-occurs within a window (the edge `weight` holds the window
    /// in days). Used by the temporal pass, not by the snapshot verifier.
    ConfoundedBy,
}
