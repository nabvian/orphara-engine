//! An in-memory [`KnowledgeProvider`] implementing the frozen contract.
//!
//! It is used by tests and by the sample dataset before the binary `.dsd` loader
//! exists, so the Core can be developed and exercised against the real contract
//! immediately. It carries no medical knowledge of its own — callers load nodes
//! and edges into it.

use std::collections::HashMap;

use crate::ids::{NodeId, NodeKind};
use crate::knowledge::{Edge, KnowledgeProvider};

#[derive(Default)]
pub struct MemKnowledge {
    kinds: HashMap<NodeId, NodeKind>,
    names: HashMap<NodeId, String>,
    fwd: HashMap<NodeId, Vec<Edge>>,
    rev: HashMap<NodeId, Vec<Edge>>,
    parents: HashMap<NodeId, Vec<NodeId>>,
    children: HashMap<NodeId, Vec<NodeId>>,
    diseases: Vec<NodeId>,
    empty_edges: Vec<Edge>,
    empty_nodes: Vec<NodeId>,
}

impl MemKnowledge {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_node(&mut self, id: NodeId, kind: NodeKind, name: &str) {
        self.kinds.insert(id, kind);
        self.names.insert(id, name.to_string());
        if kind == NodeKind::Disease && !self.diseases.contains(&id) {
            self.diseases.push(id);
        }
    }

    /// Add a forward edge `from -> edge.target`; the matching reverse edge
    /// (`edge.target -> from`) is derived automatically.
    pub fn add_edge(&mut self, from: NodeId, edge: Edge) {
        self.fwd.entry(from).or_default().push(edge);
        let back = Edge {
            target: from,
            ..edge
        };
        self.rev.entry(edge.target).or_default().push(back);
    }

    pub fn add_isa(&mut self, child: NodeId, parent: NodeId) {
        self.parents.entry(child).or_default().push(parent);
        self.children.entry(parent).or_default().push(child);
    }
}

impl KnowledgeProvider for MemKnowledge {
    fn node_kind(&self, node: NodeId) -> Option<NodeKind> {
        self.kinds.get(&node).copied()
    }

    fn name(&self, node: NodeId) -> Option<&str> {
        self.names.get(&node).map(|s| s.as_str())
    }

    fn neighbors(&self, node: NodeId) -> &[Edge] {
        self.fwd
            .get(&node)
            .map(|v| v.as_slice())
            .unwrap_or(self.empty_edges.as_slice())
    }

    fn reverse_neighbors(&self, node: NodeId) -> &[Edge] {
        self.rev
            .get(&node)
            .map(|v| v.as_slice())
            .unwrap_or(self.empty_edges.as_slice())
    }

    fn ontology_parents(&self, node: NodeId) -> &[NodeId] {
        self.parents
            .get(&node)
            .map(|v| v.as_slice())
            .unwrap_or(self.empty_nodes.as_slice())
    }

    fn ontology_children(&self, node: NodeId) -> &[NodeId] {
        self.children
            .get(&node)
            .map(|v| v.as_slice())
            .unwrap_or(self.empty_nodes.as_slice())
    }

    fn diseases(&self) -> &[NodeId] {
        &self.diseases
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::EdgeType;

    /// Build a tiny, non-medical graph and prove the reverse-index
    /// retrieval primitive works purely through the frozen contract — no
    /// disease-specific code anywhere. Node 1 is a disease, node 2 a finding.
    #[test]
    fn reverse_lookup_via_contract() {
        let mut k = MemKnowledge::new();
        k.add_node(NodeId(1), NodeKind::Disease, "disease-1");
        k.add_node(NodeId(2), NodeKind::Finding, "finding-2");
        k.add_edge(NodeId(1), Edge::new(NodeId(2), EdgeType::HasPhenotype));

        // finding -> disease, the high-recall primitive
        let back: Vec<NodeId> = k.reverse_neighbors(NodeId(2)).iter().map(|e| e.target).collect();
        assert_eq!(back, vec![NodeId(1)]);

        assert_eq!(k.diseases(), &[NodeId(1)]);
        assert_eq!(k.node_kind(NodeId(2)), Some(NodeKind::Finding));
        assert_eq!(k.name(NodeId(1)), Some("disease-1"));
        assert!(k.neighbors(NodeId(999)).is_empty());
    }

    #[test]
    fn ontology_hierarchy() {
        let mut k = MemKnowledge::new();
        k.add_node(NodeId(20), NodeKind::Finding, "anemia");
        k.add_node(NodeId(21), NodeKind::Finding, "microcytic-anemia");
        k.add_isa(NodeId(21), NodeId(20));
        assert_eq!(k.ontology_parents(NodeId(21)), &[NodeId(20)]);
        assert_eq!(k.ontology_children(NodeId(20)), &[NodeId(21)]);
    }
}
