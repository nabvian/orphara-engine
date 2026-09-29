//! Evidence gap detection.
//!
//! Four families, all phrased as statements about the RECORD: unsupported
//! claim, missing (cited-but-not-uploaded) document, unreconcilable medication,
//! and an expected follow-up with no later record. Every gap says what was
//! looked for and what was found; none says the claim is false or the care was
//! wrong.

use std::collections::HashSet;

use crate::date::{days_between, epoch_from_iso, iso_from_epoch};
use crate::engine::claims::ClaimResult;
use crate::models::{
    claim_status, gap_type, new_id, status, Document, DocumentReference, EvidenceGap, Event,
    Medication,
};
use crate::packs;

pub fn gaps_from_claims(claim_results: &[ClaimResult]) -> Vec<EvidenceGap> {
    let mut out = Vec::new();
    for res in claim_results {
        if res.status != claim_status::NO_LOCATED {
            continue;
        }
        // De-dupe out-of-window explanations by (concept, reason).
        let mut seen: HashSet<(String, String)> = HashSet::new();
        let mut oow = String::new();
        for o in &res.out_of_window {
            if seen.insert((o.concept.clone(), o.reason.clone())) {
                oow.push(' ');
                oow.push_str(&o.explanation);
            }
        }
        out.push(EvidenceGap {
            id: new_id(),
            display_id: String::new(),
            gap_type: gap_type::MISSING_SUPPORTING_TEST.into(),
            subject_type: "claim".into(),
            title: res.claim.claim_text.clone(),
            basis: format!(
                "The active pack declares evidence for this claim type. No qualifying observation was located in the uploaded records within {} days before the claim date. This is a statement about the records, not about the claim.{oow}",
                res.lookback_days
            ),
            status: status::MISSING.into(),
            subject_id: Some(res.claim.id.clone()),
            evidence_located: res.located_concepts.iter().map(|c| packs::concept_label(c)).collect(),
            evidence_not_located: res.missing_labels.clone(),
        });
    }
    out
}

const REFERENCE_LOOKBACK_DAYS: i64 = 45;
const LABEL_STOPWORDS: &[&str] = &[
    "report", "reports", "study", "studies", "scan", "scans", "the", "a", "an",
    "prior", "previous", "results", "result",
];

fn label_terms(label: &str) -> Vec<String> {
    let mut words: Vec<String> = label
        .to_lowercase()
        .split(|c: char| !(c.is_ascii_alphanumeric() || c == '-'))
        .filter(|w| !w.is_empty() && !LABEL_STOPWORDS.contains(w))
        .map(|w| w.to_string())
        .collect();
    words.sort_by_key(|w| std::cmp::Reverse(w.len()));
    words
}

/// Whole-word containment (the two-letter 'ct' must not match inside 'acute').
fn mentions(text: &str, term: &str) -> bool {
    if term.is_empty() {
        return false;
    }
    let b = text.as_bytes();
    let mut from = 0;
    while let Some(rel) = text[from..].find(term) {
        let s = from + rel;
        let e = s + term.len();
        let before = s == 0 || !b[s - 1].is_ascii_alphanumeric();
        let after = e == b.len() || !b[e].is_ascii_alphanumeric();
        if before && after {
            return true;
        }
        from = s + 1;
    }
    false
}

#[allow(clippy::type_complexity)]
pub fn gaps_from_references(
    references: &[DocumentReference],
    documents: &[Document],
    citing_date_of: &dyn Fn(&DocumentReference) -> Option<String>,
    citing_document_of: &dyn Fn(&DocumentReference) -> Option<String>,
    document_terms_of: &dyn Fn(&Document) -> String,
) -> (Vec<EvidenceGap>, Vec<(String, String)>) {
    let mut out = Vec::new();
    let mut resolved = Vec::new();
    let uploaded_names = documents
        .iter()
        .map(|d| d.filename.to_lowercase())
        .collect::<Vec<_>>()
        .join(" ");
    let mut seen: HashSet<(String, Option<String>)> = HashSet::new();

    let terms_of = |d: &Document| -> String {
        let base = d.filename.to_lowercase().replace('_', " ");
        format!("{base} {}", document_terms_of(d).to_lowercase())
    };

    for r in references {
        let cite_doc = citing_document_of(r);
        if !seen.insert((r.label.clone(), cite_doc.clone())) {
            continue;
        }
        let cite_date = citing_date_of(r);
        let terms = label_terms(&r.label);

        let candidates: Vec<&Document> = documents
            .iter()
            .filter(|d| {
                r.doc_type_hint.as_deref() == Some(d.doc_type.as_str())
                    && Some(d.id.clone()) != cite_doc
            })
            .collect();

        if candidates.is_empty() {
            let token = terms.first().cloned().unwrap_or_else(|| r.label.to_lowercase());
            if mentions(&uploaded_names, &token) {
                continue; // named in a filename; treat as present
            }
        } else {
            let mut in_window: Vec<&Document> = Vec::new();
            for d in &candidates {
                if !terms.is_empty() && !terms.iter().any(|t| mentions(&terms_of(d), t)) {
                    continue;
                }
                match gap_days(&cite_date, &d.doc_date) {
                    None => in_window.push(d),
                    Some(g) if (-REFERENCE_LOOKBACK_DAYS..=REFERENCE_LOOKBACK_DAYS).contains(&g) => {
                        in_window.push(d)
                    }
                    _ => {}
                }
            }
            if !in_window.is_empty() {
                let best = in_window
                    .iter()
                    .min_by_key(|d| gap_days(&cite_date, &d.doc_date).unwrap_or(0).abs())
                    .unwrap();
                resolved.push((r.id.clone(), best.id.clone()));
                continue;
            }
        }

        out.push(EvidenceGap {
            id: new_id(),
            display_id: String::new(),
            gap_type: gap_type::MISSING_SOURCE.into(),
            subject_type: "document_reference".into(),
            title: r.label.clone(),
            basis: format!(
                "An uploaded record refers to this document. No other uploaded document matches it within {REFERENCE_LOOKBACK_DAYS} days of the citing record, so the statements that depend on it cannot be traced to a source."
            ),
            status: status::MISSING.into(),
            subject_id: Some(r.id.clone()),
            evidence_located: vec![],
            evidence_not_located: vec![r.label.clone()],
        });
    }
    (out, resolved)
}

fn gap_days(a: &Option<String>, b: &Option<String>) -> Option<i64> {
    match (a, b) {
        (Some(a), Some(b)) => days_between(a, b),
        _ => None,
    }
}

pub fn gaps_from_medications(implicated: &[Medication]) -> Vec<EvidenceGap> {
    implicated
        .iter()
        .map(|m| EvidenceGap {
            id: new_id(),
            display_id: String::new(),
            gap_type: gap_type::MEDICATION_DOCUMENTATION_GAP.into(),
            subject_type: "medication".into(),
            title: m.drug_name.clone(),
            basis: "The medication is documented as active in one record and is absent from a later medication list, with no stop date documented. The records do not state what happened.".into(),
            status: status::MISSING.into(),
            subject_id: Some(m.id.clone()),
            evidence_located: vec![format!(
                "Documented active in a record dated {}",
                m.record_date.clone().unwrap_or_default()
            )],
            evidence_not_located: vec![
                "Entry on the later medication list".into(),
                "A documented stop date or discontinuation note".into(),
            ],
        })
        .collect()
}

pub fn gaps_from_followups(
    events: &[Event],
    documents: &[Document],
    as_of: Option<&str>,
) -> Vec<EvidenceGap> {
    let mut out = Vec::new();
    let dated: Vec<(i64, &Document)> = documents
        .iter()
        .filter_map(|d| d.doc_date.as_deref().and_then(epoch_from_iso).map(|e| (e, d)))
        .collect();
    if dated.is_empty() {
        return out;
    }
    let horizon = as_of
        .and_then(epoch_from_iso)
        .unwrap_or_else(|| dated.iter().map(|(e, _)| *e).max().unwrap());

    let mut seen: HashSet<(String, String)> = HashSet::new();
    for ev in events {
        if ev.event_type != "EXPECTED_FOLLOWUP" {
            continue;
        }
        let ed = match ev.event_date.as_deref().and_then(epoch_from_iso) {
            Some(e) => e,
            None => continue,
        };
        let label = if ev.label.is_empty() { "Expected follow-up" } else { &ev.label };
        let (interval, wanted) = packs::followup_expectations().get(label).copied().unwrap_or((0, ""));
        let due = ed + interval;
        if due > horizon {
            continue; // not yet due
        }
        let later = dated.iter().any(|(dt, d)| {
            *dt > ed && (wanted.is_empty() || d.doc_type.as_str() == wanted)
        });
        if later {
            continue;
        }
        let key = (label.to_string(), wanted.to_string());
        if !seen.insert(key) {
            continue;
        }
        let wanted_label = if wanted.is_empty() {
            "follow-up".to_string()
        } else {
            crate::models::DocType::from_code(wanted).label().to_string()
        };
        out.push(EvidenceGap {
            id: new_id(),
            display_id: String::new(),
            gap_type: gap_type::MISSING_FOLLOWUP.into(),
            subject_type: "event".into(),
            title: label.to_string(),
            basis: format!(
                "A record dated {} documents this expected follow-up. The expected interval of {interval} days has elapsed within the span of the uploaded records and no matching later document was located.",
                ev.event_date.clone().unwrap_or_default()
            ),
            status: status::MISSING.into(),
            subject_id: Some(ev.id.clone()),
            evidence_located: vec![format!(
                "Follow-up instruction documented {}",
                ev.event_date.clone().unwrap_or_default()
            )],
            evidence_not_located: vec![format!("A {wanted_label} dated after {}", iso_from_epoch(due))],
        });
    }
    out
}
