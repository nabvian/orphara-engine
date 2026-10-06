//! WebAssembly bridge for the GitHub Pages demo.
//!
//! Runs the unmodified engine crates in the browser: the same calls, in the
//! same order, as `crates/poc-server` (`ingest_bytes` -> `process_case` ->
//! `adapt` -> `engine::run` -> `temporal::analyze`). Nothing here knows about
//! any disease; it reasons over whatever model source it is given, which by
//! default is the bundled synthetic example.
//!
//! The interface is a plain C ABI so the page needs no generated glue:
//! the caller allocates input with `orphara_alloc`, calls one entry point, and
//! reads the JSON answer from `orphara_out_ptr()` / the returned length.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

use caretrace::ingest::{ingest_bytes, Ingested};
use caretrace::pipeline::{process_case, CaseResult};
use concept_adapter::{adapt, ConceptMap};
use core_types::{EdgeType, EvidenceOutcome, EvidenceState, KnowledgeProvider, NodeId};
use dsd::Dsd;
use gnn::GnnModel;
use serde_json::{json, Value};

/// The bundled synthetic example model. Not medical knowledge.
pub const EXAMPLE_SRC: &str = include_str!("../../../knowledge/example/example_disorder.dsdsrc");

pub struct Model {
    dsd: Dsd,
    keys: HashMap<String, NodeId>,
    gnn: GnnModel,
}

thread_local! {
    static MODEL: RefCell<Option<Model>> = const { RefCell::new(None) };
    static OUT: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}

// ---------------------------------------------------------------------------
// C ABI
// ---------------------------------------------------------------------------

#[no_mangle]
pub extern "C" fn orphara_alloc(len: usize) -> *mut u8 {
    let mut buf = Vec::<u8>::with_capacity(len.max(1));
    let ptr = buf.as_mut_ptr();
    std::mem::forget(buf);
    ptr
}

/// # Safety
/// `ptr` and `len` must come from one `orphara_alloc` call.
#[no_mangle]
pub unsafe extern "C" fn orphara_free(ptr: *mut u8, len: usize) {
    drop(Vec::from_raw_parts(ptr, 0, len.max(1)));
}

#[no_mangle]
pub extern "C" fn orphara_out_ptr() -> *const u8 {
    OUT.with(|o| o.borrow().as_ptr())
}

fn emit(v: Value) -> usize {
    let bytes = serde_json::to_vec(&v).unwrap_or_else(|_| b"{\"ok\":false,\"error\":\"encode\"}".to_vec());
    OUT.with(|o| {
        *o.borrow_mut() = bytes;
        o.borrow().len()
    })
}

unsafe fn input<'a>(ptr: *const u8, len: usize) -> &'a [u8] {
    if len == 0 {
        &[]
    } else {
        std::slice::from_raw_parts(ptr, len)
    }
}

/// The bundled example model source, as `{ "source": ... }`.
#[no_mangle]
pub extern "C" fn orphara_example_model() -> usize {
    emit(json!({ "ok": true, "source": EXAMPLE_SRC }))
}

/// Compile and load a model from `.dsdsrc` text (UTF-8).
///
/// # Safety
/// `ptr`/`len` must describe a readable byte range.
#[no_mangle]
pub unsafe extern "C" fn orphara_load_model(ptr: *const u8, len: usize) -> usize {
    let src = String::from_utf8_lossy(input(ptr, len)).into_owned();
    emit(match load_model(&src) {
        Ok(model) => {
            let summary = model_summary(&model);
            MODEL.with(|m| *m.borrow_mut() = Some(model));
            summary
        }
        Err(e) => json!({ "ok": false, "error": e }),
    })
}

/// Analyze documents framed as `u32 count`, then per document
/// `u32 name_len, name, u32 data_len, data` (little-endian).
///
/// # Safety
/// `ptr`/`len` must describe a readable byte range.
#[no_mangle]
pub unsafe extern "C" fn orphara_analyze(ptr: *const u8, len: usize) -> usize {
    let docs = match unframe(input(ptr, len)) {
        Some(d) => d,
        None => return emit(json!({ "ok": false, "error": "malformed input" })),
    };
    emit(MODEL.with(|m| match m.borrow().as_ref() {
        Some(model) => analyze(&docs, model),
        None => json!({ "ok": false, "error": "no model loaded" }),
    }))
}

fn unframe(mut b: &[u8]) -> Option<Vec<(String, Vec<u8>)>> {
    fn take<'a>(b: &mut &'a [u8], n: usize) -> Option<&'a [u8]> {
        if b.len() < n {
            return None;
        }
        let (head, tail) = b.split_at(n);
        *b = tail;
        Some(head)
    }
    fn u32le(b: &mut &[u8]) -> Option<usize> {
        take(b, 4).map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]) as usize)
    }
    let count = u32le(&mut b)?;
    let mut docs = Vec::with_capacity(count.min(64));
    for _ in 0..count {
        let n = u32le(&mut b)?;
        let name = String::from_utf8_lossy(take(&mut b, n)?).into_owned();
        let d = u32le(&mut b)?;
        docs.push((name, take(&mut b, d)?.to_vec()));
    }
    Some(docs)
}

#[cfg(target_arch = "wasm32")]
mod random {
    #[link(wasm_import_module = "env")]
    extern "C" {
        /// Provided by the page: fills `len` bytes at `ptr` from crypto.getRandomValues.
        fn orphara_random_fill(ptr: *mut u8, len: usize);
    }

    /// getrandom's custom backend, used only by the PDF reader's dependencies.
    ///
    /// # Safety
    /// `dest` must be valid for `len` bytes; getrandom guarantees this.
    #[no_mangle]
    unsafe extern "Rust" fn __getrandom_v03_custom(dest: *mut u8, len: usize) -> Result<(), getrandom::Error> {
        orphara_random_fill(dest, len);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Model
// ---------------------------------------------------------------------------

pub fn load_model(src: &str) -> Result<Model, String> {
    let (bytes, keys) = dsdc::source::compile_source_with_keys(src).map_err(|e| e.to_string())?;
    let dsd = Dsd::from_bytes(&bytes).map_err(|e| format!("{e:?}"))?;
    let gnn = train(&dsd);
    Ok(Model { dsd, keys, gnn })
}

/// Same disease-blind training as `crates/poc-server`: each disease's
/// discriminating and confirmatory concepts form one labelled positive case.
fn train(dsd: &Dsd) -> GnnModel {
    let k: &dyn KnowledgeProvider = dsd;
    let examples: Vec<(HashSet<NodeId>, NodeId)> = dsd
        .diseases()
        .iter()
        .filter_map(|&disease| {
            let present: HashSet<NodeId> = k
                .neighbors(disease)
                .iter()
                .filter(|e| matches!(e.edge_type, EdgeType::Discriminates | EdgeType::Requires))
                .map(|e| e.target)
                .collect();
            (!present.is_empty()).then_some((present, disease))
        })
        .collect();
    let mut model = gnn::init(42);
    if !examples.is_empty() {
        model.train(&examples, k, 400, 0.2);
    }
    model
}

fn model_summary(model: &Model) -> Value {
    let k: &dyn KnowledgeProvider = &model.dsd;
    let mut nodes: Vec<(&String, &NodeId)> = model.keys.iter().collect();
    nodes.sort_by_key(|(_, &id)| id);
    let key_of: HashMap<NodeId, &str> = model.keys.iter().map(|(s, &id)| (id, s.as_str())).collect();
    let mut edges = Vec::new();
    let concepts: Vec<Value> = nodes
        .iter()
        .map(|(key, &id)| {
            for e in k.neighbors(id) {
                edges.push(json!({
                    "from": key, "to": key_of.get(&e.target).copied().unwrap_or("?"),
                    "type": format!("{:?}", e.edge_type), "weight": e.weight,
                }));
            }
            json!({
                "key": key, "name": k.name(id).unwrap_or("?"),
                "kind": k.node_kind(id).map(|n| format!("{n:?}")).unwrap_or_default(),
            })
        })
        .collect();
    json!({
        "ok": true,
        "diseases": model.dsd.diseases().iter().map(|&d| key_of.get(&d).copied().unwrap_or("?")).collect::<Vec<_>>(),
        "concepts": concepts,
        "edges": edges,
    })
}

// ---------------------------------------------------------------------------
// Analysis
// ---------------------------------------------------------------------------

pub fn analyze(docs: &[(String, Vec<u8>)], model: &Model) -> Value {
    let ingested: Vec<Ingested> = docs.iter().enumerate().map(|(i, (name, bytes))| ingest_bytes(name, i, bytes)).collect();
    let case = process_case(ingested);
    let k: &dyn KnowledgeProvider = &model.dsd;
    let out = adapt(&case, &model.keys, &ConceptMap::identity());
    let assessment = engine::run(&out.graph, k, Some(&model.gnn));
    let flags = temporal::analyze(&out.graph, k);

    let present: HashSet<NodeId> =
        out.graph.observations.iter().filter(|o| o.state == EvidenceState::Present).map(|o| o.concept).collect();
    let name = |n: NodeId| k.name(n).unwrap_or("?").to_string();

    let candidates: Vec<Value> = assessment
        .candidates
        .iter()
        .map(|c| {
            let ev = &c.rule.evidence;
            json!({
                "disease": name(ev.disease),
                "outcome": outcome_str(c.rule.outcome),
                "support": ev.local_support,
                "contradiction": ev.local_contradiction,
                "discriminators": ev.strong_discriminators,
                "phenotype_matches": ev.phenotype_matches,
                "gnn": c.gnn_score,
                "matched": c.rule.matched.iter().map(|&n| name(n)).collect::<Vec<_>>(),
                "contradicting": c.rule.contradicting.iter().map(|&n| name(n)).collect::<Vec<_>>(),
                "missing": c.rule.missing_required.iter().map(|&n| name(n)).collect::<Vec<_>>(),
                "detail": disease_detail(k, ev.disease, &present),
            })
        })
        .collect();

    json!({
        "ok": true,
        "counts": case.counts,
        "stages": case.stages.iter().map(|s| json!({ "name": s.name, "counts": s.counts })).collect::<Vec<_>>(),
        "documents": case.documents.iter().map(|d| json!({
            "id": d.id, "filename": d.filename, "type": d.doc_type.label(), "date": d.doc_date,
            "pages": d.page_count, "bytes": d.byte_size, "sha256": d.sha256,
            "status": d.processing_status, "note": d.processing_note,
        })).collect::<Vec<_>>(),
        "facts": facts(&case),
        "medications": case.medications.iter().map(|m| json!({
            "id": m.display_id, "drug": m.drug_name, "status": m.status, "route": m.route,
            "frequency": m.frequency, "date": m.record_date,
        })).collect::<Vec<_>>(),
        "conflicts": case.conflicts.iter().map(|c| json!({
            "id": c.display_id, "concept": c.concept_label, "left": c.left_summary,
            "right": c.right_summary, "basis": c.basis,
        })).collect::<Vec<_>>(),
        "changes": case.changes.iter().map(|c| json!({
            "id": c.display_id, "concept": c.concept_label, "direction": c.direction, "basis": c.basis,
        })).collect::<Vec<_>>(),
        "gaps": case.gaps.iter().map(|g| json!({
            "id": g.display_id, "title": g.title, "basis": g.basis, "not_located": g.evidence_not_located,
        })).collect::<Vec<_>>(),
        "mapped": out.mapped.iter().map(|m| json!({
            "concept": m.extractor_concept, "key": m.dsd_key, "name": name(m.node), "state": format!("{:?}", m.state),
        })).collect::<Vec<_>>(),
        "quarantined": out.quarantined.iter().map(|q| json!({ "concept": q.extractor_concept, "reason": q.reason })).collect::<Vec<_>>(),
        "candidates": candidates,
        "abstained": assessment.abstained,
        "flags": flags.iter().map(|f| json!({
            "result": name(f.result), "confounder": name(f.confounder), "window_days": f.window_days,
            "timing": format!("{:?}", f.timing), "message": f.message,
        })).collect::<Vec<_>>(),
    })
}

fn facts(case: &CaseResult) -> Vec<Value> {
    let doc: HashMap<&str, &str> = case.documents.iter().map(|d| (d.id.as_str(), d.filename.as_str())).collect();
    let src: HashMap<&str, &caretrace::models::Source> = case.sources.iter().map(|s| (s.id.as_str(), s)).collect();
    case.facts
        .iter()
        .map(|f| {
            let s = src.get(f.source_id.as_str());
            json!({
                "id": f.display_id, "concept": f.concept, "surface": f.surface_form,
                "value": f.value_num, "text": f.value_text, "unit": f.unit, "direction": f.interpretation,
                "date": f.obs_date, "confidence": f.confidence,
                "document": s.and_then(|s| doc.get(s.document_id.as_str()).copied()),
                "page": s.map(|s| s.page_number),
                "source_text": s.map(|s| s.source_text.as_str()),
            })
        })
        .collect()
}

/// Everything the model says about a disease, grouped the way the local UI
/// groups it, with `present` marking what this patient documents.
fn disease_detail(k: &dyn KnowledgeProvider, disease: NodeId, present: &HashSet<NodeId>) -> Value {
    let mut groups: Vec<(&str, Vec<Value>)> = ["gene", "confirmatory", "discriminating", "features", "triggers", "management"]
        .iter()
        .map(|g| (*g, Vec::new()))
        .collect();
    for e in k.neighbors(disease) {
        let slot = match e.edge_type {
            EdgeType::CausedBy => 0,
            EdgeType::Requires => 1,
            EdgeType::Discriminates => 2,
            EdgeType::HasFinding | EdgeType::HasPhenotype | EdgeType::HasPattern | EdgeType::AssociatedWith => 3,
            EdgeType::TriggeredBy => 4,
            EdgeType::TreatedBy => 5,
            _ => continue,
        };
        groups[slot].1.push(json!({ "name": k.name(e.target).unwrap_or("?"), "present": present.contains(&e.target) }));
    }
    Value::Object(groups.into_iter().map(|(g, v)| (g.to_string(), Value::Array(v))).collect())
}

fn outcome_str(o: EvidenceOutcome) -> &'static str {
    match o {
        EvidenceOutcome::StronglySupported => "StronglySupported",
        EvidenceOutcome::Supported => "Supported",
        EvidenceOutcome::Possible => "Possible",
        EvidenceOutcome::ConflictingEvidence => "ConflictingEvidence",
        EvidenceOutcome::InsufficientEvidence => "InsufficientEvidence",
        EvidenceOutcome::Contradicted => "Contradicted",
        EvidenceOutcome::Abstain => "Abstain",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn top(docs: &[&str]) -> Value {
        let model = load_model(EXAMPLE_SRC).expect("example model compiles");
        let docs: Vec<(String, Vec<u8>)> = docs
            .iter()
            .map(|n| (n.to_string(), std::fs::read(format!("../../samples/{n}")).expect("sample")))
            .collect();
        let r = analyze(&docs, &model);
        assert_eq!(r["ok"], true);
        r
    }

    #[test]
    fn missing_confirmatory_test_is_named() {
        let r = top(&["doctor_note_g6pd.txt"]);
        assert_eq!(r["candidates"][0]["outcome"], "Possible");
        assert_eq!(r["candidates"][0]["missing"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn adding_the_report_confirms() {
        let r = top(&["doctor_note_g6pd.txt", "g6pd_enzyme_report.txt"]);
        assert_eq!(r["candidates"][0]["outcome"], "StronglySupported");
    }

    #[test]
    fn timing_trap_is_flagged() {
        assert_eq!(top(&["g6pd_assay_timing_trap.txt"])["flags"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn unmapped_record_abstains() {
        let r = top(&["cbc_visit1.txt", "cbc_visit2.txt", "discharge_summary.txt"]);
        assert!(r["candidates"].as_array().unwrap().is_empty());
        assert!(!r["conflicts"].as_array().unwrap().is_empty());
    }

    #[test]
    fn framing_round_trips() {
        let mut b = 1u32.to_le_bytes().to_vec();
        b.extend(5u32.to_le_bytes());
        b.extend(b"a.txt");
        b.extend(2u32.to_le_bytes());
        b.extend(b"hi");
        assert_eq!(unframe(&b), Some(vec![("a.txt".to_string(), b"hi".to_vec())]));
        assert_eq!(unframe(&b[..b.len() - 1]), None);
    }
}
