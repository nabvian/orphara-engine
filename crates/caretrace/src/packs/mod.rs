//! Pack registry — composes the enabled domain packs into one vocabulary
//!.
//!
//! The extractor reads a single flat vocabulary; this module turns N packs into
//! that view and is the only place pack precedence is decided. A concept key
//! defined by two packs is a packaging error (panics at init). A synonym
//! claimed by two packs is ambiguous: it is dropped from the synonym index, so
//! the label resolves to UNRECOGNISED and is retained as an unmapped
//! measurement.

pub mod base;
pub mod clinical;
pub mod imaging;
pub mod laboratory;
pub mod molecular;

use std::collections::HashMap;

use once_cell::sync::Lazy;

use base::{ClaimTypeDef, ConceptDef, Pack, StatusConceptDef, StatusValue};

/// The enabled packs, in precedence order. Add a modality here (imaging,
/// molecular, …) — never in the extractor.
pub fn packs() -> &'static [Pack] {
    static P: Lazy<Vec<Pack>> = Lazy::new(|| {
        vec![
            laboratory::PACK.clone(),
            clinical::PACK.clone(),
            imaging::PACK.clone(),
            molecular::PACK.clone(),
        ]
    });
    &P
}

/// concept key -> definition.
pub fn concepts() -> &'static HashMap<&'static str, ConceptDef> {
    static C: Lazy<HashMap<&'static str, ConceptDef>> = Lazy::new(|| {
        let mut out = HashMap::new();
        for p in packs() {
            for c in p.concepts {
                if out.insert(c.key, c.clone()).is_some() {
                    panic!("pack collision: concept key {:?} defined twice", c.key);
                }
            }
        }
        out
    });
    &C
}

/// surface synonym (lowercased) -> concept key; ambiguous synonyms are dropped.
pub fn synonym_index() -> &'static HashMap<String, String> {
    static S: Lazy<HashMap<String, String>> = Lazy::new(|| {
        let mut claimed: HashMap<String, Vec<&'static str>> = HashMap::new();
        for cd in concepts().values() {
            for syn in cd.synonyms {
                claimed
                    .entry(syn.trim().to_lowercase())
                    .or_default()
                    .push(cd.key);
            }
        }
        claimed
            .into_iter()
            .filter(|(_, ks)| ks.len() == 1)
            .map(|(s, ks)| (s, ks[0].to_string()))
            .collect()
    });
    &S
}

/// Synonyms longest-first, for containment matching.
pub fn synonyms_by_length() -> &'static Vec<(String, String)> {
    static L: Lazy<Vec<(String, String)>> = Lazy::new(|| {
        let mut v: Vec<(String, String)> = synonym_index()
            .iter()
            .map(|(k, val)| (k.clone(), val.clone()))
            .collect();
        v.sort_by_key(|(k, _)| std::cmp::Reverse(k.len()));
        v
    });
    &L
}

/// concept -> {alias(lower) -> canonical unit}.
pub fn unit_alias_index() -> &'static HashMap<&'static str, HashMap<String, String>> {
    static U: Lazy<HashMap<&'static str, HashMap<String, String>>> = Lazy::new(|| {
        concepts()
            .iter()
            .map(|(k, cd)| {
                let m = cd
                    .unit_aliases
                    .iter()
                    .map(|(a, c)| (a.to_lowercase(), c.to_string()))
                    .collect();
                (*k, m)
            })
            .collect()
    });
    &U
}

pub fn claim_types() -> &'static Vec<ClaimTypeDef> {
    static C: Lazy<Vec<ClaimTypeDef>> = Lazy::new(|| {
        let mut out = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for p in packs() {
            for c in p.claim_types {
                if !seen.insert(c.key) {
                    panic!("pack collision: claim key {:?} defined twice", c.key);
                }
                out.push(c.clone());
            }
        }
        out
    });
    &C
}

pub fn claim_type_index() -> &'static HashMap<&'static str, ClaimTypeDef> {
    static I: Lazy<HashMap<&'static str, ClaimTypeDef>> =
        Lazy::new(|| claim_types().iter().map(|c| (c.key, c.clone())).collect());
    &I
}

pub fn medication_synonyms() -> &'static HashMap<String, String> {
    static M: Lazy<HashMap<String, String>> = Lazy::new(|| {
        let mut out = HashMap::new();
        for p in packs() {
            for (a, b) in p.medication_synonyms {
                out.insert(a.to_string(), b.to_string());
            }
        }
        out
    });
    &M
}

fn unsupported_labels() -> &'static HashMap<&'static str, &'static str> {
    static U: Lazy<HashMap<&'static str, &'static str>> = Lazy::new(|| {
        let mut out = HashMap::new();
        for p in packs() {
            for (k, v) in p.unsupported_concept_labels {
                out.insert(*k, *v);
            }
        }
        out
    });
    &U
}

// --- status concepts (qualitative findings) --------------------------------

fn status_concepts() -> &'static HashMap<&'static str, StatusConceptDef> {
    static S: Lazy<HashMap<&'static str, StatusConceptDef>> = Lazy::new(|| {
        let mut out = HashMap::new();
        for p in packs() {
            for sc in p.status_concepts {
                if out.insert(sc.key, sc.clone()).is_some() {
                    panic!("pack collision: status concept {:?} defined twice", sc.key);
                }
            }
        }
        out
    });
    &S
}

/// `(phrase_lower, concept_key, value_key, phrase_as_written)`, longest first.
pub fn status_phrases() -> &'static Vec<(String, &'static str, &'static str, &'static str)> {
    static P: Lazy<Vec<(String, &'static str, &'static str, &'static str)>> = Lazy::new(|| {
        let mut v = Vec::new();
        for (ck, sc) in status_concepts() {
            for sv in sc.values {
                for ph in sv.phrases {
                    v.push((ph.to_lowercase(), *ck, sv.key, *ph));
                }
            }
        }
        v.sort_by_key(|t| std::cmp::Reverse(t.0.len()));
        v
    });
    &P
}

fn status_value_index() -> &'static HashMap<(String, String), StatusValue> {
    static I: Lazy<HashMap<(String, String), StatusValue>> = Lazy::new(|| {
        let mut out = HashMap::new();
        for (ck, sc) in status_concepts() {
            for sv in sc.values {
                out.insert((ck.to_string(), sv.key.to_string()), sv.clone());
            }
        }
        out
    });
    &I
}

pub fn status_value_label(concept: &str, value_key: &str) -> String {
    status_value_index()
        .get(&(concept.to_string(), value_key.to_string()))
        .map(|sv| sv.label.to_string())
        .unwrap_or_else(|| {
            let s = value_key.replace('_', " ");
            let mut c = s.chars();
            c.next()
                .map(|f| f.to_uppercase().collect::<String>() + c.as_str())
                .unwrap_or(s)
        })
}

pub fn status_value_positive(concept: &str, value_key: &str) -> Option<bool> {
    status_value_index()
        .get(&(concept.to_string(), value_key.to_string()))
        .map(|sv| sv.positive)
}

pub fn anchored_phrases() -> &'static std::collections::HashSet<String> {
    static A: Lazy<std::collections::HashSet<String>> = Lazy::new(|| {
        packs()
            .iter()
            .flat_map(|p| p.anchored_phrases.iter().map(|ph| ph.to_lowercase()))
            .collect()
    });
    &A
}

// --- windows / lookbacks / follow-ups --------------------------------------

pub const DEFAULT_CONFLICT_WINDOW_DAYS: i64 = 2;
pub const DEFAULT_CLAIM_LOOKBACK_DAYS: i64 = 180;

pub fn conflict_window_days(concept: &str) -> i64 {
    static W: Lazy<HashMap<&'static str, i64>> = Lazy::new(|| {
        let mut out = HashMap::new();
        for p in packs() {
            for (k, v) in p.conflict_window_days {
                out.insert(*k, *v);
            }
        }
        out
    });
    W.get(concept).copied().unwrap_or(DEFAULT_CONFLICT_WINDOW_DAYS)
}

pub fn claim_lookback_days(claim_type: &str) -> i64 {
    static L: Lazy<HashMap<&'static str, i64>> = Lazy::new(|| {
        let mut out = HashMap::new();
        for p in packs() {
            for (k, v) in p.claim_lookback_days {
                out.insert(*k, *v);
            }
        }
        out
    });
    L.get(claim_type).copied().unwrap_or(DEFAULT_CLAIM_LOOKBACK_DAYS)
}

/// label -> (interval_days, expected_doc_type or "").
pub fn followup_expectations() -> &'static HashMap<&'static str, (i64, &'static str)> {
    static F: Lazy<HashMap<&'static str, (i64, &'static str)>> = Lazy::new(|| {
        let mut out = HashMap::new();
        for p in packs() {
            for (k, v) in p.followup_expectations {
                out.insert(*k, *v);
            }
        }
        out
    });
    &F
}

pub fn followup_patterns() -> &'static Vec<(&'static str, &'static str)> {
    static F: Lazy<Vec<(&'static str, &'static str)>> = Lazy::new(|| {
        let mut out = Vec::new();
        for p in packs() {
            out.extend(p.followup_patterns.iter().copied());
        }
        out
    });
    &F
}

/// `(full_pattern, label, doc_type_hint)` — the citing-verb prefix is prepended.
pub fn document_reference_patterns() -> &'static Vec<(String, &'static str, &'static str)> {
    static D: Lazy<Vec<(String, &'static str, &'static str)>> = Lazy::new(|| {
        let mut out = Vec::new();
        for p in packs() {
            for (suffix, label, hint) in p.document_reference_patterns {
                out.push((format!("{}{}", laboratory::CITE_PREFIX, suffix), *label, *hint));
            }
        }
        out
    });
    &D
}

// --- classification cues (merged, additive) --------------------------------

pub fn document_cues() -> &'static HashMap<&'static str, Vec<&'static str>> {
    static D: Lazy<HashMap<&'static str, Vec<&'static str>>> =
        Lazy::new(|| merge_cues(|p| p.document_cues));
    &D
}

pub fn filename_cues() -> &'static HashMap<&'static str, Vec<&'static str>> {
    static F: Lazy<HashMap<&'static str, Vec<&'static str>>> =
        Lazy::new(|| merge_cues(|p| p.filename_cues));
    &F
}

/// A pack's classification-cue table: `(doc_type, phrases)` rows.
type CueTable = &'static [(&'static str, &'static [&'static str])];

fn merge_cues(get: fn(&Pack) -> CueTable) -> HashMap<&'static str, Vec<&'static str>> {
    let mut out: HashMap<&'static str, Vec<&'static str>> = HashMap::new();
    for p in packs() {
        for (doc_type, phrases) in get(p) {
            let e = out.entry(*doc_type).or_default();
            for ph in *phrases {
                if !e.contains(ph) {
                    e.push(ph);
                }
            }
        }
    }
    out
}

// --- label helpers ----------------------------------------------------------

pub fn concept_label(key: &str) -> String {
    if let Some(cd) = concepts().get(key) {
        return cd.label.to_string();
    }
    if let Some(l) = unsupported_labels().get(key) {
        return l.to_string();
    }
    if let Some(sc) = status_concepts().get(key) {
        return sc.label.to_string();
    }
    // Title-case the key with underscores as spaces.
    key.split('_')
        .map(|w| {
            let mut c = w.chars();
            c.next()
                .map(|f| f.to_uppercase().collect::<String>() + c.as_str())
                .unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn material_difference(concept: &str, a: f64, b: f64) -> bool {
    let diff = (a - b).abs();
    match concepts().get(concept) {
        None => diff > 0.0,
        Some(cd) => {
            let base = a.abs().max(b.abs()).max(1e-9);
            diff >= cd.material_abs || (diff / base) >= cd.material_rel
        }
    }
}

/// Register every pack's non-UCUM unit spellings with the grammar. Idempotent;
/// call once at startup (the pipeline does).
pub fn register_unit_aliases() {
    for p in packs() {
        for cd in p.concepts {
            for (surface, ucum) in cd.unit_aliases {
                if crate::units::dimension(surface).is_none() {
                    crate::units::register_alias(surface, ucum);
                }
            }
        }
    }
}
