//! Domain model.
//!
//! Deliberately domain-independent at the core: a `Fact` is a normalized
//! concept, a value, a date and a `Source`. Medical meaning lives in the packs,
//! not here. Nothing in this module asserts truth — `status` fields carry
//! documentation states (DOCUMENTED / UNRESOLVED / MISSING), never clinical
//! judgements.

use std::sync::atomic::{AtomicU64, Ordering};

/// Monotonic id allocator. A counter keeps ids reproducible across runs; the
/// display ids (`F-001`, …) are the human-facing references.
static SEQ: AtomicU64 = AtomicU64::new(1);

pub fn new_id() -> String {
    format!("n{:06}", SEQ.fetch_add(1, Ordering::Relaxed))
}

/// Reset the id counter — used only by tests that assert on ids.
pub fn reset_ids() {
    SEQ.store(1, Ordering::Relaxed);
}

// --- document kinds ---------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DocType {
    LabReport,
    Prescription,
    Consultation,
    DischargeSummary,
    MedicationList,
    Referral,
    Followup,
    ImagingReport,
    PathologyReport,
    EcgReport,
    EegReport,
    MolecularReport,
    Other,
}

impl DocType {
    pub fn as_str(self) -> &'static str {
        match self {
            DocType::LabReport => "LAB_REPORT",
            DocType::Prescription => "PRESCRIPTION",
            DocType::Consultation => "CONSULTATION",
            DocType::DischargeSummary => "DISCHARGE_SUMMARY",
            DocType::MedicationList => "MEDICATION_LIST",
            DocType::Referral => "REFERRAL",
            DocType::Followup => "FOLLOWUP",
            DocType::ImagingReport => "IMAGING_REPORT",
            DocType::PathologyReport => "PATHOLOGY_REPORT",
            DocType::EcgReport => "ECG_REPORT",
            DocType::EegReport => "EEG_REPORT",
            DocType::MolecularReport => "MOLECULAR_REPORT",
            DocType::Other => "OTHER",
        }
    }
    pub fn from_code(s: &str) -> DocType {
        match s {
            "LAB_REPORT" => DocType::LabReport,
            "PRESCRIPTION" => DocType::Prescription,
            "CONSULTATION" => DocType::Consultation,
            "DISCHARGE_SUMMARY" => DocType::DischargeSummary,
            "MEDICATION_LIST" => DocType::MedicationList,
            "REFERRAL" => DocType::Referral,
            "FOLLOWUP" => DocType::Followup,
            "IMAGING_REPORT" => DocType::ImagingReport,
            "PATHOLOGY_REPORT" => DocType::PathologyReport,
            "ECG_REPORT" => DocType::EcgReport,
            "EEG_REPORT" => DocType::EegReport,
            "MOLECULAR_REPORT" => DocType::MolecularReport,
            _ => DocType::Other,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            DocType::LabReport => "Laboratory Report",
            DocType::Prescription => "Prescription",
            DocType::Consultation => "Consultation Note",
            DocType::DischargeSummary => "Discharge Summary",
            DocType::MedicationList => "Medication List",
            DocType::Referral => "Referral Letter",
            DocType::Followup => "Follow-up Note",
            DocType::ImagingReport => "Imaging Report",
            DocType::PathologyReport => "Pathology Report",
            DocType::EcgReport => "ECG Report",
            DocType::EegReport => "EEG Report",
            DocType::MolecularReport => "Molecular Diagnostics Report",
            DocType::Other => "Other Document",
        }
    }
}

/// How the raw text was obtained. Never guessed, always recorded.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ExtractionMethod {
    PdfTextLayer,
    PlainText,
    Ocr,
    None,
}

impl ExtractionMethod {
    pub fn as_str(self) -> &'static str {
        match self {
            ExtractionMethod::PdfTextLayer => "PDF_TEXT_LAYER",
            ExtractionMethod::PlainText => "PLAIN_TEXT",
            ExtractionMethod::Ocr => "OCR",
            ExtractionMethod::None => "NONE",
        }
    }
}

/// Documentation states (never clinical judgements).
pub mod status {
    pub const VERIFIED: &str = "VERIFIED";
    pub const DOCUMENTED: &str = "DOCUMENTED";
    pub const CHANGED: &str = "CHANGED";
    pub const CONFLICT: &str = "CONFLICT";
    pub const MISSING: &str = "MISSING";
    pub const UNRESOLVED: &str = "UNRESOLVED";
}

/// Claim evidence outcomes — exactly four.
pub mod claim_status {
    pub const SUPPORTING: &str = "SUPPORTING_EVIDENCE";
    pub const CONTRADICTING: &str = "CONTRADICTING_EVIDENCE";
    pub const NO_LOCATED: &str = "NO_LOCATED_EVIDENCE";
    pub const NOT_ASSESSED: &str = "NOT_ASSESSED";
}

pub mod rel_type {
    pub const SUPPORTS: &str = "SUPPORTS";
    pub const CONTRADICTS: &str = "CONTRADICTS";
    pub const DUPLICATES: &str = "DUPLICATES";
    pub const CHANGED_FROM: &str = "CHANGED_FROM";
    pub const CHANGED_TO: &str = "CHANGED_TO";
    pub const MISSING_SUPPORT: &str = "MISSING_SUPPORT";
}

pub mod gap_type {
    pub const MISSING_SUPPORTING_TEST: &str = "MISSING_SUPPORTING_TEST";
    pub const MISSING_FOLLOWUP: &str = "MISSING_FOLLOWUP";
    pub const MEDICATION_DOCUMENTATION_GAP: &str = "MEDICATION_DOCUMENTATION_GAP";
    pub const MISSING_SOURCE: &str = "MISSING_SOURCE";
}

pub mod conflict_type {
    pub const NUMERIC_OBSERVATION: &str = "NUMERIC_OBSERVATION";
    pub const MEDICATION_DOCUMENTATION: &str = "MEDICATION_DOCUMENTATION";
    pub const STATUS_STATEMENT: &str = "STATUS_STATEMENT";
}

// --- records ----------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct Document {
    pub id: String,
    pub filename: String,
    pub doc_type: DocType,
    pub doc_date: Option<String>,
    pub page_count: usize,
    pub byte_size: usize,
    pub sha256: String,
    pub stored_path: Option<String>,
    pub processing_status: String,
    pub processing_note: Option<String>,
    pub ordinal: usize,
}

#[derive(Clone, Debug)]
pub struct DocumentPage {
    pub id: String,
    pub document_id: String,
    pub page_number: usize,
    pub text: String,
}

/// Write-once record of the complete text as the parser saw it.
#[derive(Clone, Debug)]
pub struct RawExtraction {
    pub id: String,
    pub document_id: String,
    pub text: String,
    pub text_sha256: String,
    pub char_count: usize,
    /// `[start, end)` byte offsets per page into `text`.
    pub page_offsets: Vec<(usize, usize)>,
    pub method: ExtractionMethod,
    pub tool_name: Option<String>,
    pub tool_version: Option<String>,
    pub confidence: Option<f64>,
    pub has_text_layer: bool,
    pub note: Option<String>,
    pub sidecar_path: Option<String>,
}

/// The provenance anchor: document + page + verbatim text span.
#[derive(Clone, Debug)]
pub struct Source {
    pub id: String,
    pub document_id: String,
    pub page_id: String,
    pub page_number: usize,
    pub source_text: String,
    pub char_start: usize,
    pub char_end: usize,
}

#[derive(Clone, Debug)]
pub struct Fact {
    pub id: String,
    pub display_id: String,
    pub concept: String,
    pub surface_form: String,
    pub source_id: String,
    pub value_num: Option<f64>,
    pub value_text: Option<String>,
    pub value_key: Option<String>,
    pub unit: Option<String>,
    pub obs_date: Option<String>,
    pub date_precision: String,
    pub confidence: f64,
    /// For numeric observations: the direction the report stated ("LOW"/"HIGH"/
    /// "NORMAL"), from its printed flag or reference range. `None` otherwise.
    pub interpretation: Option<String>,
}

#[derive(Clone, Debug)]
pub struct Claim {
    pub id: String,
    pub display_id: String,
    pub claim_text: String,
    pub source_id: String,
    pub claim_type: String,
    pub claim_date: Option<String>,
    pub date_precision: String,
    pub confidence: f64,
    pub evidence_status: String,
}

#[derive(Clone, Debug)]
pub struct Medication {
    pub id: String,
    pub display_id: String,
    pub drug_name: String,
    pub drug_norm: String,
    pub source_id: String,
    pub dose: Option<f64>,
    pub unit: Option<String>,
    pub frequency: Option<String>,
    pub route: Option<String>,
    pub status: String,
    pub start_date: Option<String>,
    pub stop_date: Option<String>,
    pub record_date: Option<String>,
    pub confidence: f64,
}

#[derive(Clone, Debug)]
pub struct Event {
    pub id: String,
    pub display_id: String,
    pub event_type: String,
    pub label: String,
    pub source_id: String,
    pub event_date: Option<String>,
    pub detail: Option<String>,
}

#[derive(Clone, Debug)]
pub struct DocumentReference {
    pub id: String,
    pub display_id: String,
    pub label: String,
    pub source_id: String,
    pub doc_type_hint: Option<String>,
    pub resolved_document_id: Option<String>,
    pub status: String,
}

#[derive(Clone, Debug)]
pub struct Relationship {
    pub id: String,
    pub rel_type: String,
    pub from_type: String,
    pub from_id: String,
    pub to_type: String,
    pub to_id: String,
    pub detail: Option<String>,
    pub status: String,
}

#[derive(Clone, Debug)]
pub struct Conflict {
    pub id: String,
    pub display_id: String,
    pub conflict_type: String,
    pub concept: String,
    pub concept_label: String,
    pub left_type: String,
    pub left_id: String,
    pub right_type: String,
    pub right_id: String,
    pub left_summary: String,
    pub right_summary: String,
    pub left_member_ids: Vec<String>,
    pub right_member_ids: Vec<String>,
    pub basis: String,
    pub delta: Option<f64>,
    pub status: String,
}

#[derive(Clone, Debug)]
pub struct EvidenceGap {
    pub id: String,
    pub display_id: String,
    pub gap_type: String,
    pub subject_type: String,
    pub title: String,
    pub basis: String,
    pub status: String,
    pub subject_id: Option<String>,
    pub evidence_located: Vec<String>,
    pub evidence_not_located: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct Change {
    pub id: String,
    pub display_id: String,
    pub concept: String,
    pub concept_label: String,
    pub from_fact_id: String,
    pub to_fact_id: String,
    pub direction: String,
    pub basis: String,
    pub unit: Option<String>,
    pub from_date: Option<String>,
    pub to_date: Option<String>,
    pub from_value: Option<f64>,
    pub to_value: Option<f64>,
    pub delta: Option<f64>,
}

/// An advisory vocabulary code attached to a fact, medication or claim. The
/// audit engine never reads it, so a terminology service being available or not
/// cannot change an audit outcome.
#[derive(Clone, Debug)]
pub struct Coding {
    pub id: String,
    pub subject_type: String,
    pub subject_id: String,
    pub system: String,
    pub system_uri: Option<String>,
    pub code: String,
    pub display: Option<String>,
    pub queried_text: String,
    pub match_kind: String,
    pub provider_key: String,
    pub assertable: bool,
    pub score: Option<f64>,
    pub version: Option<String>,
}
