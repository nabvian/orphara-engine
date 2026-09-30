//! Analyze a patient history + doctor's note from the command line and print the
//! full structured analysis, the way the web UI shows it.
//!
//!   # the bundled example note:
//!   cargo run -p concept-adapter --example analyze_note
//!
//!   # your own files (any mix of reports and notes, PDF or text):
//!   cargo run -p concept-adapter --example analyze_note -- note.txt report.pdf
//!
//!   # over a different model (no code change):
//!   ORPHARA_MODEL=/path/to/model.dsdsrc cargo run -p concept-adapter --example analyze_note -- note.txt
//!
//! Runs over the bundled synthetic example model by default (fictional — see its
//! file header). It makes no clinical claim.

use caretrace::ingest::{ingest_bytes, Ingested};
use caretrace::pipeline::process_case;
use concept_adapter::{adapt, ConceptMap};
use core_types::{EvidenceState, KnowledgeProvider};
use dsd::Dsd;
use engine::run;

const EXAMPLE_SRC: &str = include_str!("../../../knowledge/example/example_disorder.dsdsrc");

const DEFAULT_NOTE: &str = "\
Clinical Note
Date: 4 May 2026
28-year-old man with jaundice and dark urine.
Blood film shows bite cells and Heinz bodies.
Haemolysis following oxidant exposure.
G6PD assay low, confirming the finding.
Spleen not enlarged.
";

fn main() {
    // Model: bundled example by default; ORPHARA_MODEL overrides at runtime.
    let (src, origin) = match std::env::var("ORPHARA_MODEL") {
        Ok(p) if !p.is_empty() => (std::fs::read_to_string(&p).expect("read ORPHARA_MODEL"), p),
        _ => (EXAMPLE_SRC.to_string(), "bundled synthetic example".to_string()),
    };
    let (bytes, keys) = dsdc::source::compile_source_with_keys(&src).expect("model compiles");
    let dsd = Dsd::from_bytes(&bytes).expect("model loads");
    let k: &dyn KnowledgeProvider = &dsd;

    // Inputs: files given on the command line, or the bundled example note.
    let paths: Vec<String> = std::env::args().skip(1).collect();
    let ingested: Vec<Ingested> = if paths.is_empty() {
        println!("(no files given — using the bundled example note)\n");
        vec![ingest_bytes("note.txt", 0, DEFAULT_NOTE.as_bytes())]
    } else {
        paths
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let data = std::fs::read(p).unwrap_or_else(|e| panic!("read {p}: {e}"));
                let name = std::path::Path::new(p).file_name().and_then(|s| s.to_str()).unwrap_or(p);
                ingest_bytes(name, i, &data)
            })
            .collect()
    };

    let case = process_case(ingested);
    let out = adapt(&case, &keys, &ConceptMap::identity());
    let assessment = run(&out.graph, k, None);
    let flags = temporal::analyze(&out.graph, k);
    let name = |n| k.name(n).unwrap_or("?");

    println!("═══ ORPHARA structured analysis ═══   (model: {origin})\n");

    println!("Documents:");
    for d in &case.documents {
        println!("  - {} [{}] {}", d.filename, d.doc_type.label(), d.doc_date.clone().unwrap_or_default());
    }

    println!("\nExtracted facts:");
    for f in &case.facts {
        let val = match (f.value_num, &f.value_text) {
            (Some(v), _) => format!("{v}"),
            (None, Some(t)) => t.clone(),
            _ => "—".into(),
        };
        let dir = f.interpretation.as_deref().map(|d| format!(" [{d}]")).unwrap_or_default();
        let unit = f.unit.clone().unwrap_or_default();
        println!("  - {} = {} {}{}", f.concept, val, unit, dir);
    }
    if !out.quarantined.is_empty() {
        println!("\nQuarantined (unmapped, never invented):");
        for q in &out.quarantined {
            println!("  - {} ({})", q.extractor_concept, q.reason);
        }
    }

    if !case.conflicts.is_empty() || !case.gaps.is_empty() {
        println!("\nRecord audit:");
        for c in &case.conflicts {
            println!("  ! conflict: {} — {} vs {}", c.concept_label, c.left_summary, c.right_summary);
        }
        for g in &case.gaps {
            println!("  ? gap: {} — {}", g.title, g.basis);
        }
    }

    println!("\nDiagnosis support (ranked):");
    if assessment.candidates.is_empty() {
        println!("  (nothing in the inputs maps to a linked concept)");
    }
    for (i, c) in assessment.candidates.iter().enumerate() {
        println!(
            "  {}. {:<48} {:?}   (support {:.1}, {} discriminators)",
            i + 1,
            name(c.rule.evidence.disease),
            c.rule.outcome,
            c.rule.evidence.local_support,
            c.rule.evidence.strong_discriminators,
        );
        if !c.rule.missing_required.is_empty() {
            let ms: Vec<&str> = c.rule.missing_required.iter().map(|&m| name(m)).collect();
            println!("       still needed to confirm: {}", ms.join("; "));
        }
    }
    if assessment.abstained {
        println!("  (the engine abstains: nothing reaches acceptance)");
    }

    println!("\nTimeline validity:");
    if flags.is_empty() {
        println!("  no timing concerns");
    } else {
        for f in &flags {
            println!("  ⚠ [{:?}] {}", f.timing, f.message);
        }
    }

    // A tiny reminder of what the state discipline preserves.
    let absent: Vec<&str> = out
        .mapped
        .iter()
        .filter(|m| m.state == EvidenceState::Absent)
        .map(|m| name(m.node))
        .collect();
    if !absent.is_empty() {
        println!("\nDocumented as absent (carried, not dropped): {}", absent.join(", "));
    }

    println!("\nDecision support only — not a diagnosis, not clinically validated.");
}
