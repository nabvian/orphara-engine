//! The whole seam in one place: an extractor case (the patient side) is adapted
//! into a `PatientGraph` and reasoned over by the Blind Core against the bundled
//! synthetic example model. Nothing disease-specific lives in code — the
//! knowledge is the compiled `.dsd`, the patient facts are extractor output, and
//! the adapter is the only glue.
//!
//! These are ENGINE tests: they check the plumbing (mapping, quarantine,
//! confirmatory gating, timeline flag) over a fictional model. They make no
//! clinical claim; the model is made up (see its file header).

use caretrace::ingest::ingest_bytes;
use caretrace::models::{new_id, Fact, Medication};
use caretrace::pipeline::{process_case, CaseResult};
use concept_adapter::{adapt, ConceptMap};
use core_types::{EvidenceOutcome, KnowledgeProvider};
use dsd::Dsd;
use engine::run;

const SRC: &str = include_str!("../../../knowledge/example/example_disorder.dsdsrc");

fn finding(concept: &str) -> Fact {
    Fact {
        id: new_id(),
        display_id: "F".into(),
        concept: concept.into(),
        surface_form: concept.into(),
        source_id: String::new(),
        value_num: None,
        value_text: Some("present".into()),
        value_key: Some("present".into()),
        unit: None,
        obs_date: Some("2026-05-04".into()),
        date_precision: "day".into(),
        confidence: 0.9,
        interpretation: None,
    }
}

fn medication(norm: &str) -> Medication {
    Medication {
        id: new_id(),
        display_id: "M".into(),
        drug_name: norm.into(),
        drug_norm: norm.into(),
        source_id: String::new(),
        dose: None,
        unit: None,
        frequency: None,
        route: None,
        status: "ACTIVE".into(),
        start_date: None,
        stop_date: None,
        record_date: Some("2026-05-01".into()),
        confidence: 0.9,
    }
}

#[test]
fn a_case_flows_through_the_adapter_into_the_blind_core() {
    // Knowledge side: compile the example model, keeping the key -> NodeId map.
    let (bytes, keys) = dsdc::source::compile_source_with_keys(SRC).expect("model compiles");
    let dsd = Dsd::from_bytes(&bytes).expect("model loads");
    let k: &dyn KnowledgeProvider = &dsd;

    // Patient side: a case as a clinical-note findings pack would yield it —
    // discriminators + the confirmatory concept + a trigger drug, plus one
    // analyte the extractor could not map (must be quarantined).
    let mut case = CaseResult::default();
    for c in ["jaundice", "hemoglobinuria", "heinz_bite_cells", "oxidant_temporal", "g6pd_assay_low"] {
        case.facts.push(finding(c));
    }
    case.facts.push(finding("unmapped:reticulocyte_percentage")); // -> quarantine
    case.facts.push(finding("dark_tea_urine")); // -> alias to hemoglobinuria
    case.medications.push(medication("primaquine")); // -> trigger node

    // One explicit alias; everything else maps by identity.
    let map = ConceptMap::from_aliases(&[("dark_tea_urine", "hemoglobinuria")]);
    let out = adapt(&case, &keys, &map);

    // The adapter resolved the known concepts and quarantined the unmappable one.
    assert!(out.mapped.iter().any(|m| m.node == keys["g6pd_assay_low"]));
    assert!(out.mapped.iter().any(|m| m.dsd_key == "hemoglobinuria")); // via alias + identity
    assert!(out.mapped.iter().any(|m| m.node == keys["primaquine"]));
    assert_eq!(out.quarantined.len(), 1);
    assert!(out.quarantined[0].extractor_concept.starts_with("unmapped:"));

    // Blind Core reasons over the adapted graph — the top disorder is strongly
    // supported, with no disease-specific code anywhere in the engine.
    let assessment = run(&out.graph, k, None);
    let top = &assessment.candidates[0];
    assert_eq!(top.rule.evidence.disease, keys["demo_hemolytic"], "should rank first");
    assert_eq!(top.rule.outcome, EvidenceOutcome::StronglySupported);
    assert!(top.rule.evidence.strong_discriminators >= 3);
}

#[test]
fn confirmatory_concept_absent_downgrades_to_possible() {
    let (bytes, keys) = dsdc::source::compile_source_with_keys(SRC).unwrap();
    let dsd = Dsd::from_bytes(&bytes).unwrap();
    let k: &dyn KnowledgeProvider = &dsd;

    // Same discriminators, but the confirmatory concept is not in the record.
    let mut case = CaseResult::default();
    for c in ["jaundice", "hemoglobinuria", "heinz_bite_cells", "oxidant_temporal"] {
        case.facts.push(finding(c));
    }
    let out = adapt(&case, &keys, &ConceptMap::identity());
    let a = run(&out.graph, k, None);
    let g = a
        .candidates
        .iter()
        .find(|c| c.rule.evidence.disease == keys["demo_hemolytic"])
        .expect("candidate present");
    assert_eq!(g.rule.outcome, EvidenceOutcome::Possible);
    assert_eq!(g.rule.missing_required, vec![keys["g6pd_assay_low"]]);
}

#[test]
fn a_doctor_note_flows_all_the_way_through() {
    // The entire input side, for real: free text -> the extractor (clinical
    // findings pack) -> concept adapter -> PatientGraph -> Blind Core.
    let note = "\
Clinical Note
Date: 4 May 2026
28-year-old man with jaundice and dark urine.
Blood film shows bite cells and Heinz bodies.
Haemolysis following oxidant exposure.
G6PD assay low, confirming the finding.
Spleen not enlarged.
";
    let case = process_case(vec![ingest_bytes("note.txt", 0, note.as_bytes())]);

    let (bytes, keys) = dsdc::source::compile_source_with_keys(SRC).unwrap();
    let dsd = Dsd::from_bytes(&bytes).unwrap();
    let k: &dyn KnowledgeProvider = &dsd;

    // Identity mapping is enough — the clinical pack shares the model's keys.
    let out = adapt(&case, &keys, &ConceptMap::identity());
    // The documented ABSENCE (spleen not enlarged) is carried as Absent, not
    // dropped and not turned into a false Present.
    assert!(out
        .mapped
        .iter()
        .any(|m| m.node == keys["splenomegaly"] && m.state == core_types::EvidenceState::Absent));

    let a = run(&out.graph, k, None);
    let top = &a.candidates[0];
    assert_eq!(top.rule.evidence.disease, keys["demo_hemolytic"], "should rank first from the note");
    assert_eq!(top.rule.outcome, EvidenceOutcome::StronglySupported);
}

#[test]
fn a_lab_report_biomarker_plus_a_note_confirm_the_top_disorder() {
    // Report + TXT: a numeric enzyme biomarker (interpreted LOW from the report's
    // OWN printed reference range) plus the note's findings together confirm the
    // top disorder — the "biomarker + finding -> disorder" flow.
    let report = "\
Enzyme Assay
Report Date: 6 May 2026
G6PD activity      2.1 U/g       6.0 - 18.0
";
    let note = "\
Clinical Note
Date: 6 May 2026
Jaundice and dark urine. Blood film: bite cells and Heinz bodies.
";
    let case = process_case(vec![
        ingest_bytes("enzyme_report.txt", 0, report.as_bytes()),
        ingest_bytes("note.txt", 1, note.as_bytes()),
    ]);
    let (bytes, keys) = dsdc::source::compile_source_with_keys(SRC).unwrap();
    let dsd = Dsd::from_bytes(&bytes).unwrap();
    let k: &dyn KnowledgeProvider = &dsd;

    // The low enzyme biomarker becomes the confirmatory concept.
    let out = adapt(&case, &keys, &ConceptMap::identity());
    assert!(
        out.mapped.iter().any(|m| m.node == keys["g6pd_assay_low"]),
        "a low enzyme-activity biomarker should map to the confirmatory concept"
    );

    let a = run(&out.graph, k, None);
    let top = &a.candidates[0];
    assert_eq!(top.rule.evidence.disease, keys["demo_hemolytic"]);
    assert_eq!(top.rule.outcome, EvidenceOutcome::StronglySupported);
}

#[test]
fn the_other_disorder_is_reached_from_its_own_note() {
    let (bytes, keys) = dsdc::source::compile_source_with_keys(SRC).unwrap();
    let dsd = Dsd::from_bytes(&bytes).unwrap();
    let k: &dyn KnowledgeProvider = &dsd;

    let note = "\
Clinical Note
Date: 2 March 2026
Central cyanosis with blue lips.
Elevated methaemoglobin on co-oximetry.
";
    let case = process_case(vec![ingest_bytes("note.txt", 0, note.as_bytes())]);
    let out = adapt(&case, &keys, &ConceptMap::identity());
    let a = run(&out.graph, k, None);
    let top = &a.candidates[0];
    assert_eq!(top.rule.evidence.disease, keys["demo_cyanotic"], "wrong top disorder");
    assert_eq!(top.rule.outcome, EvidenceOutcome::StronglySupported);
}

#[test]
fn timing_trap_is_flagged_by_the_temporal_pass() {
    // A confirmatory result read NORMAL during a confounding state is unreliable.
    // The snapshot verifier sees no LOW result (so the disorder is only Possible),
    // and the timeline pass raises the "repeat after recovery" flag — from the
    // note's timing, with no disease-specific code.
    let note = "\
Clinical Note
Date: 4 May 2026
28-year-old man with jaundice and dark urine.
Blood film shows bite cells and Heinz bodies.
Reticulocytosis noted on the film.
G6PD assay normal.
";
    let case = process_case(vec![ingest_bytes("note.txt", 0, note.as_bytes())]);
    let (bytes, keys) = dsdc::source::compile_source_with_keys(SRC).unwrap();
    let dsd = Dsd::from_bytes(&bytes).unwrap();
    let k: &dyn KnowledgeProvider = &dsd;
    let out = adapt(&case, &keys, &ConceptMap::identity());

    // The confirmatory LOW result is absent, so the disorder is Possible, not asserted.
    let a = run(&out.graph, k, None);
    let g = a.candidates.iter().find(|c| c.rule.evidence.disease == keys["demo_hemolytic"]).unwrap();
    assert_eq!(g.rule.outcome, EvidenceOutcome::Possible);
    assert!(g.rule.missing_required.contains(&keys["g6pd_assay_low"]));

    // The timeline pass flags the normal result drawn during the confounding state.
    let flags = temporal::analyze(&out.graph, k);
    assert_eq!(flags.len(), 1, "expected one timing flag");
    assert_eq!(flags[0].result, keys["g6pd_assay_normal"]);
    assert_eq!(flags[0].confounder, keys["reticulocytosis"]);
    assert_eq!(flags[0].timing, temporal::Timing::WithinWindow);
    assert!(flags[0].message.contains("repeating"));
}
