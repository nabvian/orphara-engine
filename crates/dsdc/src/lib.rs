//! The DSD compiler.
//!
//! A builder assembles the knowledge graph and hands out dense `NodeId`s,
//! validates every reference, flattens to CSR, and emits the compiled `.dsd` via
//! [`dsd::format`]. Concepts relate through typed edges (has_finding, requires,
//! triggered_by, treated_by, discriminates, …). No doses: management is carried
//! as named nodes. A `.dsdsrc` text parser sits on top of the builder in
//! [`source`].

pub mod source;

use core_types::{Edge, NodeId, NodeKind};
use dsd::format::{serialize, CompiledGraph};

#[derive(Debug, PartialEq, Eq)]
pub enum CompileError {
    UnknownNode(u32),
}

impl std::fmt::Display for CompileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CompileError::UnknownNode(id) => write!(f, "edge references unknown node id {id}"),
        }
    }
}
impl std::error::Error for CompileError {}

/// Authoring builder. Nodes get dense ids in creation order; edges are validated
/// as they are added, so a compiled package can never contain a dangling
/// reference.
#[derive(Debug, Default)]
pub struct GraphBuilder {
    kinds: Vec<NodeKind>,
    names: Vec<String>,
    fwd: Vec<Vec<Edge>>,
    rev: Vec<Vec<Edge>>,
    parents: Vec<Vec<u32>>,
    children: Vec<Vec<u32>>,
    diseases: Vec<u32>,
}

impl GraphBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_node(&mut self, kind: NodeKind, name: &str) -> NodeId {
        let id = self.kinds.len() as u32;
        self.kinds.push(kind);
        self.names.push(name.to_string());
        self.fwd.push(Vec::new());
        self.rev.push(Vec::new());
        self.parents.push(Vec::new());
        self.children.push(Vec::new());
        if kind == NodeKind::Disease {
            self.diseases.push(id);
        }
        NodeId(id)
    }

    fn check(&self, n: NodeId) -> Result<(), CompileError> {
        if (n.0 as usize) < self.kinds.len() {
            Ok(())
        } else {
            Err(CompileError::UnknownNode(n.0))
        }
    }

    /// Add a forward edge `from -> edge.target`; the reverse edge is derived.
    pub fn add_edge(&mut self, from: NodeId, edge: Edge) -> Result<(), CompileError> {
        self.check(from)?;
        self.check(edge.target)?;
        self.fwd[from.0 as usize].push(edge);
        let back = Edge {
            target: from,
            ..edge
        };
        self.rev[edge.target.0 as usize].push(back);
        Ok(())
    }

    pub fn add_isa(&mut self, child: NodeId, parent: NodeId) -> Result<(), CompileError> {
        self.check(child)?;
        self.check(parent)?;
        self.parents[child.0 as usize].push(parent.0);
        self.children[parent.0 as usize].push(child.0);
        Ok(())
    }

    /// Compile to the DSD binary.
    pub fn compile(&self) -> Vec<u8> {
        serialize(&self.to_compiled())
    }

    fn to_compiled(&self) -> CompiledGraph {
        let mut fwd_off = vec![0u32];
        let mut fwd_edges = Vec::new();
        for e in &self.fwd {
            fwd_edges.extend_from_slice(e);
            fwd_off.push(fwd_edges.len() as u32);
        }
        let mut rev_off = vec![0u32];
        let mut rev_edges = Vec::new();
        for e in &self.rev {
            rev_edges.extend_from_slice(e);
            rev_off.push(rev_edges.len() as u32);
        }
        let mut par_off = vec![0u32];
        let mut par_nodes = Vec::new();
        for p in &self.parents {
            par_nodes.extend_from_slice(p);
            par_off.push(par_nodes.len() as u32);
        }
        let mut chi_off = vec![0u32];
        let mut chi_nodes = Vec::new();
        for c in &self.children {
            chi_nodes.extend_from_slice(c);
            chi_off.push(chi_nodes.len() as u32);
        }
        CompiledGraph {
            kinds: self.kinds.clone(),
            names: self.names.clone(),
            fwd_off,
            fwd_edges,
            rev_off,
            rev_edges,
            par_off,
            par_nodes,
            chi_off,
            chi_nodes,
            diseases: self.diseases.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core_types::{Edge, EdgeType, MemKnowledge, NodeKind, Observation, PatientGraph};
    use dsd::Dsd;
    use reasoning::assess;

    /// Build the same graph two ways — once compiled to a `.dsd` and loaded, once
    /// in memory — and assert the Blind Core produces an identical assessment.
    /// This is the proof that the Core does not change across knowledge backends.
    #[test]
    fn core_reads_compiled_dsd_identically_to_memory() {
        // --- author via the compiler ---
        let mut b = GraphBuilder::new();
        let d = b.add_node(NodeKind::Disease, "disease");
        let p1 = b.add_node(NodeKind::Finding, "finding-a");
        let p2 = b.add_node(NodeKind::Finding, "finding-b");
        let disc = b.add_node(NodeKind::Finding, "discriminator");
        let req = b.add_node(NodeKind::Test, "confirmatory-test");
        b.add_edge(d, Edge::new(p1, EdgeType::HasPhenotype)).unwrap();
        b.add_edge(d, Edge::new(p2, EdgeType::HasPhenotype)).unwrap();
        b.add_edge(d, Edge::new(disc, EdgeType::Discriminates)).unwrap();
        b.add_edge(d, Edge::new(req, EdgeType::Requires)).unwrap();
        let bytes = b.compile();
        let dsd = Dsd::from_bytes(&bytes).unwrap();

        // --- same graph in memory ---
        let mut mem = MemKnowledge::new();
        mem.add_node(d, NodeKind::Disease, "disease");
        mem.add_node(p1, NodeKind::Finding, "finding-a");
        mem.add_node(p2, NodeKind::Finding, "finding-b");
        mem.add_node(disc, NodeKind::Finding, "discriminator");
        mem.add_node(req, NodeKind::Test, "confirmatory-test");
        mem.add_edge(d, Edge::new(p1, EdgeType::HasPhenotype));
        mem.add_edge(d, Edge::new(p2, EdgeType::HasPhenotype));
        mem.add_edge(d, Edge::new(disc, EdgeType::Discriminates));
        mem.add_edge(d, Edge::new(req, EdgeType::Requires));

        // --- same patient, both providers ---
        let mut patient = PatientGraph::new();
        for c in [p1, p2, disc, req] {
            patient.push(Observation::present(c));
        }

        let a_mem = assess(&patient, &mem);
        let a_dsd = assess(&patient, &dsd);

        assert!(!a_mem.abstained && !a_dsd.abstained);
        assert_eq!(a_mem.candidates[0].evidence.disease, a_dsd.candidates[0].evidence.disease);
        assert_eq!(a_mem.candidates[0].outcome, a_dsd.candidates[0].outcome);
        // and it is actually the disease we authored, strongly supported
        assert_eq!(a_dsd.candidates[0].evidence.disease, d);
    }
}
