//! Deterministic evidence verification + abstention.
//!
//! Ranking here is a transparent ordering of candidates, never a calibrated
//! probability. Every result carries its components so it can be
//! explained. `Abstain` is a first-class outcome, not a failure.

use std::collections::HashSet;

use core_types::{
    CandidateEvidence, EdgeType, EvidenceOutcome, KnowledgeProvider, NodeId, PatientGraph,
};

use crate::candidates::{candidate_diseases, expand_present};

/// Verification result for one candidate disease.
#[derive(Clone, Debug)]
pub struct CandidateResult {
    pub evidence: CandidateEvidence,
    pub outcome: EvidenceOutcome,
    pub matched: Vec<NodeId>,
    pub contradicting: Vec<NodeId>,
    pub missing_required: Vec<NodeId>,
}

/// The whole assessment: candidates ranked most- to least-supported, plus an
/// explicit abstention flag when nothing reaches acceptance.
#[derive(Clone, Debug)]
pub struct Assessment {
    pub candidates: Vec<CandidateResult>,
    pub abstained: bool,
}

/// Run the Blind Core over a patient graph against any knowledge provider.
///
/// The provider is a trait object, so this exact function works over the
/// in-memory graph today and over an mmap `.dsd` loader later, unchanged — which
/// is the whole point of the frozen contract.
pub fn assess(patient: &PatientGraph, k: &dyn KnowledgeProvider) -> Assessment {
    let present = expand_present(patient, k);
    let diseases = candidate_diseases(&present, k);

    let mut candidates: Vec<CandidateResult> =
        diseases.iter().map(|&d| evaluate(d, &present, k)).collect();

    candidates.sort_by_key(|r| std::cmp::Reverse(rank_key(r)));

    let abstained = candidates.is_empty()
        || candidates.iter().all(|r| {
            matches!(
                r.outcome,
                EvidenceOutcome::Abstain
                    | EvidenceOutcome::InsufficientEvidence
                    | EvidenceOutcome::Contradicted
            )
        });

    Assessment {
        candidates,
        abstained,
    }
}

fn evaluate(disease: NodeId, present: &HashSet<NodeId>, k: &dyn KnowledgeProvider) -> CandidateResult {
    let mut ev = CandidateEvidence::new(disease);
    let mut matched = Vec::new();
    let mut contradicting = Vec::new();
    let mut missing_required = Vec::new();

    for e in k.neighbors(disease) {
        let target_present = present.contains(&e.target);
        match e.edge_type {
            EdgeType::ContradictedBy | EdgeType::Excludes => {
                if target_present {
                    ev.contradictions += 1;
                    ev.local_contradiction += e.weight;
                    contradicting.push(e.target);
                }
            }
            EdgeType::Requires => {
                if target_present {
                    ev.phenotype_matches += 1;
                    ev.local_support += e.weight;
                    matched.push(e.target);
                } else {
                    // Not confirmed present. This is NOT a contradiction — it is
                    // missing required evidence, a softer signal.
                    ev.required_missing += 1;
                    missing_required.push(e.target);
                }
            }
            EdgeType::Discriminates => {
                if target_present {
                    ev.strong_discriminators += 1;
                    ev.phenotype_matches += 1;
                    ev.local_support += e.weight;
                    matched.push(e.target);
                }
            }
            EdgeType::HasPhenotype
            | EdgeType::HasFinding
            | EdgeType::HasPattern
            | EdgeType::AssociatedWith
            | EdgeType::CausedBy
            | EdgeType::TriggeredBy
            | EdgeType::ParticipatesIn => {
                if target_present {
                    ev.phenotype_matches += 1;
                    ev.local_support += e.weight;
                    matched.push(e.target);
                }
            }
            // Not scored as diagnostic support here. `ConfoundedBy` is a
            // timeline-only relation read by the temporal pass, never by the
            // snapshot verifier — so it can never move a ranking.
            EdgeType::TreatedBy
            | EdgeType::SupportedBy
            | EdgeType::Mimics
            | EdgeType::IsA
            | EdgeType::ConfoundedBy => {}
        }
    }

    let outcome = classify(&ev);
    CandidateResult {
        evidence: ev,
        outcome,
        matched,
        contradicting,
        missing_required,
    }
}

/// Deterministic mapping from evidence components to an outcome. The thresholds
/// are generic reasoning parameters, not medical constants, and are expected to
/// be tuned by benchmarking — never hard-coded per disease.
fn classify(ev: &CandidateEvidence) -> EvidenceOutcome {
    if ev.phenotype_matches == 0 {
        return EvidenceOutcome::InsufficientEvidence;
    }
    if ev.contradictions > 0 {
        // A present contradiction blocks confident acceptance. If it
        // outweighs support it is a contradiction; otherwise mixed evidence.
        if ev.local_contradiction >= ev.local_support {
            return EvidenceOutcome::Contradicted;
        }
        return EvidenceOutcome::ConflictingEvidence;
    }
    if ev.required_missing > 0 {
        return EvidenceOutcome::Possible;
    }
    if ev.strong_discriminators >= 1 && ev.phenotype_matches >= 2 {
        return EvidenceOutcome::StronglySupported;
    }
    if ev.phenotype_matches >= 2 {
        return EvidenceOutcome::Supported;
    }
    EvidenceOutcome::Possible
}

/// Transparent ranking key (outcome, discriminators, matches, fewer
/// contradictions). This is an ordering, not a probability.
fn rank_key(r: &CandidateResult) -> (u8, u32, u32, i64) {
    let outcome_rank = match r.outcome {
        EvidenceOutcome::StronglySupported => 6,
        EvidenceOutcome::Supported => 5,
        EvidenceOutcome::Possible => 4,
        EvidenceOutcome::ConflictingEvidence => 3,
        EvidenceOutcome::InsufficientEvidence => 2,
        EvidenceOutcome::Contradicted => 1,
        EvidenceOutcome::Abstain => 0,
    };
    (
        outcome_rank,
        r.evidence.strong_discriminators,
        r.evidence.phenotype_matches,
        -(r.evidence.contradictions as i64),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use core_types::{Edge, MemKnowledge, NodeKind, Observation};

    /// A non-medical fixture: one disease with two plain
    /// phenotypes, one strong discriminator, one required finding, one
    /// contradiction. Node numbers are opaque — the Core reasons with no
    /// disease knowledge of its own.
    fn fixture() -> MemKnowledge {
        let mut k = MemKnowledge::new();
        k.add_node(NodeId(1), NodeKind::Disease, "disease-1");
        for (id, name) in [
            (2, "pheno-a"),
            (3, "pheno-b"),
            (4, "discriminator"),
            (5, "contradiction"),
            (6, "required"),
        ] {
            k.add_node(NodeId(id), NodeKind::Finding, name);
        }
        k.add_edge(NodeId(1), Edge::new(NodeId(2), EdgeType::HasPhenotype));
        k.add_edge(NodeId(1), Edge::new(NodeId(3), EdgeType::HasPhenotype));
        k.add_edge(NodeId(1), Edge::new(NodeId(4), EdgeType::Discriminates));
        k.add_edge(NodeId(1), Edge::new(NodeId(5), EdgeType::ContradictedBy));
        k.add_edge(NodeId(1), Edge::new(NodeId(6), EdgeType::Requires));
        k
    }

    fn patient(present: &[u32]) -> PatientGraph {
        let mut p = PatientGraph::new();
        for &c in present {
            p.push(Observation::present(NodeId(c)));
        }
        p
    }

    #[test]
    fn strongly_supported_with_discriminator_and_requirement_met() {
        let k = fixture();
        let a = assess(&patient(&[2, 3, 4, 6]), &k);
        assert!(!a.abstained);
        let top = &a.candidates[0];
        assert_eq!(top.evidence.disease, NodeId(1));
        assert_eq!(top.outcome, EvidenceOutcome::StronglySupported);
        assert_eq!(top.evidence.strong_discriminators, 1);
        assert_eq!(top.contradicting.len(), 0);
        assert_eq!(top.missing_required.len(), 0);
    }

    #[test]
    fn required_missing_drops_to_possible() {
        let k = fixture();
        let a = assess(&patient(&[2, 3, 4]), &k); // no required finding (6)
        let top = &a.candidates[0];
        assert_eq!(top.outcome, EvidenceOutcome::Possible);
        assert_eq!(top.missing_required, vec![NodeId(6)]);
    }

    #[test]
    fn present_contradiction_blocks_confidence() {
        let k = fixture();
        // one support (pheno-a) vs one contradiction -> contradiction wins
        let a = assess(&patient(&[2, 5]), &k);
        let top = &a.candidates[0];
        assert_eq!(top.outcome, EvidenceOutcome::Contradicted);
        assert_eq!(top.contradicting, vec![NodeId(5)]);
    }

    #[test]
    fn no_linked_finding_abstains() {
        let k = fixture();
        let a = assess(&patient(&[999]), &k); // unrelated concept
        assert!(a.abstained);
        assert!(a.candidates.is_empty());
    }

    #[test]
    fn ontology_ancestor_matches() {
        // patient has a narrow finding; disease is annotated with its parent.
        let mut k = MemKnowledge::new();
        k.add_node(NodeId(1), NodeKind::Disease, "disease-1");
        k.add_node(NodeId(20), NodeKind::Finding, "anaemia");
        k.add_node(NodeId(21), NodeKind::Finding, "microcytic-anaemia");
        k.add_node(NodeId(22), NodeKind::Finding, "pheno-b");
        k.add_isa(NodeId(21), NodeId(20));
        k.add_edge(NodeId(1), Edge::new(NodeId(20), EdgeType::HasPhenotype));
        k.add_edge(NodeId(1), Edge::new(NodeId(22), EdgeType::HasPhenotype));

        // present the NARROW finding (21) and pheno-b (22); disease links the
        // BROAD one (20). Ancestor expansion should still match it.
        let a = assess(&patient(&[21, 22]), &k);
        assert!(!a.abstained);
        assert_eq!(a.candidates[0].evidence.disease, NodeId(1));
        assert!(a.candidates[0].matched.contains(&NodeId(20)));
    }
}
