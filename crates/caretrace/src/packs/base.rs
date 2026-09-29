//! Pack contract.
//!
//! A pack is *data*. The engine reads a pack; it never hard-codes one. Adding a
//! modality (imaging, cardiology, molecular, …) means adding a pack and
//! registering it in `packs::registry`, not editing the extractor.

/// A measurable concept: what it is called, its unit, and what counts as a
/// material difference between two documented values of it.
#[derive(Clone, Debug)]
pub struct ConceptDef {
    pub key: &'static str,
    pub label: &'static str,
    pub canonical_unit: &'static str,
    /// Absolute difference at/above which two same-episode values are material.
    pub material_abs: f64,
    /// Relative-difference threshold (fraction).
    pub material_rel: f64,
    pub synonyms: &'static [&'static str],
    pub unit_aliases: &'static [(&'static str, &'static str)],
    pub modality: &'static str,
}

/// `(concept, comparator, threshold)` where comparator is "lt"/"lte"/"gt"/"gte".
pub type Direction = (&'static str, &'static str, f64);

#[derive(Clone, Debug)]
pub struct ClaimTypeDef {
    pub key: &'static str,
    pub label: &'static str,
    pub patterns: &'static [&'static str],
    pub requires_any: &'static [&'static str],
    pub requires_labels: &'static [&'static str],
    pub supportive_direction: &'static [Direction],
    /// For claims whose evidence is a qualitative finding: whether the claim is
    /// supported by a finding being PRESENT (`Some(true)`) or ABSENT
    /// (`Some(false)`). `None` means this claim takes no status evidence.
    pub supportive_status_polarity: Option<bool>,
    pub modality: &'static str,
}

/// One mutually-exclusive value a qualitative finding can take.
#[derive(Clone, Debug)]
pub struct StatusValue {
    pub key: &'static str,
    pub label: &'static str,
    pub phrases: &'static [&'static str],
    /// Whether this value asserts the finding is present.
    pub positive: bool,
}

#[derive(Clone, Debug)]
pub struct StatusConceptDef {
    pub key: &'static str,
    pub label: &'static str,
    pub values: &'static [StatusValue],
    pub modality: &'static str,
}

/// One domain vocabulary. Packs compose; none is privileged.
#[derive(Clone, Debug, Default)]
pub struct Pack {
    pub key: &'static str,
    pub label: &'static str,
    pub modality: &'static str,
    pub concepts: &'static [ConceptDef],
    pub claim_types: &'static [ClaimTypeDef],
    pub medication_synonyms: &'static [(&'static str, &'static str)],
    pub unsupported_concept_labels: &'static [(&'static str, &'static str)],
    pub followup_patterns: &'static [(&'static str, &'static str)],
    /// `(pattern, label, doc_type_hint)`.
    pub document_reference_patterns: &'static [(&'static str, &'static str, &'static str)],
    pub conflict_window_days: &'static [(&'static str, i64)],
    pub claim_lookback_days: &'static [(&'static str, i64)],
    /// `(label, (interval_days, expected_doc_type or ""))`.
    pub followup_expectations: &'static [(&'static str, (i64, &'static str))],
    pub status_concepts: &'static [StatusConceptDef],
    pub anchored_phrases: &'static [&'static str],
    pub document_cues: &'static [(&'static str, &'static [&'static str])],
    pub filename_cues: &'static [(&'static str, &'static [&'static str])],
}
