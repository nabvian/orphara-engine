//! The hybrid engine: **rules + GNN**.
//!
//! Two signals, kept separate:
//! * the deterministic rule verifier (`reasoning`) decides the *outcome*
//!   (StronglySupported … Contradicted, and abstention) — this is the safety
//!   gate;
//! * the `gnn` produces a *ranking score* only.
//!
//! Composition law: **the rule outcome tiers candidates; the GNN orders within a
//! tier and breaks ties.** A GNN preference can never lift a contradicted or
//! abstained candidate above a supported one. If no model is supplied, the
//! engine degrades to rules-only with no change in behaviour or safety.

use std::collections::HashMap;

use core_types::{EvidenceOutcome, KnowledgeProvider, NodeId, PatientGraph};
use gnn::GnnModel;
use reasoning::CandidateResult;

/// One candidate with both signals attached.
#[derive(Debug)]
pub struct HybridCandidate {
    pub rule: CandidateResult,
    pub gnn_score: Option<f32>,
}

#[derive(Debug)]
pub struct HybridAssessment {
    pub candidates: Vec<HybridCandidate>,
    pub abstained: bool,
}

/// Higher tier = more accepted. This is the safety ordering; the GNN never
/// crosses it.
fn tier(o: EvidenceOutcome) -> u8 {
    match o {
        EvidenceOutcome::StronglySupported => 6,
        EvidenceOutcome::Supported => 5,
        EvidenceOutcome::Possible => 4,
        EvidenceOutcome::ConflictingEvidence => 3,
        EvidenceOutcome::InsufficientEvidence => 2,
        EvidenceOutcome::Contradicted => 1,
        EvidenceOutcome::Abstain => 0,
    }
}

/// Run the hybrid engine. `model` is optional — `None` means rules-only.
pub fn run(
    patient: &PatientGraph,
    k: &dyn KnowledgeProvider,
    model: Option<&GnnModel>,
) -> HybridAssessment {
    let rule = reasoning::assess(patient, k);

    let scores: HashMap<NodeId, f32> = match model {
        Some(m) => {
            let present = reasoning::candidates::expand_present(patient, k);
            m.score_diseases(k, &present).into_iter().collect()
        }
        None => HashMap::new(),
    };

    let mut candidates: Vec<HybridCandidate> = rule
        .candidates
        .into_iter()
        .map(|c| {
            let gnn_score = scores.get(&c.evidence.disease).copied();
            HybridCandidate { rule: c, gnn_score }
        })
        .collect();

    candidates.sort_by(|a, b| {
        // 1) rule tier (the gate) 2) GNN score 3) phenotype matches — all descending
        tier(b.rule.outcome)
            .cmp(&tier(a.rule.outcome))
            .then_with(|| {
                b.gnn_score
                    .unwrap_or(f32::MIN)
                    .total_cmp(&a.gnn_score.unwrap_or(f32::MIN))
            })
            .then_with(|| b.rule.evidence.phenotype_matches.cmp(&a.rule.evidence.phenotype_matches))
    });

    HybridAssessment {
        candidates,
        abstained: rule.abstained,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core_types::{Edge, EdgeType, MemKnowledge, NodeKind, Observation};

    fn present(cs: &[u32]) -> PatientGraph {
        let mut p = PatientGraph::new();
        for &c in cs {
            p.push(Observation::present(NodeId(c)));
        }
        p
    }

    /// The safety property: whatever the GNN thinks, a rule-contradicted disease
    /// never outranks a rule-supported one.
    #[test]
    fn rules_gate_over_gnn() {
        let mut k = MemKnowledge::new();
        k.add_node(NodeId(0), NodeKind::Disease, "A");
        k.add_node(NodeId(1), NodeKind::Disease, "B");
        for f in [10u32, 11, 12, 14] {
            k.add_node(NodeId(f), NodeKind::Finding, "f");
        }
        // A: two phenotypes  ->  Supported when both present
        k.add_edge(NodeId(0), Edge::new(NodeId(10), EdgeType::HasPhenotype));
        k.add_edge(NodeId(0), Edge::new(NodeId(11), EdgeType::HasPhenotype));
        // B: one phenotype + one contradiction  ->  Contradicted when contra present
        k.add_edge(NodeId(1), Edge::new(NodeId(12), EdgeType::HasPhenotype));
        k.add_edge(NodeId(1), Edge::new(NodeId(14), EdgeType::ContradictedBy));

        // present: A's two findings, plus B's finding AND B's contradiction
        let patient = present(&[10, 11, 12, 14]);

        // an untrained model gives arbitrary GNN scores — the gate must hold anyway
        let model = gnn::init(7);
        let a = run(&patient, &k, Some(&model));

        let pos = |d: NodeId| a.candidates.iter().position(|c| c.rule.evidence.disease == d).unwrap();
        assert_eq!(a.candidates[0].rule.evidence.disease, NodeId(0));
        assert_eq!(a.candidates[0].rule.outcome, EvidenceOutcome::Supported);
        assert!(pos(NodeId(0)) < pos(NodeId(1)), "contradicted B outranked supported A");
        let b = a.candidates.iter().find(|c| c.rule.evidence.disease == NodeId(1)).unwrap();
        assert_eq!(b.rule.outcome, EvidenceOutcome::Contradicted);
    }

    /// Within the same rule tier, the GNN score orders the candidates.
    #[test]
    fn gnn_orders_within_a_tier() {
        let mut k = MemKnowledge::new();
        k.add_node(NodeId(0), NodeKind::Disease, "A");
        k.add_node(NodeId(1), NodeKind::Disease, "B");
        for f in [10u32, 11, 12, 13] {
            k.add_node(NodeId(f), NodeKind::Finding, "f");
        }
        k.add_edge(NodeId(0), Edge::new(NodeId(10), EdgeType::HasPhenotype));
        k.add_edge(NodeId(0), Edge::new(NodeId(11), EdgeType::HasPhenotype));
        k.add_edge(NodeId(1), Edge::new(NodeId(12), EdgeType::HasPhenotype));
        k.add_edge(NodeId(1), Edge::new(NodeId(13), EdgeType::HasPhenotype));

        // both diseases Supported (same tier)
        let patient = present(&[10, 11, 12, 13]);
        let model = gnn::init(3);
        let a = run(&patient, &k, Some(&model));

        assert!(a.candidates.iter().all(|c| c.rule.outcome == EvidenceOutcome::Supported));
        // the ordering follows the GNN score, descending
        let s0 = a.candidates[0].gnn_score.unwrap();
        let s1 = a.candidates[1].gnn_score.unwrap();
        assert!(s0 >= s1, "within-tier order did not follow GNN score: {s0} < {s1}");
    }
}
