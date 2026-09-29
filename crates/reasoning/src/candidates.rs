//! Candidate generation: pull back diseases linked to the present concepts and
//! their ontology ancestors, via the reverse index. Wide by design; the verifier
//! applies precision later.

use std::collections::HashSet;

use core_types::{EvidenceState, KnowledgeProvider, NodeId, NodeKind, PatientGraph};

/// The set of present concepts, expanded upward through the IS_A ontology so a
/// disease annotated with a broader concept (e.g. "anaemia") is still matched by
/// a narrower patient finding (e.g. "microcytic anaemia")..
pub fn expand_present(patient: &PatientGraph, k: &dyn KnowledgeProvider) -> HashSet<NodeId> {
    let mut set = HashSet::new();
    for o in patient
        .observations
        .iter()
        .filter(|o| o.state == EvidenceState::Present)
    {
        collect_ancestors(o.concept, k, &mut set);
    }
    set
}

fn collect_ancestors(n: NodeId, k: &dyn KnowledgeProvider, set: &mut HashSet<NodeId>) {
    if !set.insert(n) {
        return; // already visited — also breaks any cycle
    }
    for &parent in k.ontology_parents(n) {
        collect_ancestors(parent, k, set);
    }
}

/// Every disease reachable from a present concept through the reverse index.
/// Deterministic order: first appearance while scanning the present set.
pub fn candidate_diseases(present: &HashSet<NodeId>, k: &dyn KnowledgeProvider) -> Vec<NodeId> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    // Sort the present concepts so retrieval order does not depend on HashSet
    // iteration order — determinism is a hard requirement.
    let mut concepts: Vec<NodeId> = present.iter().copied().collect();
    concepts.sort();
    for c in concepts {
        for e in k.reverse_neighbors(c) {
            if k.node_kind(e.target) == Some(NodeKind::Disease) && seen.insert(e.target) {
                out.push(e.target);
            }
        }
    }
    out
}
