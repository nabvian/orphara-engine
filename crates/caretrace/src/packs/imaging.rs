//! Imaging Pack — imaging-reported findings for the red-cell enzyme disorders.
//! For this subfamily the load-bearing imaging finding is pigment gallstones
//! (chronic haemolysis), typically on abdominal ultrasound. Splenomegaly stays
//! in the clinical pack (it is an examination finding). Keys match the DSD.

use super::base::{Pack, StatusConceptDef as SC, StatusValue as SV};

const M: &str = "IMAGING";

const V_GALLSTONES: &[SV] = &[SV { key: "present", label: "Present", positive: true,
    phrases: &["pigment gallstones", "gallstones", "cholelithiasis",
               "gallstones on ultrasound", "cholelithiasis on ultrasound"] }];

const STATUS_CONCEPTS: &[SC] = &[
    SC { key: "pigment_gallstones", label: "Pigment gallstones", values: V_GALLSTONES, modality: M },
];

const DOCUMENT_CUES: &[(&str, &[&str])] = &[(
    "IMAGING_REPORT",
    &["ultrasound", "abdominal ultrasound", "usg", "ultrasonography", "imaging report"],
)];
const FILENAME_CUES: &[(&str, &[&str])] = &[("IMAGING_REPORT", &["ultrasound", "usg", "imaging"])];

pub const PACK: Pack = Pack {
    key: "imaging",
    label: "Imaging Pack",
    modality: M,
    concepts: &[],
    claim_types: &[],
    medication_synonyms: &[],
    unsupported_concept_labels: &[],
    followup_patterns: &[],
    document_reference_patterns: &[],
    conflict_window_days: &[],
    claim_lookback_days: &[],
    followup_expectations: &[],
    status_concepts: STATUS_CONCEPTS,
    anchored_phrases: &[],
    document_cues: DOCUMENT_CUES,
    filename_cues: FILENAME_CUES,
};
