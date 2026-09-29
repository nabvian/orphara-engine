//! Maps extractor concept keys to DSD `NodeId`s and builds the `PatientGraph`
//! the core consumes.
//!
//! - Mapping is identity by default plus an explicit alias table; numeric
//!   biomarkers resolve through the `(analyte, direction)` table. No fuzzy match.
//! - A concept that is `unmapped:*` or absent from the DSD registry is
//!   quarantined with a reason, never assigned a NodeId.
//! - Only documented concepts become observations; nothing is turned into a
//!   manufactured `Absent`. Provenance and dates are carried through.

use std::collections::HashMap;

use caretrace::date::epoch_from_iso;
use caretrace::pipeline::CaseResult;
use core_types::{
    EvidenceState, Measurement, NodeId, Observation, PatientGraph, Provenance, Temporal,
};

/// The default biomarker interpretation map: `(analyte, direction) → DSD key`.
/// These are DEFINITIONAL, not thresholds — "a low G6PD enzyme activity" *is*
/// the `g6pd_assay_low` concept. The direction itself comes from each report's
/// own printed reference range or flag (see `caretrace`), never a guessed cutoff.
const DEFAULT_BIOMARKERS: &[(&str, &str, &str)] = &[
    ("g6pd_activity", "LOW", "g6pd_assay_low"),
    ("g6pd_activity", "NORMAL", "g6pd_assay_normal"),
    ("pk_activity", "LOW", "pk_assay_low"),
    ("methaemoglobin", "HIGH", "methb_level"),
    // Haemolysis biomarkers: a high reticulocyte count is reticulocytosis; a
    // high (unconjugated) bilirubin is the biochemical face of jaundice.
    ("reticulocyte_percentage", "HIGH", "reticulocytosis"),
    ("bilirubin", "HIGH", "jaundice"),
];

/// Extractor-concept → DSD-registry-key mapping. Identity for qualitative
/// findings unless an alias says otherwise; numeric biomarkers resolve through
/// the `(analyte, direction)` table. `unmapped:*` extractor concepts are never
/// mapped.
#[derive(Clone, Debug, Default)]
pub struct ConceptMap {
    aliases: HashMap<String, String>,
    /// `(analyte, direction)` → DSD key.
    biomarkers: HashMap<(String, String), String>,
}

impl ConceptMap {
    /// Identity mapping for findings + the default biomarker table.
    pub fn identity() -> Self {
        let mut m = ConceptMap::default();
        m.load_default_biomarkers();
        m
    }

    /// Build from `(extractor_concept, dsd_key)` alias pairs (identity still
    /// applies to anything not listed; the default biomarker table is included).
    pub fn from_aliases(pairs: &[(&str, &str)]) -> Self {
        let mut m = ConceptMap::identity();
        for (from, to) in pairs {
            m.aliases.insert((*from).to_string(), (*to).to_string());
        }
        m
    }

    fn load_default_biomarkers(&mut self) {
        for (a, d, k) in DEFAULT_BIOMARKERS {
            self.biomarkers.insert(((*a).to_string(), (*d).to_string()), (*k).to_string());
        }
    }

    pub fn add_alias(&mut self, from: &str, to: &str) {
        self.aliases.insert(from.to_string(), to.to_string());
    }

    /// The DSD key a numeric biomarker resolves to, given its analyte and the
    /// report-stated direction — or `None` when the record does not establish a
    /// diagnostically meaningful state.
    pub fn biomarker_target(&self, analyte: &str, direction: &str) -> Option<String> {
        self.biomarkers.get(&(analyte.to_string(), direction.to_string())).cloned()
    }

    /// The DSD registry key an extractor concept resolves to, or `None` for an
    /// unrecognised (`unmapped:*`) analyte the adapter must not name.
    pub fn target_key(&self, concept: &str) -> Option<String> {
        if concept.starts_with("unmapped:") {
            return None;
        }
        Some(self.aliases.get(concept).cloned().unwrap_or_else(|| concept.to_string()))
    }
}

/// One extractor concept successfully resolved to a registry node.
#[derive(Clone, Debug)]
pub struct Mapping {
    pub extractor_concept: String,
    pub dsd_key: String,
    pub node: NodeId,
    pub state: EvidenceState,
}

/// One extractor concept the adapter refused to map, with why.
#[derive(Clone, Debug)]
pub struct Quarantined {
    pub extractor_concept: String,
    pub reason: String,
}

/// The adapter's output: the graph plus a full account of what mapped and what
/// was quarantined, so the seam is auditable.
#[derive(Debug, Default)]
pub struct AdapterResult {
    pub graph: PatientGraph,
    pub mapped: Vec<Mapping>,
    pub quarantined: Vec<Quarantined>,
}

/// Turn a completed extractor case into a `PatientGraph` for the Blind Core,
/// resolving concepts through `keys` (the DSD's key→NodeId map from
/// `dsdc::source::compile_source_with_keys`) and `map`.
pub fn adapt(case: &CaseResult, keys: &HashMap<String, NodeId>, map: &ConceptMap) -> AdapterResult {
    let mut result = AdapterResult::default();

    let src_by_id: HashMap<&str, &caretrace::models::Source> =
        case.sources.iter().map(|s| (s.id.as_str(), s)).collect();
    let doc_by_id: HashMap<&str, &caretrace::models::Document> =
        case.documents.iter().map(|d| (d.id.as_str(), d)).collect();

    let provenance = |source_id: &str| -> Option<Provenance> {
        let s = src_by_id.get(source_id)?;
        let document = doc_by_id
            .get(s.document_id.as_str())
            .map(|d| d.filename.clone())
            .unwrap_or_else(|| s.document_id.clone());
        Some(Provenance {
            document,
            locator: format!("p{}", s.page_number),
            exact_text: s.source_text.clone(),
        })
    };

    let temporal = |iso: &Option<String>| Temporal {
        day: iso.as_deref().and_then(epoch_from_iso),
        ..Temporal::default()
    };

    // --- facts (numeric biomarkers + qualitative findings) ---
    for f in &case.facts {
        // A numeric biomarker resolves through the (analyte, report-stated
        // direction) table — e.g. a low G6PD enzyme activity becomes the
        // `g6pd_assay_low` concept. A finding resolves by identity/alias.
        let biomarker_key = match (f.value_num, f.interpretation.as_deref()) {
            (Some(_), Some(dir)) => map.biomarker_target(&f.concept, dir),
            _ => None,
        };
        let (node, dsd_key) = if let Some(dkey) = biomarker_key {
            match keys.get(&dkey) {
                Some(n) => (*n, dkey),
                None => {
                    result.quarantined.push(Quarantined {
                        extractor_concept: f.concept.clone(),
                        reason: format!("biomarker maps to '{dkey}', absent from the DSD registry"),
                    });
                    continue;
                }
            }
        } else {
            match resolve(&f.concept, keys, map, &mut result) {
                Some(x) => x,
                None => continue,
            }
        };
        // State: an observed biomarker or numeric analyte is Present; a
        // qualitative finding takes its pack-declared polarity. Missing /
        // NotAssessed are never manufactured here.
        let state = if f.value_num.is_some() {
            EvidenceState::Present
        } else if let Some(vk) = &f.value_key {
            match caretrace::packs::status_value_positive(&f.concept, vk) {
                Some(false) => EvidenceState::Absent,
                _ => EvidenceState::Present,
            }
        } else {
            EvidenceState::Present
        };
        let measurement = if f.value_num.is_some() || f.value_text.is_some() {
            Some(Measurement {
                value: f.value_num,
                unit: f.unit.clone(),
                interpretation: f.value_text.clone().or_else(|| f.interpretation.clone()),
            })
        } else {
            None
        };
        result.graph.push(Observation {
            concept: node,
            state,
            temporal: temporal(&f.obs_date),
            measurement,
            provenance: provenance(&f.source_id),
        });
        result.mapped.push(Mapping {
            extractor_concept: f.concept.clone(),
            dsd_key,
            node,
            state,
        });
    }

    // --- medications map to their drug/trigger concept, documented Present ---
    // (e.g. an oxidant drug in the record resolves to a DSD trigger node.)
    for m in &case.medications {
        let (node, dsd_key) = match resolve(&m.drug_norm, keys, map, &mut result) {
            Some(x) => x,
            None => continue,
        };
        let state = EvidenceState::Present;
        result.graph.push(Observation {
            concept: node,
            state,
            temporal: temporal(&m.record_date),
            measurement: None,
            provenance: provenance(&m.source_id),
        });
        result.mapped.push(Mapping {
            extractor_concept: m.drug_norm.clone(),
            dsd_key,
            node,
            state,
        });
    }

    result
}

/// Resolve one extractor concept to `(NodeId, dsd_key)`, or record a quarantine
/// and return `None`.
fn resolve(
    concept: &str,
    keys: &HashMap<String, NodeId>,
    map: &ConceptMap,
    result: &mut AdapterResult,
) -> Option<(NodeId, String)> {
    match map.target_key(concept) {
        None => {
            result.quarantined.push(Quarantined {
                extractor_concept: concept.to_string(),
                reason: "unrecognised analyte (the extractor could not map it to a known concept)".into(),
            });
            None
        }
        Some(dkey) => match keys.get(&dkey) {
            Some(node) => Some((*node, dkey)),
            None => {
                result.quarantined.push(Quarantined {
                    extractor_concept: concept.to_string(),
                    reason: format!("no concept '{dkey}' in the DSD registry (nothing invented)"),
                });
                None
            }
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fact(concept: &str, value: Option<f64>) -> caretrace::models::Fact {
        caretrace::models::Fact {
            id: caretrace::models::new_id(),
            display_id: "F-001".into(),
            concept: concept.into(),
            surface_form: concept.into(),
            source_id: String::new(),
            value_num: value,
            value_text: None,
            value_key: if value.is_none() { Some("present".into()) } else { None },
            unit: None,
            obs_date: Some("2026-05-04".into()),
            date_precision: "day".into(),
            confidence: 0.9,
            interpretation: None,
        }
    }

    #[test]
    fn maps_identity_alias_and_quarantines_the_rest() {
        let mut keys = HashMap::new();
        keys.insert("jaundice".to_string(), NodeId(10));
        keys.insert("hemoglobinuria".to_string(), NodeId(11));

        let map = ConceptMap::from_aliases(&[("dark_urine", "hemoglobinuria")]);
        let mut case = CaseResult::default();
        case.facts.push(fact("jaundice", None)); // identity -> 10
        case.facts.push(fact("dark_urine", None)); // alias -> hemoglobinuria -> 11
        case.facts.push(fact("unmapped:reticulocytes", Some(4.2))); // unmapped -> quarantine
        case.facts.push(fact("cyanosis", None)); // not in this registry -> quarantine

        let out = adapt(&case, &keys, &map);
        assert_eq!(out.graph.observations.len(), 2);
        assert!(out.mapped.iter().any(|m| m.node == NodeId(10)));
        assert!(out.mapped.iter().any(|m| m.node == NodeId(11) && m.dsd_key == "hemoglobinuria"));
        assert_eq!(out.quarantined.len(), 2);
        assert!(out.quarantined.iter().any(|q| q.extractor_concept == "cyanosis"));
    }
}
