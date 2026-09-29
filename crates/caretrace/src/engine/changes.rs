//! Change detection.
//!
//! A documented change is: same normalized concept, different observation
//! dates, different values, in DIFFERENT clinical episodes. It is a statement
//! about the documents, not the patient. Same-episode pairs belong to the
//! conflict engine. Duplicates (same concept + date + value) are recorded as
//! DUPLICATES relationships and never counted as changes.

use std::collections::HashMap;

use crate::date::days_between;
use crate::models::{new_id, rel_type, status, Change, Fact, Relationship};
use crate::packs;

fn days(a: &Option<String>, b: &Option<String>) -> i64 {
    match (a, b) {
        (Some(a), Some(b)) => days_between(b, a).map(|d| d.abs()).unwrap_or(1_000_000),
        _ => 1_000_000,
    }
}

fn sort_key(f: &Fact) -> (String, String, String) {
    (
        f.obs_date.clone().unwrap_or_else(|| "9999-12-31".into()),
        f.display_id.clone(),
        f.id.clone(),
    )
}

fn group_by_concept(facts: &[Fact]) -> Vec<(String, Vec<Fact>)> {
    let mut map: HashMap<String, Vec<Fact>> = HashMap::new();
    for f in facts {
        map.entry(f.concept.clone()).or_default().push(f.clone());
    }
    let mut keys: Vec<String> = map.keys().cloned().collect();
    keys.sort();
    keys.into_iter()
        .map(|k| {
            let mut v = map.remove(&k).unwrap();
            v.sort_by_key(sort_key);
            (k, v)
        })
        .collect()
}

/// One representative fact per observation date (highest confidence, then id).
fn representatives(group: &[Fact]) -> HashMap<String, Fact> {
    let mut by_date: HashMap<String, Fact> = HashMap::new();
    for f in group {
        let (v, d) = match (f.value_num, &f.obs_date) {
            (Some(v), Some(d)) => (v, d.clone()),
            _ => continue,
        };
        let _ = v;
        match by_date.get(&d) {
            Some(cur) if (cur.confidence, &cur.id) >= (f.confidence, &f.id) => {}
            _ => {
                by_date.insert(d, f.clone());
            }
        }
    }
    by_date
}

pub fn detect_duplicates(facts: &[Fact]) -> Vec<Relationship> {
    let mut rels = Vec::new();
    let mut by_key: HashMap<(String, String, i64), Vec<Fact>> = HashMap::new();
    for f in facts {
        if let (Some(v), Some(d)) = (f.value_num, &f.obs_date) {
            let key = (f.concept.clone(), d.clone(), (v * 1e6).round() as i64);
            by_key.entry(key).or_default().push(f.clone());
        }
    }
    let mut keys: Vec<_> = by_key.keys().cloned().collect();
    keys.sort();
    for key in keys {
        let mut group = by_key.remove(&key).unwrap();
        if group.len() < 2 {
            continue;
        }
        group.sort_by_key(sort_key);
        let primary = group[0].clone();
        let value = key.2 as f64 / 1e6;
        for other in &group[1..] {
            rels.push(Relationship {
                id: new_id(),
                rel_type: rel_type::DUPLICATES.into(),
                from_type: "fact".into(),
                from_id: other.id.clone(),
                to_type: "fact".into(),
                to_id: primary.id.clone(),
                detail: Some(format!(
                    "{} {} dated {} is recorded in more than one document.",
                    packs::concept_label(&key.0),
                    fmt_g(value),
                    key.1
                )),
                status: status::DOCUMENTED.into(),
            });
        }
    }
    rels
}

pub fn detect_changes(facts: &[Fact]) -> (Vec<Change>, Vec<Relationship>) {
    let mut changes = Vec::new();
    let mut rels = Vec::new();

    for (concept, group) in group_by_concept(facts) {
        let window = packs::conflict_window_days(&concept);
        let by_date = representatives(&group);
        let mut dates: Vec<&String> = by_date.keys().collect();
        dates.sort();
        let series: Vec<&Fact> = dates.iter().map(|d| &by_date[*d]).collect();
        if series.len() < 2 {
            continue;
        }
        let label = packs::concept_label(&concept);
        for pair in series.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            let (av, bv) = match (a.value_num, b.value_num) {
                (Some(av), Some(bv)) => (av, bv),
                _ => continue,
            };
            if av == bv {
                continue;
            }
            if days(&a.obs_date, &b.obs_date) <= window {
                continue; // same episode → conflict, not change
            }
            let delta = ((bv - av) * 1e6).round() / 1e6;
            let unit = b.unit.clone().or_else(|| a.unit.clone()).unwrap_or_default();
            let basis = format!(
                "{label} is documented as {} {unit} on {} and {} {unit} on {} in the uploaded records.",
                fmt_g(av),
                a.obs_date.clone().unwrap_or_default(),
                fmt_g(bv),
                b.obs_date.clone().unwrap_or_default()
            )
            .replace("  ", " ");
            changes.push(Change {
                id: new_id(),
                display_id: String::new(),
                concept: concept.clone(),
                concept_label: label.clone(),
                from_fact_id: a.id.clone(),
                to_fact_id: b.id.clone(),
                direction: if delta > 0.0 { "increase".into() } else { "decrease".into() },
                basis,
                unit: if unit.is_empty() { None } else { Some(unit) },
                from_date: a.obs_date.clone(),
                to_date: b.obs_date.clone(),
                from_value: Some(av),
                to_value: Some(bv),
                delta: Some(delta),
            });
            rels.push(Relationship {
                id: new_id(),
                rel_type: rel_type::CHANGED_TO.into(),
                from_type: "fact".into(),
                from_id: a.id.clone(),
                to_type: "fact".into(),
                to_id: b.id.clone(),
                detail: Some(format!(
                    "Documented change in {label}: {} on {} to {} on {}.",
                    fmt_g(av),
                    a.obs_date.clone().unwrap_or_default(),
                    fmt_g(bv),
                    b.obs_date.clone().unwrap_or_default()
                )),
                status: status::CHANGED.into(),
            });
            rels.push(Relationship {
                id: new_id(),
                rel_type: rel_type::CHANGED_FROM.into(),
                from_type: "fact".into(),
                from_id: b.id.clone(),
                to_type: "fact".into(),
                to_id: a.id.clone(),
                detail: Some(format!(
                    "Preceding documented {label}: {} on {}.",
                    fmt_g(av),
                    a.obs_date.clone().unwrap_or_default()
                )),
                status: status::CHANGED.into(),
            });
        }
    }
    changes.sort_by(|a, b| {
        (
            &a.concept,
            a.from_date.clone().unwrap_or_default(),
            a.to_date.clone().unwrap_or_default(),
        )
            .cmp(&(
                &b.concept,
                b.from_date.clone().unwrap_or_default(),
                b.to_date.clone().unwrap_or_default(),
            ))
    });
    (changes, rels)
}

/// Format a float like `%g` (drop trailing
/// zeros, no exponent for the magnitudes involved here).
pub fn fmt_g(v: f64) -> String {
    if v == v.trunc() && v.abs() < 1e15 {
        format!("{}", v as i64)
    } else {
        let s = format!("{v}");
        s
    }
}
