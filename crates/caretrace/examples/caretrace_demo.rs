//! End-to-end demo of the extractor.
//!
//!   cargo run -p caretrace --example caretrace_demo
//!
//! Ingests several synthetic text "reports", runs the full deterministic
//! pipeline, and prints the audit: extracted facts, a same-episode numeric
//! conflict, a documented change over time, and an unsupported claim's evidence
//! gap — each traceable to its source. Then it shows UCUM validation (always
//! available) and the terminology layer reporting its state when no
//! vocabulary has been imported (LOINC/RxNorm/SNOMED are user-activated).

use caretrace::ingest::ingest_bytes;
use caretrace::pipeline::process_case;
use caretrace::terminology::local_index::LocalIndexProvider;
use caretrace::terminology::{resolve_case, ucum, TerminologyProvider};

fn main() {
    let cbc1 = "\
Complete Blood Count
Report Date: 4 May 2026
Hemoglobin        9.2 g/dL      13.0-17.0
MCV               70 fL         80-100
Ferritin          6 ng/mL       30-300
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
Ferrous sulfate 200 mg once daily oral
";

    let r = process_case(vec![
        ingest_bytes("cbc1.txt", 0, cbc1.as_bytes()),
        ingest_bytes("cbc2.txt", 1, cbc2.as_bytes()),
        ingest_bytes("cbc3.txt", 2, cbc3.as_bytes()),
        ingest_bytes("discharge.txt", 3, discharge.as_bytes()),
    ]);

    println!("── PIPELINE STAGES ──");
    for s in &r.stages {
        let counts = s
            .counts
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join(" ");
        println!("  {:<20} {counts}", s.name);
    }

    let doc_of = |id: &str| r.documents.iter().find(|d| d.id == id).map(|d| d.filename.as_str()).unwrap_or("?");
    let src_of = |sid: &str| r.sources.iter().find(|s| s.id == sid);
    let where_of = |sid: &str| src_of(sid).map(|s| format!("{}:p{}", doc_of(&s.document_id), s.page_number)).unwrap_or_default();

    println!("\n── FACTS ({}) ──", r.facts.len());
    for f in &r.facts {
        println!(
            "  {}  {:<16} {} {}   [{}]  <- {}",
            f.display_id,
            f.concept,
            f.value_num.map(|v| v.to_string()).unwrap_or_default(),
            f.unit.clone().unwrap_or_default(),
            f.obs_date.clone().unwrap_or_default(),
            where_of(&f.source_id),
        );
    }

    println!("\n── DOCUMENTED CHANGES ({}) ──", r.changes.len());
    for c in &r.changes {
        println!("  {}  {}", c.display_id, c.basis);
    }

    println!("\n── CONFLICTS ({}) ──", r.conflicts.len());
    for c in &r.conflicts {
        println!("  {}  {} : {} vs {}", c.display_id, c.concept_label, c.left_summary, c.right_summary);
        println!("        {}", c.basis);
    }

    println!("\n── CLAIMS ({}) ──", r.claims.len());
    for c in &r.claims {
        println!("  {}  {} — {}", c.display_id, c.claim_type, c.evidence_status);
    }

    println!("\n── EVIDENCE GAPS ({}) ──", r.gaps.len());
    for g in &r.gaps {
        println!("  {}  [{}] {}", g.display_id, g.gap_type, g.title);
        if !g.evidence_not_located.is_empty() {
            println!("        not located: {}", g.evidence_not_located.join(", "));
        }
    }

    println!(
        "\n  source traceability: {:.0}%   medications: {}",
        r.counts.get("source_traceability").copied().unwrap_or(0.0) * 100.0,
        r.medications.len()
    );

    // UCUM validation — always available, no licence.
    println!("\n── UCUM VALIDATION (always on) ──");
    for u in ["g/dL", "gm/dl", "fL", "banana"] {
        let c = ucum::validate(u);
        println!(
            "  {:<8} valid={} canonical={:?} dim={:?}",
            u, c.valid, c.canonical, c.dimension
        );
    }

    // Terminology — user-activated. With nothing imported it reports its state
    // and produces no codings; the audit above is unaffected either way.
    println!("\n── TERMINOLOGY (user-activated; none imported here) ──");
    let providers: Vec<Box<dyn TerminologyProvider>> = vec![
        Box::new(LocalIndexProvider::rxnorm(None)),
        Box::new(LocalIndexProvider::loinc(None)),
        Box::new(LocalIndexProvider::snomed(None)),
    ];
    for p in &providers {
        let h = p.health();
        println!("  {:<14} {:<11} {}", h.label, h.state.as_str(), h.detail);
    }
    let (codings, report) = resolve_case(&r, &providers);
    println!(
        "  resolved: {} codings ({} attempted, {} skipped-unlicensed) — audit unchanged",
        codings.len(),
        report.attempted,
        report.skipped_unlicensed.values().sum::<usize>()
    );
}
