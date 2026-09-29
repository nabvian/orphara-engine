//! THE FROZEN CORE/DSD CONTRACT.
//!
//! The Blind Core programs against [`KnowledgeProvider`] and nothing
//! disease-specific. It must never contain a method like `check_g6pd()`.
//! Adding subfamilies changes the *implementation* (the DSD behind this trait),
//! never this trait and never the Core. That is what makes "the Core will not
//! change" structurally true rather than a promise.
//!
//! Every method returns borrowed slices so the same contract serves both the
//! in-memory provider (this crate) and, later, an mmap provider over a compiled
//! `.dsd` (slices into the mapped file). Nothing here forces a heap copy.

use crate::ids::{EdgeType, EvidenceId, NodeId, NodeKind, QualifierId};

/// A directed, typed, weighted edge in the knowledge graph.
#[derive(Clone, Copy, Debug)]
pub struct Edge {
    pub target: NodeId,
    pub edge_type: EdgeType,
    /// Information / discriminative weight: low for hub findings such as
    /// fatigue, high for specific discriminators. The Core uses it without
    /// knowing the medical meaning.
    pub weight: f32,
    pub qualifier: Option<QualifierId>,
    pub evidence: Option<EvidenceId>,
}

impl Edge {
    /// A plain typed edge with unit weight and no qualifier/evidence.
    pub fn new(target: NodeId, edge_type: EdgeType) -> Self {
        Edge {
            target,
            edge_type,
            weight: 1.0,
            qualifier: None,
            evidence: None,
        }
    }
}

/// The generic knowledge interface. Implemented by any knowledge source (the
/// in-memory graph here; an mmap `.dsd` loader later). The Core depends only on
/// this.
pub trait KnowledgeProvider {
    fn node_kind(&self, node: NodeId) -> Option<NodeKind>;
    fn name(&self, node: NodeId) -> Option<&str>;

    /// Forward adjacency (CSR): e.g. disease -> findings.
    fn neighbors(&self, node: NodeId) -> &[Edge];

    /// Reverse adjacency (CSC): e.g. finding -> diseases. This is the primitive
    /// high-recall candidate generation is built on.
    fn reverse_neighbors(&self, node: NodeId) -> &[Edge];

    /// IS_A ontology hierarchy: broader/narrower concepts.
    fn ontology_parents(&self, node: NodeId) -> &[NodeId];
    fn ontology_children(&self, node: NodeId) -> &[NodeId];

    /// Every disease node, for enumeration and abstention accounting.
    fn diseases(&self) -> &[NodeId];
}
