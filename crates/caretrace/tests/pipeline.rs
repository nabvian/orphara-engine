//! End-to-end pipeline test over synthetic multi-report text (no PDF needed):
//! two same-day CBCs that disagree, a later CBC (a documented change), and a
//! discharge summary whose claim has no located supporting evidence.

use caretrace::ingest::ingest_bytes;
use caretrace::models::{claim_status, conflict_type, gap_type};
use caretrace::pipeline::process_case;

fn docs() -> Vec<caretrace::ingest::Ingested> {
    let cbc1 = "\
Complete Blood Count
Report Date: 4 May 2026
Hemoglobin        9.2 g/dL      13.0-17.0
MCV               70 fL         80-100
";
    let cbc2 = "\
Complete Blood Count
Report Date: 4 May 2026
Hemoglobin        11.5 g/dL     13.0-17.0
";
    let cbc3 = "\
Complete Blood Count
Report Date: 24 May 2026
Hemoglobin        12.4 g/dL     13.0-17.0
";
    let discharge = "\
Discharge Summary
Discharge Date: 25 May 2026
Impression:
Iron deficiency confirmed on clinical grounds.
";
    vec![
        ingest_bytes("cbc1.txt", 0, cbc1.as_bytes()),
        ingest_bytes("cbc2.txt", 1, cbc2.as_bytes()),
        ingest_bytes("cbc3.txt", 2, cbc3.as_bytes()),
        ingest_bytes("discharge.txt", 3, discharge.as_bytes()),
    ]
}

#[test]
fn full_run_produces_facts_conflict_change_and_gap() {
    let r = process_case(docs());

    // Facts: at least the three haemoglobins + the MCV.
    let hb = r.facts.iter().filter(|f| f.concept == "hemoglobin").count();
    assert!(hb >= 3, "expected >=3 haemoglobin facts, got {hb}");
    assert!(r.facts.iter().any(|f| f.concept == "mcv"));

    // Same-day disagreement across two documents -> a numeric conflict.
    assert!(
        r.conflicts
            .iter()
            .any(|c| c.conflict_type == conflict_type::NUMERIC_OBSERVATION && c.concept == "hemoglobin"),
        "expected a numeric haemoglobin conflict"
    );

    // 9.2 (4 May) -> 12.4 (24 May) is a documented change over a later episode.
    assert!(
        r.changes.iter().any(|c| c.concept == "hemoglobin"),
        "expected a documented haemoglobin change"
    );

    // The discharge claim's required evidence (ferritin/TSAT/iron) is not in the
    // records, so the claim has NO_LOCATED_EVIDENCE and a gap is raised.
    assert!(
        r.claims.iter().any(|c| c.claim_type == "IRON_DEFICIENCY"
            && c.evidence_status == claim_status::NO_LOCATED),
        "expected an unsupported IRON_DEFICIENCY claim"
    );
    assert!(
        r.gaps.iter().any(|g| g.gap_type == gap_type::MISSING_SUPPORTING_TEST),
        "expected a missing-supporting-test gap"
    );

    // Every fact/claim/medication traces to a source.
    assert_eq!(r.counts.get("source_traceability").copied(), Some(1.0));

    // The declared pipeline ran end to end.
    assert_eq!(r.stages.first().unwrap().name, "INGEST");
    assert_eq!(r.stages.last().unwrap().name, "GENERATE_AUDIT");
}

#[test]
fn unit_first_extraction_is_layout_agnostic() {
    // A different layout (colon-delimited inline) still yields the analytes.
    let note = "\
Laboratory Summary
Report Date: 1 June 2026
Hb: 8.1 g/dL, MCV: 68 fL, Ferritin: 6 ng/mL
";
    let r = process_case(vec![ingest_bytes("labs.txt", 0, note.as_bytes())]);
    for concept in ["hemoglobin", "mcv", "ferritin"] {
        assert!(r.facts.iter().any(|f| f.concept == concept), "missing {concept}");
    }
}
