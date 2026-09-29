//! The processing pipeline.
//!
//! ```text
//! INGEST -> CLASSIFY -> EXTRACT -> NORMALIZE -> FACTS/CLAIMS/MEDS/EVENTS
//!        -> CHANGES -> CONFLICTS -> RESOLVE_CLAIMS -> RELATIONSHIPS
//!        -> GAPS -> AUDIT
//! ```
//!
//! Each stage is recorded on the result. The case is treated as a single
//! patient; detection runs over a fact slice, so partitioning by patient later
//! needs no change here.

use std::collections::HashMap;

use crate::engine::{changes, claims, conflicts, gaps};
use crate::extract::{Payload, RuleExtractor};
use crate::ingest::Ingested;
use crate::models::{
    claim_status, new_id, status, Change, Claim, Conflict, Document, DocumentReference,
    EvidenceGap, Event, Fact, Medication, Relationship, Source,
};

/// One recorded pipeline stage.
#[derive(Clone, Debug)]
pub struct Stage {
    pub name: &'static str,
    pub counts: Vec<(&'static str, i64)>,
}

/// The full result of one audit run over a case.
#[derive(Debug, Default)]
pub struct CaseResult {
    pub documents: Vec<Document>,
    pub sources: Vec<Source>,
    pub facts: Vec<Fact>,
    pub claims: Vec<Claim>,
    pub medications: Vec<Medication>,
    pub events: Vec<Event>,
    pub references: Vec<DocumentReference>,
    pub changes: Vec<Change>,
    pub conflicts: Vec<Conflict>,
    pub gaps: Vec<EvidenceGap>,
    pub relationships: Vec<Relationship>,
    pub claim_results: Vec<claims::ClaimResult>,
    pub counts: HashMap<&'static str, f64>,
    pub stages: Vec<Stage>,
}

fn display(prefix: &str, n: usize) -> String {
    format!("{prefix}-{n:03}")
}

/// Run the full audit over already-ingested documents.
pub fn process_case(mut ingested: Vec<Ingested>) -> CaseResult {
    crate::packs::register_unit_aliases();
    let extractor = RuleExtractor;
    let mut result = CaseResult::default();
    let mut stages: Vec<Stage> = Vec::new();

    // Preserve upload order.
    ingested.sort_by_key(|d| d.document.ordinal);

    let total_pages: usize = ingested.iter().map(|d| d.document.page_count).sum();
    stages.push(Stage { name: "INGEST", counts: vec![("documents", ingested.len() as i64), ("pages", total_pages as i64)] });

    // ---- CLASSIFY ----
    for ing in &mut ingested {
        let full_text = ing.pages.iter().map(|p| p.text.as_str()).collect::<Vec<_>>().join("\n");
        ing.document.doc_type = extractor.classify(&full_text, &ing.document.filename);
        ing.document.doc_date = extractor.document_date(&full_text).0;
    }
    let types: std::collections::HashSet<&str> =
        ingested.iter().map(|d| d.document.doc_type.as_str()).collect();
    stages.push(Stage { name: "CLASSIFY", counts: vec![("documents", ingested.len() as i64), ("types", types.len() as i64)] });

    // ---- EXTRACT + NORMALIZE ----
    // Collect (doc_index, page_index, RawItem) so sources can be created next.
    let mut raw: Vec<(usize, usize, crate::extract::RawItem)> = Vec::new();
    let mut n_items = 0;
    for (di, ing) in ingested.iter_mut().enumerate() {
        let mut any = false;
        for (pi, page) in ing.pages.iter().enumerate() {
            if page.text.trim().is_empty() {
                continue;
            }
            for item in extractor.extract_page(&page.text, ing.document.doc_type, &ing.document.doc_date) {
                raw.push((di, pi, item));
                any = true;
                n_items += 1;
            }
        }
        if any {
            ing.document.processing_status = "PROCESSED".into();
            ing.document.processing_note = None;
        } else {
            ing.document.processing_status = "EXTRACTION_INCOMPLETE".into();
            ing.document.processing_note = Some(
                "Unable to identify structured observations, claims or medications in this document. Requires review.".into(),
            );
        }
    }
    stages.push(Stage { name: "EXTRACT", counts: vec![("items", n_items as i64)] });
    let concepts: std::collections::HashSet<String> = raw
        .iter()
        .filter_map(|(_, _, it)| match &it.payload {
            Payload::Observation(o) => Some(o.concept.clone()),
            _ => None,
        })
        .collect();
    stages.push(Stage { name: "NORMALIZE", counts: vec![("concepts", concepts.len() as i64)] });

    // ---- SOURCES + records ----
    let mut doc_of_source: HashMap<String, String> = HashMap::new();
    let mut counters = HashMap::from([("F", 0usize), ("C", 0), ("M", 0), ("E", 0), ("R", 0)]);

    for (di, pi, item) in &raw {
        let doc = &ingested[*di].document;
        let page = &ingested[*di].pages[*pi];
        let src = Source {
            id: new_id(),
            document_id: doc.id.clone(),
            page_id: page.id.clone(),
            page_number: page.page_number,
            source_text: item.source_text.clone(),
            char_start: item.char_start,
            char_end: item.char_end,
        };
        doc_of_source.insert(src.id.clone(), doc.id.clone());
        let src_id = src.id.clone();
        result.sources.push(src);

        match &item.payload {
            Payload::Observation(o) => {
                let c = counters.get_mut("F").unwrap();
                *c += 1;
                result.facts.push(Fact {
                    id: new_id(),
                    display_id: display("F", *c),
                    concept: o.concept.clone(),
                    surface_form: o.surface_form.clone(),
                    source_id: src_id,
                    value_num: o.value,
                    value_text: None,
                    value_key: None,
                    unit: o.unit.clone(),
                    obs_date: o.date.clone(),
                    date_precision: o.date_precision.clone(),
                    confidence: item.confidence,
                    interpretation: o.interpretation.clone(),
                });
            }
            Payload::Finding(f) => {
                let c = counters.get_mut("F").unwrap();
                *c += 1;
                result.facts.push(Fact {
                    id: new_id(),
                    display_id: display("F", *c),
                    concept: f.concept.clone(),
                    surface_form: f.surface_form.clone(),
                    source_id: src_id,
                    value_num: None,
                    value_text: Some(f.value_text.clone()),
                    value_key: Some(f.value_key.clone()),
                    unit: None,
                    obs_date: f.date.clone(),
                    date_precision: f.date_precision.clone(),
                    confidence: item.confidence,
                    interpretation: None,
                });
            }
            Payload::Claim(cl) => {
                let c = counters.get_mut("C").unwrap();
                *c += 1;
                result.claims.push(Claim {
                    id: new_id(),
                    display_id: display("C", *c),
                    claim_text: cl.claim_text.clone(),
                    source_id: src_id,
                    claim_type: cl.claim_type.clone(),
                    claim_date: cl.date.clone(),
                    date_precision: cl.date_precision.clone(),
                    confidence: item.confidence,
                    evidence_status: claim_status::NOT_ASSESSED.into(),
                });
            }
            Payload::Medication(m) => {
                let c = counters.get_mut("M").unwrap();
                *c += 1;
                result.medications.push(Medication {
                    id: new_id(),
                    display_id: display("M", *c),
                    drug_name: m.drug_name.clone(),
                    drug_norm: m.drug_norm.clone(),
                    source_id: src_id,
                    dose: m.dose,
                    unit: m.unit.clone(),
                    frequency: m.frequency.clone(),
                    route: m.route.clone(),
                    status: m.status.clone(),
                    start_date: m.start_date.clone(),
                    stop_date: m.stop_date.clone(),
                    record_date: m.record_date.clone(),
                    confidence: item.confidence,
                });
            }
            Payload::Event(e) => {
                let c = counters.get_mut("E").unwrap();
                *c += 1;
                result.events.push(Event {
                    id: new_id(),
                    display_id: display("E", *c),
                    event_type: e.event_type.clone(),
                    label: e.label.clone(),
                    source_id: src_id,
                    event_date: e.date.clone(),
                    detail: e.detail.clone(),
                });
            }
            Payload::Reference(r) => {
                let c = counters.get_mut("R").unwrap();
                *c += 1;
                result.references.push(DocumentReference {
                    id: new_id(),
                    display_id: display("R", *c),
                    label: r.label.clone(),
                    source_id: src_id,
                    doc_type_hint: Some(r.doc_type_hint.clone()),
                    resolved_document_id: None,
                    status: status::MISSING.into(),
                });
            }
        }
    }
    stages.push(Stage { name: "CREATE_FACTS", counts: vec![("facts", result.facts.len() as i64)] });
    stages.push(Stage { name: "CREATE_CLAIMS", counts: vec![("claims", result.claims.len() as i64)] });
    stages.push(Stage {
        name: "CREATE_EVENTS",
        counts: vec![("events", result.events.len() as i64), ("references", result.references.len() as i64)],
    });

    // Provenance helpers.
    let doc_by_id: HashMap<String, Document> =
        ingested.iter().map(|i| (i.document.id.clone(), i.document.clone())).collect();
    let doc_of_source_c = doc_of_source.clone();
    let doc_of_fact = move |f: &Fact| doc_of_source_c.get(&f.source_id).cloned();
    let doc_of_source_m = doc_of_source.clone();
    let doc_of_med = move |m: &Medication| doc_of_source_m.get(&m.source_id).cloned();
    let doc_by_id_t = doc_by_id.clone();
    let doc_type_of_med = move |m: &Medication| {
        doc_of_source.get(&m.source_id).and_then(|d| doc_by_id_t.get(d)).map(|d| d.doc_type.as_str().to_string())
    };
    let doc_by_id_l = doc_by_id.clone();
    let doc_label = move |id: &str| doc_by_id_l.get(id).map(|d| d.filename.clone()).unwrap_or_else(|| id.to_string());

    // ---- CHANGES ----
    let (change_rows, change_rels) = changes::detect_changes(&result.facts);
    let dup_rels = changes::detect_duplicates(&result.facts);
    for (i, ch) in change_rows.into_iter().enumerate() {
        let mut ch = ch;
        ch.display_id = display("CH", i + 1);
        result.changes.push(ch);
    }
    stages.push(Stage { name: "DETECT_CHANGES", counts: vec![("changes", result.changes.len() as i64), ("duplicates", dup_rels.len() as i64)] });

    // ---- CONFLICTS ----
    let (num_c, num_r) = conflicts::detect_numeric_conflicts(&result.facts, &doc_of_fact);
    let (stat_c, stat_r) = conflicts::detect_status_conflicts(&result.facts, &doc_of_fact);
    let (med_c, med_r, implicated) =
        conflicts::detect_medication_conflicts(&result.medications, &doc_of_med, &doc_type_of_med, &doc_label);
    let n_num = num_c.len();
    let n_stat = stat_c.len();
    let n_med = med_c.len();
    let mut all_conflicts: Vec<Conflict> = Vec::new();
    all_conflicts.extend(num_c);
    all_conflicts.extend(stat_c);
    all_conflicts.extend(med_c);
    for (i, cf) in all_conflicts.into_iter().enumerate() {
        let mut cf = cf;
        cf.display_id = display("CF", i + 1);
        result.conflicts.push(cf);
    }
    stages.push(Stage { name: "DETECT_CONFLICTS", counts: vec![("numeric", n_num as i64), ("status", n_stat as i64), ("medication", n_med as i64)] });

    // ---- RESOLVE CLAIMS ----
    let (claim_results, claim_rels) = claims::resolve_claims(&result.claims, &result.facts);
    for res in &claim_results {
        if let Some(c) = result.claims.iter_mut().find(|c| c.id == res.claim.id) {
            c.evidence_status = res.status.clone();
        }
    }
    stages.push(Stage { name: "RESOLVE_CLAIMS", counts: vec![("assessed", claim_results.len() as i64)] });
    result.claim_results = claim_results.clone();

    // ---- RELATIONSHIPS ----
    let mut n_rels = 0;
    for rel in change_rels.into_iter().chain(dup_rels).chain(num_r).chain(stat_r).chain(med_r).chain(claim_rels) {
        result.relationships.push(rel);
        n_rels += 1;
    }
    stages.push(Stage { name: "BUILD_RELATIONSHIPS", counts: vec![("relationships", n_rels)] });

    // ---- GAPS ----
    let src_by_id: HashMap<String, Source> =
        result.sources.iter().map(|s| (s.id.clone(), s.clone())).collect();
    let doc_by_id_g = doc_by_id.clone();
    let src_map = src_by_id.clone();
    let citing_doc_of = move |r: &DocumentReference| src_map.get(&r.source_id).map(|s| s.document_id.clone());
    let src_map2 = src_by_id.clone();
    let doc_by_id_g2 = doc_by_id_g.clone();
    let citing_date_of = move |r: &DocumentReference| {
        src_map2
            .get(&r.source_id)
            .and_then(|s| doc_by_id_g2.get(&s.document_id))
            .and_then(|d| d.doc_date.clone())
    };
    // A document names itself in its first few lines (page-one header).
    let pages_by_doc: HashMap<String, String> = ingested
        .iter()
        .map(|i| {
            let head = i
                .pages
                .first()
                .map(|p| {
                    p.text
                        .lines()
                        .map(|l| l.trim())
                        .filter(|l| !l.is_empty())
                        .take(3)
                        .collect::<Vec<_>>()
                        .join(" ")
                })
                .unwrap_or_default();
            (i.document.id.clone(), head)
        })
        .collect();
    let document_terms_of = move |d: &Document| pages_by_doc.get(&d.id).cloned().unwrap_or_default();

    let documents_vec: Vec<Document> = ingested.iter().map(|i| i.document.clone()).collect();
    let (ref_gaps, ref_resolved) = gaps::gaps_from_references(
        &result.references,
        &documents_vec,
        &citing_date_of,
        &citing_doc_of,
        &document_terms_of,
    );
    for (ref_id, doc_id) in ref_resolved {
        if let Some(r) = result.references.iter_mut().find(|r| r.id == ref_id) {
            r.resolved_document_id = Some(doc_id);
            r.status = "RESOLVED".into();
        }
    }
    let mut gap_rows: Vec<EvidenceGap> = Vec::new();
    gap_rows.extend(gaps::gaps_from_claims(&claim_results));
    gap_rows.extend(ref_gaps);
    gap_rows.extend(gaps::gaps_from_medications(&implicated));
    gap_rows.extend(gaps::gaps_from_followups(&result.events, &documents_vec, None));
    for (i, g) in gap_rows.into_iter().enumerate() {
        let mut g = g;
        g.display_id = display("G", i + 1);
        result.gaps.push(g);
    }
    stages.push(Stage { name: "DETECT_GAPS", counts: vec![("gaps", result.gaps.len() as i64)] });

    // ---- AUDIT ----
    result.documents = documents_vec;
    let total_items = result.facts.len() + result.claims.len() + result.medications.len();
    let traceable = result.facts.iter().filter(|f| !f.source_id.is_empty()).count()
        + result.claims.iter().filter(|c| !c.source_id.is_empty()).count()
        + result.medications.iter().filter(|m| !m.source_id.is_empty()).count();
    let unresolved = result.conflicts.iter().filter(|c| c.status == status::UNRESOLVED).count();
    let mut counts: HashMap<&'static str, f64> = HashMap::new();
    counts.insert("documents", result.documents.len() as f64);
    counts.insert("pages", total_pages as f64);
    counts.insert("facts", result.facts.len() as f64);
    counts.insert("claims", result.claims.len() as f64);
    counts.insert("medications", result.medications.len() as f64);
    counts.insert("events", result.events.len() as f64);
    counts.insert("changes", result.changes.len() as f64);
    counts.insert("conflicts", result.conflicts.len() as f64);
    counts.insert("evidence_gaps", result.gaps.len() as f64);
    counts.insert("relationships", n_rels as f64);
    counts.insert("duplicates", result.relationships.iter().filter(|r| r.rel_type == "DUPLICATES").count() as f64);
    counts.insert("unresolved", unresolved as f64);
    counts.insert("source_traceability", if total_items > 0 { traceable as f64 / total_items as f64 } else { 0.0 });
    stages.push(Stage {
        name: "GENERATE_AUDIT",
        counts: vec![
            ("changes", result.changes.len() as i64),
            ("conflicts", result.conflicts.len() as i64),
            ("evidence_gaps", result.gaps.len() as i64),
        ],
    });

    result.counts = counts;
    result.stages = stages;
    result
}
