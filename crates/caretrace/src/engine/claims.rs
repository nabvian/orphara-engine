//! Claim evidence resolution.
//!
//! For each claim, look for observations the pack declares relevant to that
//! claim type and classify each as supporting or contradicting using the pack's
//! declared thresholds. Three outcomes only: SUPPORTING_EVIDENCE,
//! CONTRADICTING_EVIDENCE, NO_LOCATED_EVIDENCE. NO_LOCATED_EVIDENCE never means
//! the claim is false — only that the required evidence is not in the records.

use std::collections::{HashMap, HashSet};

use crate::date::days_between;
use crate::engine::changes::fmt_g;
use crate::models::{claim_status, new_id, rel_type, status, Claim, Fact, Relationship};
use crate::packs;

/// A required test present in the record but outside the claim's window.
#[derive(Clone, Debug)]
pub struct OutOfWindow {
    pub concept: String,
    pub reason: String, // "after_claim" | "before_lookback"
    pub explanation: String,
}

/// One resolved claim, with everything the gap engine and UI need.
#[derive(Clone, Debug)]
pub struct ClaimResult {
    pub claim: Claim,
    pub status: String,
    pub located_concepts: Vec<String>,
    pub missing_concepts: Vec<String>,
    pub missing_labels: Vec<String>,
    pub lookback_days: i64,
    pub out_of_window: Vec<OutOfWindow>,
}

fn direction_matches(op: &str, value: f64, threshold: f64) -> bool {
    match op {
        "lt" => value < threshold,
        "lte" => value <= threshold,
        "gt" => value > threshold,
        "gte" => value >= threshold,
        _ => false,
    }
}

fn opposite(op: &str) -> &'static str {
    match op {
        "lt" => "gte",
        "lte" => "gt",
        "gt" => "lte",
        "gte" => "lt",
        _ => "",
    }
}

fn dedupe(items: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    items.into_iter().filter(|x| seen.insert(x.clone())).collect()
}

pub fn resolve_claims(claims: &[Claim], facts: &[Fact]) -> (Vec<ClaimResult>, Vec<Relationship>) {
    let mut results = Vec::new();
    let mut rels = Vec::new();

    let mut facts_by_concept: HashMap<String, Vec<Fact>> = HashMap::new();
    let mut status_by_concept: HashMap<String, Vec<Fact>> = HashMap::new();
    for f in facts {
        if f.value_num.is_some() {
            facts_by_concept.entry(f.concept.clone()).or_default().push(f.clone());
        } else if f.value_key.is_some() {
            status_by_concept.entry(f.concept.clone()).or_default().push(f.clone());
        }
    }

    for claim in claims {
        let ctd = match packs::claim_type_index().get(claim.claim_type.as_str()) {
            Some(c) => c.clone(),
            None => {
                results.push(ClaimResult {
                    claim: claim.clone(),
                    status: claim_status::NOT_ASSESSED.into(),
                    located_concepts: vec![],
                    missing_concepts: vec![],
                    missing_labels: vec![],
                    lookback_days: 0,
                    out_of_window: vec![],
                });
                continue;
            }
        };

        let lookback = packs::claim_lookback_days(&claim.claim_type);
        let claim_date = claim.claim_date.clone();
        let directions: HashMap<&str, (&str, f64)> = ctd
            .supportive_direction
            .iter()
            .map(|(c, op, thr)| (*c, (*op, *thr)))
            .collect();

        let mut supporting: Vec<(Fact, String)> = Vec::new();
        let mut contradicting: Vec<(Fact, String)> = Vec::new();
        let mut located: HashSet<String> = HashSet::new();
        let mut out_of_window: Vec<OutOfWindow> = Vec::new();

        // Qualitative (status) evidence.
        if let Some(want) = ctd.supportive_status_polarity {
            for concept in ctd.requires_any {
                for f in status_by_concept.get(*concept).into_iter().flatten() {
                    if let (Some(cd), Some(fd)) = (&claim_date, &f.obs_date) {
                        match days_between(cd, fd) {
                            Some(d) if d < 0 => {
                                out_of_window.push(oow(f, concept, "after_claim", lookback));
                                continue;
                            }
                            Some(d) if d > lookback => {
                                out_of_window.push(oow(f, concept, "before_lookback", lookback));
                                continue;
                            }
                            _ => {}
                        }
                    }
                    located.insert(concept.to_string());
                    if let Some(vk) = &f.value_key {
                        match packs::status_value_positive(concept, vk) {
                            None => {}
                            Some(p) if p == want => supporting.push((f.clone(), concept.to_string())),
                            Some(_) => contradicting.push((f.clone(), concept.to_string())),
                        }
                    }
                }
            }
        }

        // Numeric evidence.
        for concept in ctd.requires_any {
            for f in facts_by_concept.get(*concept).into_iter().flatten() {
                if let (Some(cd), Some(fd)) = (&claim_date, &f.obs_date) {
                    match days_between(cd, fd) {
                        Some(d) if d < 0 => {
                            out_of_window.push(oow(f, concept, "after_claim", lookback));
                            continue;
                        }
                        Some(d) if d > lookback => {
                            out_of_window.push(oow(f, concept, "before_lookback", lookback));
                            continue;
                        }
                        _ => {}
                    }
                }
                located.insert(concept.to_string());
                if let Some((op, thr)) = directions.get(*concept) {
                    let v = f.value_num.unwrap();
                    if direction_matches(op, v, *thr) {
                        supporting.push((f.clone(), concept.to_string()));
                    } else if direction_matches(opposite(op), v, *thr) {
                        contradicting.push((f.clone(), concept.to_string()));
                    }
                }
            }
        }

        let required: Vec<String> = ctd.requires_any.iter().map(|s| s.to_string()).collect();
        let missing: Vec<String> = required.iter().filter(|c| !located.contains(*c)).cloned().collect();
        let missing_labels = {
            let m = dedupe(missing.iter().map(|c| packs::concept_label(c)).collect());
            if m.is_empty() {
                dedupe(ctd.requires_labels.iter().map(|s| s.to_string()).collect())
            } else {
                m
            }
        };

        let st = if !contradicting.is_empty() {
            claim_status::CONTRADICTING
        } else if !supporting.is_empty() {
            claim_status::SUPPORTING
        } else {
            claim_status::NO_LOCATED
        };

        for (f, concept) in &supporting {
            rels.push(Relationship {
                id: new_id(),
                rel_type: rel_type::SUPPORTS.into(),
                from_type: "fact".into(),
                from_id: f.id.clone(),
                to_type: "claim".into(),
                to_id: claim.id.clone(),
                detail: Some(detail(f, concept)),
                status: status::DOCUMENTED.into(),
            });
        }
        for (f, concept) in &contradicting {
            rels.push(Relationship {
                id: new_id(),
                rel_type: rel_type::CONTRADICTS.into(),
                from_type: "fact".into(),
                from_id: f.id.clone(),
                to_type: "claim".into(),
                to_id: claim.id.clone(),
                detail: Some(detail(f, concept)),
                status: status::UNRESOLVED.into(),
            });
        }
        if st == claim_status::NO_LOCATED {
            rels.push(Relationship {
                id: new_id(),
                rel_type: rel_type::MISSING_SUPPORT.into(),
                from_type: "claim".into(),
                from_id: claim.id.clone(),
                to_type: "concept".into(),
                to_id: if missing.is_empty() { claim.claim_type.clone() } else { missing.join(",") },
                detail: Some(format!(
                    "Evidence declared as required for this claim type was not located in the uploaded records: {}",
                    missing_labels.join(", ")
                )),
                status: status::MISSING.into(),
            });
        }

        let mut located_sorted: Vec<String> = located.into_iter().collect();
        located_sorted.sort();
        results.push(ClaimResult {
            claim: claim.clone(),
            status: st.to_string(),
            located_concepts: located_sorted,
            missing_concepts: missing,
            missing_labels,
            lookback_days: lookback,
            out_of_window,
        });
    }
    (results, rels)
}

fn detail(f: &Fact, concept: &str) -> String {
    match f.value_num {
        None => format!(
            "{} documented {}.",
            f.value_text.clone().or_else(|| f.value_key.clone()).unwrap_or_default(),
            f.obs_date.clone().unwrap_or_default()
        ),
        Some(v) => format!(
            "{} {} {} documented {}.",
            packs::concept_label(concept),
            fmt_g(v),
            f.unit.clone().unwrap_or_default(),
            f.obs_date.clone().unwrap_or_default()
        )
        .replace("  ", " "),
    }
}

fn oow(f: &Fact, concept: &str, reason: &str, lookback: i64) -> OutOfWindow {
    let label = packs::concept_label(concept);
    let value_part = match f.value_num {
        None => format!(
            "({} on {}) ",
            f.value_text.clone().or_else(|| f.value_key.clone()).unwrap_or_default(),
            f.obs_date.clone().unwrap_or_default()
        ),
        Some(v) => format!(
            "({} {} on {}) ",
            fmt_g(v),
            f.unit.clone().unwrap_or_default(),
            f.obs_date.clone().unwrap_or_default()
        ),
    };
    let tail = if reason == "after_claim" {
        "after this claim, so it cannot be evidence the claim was based on.".to_string()
    } else {
        format!("more than {lookback} days before this claim.")
    };
    let explanation = format!("{label} is documented in the records {value_part}but is dated {tail}")
        .replace("  ", " ");
    OutOfWindow {
        concept: concept.to_string(),
        reason: reason.to_string(),
        explanation,
    }
}
