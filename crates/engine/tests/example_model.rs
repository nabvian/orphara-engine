//! Engine mechanics over the bundled synthetic example model: compile the
//! `.dsdsrc`, load it through the frozen trait, and assert the hybrid engine's
//! diagnosis-support behaviour. This exercises the whole pipeline without any
//! real disease model — the model here is fictional (see the file header).

use std::collections::HashSet;

use core_types::{EvidenceOutcome, KnowledgeProvider, NodeId, Observation, PatientGraph};
use dsd::Dsd;
use engine::run;

const SRC: &str = include_str!("../../../knowledge/example/example_disorder.dsdsrc");

fn load() -> (Dsd, std::collections::HashMap<String, NodeId>) {
    let (bytes, keys) = dsdc::source::compile_source_with_keys(SRC).expect("compiles");
    (Dsd::from_bytes(&bytes).expect("loads and verifies"), keys)
}

fn patient(keys: &std::collections::HashMap<String, NodeId>, present: &[&str]) -> PatientGraph {
    let mut p = PatientGraph::new();
    for k in present {
        p.push(Observation::present(keys[*k]));
    }
    p
}

#[test]
fn example_model_has_its_two_disorders() {
    let (dsd, _) = load();
    assert_eq!(dsd.diseases().len(), 2);
}

#[test]
fn a_full_case_is_strongly_supported_and_ranks_top() {
    let (dsd, keys) = load();
    let k: &dyn KnowledgeProvider = &dsd;

    let p = patient(
        &keys,
        &[
            "fatigue",
            "pallor",
            "jaundice",
            "hemoglobinuria",
            "heinz_bite_cells",
            "oxidant_temporal",
            "g6pd_assay_low",
        ],
    );
    // rules-only: the safety layer alone must reach the right conclusion.
    let a = run(&p, k, None);
    let top = &a.candidates[0];
    assert_eq!(top.rule.evidence.disease, keys["demo_hemolytic"]);
    assert_eq!(top.rule.outcome, EvidenceOutcome::StronglySupported);
    // discriminators, not the weak findings, carry it.
    assert!(top.rule.evidence.strong_discriminators >= 3);
    assert!(top.rule.contradicting.is_empty());
    assert!(top.rule.missing_required.is_empty());
}

#[test]
fn missing_confirmatory_test_drops_it_to_possible() {
    let (dsd, keys) = load();
    let k: &dyn KnowledgeProvider = &dsd;

    let p = patient(
        &keys,
        &["jaundice", "hemoglobinuria", "heinz_bite_cells", "oxidant_temporal"],
    );
    let a = run(&p, k, None);
    let g = a
        .candidates
        .iter()
        .find(|c| c.rule.evidence.disease == keys["demo_hemolytic"])
        .expect("candidate present");
    // Without the confirmatory test the engine must NOT assert the disorder; it
    // names the missing required concept instead.
    assert_eq!(g.rule.outcome, EvidenceOutcome::Possible);
    assert_eq!(g.rule.missing_required, vec![keys["g6pd_assay_low"]]);
}

#[test]
fn a_trained_gnn_never_lifts_a_worse_tier_over_the_top_rule() {
    let (dsd, keys) = load();
    let k: &dyn KnowledgeProvider = &dsd;

    // one labelled case per disorder (its discriminators + confirmatory concept)
    let sets: [(&str, &[&str]); 2] = [
        ("demo_hemolytic", &["hemoglobinuria", "heinz_bite_cells", "oxidant_temporal", "g6pd_assay_low"]),
        ("demo_cyanotic", &["cyanosis", "methb_level"]),
    ];
    let examples: Vec<(HashSet<NodeId>, NodeId)> = sets
        .iter()
        .map(|(d, fs)| (fs.iter().map(|f| keys[*f]).collect(), keys[*d]))
        .collect();
    let mut model = gnn::init(42);
    model.train(&examples, k, 400, 0.2);

    // demo_hemolytic StronglySupported; the other only Possible at best. Whatever
    // the GNN says, the rule tier is the gate, so the strong one stays on top.
    let p = patient(
        &keys,
        &["fatigue", "pallor", "jaundice", "hemoglobinuria", "heinz_bite_cells", "oxidant_temporal", "g6pd_assay_low"],
    );
    let a = run(&p, k, Some(&model));
    assert_eq!(a.candidates[0].rule.evidence.disease, keys["demo_hemolytic"]);
    assert_eq!(a.candidates[0].rule.outcome, EvidenceOutcome::StronglySupported);
    assert!(a.candidates[1..].iter().all(|c| engine_tier(c.rule.outcome) < 6));
}

// mirror of the engine's internal tier order, for the assertion above
fn engine_tier(o: EvidenceOutcome) -> u8 {
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
