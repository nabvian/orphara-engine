//! On one screen, from free-text notes:
//!   free text -> the extractor (clinical findings pack) -> concept adapter
//!   -> PatientGraph -> Blind Core (snapshot) + timeline pass, over the
//!   compiled synthetic example model (fictional — see its file header).
//!
//!   cargo run -p concept-adapter --example poc_end_to_end
//!
//! Two notes contrast: a confirmed low result is strongly supported with no
//! timing caveat; a NORMAL result read during the confounding state is only
//! Possible AND the timeline pass flags it as unreliable.

use caretrace::ingest::ingest_bytes;
use caretrace::pipeline::process_case;
use concept_adapter::{adapt, ConceptMap};
use core_types::KnowledgeProvider;
use dsd::Dsd;
use engine::run;

const SRC: &str = include_str!("../../../knowledge/example/example_disorder.dsdsrc");

const CONFIRMED: &str = "\
Clinical Note — Date: 4 May 2026
28-year-old man with jaundice and dark urine.
Blood film shows bite cells and Heinz bodies.
Haemolysis following oxidant exposure.
G6PD assay low, confirming the finding.
";

const TRAP: &str = "\
Clinical Note — Date: 4 May 2026
28-year-old man with jaundice and dark urine.
Blood film shows bite cells and Heinz bodies.
Reticulocytosis noted on the film.
G6PD assay normal.
";

fn main() {
    let (bytes, keys) = dsdc::source::compile_source_with_keys(SRC).unwrap();
    let dsd = Dsd::from_bytes(&bytes).unwrap();
    let k: &dyn KnowledgeProvider = &dsd;

    for (title, note) in [("CONFIRMED (assay low)", CONFIRMED), ("TRAP (assay normal in reticulocytosis)", TRAP)] {
        println!("\n══════ {title} ══════");
        let case = process_case(vec![ingest_bytes("note.txt", 0, note.as_bytes())]);
        let out = adapt(&case, &keys, &ConceptMap::identity());

        print!("  findings: ");
        let fs: Vec<String> = out
            .mapped
            .iter()
            .map(|m| format!("{}[{:?}]", m.extractor_concept, m.state))
            .collect();
        println!("{}", fs.join(", "));

        let a = run(&out.graph, k, None);
        let top = &a.candidates[0];
        println!("  → {} : {:?}", k.name(top.rule.evidence.disease).unwrap_or("?"), top.rule.outcome);
        if !top.rule.missing_required.is_empty() {
            let ms: Vec<&str> = top.rule.missing_required.iter().map(|&m| k.name(m).unwrap_or("?")).collect();
            println!("    missing required: {}", ms.join("; "));
        }

        let flags = temporal::analyze(&out.graph, k);
        if flags.is_empty() {
            println!("  timeline: no validity concerns");
        } else {
            for f in &flags {
                println!("  ⚠ timeline [{:?}]: {}", f.timing, f.message);
            }
        }
    }
}
