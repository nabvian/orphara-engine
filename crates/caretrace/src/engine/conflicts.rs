//! Conflict detection.
//!
//! A winning value is never selected. A conflict is recorded with both
//! sides, both provenance chains, and status UNRESOLVED. Three families:
//! numeric (same concept, same episode, materially different values), status
//! (mutually exclusive qualitative findings), and medication (active in one
//! record, absent from a later reconciliation list).

use std::collections::{HashMap, HashSet};

use crate::date::epoch_from_iso;
use crate::engine::changes::fmt_g;
use crate::models::{
    conflict_type, new_id, rel_type, status, Conflict, Fact, Medication, Relationship,
};
use crate::packs;

fn episode_clusters(group: &[Fact], window: i64) -> Vec<Vec<Fact>> {
    let mut sorted: Vec<Fact> = group.to_vec();
    sorted.sort_by(|a, b| {
        (a.obs_date.clone().unwrap_or_default(), &a.id)
            .cmp(&(b.obs_date.clone().unwrap_or_default(), &b.id))
    });
    let mut clusters: Vec<Vec<Fact>> = Vec::new();
    for f in sorted {
        let fd = match f.obs_date.as_deref().and_then(epoch_from_iso) {
            Some(d) => d,
            None => continue,
        };
        if let Some(last) = clusters.last_mut() {
            let prev = last.last().unwrap().obs_date.as_deref().and_then(epoch_from_iso);
            if let Some(prev) = prev {
                if fd - prev <= window {
                    last.push(f);
                    continue;
                }
            }
        }
        clusters.push(vec![f]);
    }
    clusters
}

pub fn detect_numeric_conflicts(
    facts: &[Fact],
    doc_of: &dyn Fn(&Fact) -> Option<String>,
) -> (Vec<Conflict>, Vec<Relationship>) {
    let mut conflicts = Vec::new();
    let mut rels = Vec::new();

    let mut by_concept: HashMap<String, Vec<Fact>> = HashMap::new();
    for f in facts {
        if f.value_num.is_some() && f.obs_date.is_some() {
            by_concept.entry(f.concept.clone()).or_default().push(f.clone());
        }
    }
    let mut concepts: Vec<String> = by_concept.keys().cloned().collect();
    concepts.sort();

    for concept in concepts {
        let group = &by_concept[&concept];
        let cd = packs::concepts().get(concept.as_str()).cloned();
        let window = packs::conflict_window_days(&concept);
        let label = packs::concept_label(&concept);
        let threshold = match &cd {
            Some(cd) => format!(
                "{} {} or {}% relative",
                fmt_g(cd.material_abs),
                cd.canonical_unit,
                fmt_g(cd.material_rel * 100.0)
            ),
            None => "any difference".to_string(),
        };

        for episode in episode_clusters(group, window) {
            if episode.len() < 2 {
                continue;
            }
            // Partition into value groups: not materially different -> same group.
            let mut groups: Vec<Vec<Fact>> = Vec::new();
            for f in &episode {
                let mut placed = false;
                for g in groups.iter_mut() {
                    if !packs::material_difference(&concept, g[0].value_num.unwrap(), f.value_num.unwrap()) {
                        g.push(f.clone());
                        placed = true;
                        break;
                    }
                }
                if !placed {
                    groups.push(vec![f.clone()]);
                }
            }
            if groups.len() < 2 {
                continue;
            }
            groups.sort_by(|a, b| {
                (a[0].obs_date.clone().unwrap_or_default(), &a[0].id)
                    .cmp(&(b[0].obs_date.clone().unwrap_or_default(), &b[0].id))
            });
            for i in 0..groups.len() {
                for j in (i + 1)..groups.len() {
                    let ga = &groups[i];
                    let gb = &groups[j];
                    let docs_a: HashSet<Option<String>> = ga.iter().map(doc_of).collect();
                    let docs_b: HashSet<Option<String>> = gb.iter().map(doc_of).collect();
                    if docs_a.union(&docs_b).count() < 2 {
                        continue;
                    }
                    let rep_a = ga.iter().min_by(rep_ord).unwrap();
                    let rep_b = gb.iter().min_by(rep_ord).unwrap();
                    let unit = rep_a.unit.clone().or_else(|| rep_b.unit.clone()).unwrap_or_default();
                    let (n_a, n_b) = (ga.len(), gb.len());
                    let all_docs = docs_a.union(&docs_b).count();
                    let extra = if n_a > 1 || n_b > 1 {
                        format!(
                            " The values are restated across {all_docs} documents ({n_a} record(s) on one side, {n_b} on the other); all are listed as sources."
                        )
                    } else {
                        String::new()
                    };
                    conflicts.push(Conflict {
                        id: new_id(),
                        display_id: String::new(),
                        conflict_type: conflict_type::NUMERIC_OBSERVATION.into(),
                        concept: concept.clone(),
                        concept_label: label.clone(),
                        left_type: "fact".into(),
                        left_id: rep_a.id.clone(),
                        right_type: "fact".into(),
                        right_id: rep_b.id.clone(),
                        left_summary: format!("{} {}", fmt_g(rep_a.value_num.unwrap()), unit).trim().to_string(),
                        right_summary: format!("{} {}", fmt_g(rep_b.value_num.unwrap()), unit).trim().to_string(),
                        left_member_ids: ga.iter().map(|f| f.id.clone()).collect(),
                        right_member_ids: gb.iter().map(|f| f.id.clone()).collect(),
                        basis: format!(
                            "Source documents record different {label} values for the same episode (records within {window} day(s) of each other). Materiality threshold: {threshold}. Neither value is selected.{extra}"
                        ),
                        delta: Some(((rep_b.value_num.unwrap() - rep_a.value_num.unwrap()) * 1e6).round() / 1e6),
                        status: status::UNRESOLVED.into(),
                    });
                    for fa in ga {
                        for fb in gb {
                            if doc_of(fa) == doc_of(fb) {
                                continue;
                            }
                            rels.push(Relationship {
                                id: new_id(),
                                rel_type: rel_type::CONTRADICTS.into(),
                                from_type: "fact".into(),
                                from_id: fa.id.clone(),
                                to_type: "fact".into(),
                                to_id: fb.id.clone(),
                                detail: Some(format!(
                                    "Different documented {label} values within the same {window}-day window."
                                )),
                                status: status::UNRESOLVED.into(),
                            });
                        }
                    }
                }
            }
        }
    }
    (conflicts, rels)
}

pub fn detect_status_conflicts(
    facts: &[Fact],
    doc_of: &dyn Fn(&Fact) -> Option<String>,
) -> (Vec<Conflict>, Vec<Relationship>) {
    let mut conflicts = Vec::new();
    let mut rels = Vec::new();

    let mut by_concept: HashMap<String, Vec<Fact>> = HashMap::new();
    for f in facts {
        if f.value_key.is_some() && f.obs_date.is_some() {
            by_concept.entry(f.concept.clone()).or_default().push(f.clone());
        }
    }
    let mut concepts: Vec<String> = by_concept.keys().cloned().collect();
    concepts.sort();

    for concept in concepts {
        let group = &by_concept[&concept];
        let window = packs::conflict_window_days(&concept);
        let label = packs::concept_label(&concept);

        for episode in episode_clusters(group, window) {
            if episode.len() < 2 {
                continue;
            }
            let mut groups: HashMap<String, Vec<Fact>> = HashMap::new();
            for f in &episode {
                groups.entry(f.value_key.clone().unwrap()).or_default().push(f.clone());
            }
            if groups.len() < 2 {
                continue;
            }
            let mut keys: Vec<String> = groups.keys().cloned().collect();
            keys.sort_by(|a, b| {
                (groups[a][0].obs_date.clone().unwrap_or_default(), &groups[a][0].id)
                    .cmp(&(groups[b][0].obs_date.clone().unwrap_or_default(), &groups[b][0].id))
            });
            for i in 0..keys.len() {
                for j in (i + 1)..keys.len() {
                    let (ka, kb) = (&keys[i], &keys[j]);
                    let ga = &groups[ka];
                    let gb = &groups[kb];
                    let docs_a: HashSet<Option<String>> = ga.iter().map(doc_of).collect();
                    let docs_b: HashSet<Option<String>> = gb.iter().map(doc_of).collect();
                    let all_docs = docs_a.union(&docs_b).count();
                    if all_docs < 2 {
                        continue;
                    }
                    let rep_a = ga.iter().min_by(rep_ord).unwrap();
                    let rep_b = gb.iter().min_by(rep_ord).unwrap();
                    let la = packs::status_value_label(&concept, ka);
                    let lb = packs::status_value_label(&concept, kb);
                    let extra = if ga.len() > 1 || gb.len() > 1 {
                        format!(
                            " The findings are restated across {all_docs} documents ({} record(s) on one side, {} on the other); all are listed as sources.",
                            ga.len(), gb.len()
                        )
                    } else {
                        String::new()
                    };
                    conflicts.push(Conflict {
                        id: new_id(),
                        display_id: String::new(),
                        conflict_type: conflict_type::STATUS_STATEMENT.into(),
                        concept: concept.clone(),
                        concept_label: label.clone(),
                        left_type: "fact".into(),
                        left_id: rep_a.id.clone(),
                        right_type: "fact".into(),
                        right_id: rep_b.id.clone(),
                        left_summary: la.clone(),
                        right_summary: lb.clone(),
                        left_member_ids: ga.iter().map(|f| f.id.clone()).collect(),
                        right_member_ids: gb.iter().map(|f| f.id.clone()).collect(),
                        basis: format!(
                            "Source documents state mutually exclusive findings for {label} within the same episode (records within {window} day(s) of each other): \u{201c}{la}\u{201d} versus \u{201c}{lb}\u{201d}. Both are documented statements; Neither value is selected.{extra}"
                        ),
                        delta: None,
                        status: status::UNRESOLVED.into(),
                    });
                    for fa in ga {
                        for fb in gb {
                            if doc_of(fa) == doc_of(fb) {
                                continue;
                            }
                            rels.push(Relationship {
                                id: new_id(),
                                rel_type: rel_type::CONTRADICTS.into(),
                                from_type: "fact".into(),
                                from_id: fa.id.clone(),
                                to_type: "fact".into(),
                                to_id: fb.id.clone(),
                                detail: Some(format!(
                                    "Mutually exclusive documented {label} findings within the same {window}-day window."
                                )),
                                status: status::UNRESOLVED.into(),
                            });
                        }
                    }
                }
            }
        }
    }
    (conflicts, rels)
}

/// A drug documented ACTIVE in one record and absent from a later
/// reconciliation list. Absence is a DOCUMENTATION conflict — never an
/// inference that the drug was stopped or that the list is wrong.
pub fn detect_medication_conflicts(
    medications: &[Medication],
    doc_of: &dyn Fn(&Medication) -> Option<String>,
    doc_type_of: &dyn Fn(&Medication) -> Option<String>,
    doc_label: &dyn Fn(&str) -> String,
) -> (Vec<Conflict>, Vec<Relationship>, Vec<Medication>) {
    let mut conflicts = Vec::new();
    let mut rels = Vec::new();
    let mut implicated = Vec::new();

    let recon: Vec<&Medication> = medications
        .iter()
        .filter(|m| doc_type_of(m).as_deref() == Some("MEDICATION_LIST"))
        .collect();
    if recon.is_empty() {
        return (conflicts, rels, implicated);
    }
    let mut lists_by_doc: HashMap<String, Vec<&Medication>> = HashMap::new();
    for m in &recon {
        if let Some(did) = doc_of(m) {
            lists_by_doc.entry(did).or_default().push(m);
        }
    }
    let mut doc_ids: Vec<String> = lists_by_doc.keys().cloned().collect();
    doc_ids.sort();

    for doc_id in doc_ids {
        let listed = &lists_by_doc[&doc_id];
        let list_date = listed.iter().find_map(|m| m.record_date.clone());
        let present: HashSet<String> = listed.iter().map(|m| m.drug_norm.clone()).collect();
        let ld = list_date.as_deref().and_then(epoch_from_iso);

        let mut by_drug: HashMap<String, Vec<Medication>> = HashMap::new();
        for m in medications {
            if doc_of(m).as_deref() == Some(doc_id.as_str()) {
                continue;
            }
            if m.status != "ACTIVE" || present.contains(&m.drug_norm) {
                continue;
            }
            if m.stop_date.is_some() {
                continue;
            }
            let md = m.record_date.as_deref().and_then(epoch_from_iso);
            if let (Some(md), Some(ld)) = (md, ld) {
                if md > ld {
                    continue;
                }
            }
            by_drug.entry(m.drug_norm.clone()).or_default().push(m.clone());
        }

        let mut drugs: Vec<String> = by_drug.keys().cloned().collect();
        drugs.sort();
        for drug_norm in drugs {
            let mut group = by_drug.remove(&drug_norm).unwrap();
            group.sort_by(|a, b| {
                (a.record_date.clone().unwrap_or_default(), &a.id)
                    .cmp(&(b.record_date.clone().unwrap_or_default(), &b.id))
            });
            let rep = group.last().unwrap().clone();
            implicated.push(rep.clone());
            let dates = group
                .iter()
                .map(|m| m.record_date.clone().unwrap_or_else(|| "undated".into()))
                .collect::<Vec<_>>()
                .join(", ");
            conflicts.push(Conflict {
                id: new_id(),
                display_id: String::new(),
                conflict_type: conflict_type::MEDICATION_DOCUMENTATION.into(),
                concept: rep.drug_norm.clone(),
                concept_label: rep.drug_name.clone(),
                left_type: "medication".into(),
                left_id: rep.id.clone(),
                right_type: "document".into(),
                right_id: doc_id.clone(),
                left_summary: format!("{} — documented active ({dates})", rep.drug_name),
                right_summary: format!("{} — not listed on {}", rep.drug_name, doc_label(&doc_id)),
                left_member_ids: group.iter().map(|m| m.id.clone()).collect(),
                right_member_ids: vec![doc_id.clone()],
                basis: "A medication documented as active in one or more records does not appear on a later medication list, and no stop date is documented. No inference is made that it was stopped, or that the list is wrong.".into(),
                delta: None,
                status: status::UNRESOLVED.into(),
            });
            for m in &group {
                rels.push(Relationship {
                    id: new_id(),
                    rel_type: rel_type::CONTRADICTS.into(),
                    from_type: "medication".into(),
                    from_id: m.id.clone(),
                    to_type: "document".into(),
                    to_id: doc_id.clone(),
                    detail: Some("Active medication absent from a later medication list.".into()),
                    status: status::UNRESOLVED.into(),
                });
            }
        }
    }
    (conflicts, rels, implicated)
}

fn rep_ord(a: &&Fact, b: &&Fact) -> std::cmp::Ordering {
    (a.obs_date.clone().unwrap_or_default(), &a.id)
        .cmp(&(b.obs_date.clone().unwrap_or_default(), &b.id))
}
