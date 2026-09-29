//! The clinical-findings pack turns a doctor's free-text note into documented
//! qualitative findings (present AND absent), with the same keys the Red Cell
//! Enzyme DSD uses — so the concept adapter later maps them by identity.

use caretrace::ingest::ingest_bytes;
use caretrace::pipeline::process_case;

const NOTE: &str = "\
Clinical Note
Date: 4 May 2026
28-year-old man with jaundice and dark urine.
Blood film shows bite cells and Heinz bodies.
Haemolysis following oxidant exposure.
G6PD assay low, confirming the diagnosis.
Spleen not enlarged.
";

#[test]
fn doctor_note_yields_present_and_absent_findings() {
    let r = process_case(vec![ingest_bytes("note.txt", 0, NOTE.as_bytes())]);
    let present: Vec<&str> = r
        .facts
        .iter()
        .filter(|f| f.value_key.as_deref() == Some("present"))
        .map(|f| f.concept.as_str())
        .collect();

    for c in ["jaundice", "hemoglobinuria", "heinz_bite_cells", "oxidant_temporal", "g6pd_assay_low"] {
        assert!(present.contains(&c), "expected present finding {c}, got {present:?}");
    }

    // "Spleen not enlarged" is a DOCUMENTED ABSENCE, not silence.
    assert!(
        r.facts
            .iter()
            .any(|f| f.concept == "splenomegaly" && f.value_key.as_deref() == Some("absent")),
        "expected a documented-absent splenomegaly finding"
    );

    // Every finding is dated from the note's own header and traces to a source.
    assert!(r.facts.iter().all(|f| f.obs_date.as_deref() == Some("2026-05-04")));
    assert_eq!(r.counts.get("source_traceability").copied(), Some(1.0));
}
