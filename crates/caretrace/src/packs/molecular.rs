//! Molecular Pack — genetic / enzyme confirmatory calls for the red-cell enzyme
//! disorders, as reported by molecular diagnostics. Keys match the DSD's
//! confirmatory concepts so the adapter resolves them by identity.
//!
//! Sourced from each dossier's confirmatory section: PK requires biallelic
//! pathogenic PKLR variants; recessive methaemoglobinaemia requires decreased
//! cytochrome b5 reductase with biallelic CYB5R3; a pathogenic G6PD variant
//! confirms G6PD deficiency (notably in female heterozygotes, F0016).

use super::base::{Pack, StatusConceptDef as SC, StatusValue as SV};

const M: &str = "MOLECULAR";

const V_PKLR: &[SV] = &[SV { key: "present", label: "Present", positive: true,
    phrases: &["biallelic pklr", "biallelic pathogenic pklr", "pklr mutations",
               "two pklr variants", "pklr variants", "homozygous pklr",
               "compound heterozygous pklr"] }];
const V_CYB5R: &[SV] = &[SV { key: "present", label: "Present", positive: true,
    phrases: &["decreased cytochrome b5 reductase", "low cytochrome b5 reductase",
               "reduced cytochrome b5 reductase", "cytochrome b5 reductase deficiency",
               "biallelic cyb5r3", "cyb5r3 variants", "cyb5r3"] }];
const V_G6PDVAR: &[SV] = &[SV { key: "present", label: "Present", positive: true,
    phrases: &["pathogenic g6pd variant", "g6pd pathogenic variant", "g6pd mutation",
               "hemizygous g6pd", "g6pd class i variant"] }];

const STATUS_CONCEPTS: &[SC] = &[
    SC { key: "pklr_biallelic", label: "Biallelic pathogenic PKLR variants", values: V_PKLR, modality: M },
    SC { key: "cyb5r_low", label: "Decreased cytochrome b5 reductase + biallelic CYB5R3", values: V_CYB5R, modality: M },
    SC { key: "g6pd_variant", label: "Pathogenic G6PD variant (molecular)", values: V_G6PDVAR, modality: M },
];

const DOCUMENT_CUES: &[(&str, &[&str])] = &[(
    "MOLECULAR_REPORT",
    &["molecular", "genetic", "sequencing", "next-generation sequencing", "variant analysis"],
)];
const FILENAME_CUES: &[(&str, &[&str])] = &[("MOLECULAR_REPORT", &["molecular", "genetic", "ngs", "sequencing"])];

pub const PACK: Pack = Pack {
    key: "molecular",
    label: "Molecular Pack",
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
