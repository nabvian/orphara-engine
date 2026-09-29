//! Optional, user-activated vocabulary coding. Runs after the audit and writes
//! only codings, so it cannot change a conflict, change or gap.
//!
//! Rules:
//! - a coding is advisory; the audit never reads it;
//! - only EXACT / SYNONYM matches are assertable, an approximate match is not;
//! - a vocabulary whose licence is not held is reported UNLICENSED, and a code
//!   is never guessed.
//!
//! LOINC / RxNorm / SNOMED are activated by importing a local release into
//! [`local_index`]. Without activation the base packs remain the default.

pub mod local_index;
pub mod ucum;

use std::collections::HashMap;

use crate::models;
use crate::packs;
use crate::pipeline::CaseResult;

/// How closely a returned code corresponds to the queried phrase.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MatchKind {
    Exact,
    Synonym,
    Approximate,
    None,
}

impl MatchKind {
    pub fn as_str(self) -> &'static str {
        match self {
            MatchKind::Exact => "EXACT",
            MatchKind::Synonym => "SYNONYM",
            MatchKind::Approximate => "APPROXIMATE",
            MatchKind::None => "NONE",
        }
    }
    /// Only EXACT / SYNONYM may be treated as denoting the concept.
    pub fn assertable(self) -> bool {
        matches!(self, MatchKind::Exact | MatchKind::Synonym)
    }
}

/// Why a provider is or is not usable right now.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ProviderState {
    Active,
    Unlicensed,
    Unreachable,
    Disabled,
    Unknown,
}

impl ProviderState {
    pub fn as_str(self) -> &'static str {
        match self {
            ProviderState::Active => "ACTIVE",
            ProviderState::Unlicensed => "UNLICENSED",
            ProviderState::Unreachable => "UNREACHABLE",
            ProviderState::Disabled => "DISABLED",
            ProviderState::Unknown => "UNKNOWN",
        }
    }
}

/// What a provider needs before it can be used, in the user's terms.
#[derive(Clone, Debug, Default)]
pub struct LicenceRequirement {
    pub required: bool,
    pub licence_name: Option<String>,
    pub obtain_url: Option<String>,
    pub credential_fields: Vec<String>,
    pub note: String,
}

/// One code from one vocabulary, with the provenance of how it was obtained.
#[derive(Clone, Debug)]
pub struct Coding {
    pub system: String,
    pub system_uri: String,
    pub code: String,
    pub display: String,
    pub queried_text: String,
    pub match_kind: MatchKind,
    pub score: Option<f64>,
    pub version: Option<String>,
}

impl Coding {
    pub fn assertable(&self) -> bool {
        self.match_kind.assertable()
    }
}

/// Result of probing a provider — what an admin screen renders.
#[derive(Clone, Debug)]
pub struct ProviderHealth {
    pub key: String,
    pub label: String,
    pub state: ProviderState,
    pub detail: String,
    pub version: Option<String>,
}

/// Vocabulary kinds a subject can be coded as.
pub const KIND_OBSERVATION: &str = "observation";
pub const KIND_MEDICATION: &str = "medication";
pub const KIND_DIAGNOSIS: &str = "diagnosis_statement";

/// What every vocabulary adapter implements. Implementations must not fail on an
/// outage: they return an empty result and report the state through `health`.
pub trait TerminologyProvider {
    fn key(&self) -> &str;
    fn system(&self) -> &str;
    fn system_uri(&self) -> &str;
    fn licence(&self) -> &LicenceRequirement;
    /// Which kinds this provider can code (e.g. `["observation"]`).
    fn codes_kinds(&self) -> &[&'static str];
    /// Whether the provider has what it needs to answer (credentials / data).
    fn configured(&self) -> bool;
    fn health(&self) -> ProviderHealth;
    fn lookup(&self, text: &str, kind: &str, limit: usize) -> Vec<Coding>;
}

/// Cache-key normalisation: casefolded, whitespace-collapsed, punctuation-trimmed.
pub fn norm_query(text: &str) -> String {
    crate::normalize_ws(text)
        .to_lowercase()
        .trim_matches(|c| " .,:;".contains(c))
        .to_string()
}

/// What a resolution pass did.
#[derive(Clone, Debug, Default)]
pub struct ResolutionReport {
    pub attempted: usize,
    pub coded: usize,
    pub assertable: usize,
    pub approximate: usize,
    pub misses: usize,
    pub cache_hits: usize,
    pub provider_calls: usize,
    pub skipped_unlicensed: HashMap<String, usize>,
    pub by_system: HashMap<String, usize>,
}

/// Annotate every fact, medication and claim in a completed case with codings.
///
/// Strictly additive: it reads the audited records and returns codings; it never
/// mutates the audit. Providers that are not configured are recorded in
/// `skipped_unlicensed`, never queried, and never cached as a miss — so a later
/// activated run still tries them.
pub fn resolve_case(
    case: &CaseResult,
    providers: &[Box<dyn TerminologyProvider>],
) -> (Vec<models::Coding>, ResolutionReport) {
    let mut report = ResolutionReport::default();
    let mut out: Vec<models::Coding> = Vec::new();
    // (provider_key, kind, norm_query) -> codings (empty = a cached miss).
    let mut cache: HashMap<(String, String, String), Vec<Coding>> = HashMap::new();

    // Build the work list: (subject_type, subject_id, query, surface, kind).
    let mut rows: Vec<(&'static str, String, String, String, &'static str)> = Vec::new();
    for f in &case.facts {
        if let Some(q) = fact_query(&f.concept) {
            rows.push((models_subject_fact(), f.id.clone(), q, f.surface_form.clone(), KIND_OBSERVATION));
        }
    }
    for m in &case.medications {
        let name = m.drug_name.trim();
        if !name.is_empty() {
            rows.push((models_subject_med(), m.id.clone(), name.to_string(), name.to_string(), KIND_MEDICATION));
        }
    }
    for c in &case.claims {
        let t = c.claim_text.trim();
        if !t.is_empty() {
            rows.push((models_subject_claim(), c.id.clone(), t.to_string(), t.to_string(), KIND_DIAGNOSIS));
        }
    }

    for (subject_type, subject_id, query, surface, kind) in rows {
        report.attempted += 1;
        let codings = lookup_cached(&query, kind, providers, &mut cache, &mut report);
        if codings.is_empty() {
            report.misses += 1;
            continue;
        }
        for pc in codings {
            report.coded += 1;
            if pc.assertable() {
                report.assertable += 1;
            } else {
                report.approximate += 1;
            }
            *report.by_system.entry(pc.system.clone()).or_insert(0) += 1;
            let provider_key = providers
                .iter()
                .find(|p| p.system() == pc.system)
                .map(|p| p.key().to_string())
                .unwrap_or_else(|| "unknown".into());
            out.push(models::Coding {
                id: models::new_id(),
                subject_type: subject_type.to_string(),
                subject_id: subject_id.clone(),
                system: pc.system.clone(),
                system_uri: Some(pc.system_uri.clone()),
                code: pc.code.clone(),
                display: Some(pc.display.clone()),
                queried_text: surface.clone(), // the document's own wording
                match_kind: pc.match_kind.as_str().to_string(),
                provider_key,
                assertable: pc.assertable(),
                score: pc.score,
                version: pc.version.clone(),
            });
        }
    }
    (out, report)
}

fn lookup_cached(
    query: &str,
    kind: &str,
    providers: &[Box<dyn TerminologyProvider>],
    cache: &mut HashMap<(String, String, String), Vec<Coding>>,
    report: &mut ResolutionReport,
) -> Vec<Coding> {
    let key_q = norm_query(query);
    let mut results = Vec::new();
    for p in providers.iter().filter(|p| p.codes_kinds().contains(&kind)) {
        let ck = (p.key().to_string(), kind.to_string(), key_q.clone());
        if let Some(cached) = cache.get(&ck) {
            report.cache_hits += 1;
            results.extend(cached.clone());
            continue;
        }
        if !p.configured() {
            // Unlicensed / no data: record nothing, do NOT cache a miss.
            *report.skipped_unlicensed.entry(p.key().to_string()).or_insert(0) += 1;
            continue;
        }
        report.provider_calls += 1;
        let found = p.lookup(query, kind, 3);
        cache.insert(ck, found.clone());
        results.extend(found);
    }
    results
}

/// Query the pack's canonical label, not the document's abbreviation.
fn fact_query(concept: &str) -> Option<String> {
    if let Some(cd) = packs::concepts().get(concept) {
        return Some(cd.label.to_string());
    }
    let l = packs::concept_label(concept);
    if l.is_empty() {
        None
    } else {
        Some(l)
    }
}

fn models_subject_fact() -> &'static str {
    "fact"
}
fn models_subject_med() -> &'static str {
    "medication"
}
fn models_subject_claim() -> &'static str {
    "claim"
}
