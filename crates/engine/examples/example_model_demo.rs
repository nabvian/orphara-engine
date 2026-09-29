//! End-to-end demo over the bundled synthetic example model, reasoned over by
//! the hybrid (rules + GNN) Core.
//!
//! What it shows, top to bottom:
//! 1. a `.dsdsrc` model → `dsdc` compiler → `.dsd` bytes;
//! 2. the Blind Core loads those bytes through the frozen `KnowledgeProvider`
//!    trait and reasons with NO disease-specific code;
//! 3. the GNN, trained purely on the graph, supplies a ranking signal that never
//!    crosses the rule tier (the safety gate);
//! 4. a missing CONFIRMATORY concept drops a candidate from StronglySupported to
//!    Possible — the engine asks for it rather than asserting the disorder.
//!
//! The model is fictional (see its file header). Run with:
//!   cargo run -p engine --example example_model_demo

use std::collections::{HashMap, HashSet};

use core_types::{EdgeType, KnowledgeProvider, NodeId, Observation, PatientGraph};
use dsd::Dsd;
use engine::run;

const SRC: &str = include_str!("../../../knowledge/example/example_disorder.dsdsrc");

fn main() {
    // 1) compile the model to .dsd bytes, keeping the key → NodeId map.
    let (bytes, keys) =
        dsdc::source::compile_source_with_keys(SRC).expect("model should compile");
    println!("compiled example_disorder.dsd  ({} bytes)\n", bytes.len());

    // 2) the Blind Core loads it through the frozen trait — the only thing the
    //    reasoning side ever sees.
    let dsd = Dsd::from_bytes(&bytes).expect("compiled .dsd should load and verify");
    let k: &dyn KnowledgeProvider = &dsd;
    let name = |n: NodeId| k.name(n).unwrap_or("<?>");

    println!("disorders in model:");
    for &d in k.diseases() {
        println!("  - {}", name(d));
    }
    println!();

    // 3) train the GNN from the graph itself, disease-blind: one labelled case
    //    per disorder, its discriminating + confirmatory concepts. Weights are
    //    DATA; the Core stays blind.
    let examples: Vec<(HashSet<NodeId>, NodeId)> = k
        .diseases()
        .iter()
        .filter_map(|&d| {
            let present: HashSet<NodeId> = k
                .neighbors(d)
                .iter()
                .filter(|e| matches!(e.edge_type, EdgeType::Discriminates | EdgeType::Requires))
                .map(|e| e.target)
                .collect();
            (!present.is_empty()).then_some((present, d))
        })
        .collect();
    let mut model = gnn::init(42);
    let loss = model.train(&examples, k, 400, 0.2);
    println!(
        "GNN trained: loss {:.3} → {:.3} over {} epochs\n",
        loss[0],
        loss[loss.len() - 1],
        loss.len()
    );

    // 4) the demo patient — a full demo_hemolytic case as a present-finding
    //    snapshot. (Weak findings included on purpose: they must NOT be what
    //    carries the conclusion.)
    let full_case = [
        "fatigue",
        "pallor",
        "jaundice",
        "hemoglobinuria",
        "heinz_bite_cells",
        "oxidant_temporal",
        "g6pd_assay_low",
    ];
    scenario("Full case — discriminators + confirmatory present", &full_case, k, &keys, &model);

    // 5) same case, confirmatory concept absent → Possible, and the engine names
    //    what is missing rather than asserting the disorder.
    let no_assay: Vec<&str> = full_case.iter().copied().filter(|f| *f != "g6pd_assay_low").collect();
    scenario("Same case, confirmatory concept NOT present", &no_assay, k, &keys, &model);
}

fn scenario(
    title: &str,
    present_keys: &[&str],
    k: &dyn KnowledgeProvider,
    keys: &HashMap<String, NodeId>,
    model: &gnn::GnnModel,
) {
    let id = |key: &str| *keys.get(key).unwrap_or_else(|| panic!("unknown concept key '{key}'"));
    let name = |n: NodeId| k.name(n).unwrap_or("<?>");

    let mut patient = PatientGraph::new();
    for key in present_keys {
        patient.push(Observation::present(id(key)));
    }

    let assessment = run(&patient, k, Some(model));

    println!("── {title} ──");
    println!("  present: {}", present_keys.join(", "));
    if assessment.abstained {
        println!("  → engine ABSTAINS (nothing reaches acceptance)");
    }
    for (rank, c) in assessment.candidates.iter().enumerate() {
        let gnn = c.gnn_score.map(|s| format!("{s:+.2}")).unwrap_or_else(|| "—".into());
        println!(
            "  {}. {:<48} {:?}   [gnn {}]",
            rank + 1,
            name(c.rule.evidence.disease),
            c.rule.outcome,
            gnn
        );
        if !c.rule.matched.is_empty() {
            let ms: Vec<&str> = c.rule.matched.iter().map(|&m| name(m)).collect();
            println!("       matched: {}", ms.join("; "));
        }
        if !c.rule.missing_required.is_empty() {
            let ms: Vec<&str> = c.rule.missing_required.iter().map(|&m| name(m)).collect();
            println!("       MISSING required: {}", ms.join("; "));
        }
        if !c.rule.contradicting.is_empty() {
            let ms: Vec<&str> = c.rule.contradicting.iter().map(|&m| name(m)).collect();
            println!("       contradicted by: {}", ms.join("; "));
        }
    }
    println!();
}
