//! Extraction layer.
//!
//! [`RuleExtractor`] is the deterministic implementation. Nothing here invents a
//! value: if a pattern does not match, no item is emitted and the document is
//! reported as extraction-incomplete. An OCR- or model-backed extractor could
//! be substituted by producing the same [`RawItem`] records.

use once_cell::sync::Lazy;
use regex::Regex;

use crate::extract_generic as eg;
use crate::models::DocType;
use crate::normalize as n;
use crate::packs;

// --- raw items (pre-persistence) --------------------------------------------

#[derive(Clone, Debug)]
pub struct Obs {
    pub concept: String,
    pub surface_form: String,
    pub value: Option<f64>,
    pub unit: Option<String>,
    pub date: Option<String>,
    pub date_precision: String,
    pub date_basis: String,
    /// Direction the REPORT itself states for this value — "LOW" / "HIGH" /
    /// "NORMAL" — derived from the printed abnormality flag or the printed
    /// reference range. `None` when the report gives neither. Never a guessed
    /// threshold: the report supplies the range or the flag.
    pub interpretation: Option<String>,
}

#[derive(Clone, Debug)]
pub struct Find {
    pub concept: String,
    pub surface_form: String,
    pub value_key: String,
    pub value_text: String,
    pub date: Option<String>,
    pub date_precision: String,
}

#[derive(Clone, Debug)]
pub struct Cl {
    pub claim_text: String,
    pub claim_type: String,
    pub date: Option<String>,
    pub date_precision: String,
}

#[derive(Clone, Debug)]
pub struct Med {
    pub drug_name: String,
    pub drug_norm: String,
    pub dose: Option<f64>,
    pub unit: Option<String>,
    pub frequency: Option<String>,
    pub route: Option<String>,
    pub status: String,
    pub start_date: Option<String>,
    pub stop_date: Option<String>,
    pub record_date: Option<String>,
}

#[derive(Clone, Debug)]
pub struct Ref {
    pub label: String,
    pub doc_type_hint: String,
}

#[derive(Clone, Debug)]
pub struct Ev {
    pub event_type: String,
    pub label: String,
    pub date: Option<String>,
    pub detail: Option<String>,
}

#[derive(Clone, Debug)]
pub enum Payload {
    Observation(Obs),
    Finding(Find),
    Claim(Cl),
    Medication(Med),
    Reference(Ref),
    Event(Ev),
}

#[derive(Clone, Debug)]
pub struct RawItem {
    pub source_text: String,
    pub char_start: usize,
    pub char_end: usize,
    pub confidence: f64,
    pub payload: Payload,
}

// --- classification ---------------------------------------------------------

const CLASSIFIER_RULES: &[(&str, &[&str])] = &[
    ("DISCHARGE_SUMMARY", &["discharge summary", "discharge date"]),
    ("MEDICATION_LIST", &["medication list", "current medications", "medications at discharge"]),
    ("PRESCRIPTION", &["rx", "prescription"]),
    ("LAB_REPORT", &["complete blood count", "laboratory summary", "iron studies", "cumulative laboratory", "reference", "verified by"]),
    ("REFERRAL", &["referral", "dear colleague", "thank you for seeing"]),
    ("FOLLOWUP", &["follow-up note", "follow up note", "interval history"]),
    ("CONSULTATION", &["consultation note", "consultation date", "examination:"]),
    ("PATHOLOGY_REPORT", &["histopathology", "biopsy report"]),
];

const FILENAME_HINTS: &[(&str, &[&str])] = &[
    ("LAB_REPORT", &["cbc", "lab", "ferritin", "laboratory"]),
    ("PRESCRIPTION", &["prescription", "rx"]),
    ("CONSULTATION", &["consultation"]),
    ("DISCHARGE_SUMMARY", &["discharge"]),
    ("MEDICATION_LIST", &["medication_list", "medication list", "med_list"]),
    ("REFERRAL", &["referral"]),
    ("FOLLOWUP", &["followup", "follow_up", "follow-up"]),
];

pub fn classify_text(full_text: &str, filename: &str) -> DocType {
    let t = full_text.to_lowercase();
    let mut scores: std::collections::HashMap<String, i32> = std::collections::HashMap::new();
    // core rules + pack document cues
    let mut rules: Vec<(String, Vec<String>)> = CLASSIFIER_RULES
        .iter()
        .map(|(d, ks)| (d.to_string(), ks.iter().map(|s| s.to_string()).collect()))
        .collect();
    for (dt, phrases) in packs::document_cues() {
        rules.push((dt.to_string(), phrases.iter().map(|s| s.to_string()).collect()));
    }
    for (dt, keys) in &rules {
        let score: i32 = keys.iter().filter(|k| t.contains(k.as_str())).count() as i32 * 3;
        if score > 0 {
            *scores.entry(dt.clone()).or_insert(0) += score;
        }
    }
    // Filename is a weak hint only.
    let fname = filename.to_lowercase();
    let mut hints: Vec<(String, Vec<String>)> = FILENAME_HINTS
        .iter()
        .map(|(d, ks)| (d.to_string(), ks.iter().map(|s| s.to_string()).collect()))
        .collect();
    for (dt, keys) in packs::filename_cues() {
        hints.push((dt.to_string(), keys.iter().map(|s| s.to_string()).collect()));
    }
    for (dt, keys) in &hints {
        if keys.iter().any(|k| fname.contains(k.as_str())) {
            *scores.entry(dt.clone()).or_insert(0) += 2;
        }
    }
    if scores.is_empty() {
        return DocType::Other;
    }
    // Deterministic argmax: highest score, ties broken by doc-type name.
    let mut best: Option<(String, i32)> = None;
    let mut keys: Vec<&String> = scores.keys().collect();
    keys.sort();
    for k in keys {
        let v = scores[k];
        if best.as_ref().map(|(_, bv)| v > *bv).unwrap_or(true) {
            best = Some((k.clone(), v));
        }
    }
    DocType::from_code(&best.unwrap().0)
}

// --- date attribution -------------------------------------------------------

static ROW_DATE_RX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^\s*(\d{1,2}\s+[A-Za-z]{3,9}\.?\s+\d{4})\s{2,}(.*)$").unwrap());
static DATED_ATTRIB_RX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)\bdated\s+(\d{1,2}\s+[A-Za-z]{3,9}\.?\s+\d{4}|\d{4}-\d{2}-\d{2})").unwrap()
});
static CITED_DOC_RX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(concat!(
        r"(?i)(?:discharge\s+summary|cbc|complete\s+blood\s+count|",
        r"consultation(?:\s+note)?|prescription|medication\s+list|",
        r"referral(?:\s+letter)?|ferritin\s+report|laboratory\s+summary|",
        r"lab(?:oratory)?\s+report|report)",
        r"\s+dated\s+(\d{1,2}\s+[A-Za-z]{3,9}\.?\s+\d{4}|\d{4}-\d{2}-\d{2})"
    ))
    .unwrap()
});

fn cited_document_dates(page_text: &str) -> Vec<(String, (String, &'static str))> {
    let mut out = Vec::new();
    for m in CITED_DOC_RX.captures_iter(page_text) {
        let whole = m.get(0).unwrap().as_str();
        let cut = whole.to_lowercase().rfind("dated").unwrap_or(whole.len());
        let noun = crate::normalize_ws(&whole[..cut]).to_lowercase();
        let (iso, prec) = n::parse_date(&m[1]);
        if let Some(iso) = iso {
            out.push((noun, (iso, prec)));
        }
    }
    out
}

/// Sentence around byte offset `pos` (delimiters `.`,`;` + whitespace, or `\n`).
fn sentence_at(page_text: &str, pos: usize) -> String {
    let mut start = 0usize;
    for d in [". ", ".\n", ";", "\n"] {
        if let Some(i) = page_text[..pos.min(page_text.len())].rfind(d) {
            start = start.max(i + 1);
        }
    }
    // end: earliest of '\n' or a '.'/';' followed by whitespace, at/after pos.
    let hay = &page_text[pos.min(page_text.len())..];
    let mut end = page_text.len();
    let bytes = hay.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        if b == b'\n' {
            end = pos + i;
            break;
        }
        if (b == b'.' || b == b';')
            && bytes.get(i + 1).map(|c| c.is_ascii_whitespace()).unwrap_or(false)
        {
            end = pos + i;
            break;
        }
    }
    page_text[start..end.min(page_text.len())].to_string()
}

#[allow(clippy::too_many_arguments)]
fn attribute_date(
    page_text: &str,
    line: &str,
    line_start: usize,
    match_pos: usize,
    default_date: &Option<String>,
    default_precision: &str,
) -> (Option<String>, String, &'static str) {
    if let Some(c) = ROW_DATE_RX.captures(line) {
        let (iso, prec) = n::parse_date(&c[1]);
        if let Some(iso) = iso {
            return (Some(iso), prec.to_string(), "row");
        }
    }
    let same: Vec<_> = DATED_ATTRIB_RX.captures_iter(line).collect();
    if let Some(last) = same.last() {
        let (iso, prec) = n::parse_date(&last[1]);
        if let Some(iso) = iso {
            return (Some(iso), prec.to_string(), "attributed");
        }
    }
    let cited = cited_document_dates(page_text);
    if !cited.is_empty() {
        let sentence = sentence_at(page_text, match_pos).to_lowercase();
        let hit = cited
            .iter()
            .filter_map(|entry| sentence.rfind(&entry.0).map(|i| (i, entry)))
            .max_by_key(|(i, _)| *i);
        if let Some((_, entry)) = hit {
            let (iso, prec) = &entry.1;
            return (Some(iso.clone()), prec.to_string(), "cited");
        }
    }
    let ctx_start = line_start.saturating_sub(240);
    let preceding: Vec<_> = DATED_ATTRIB_RX
        .captures_iter(&page_text[ctx_start..match_pos.min(page_text.len())])
        .collect();
    if let Some(last) = preceding.last() {
        let (iso, prec) = n::parse_date(&last[1]);
        if let Some(iso) = iso {
            return (Some(iso), prec.to_string(), "attributed");
        }
    }
    (default_date.clone(), default_precision.to_string(), "document")
}

// --- observations -----------------------------------------------------------

/// The direction the report itself states for a value: from the printed
/// abnormality flag first, else from the printed reference range. Never a
/// guessed threshold — the report supplies the flag or the range.
fn interpret_direction(value: f64, flag: Option<&str>, reference_range: Option<&str>) -> Option<String> {
    if let Some(f) = flag {
        let f = f.trim().to_uppercase();
        if f.starts_with('L') {
            return Some("LOW".into());
        }
        if f.starts_with('H') {
            return Some("HIGH".into());
        }
    }
    // Reference range may be "13.0-17.0", "6.0 - 18.0", "(Low)", "< 5", "> 2" …
    if let Some(r) = reference_range {
        let low = r.to_lowercase();
        if low.contains("low") {
            return Some("LOW".into());
        }
        if low.contains("high") {
            return Some("HIGH".into());
        }
        if let Some((lo, hi)) = parse_range(r) {
            if value < lo {
                return Some("LOW".into());
            }
            if value > hi {
                return Some("HIGH".into());
            }
            return Some("NORMAL".into());
        }
    }
    None
}

/// Parse a printed `lo - hi` reference interval to `(lo, hi)`.
fn parse_range(r: &str) -> Option<(f64, f64)> {
    static RANGE: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r"(-?\d+(?:\.\d+)?)\s*(?:-|--|–|—|to)\s*(-?\d+(?:\.\d+)?)").unwrap()
    });
    let c = RANGE.captures(r)?;
    let lo: f64 = c[1].parse().ok()?;
    let hi: f64 = c[2].parse().ok()?;
    if lo <= hi {
        Some((lo, hi))
    } else {
        Some((hi, lo))
    }
}

pub fn extract_observations(
    page_text: &str,
    default_date: &Option<String>,
    default_precision: &str,
) -> Vec<RawItem> {
    let mut items = Vec::new();
    let mut seen: std::collections::HashSet<(usize, usize, String)> = std::collections::HashSet::new();

    for g in eg::extract_measurements(page_text) {
        let (concept, display, method) = n::resolve_label(&g.label);
        let concept = match concept {
            Some(c) => c,
            None => continue,
        };
        // An unrecognised label carrying a plain amount is a dose, not a patient
        // observation — medication extraction owns those.
        if method == n::UNRECOGNISED && g.amount_like() {
            continue;
        }
        let ls = g.line_start;
        let le = g.line_start + g.line.len();
        let key = (ls, le, concept.clone());
        if seen.contains(&key) {
            continue;
        }
        let unit = if n::is_unmapped(&concept) {
            Some(g.unit_canonical.clone())
        } else {
            n::normalize_unit(&concept, if g.unit.is_empty() { None } else { Some(&g.unit) })
        };
        let mut conf = g.confidence;
        if method == "CONTAINED" {
            conf = conf.min(0.86);
        } else if method == n::UNRECOGNISED {
            conf = conf.min(0.72);
        }
        if ROW_DATE_RX.is_match(&g.line) {
            conf = conf.min(0.93);
        }
        let (obs_date, precision, basis) =
            attribute_date(page_text, &g.line, ls, g.span.0, default_date, default_precision);
        if basis == "row" {
            conf = conf.min(0.93);
        }
        seen.insert(key);
        let surface = if method != n::UNRECOGNISED { display } else { g.label.clone() };
        let interpretation = interpret_direction(g.value, g.flag.as_deref(), g.reference_range.as_deref());
        items.push(RawItem {
            source_text: g.line.trim().to_string(),
            char_start: ls,
            char_end: le,
            confidence: conf,
            payload: Payload::Observation(Obs {
                concept,
                surface_form: surface,
                value: Some(g.value),
                unit,
                date: obs_date,
                date_precision: precision,
                date_basis: basis.to_string(),
                interpretation,
            }),
        });
    }
    items
}

// --- claims -----------------------------------------------------------------

const ASSESSMENT_CUES: &[&str] = &[
    "assessment", "impression", "diagnosis", "conclusion", "condition at discharge",
];
static MEASURED_NUM_RX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)\b\d+(\.\d+)?\s*(g/dL|fL|pg|ng/mL|mg/dL)\b").unwrap());

pub fn extract_claims(
    page_text: &str,
    default_date: &Option<String>,
    default_precision: &str,
) -> Vec<RawItem> {
    let mut items = Vec::new();
    let lowered = page_text.to_lowercase();
    let mut claimed_spans: Vec<(usize, usize)> = Vec::new();

    for ctd in packs::claim_types() {
        for pat in ctd.patterns {
            let mut start = 0usize;
            while let Some(rel) = lowered[start..].find(pat) {
                let idx = start + rel;
                start = idx + pat.len();
                if claimed_spans.iter().any(|&(a, b)| a <= idx && idx < b) {
                    continue;
                }
                let line_start = page_text[..idx].rfind('\n').map(|i| i + 1).unwrap_or(0);
                let line_end = page_text[idx..].find('\n').map(|i| idx + i).unwrap_or(page_text.len());
                let line = page_text[line_start..line_end].trim();
                if line.is_empty() {
                    continue;
                }
                if MEASURED_NUM_RX.is_match(line) && ctd.key != "HB_NORMALIZED" {
                    continue;
                }
                let ctx_start = line_start.saturating_sub(400);
                let ctx = page_text[ctx_start..line_end].to_lowercase();
                let in_assessment = ASSESSMENT_CUES.iter().any(|c| ctx.contains(c));
                let conf = if in_assessment { 0.95 } else { 0.82 };
                claimed_spans.push((line_start, line_end));
                items.push(RawItem {
                    source_text: line.to_string(),
                    char_start: line_start,
                    char_end: line_end,
                    confidence: conf,
                    payload: Payload::Claim(Cl {
                        claim_text: line.trim_end_matches(['.', ' ']).to_string(),
                        claim_type: ctd.key.to_string(),
                        date: default_date.clone(),
                        date_precision: default_precision.to_string(),
                    }),
                });
                break; // one hit per pattern per page
            }
        }
    }
    items
}

// --- medications ------------------------------------------------------------

const FREQ_TERMS: &[&str] = &[
    "once daily", "twice daily", "three times daily", "at night", "as required",
    "every 8 hours", "every 12 hours", "od", "bd", "tds",
];
const ROUTE_TERMS: &[&str] = &["oral", "iv", "intravenous", "im", "subcutaneous", "topical"];
const NON_DRUG_NAMES: &[&str] = &[
    "page", "date", "case ref", "reference", "test", "result", "drug", "hemoglobin",
    "haemoglobin", "mcv", "mch", "mchc", "rdw", "wbc", "rbc", "platelet count",
    "hematocrit", "hb", "serum ferritin", "serum creatinine", "plt", "tlc", "pcv",
    "glucose", "ferritin", "creatinine",
];

static MED_TABLE_RX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(concat!(
        r"(?im)^[ \t]*(?P<name>[A-Za-z][A-Za-z '\-]{2,38}?)\s{2,}",
        r"(?P<dose>\d+(?:\.\d+)?)\s*(?P<unit>mg|mcg|g|ml|iu|units?)",
        r"\s{2,}(?P<freq>[A-Za-z ]{2,20}?)\s{2,}(?P<route>[A-Za-z]{2,12})\s{2,}",
        r"(?P<status>[A-Za-z]{3,12})\s*$"
    ))
    .unwrap()
});
static MED_LINE_RX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(concat!(
        r"(?im)^[ \t]*(?:\d+[.)]\s*)?(?P<name>[A-Za-z][A-Za-z '\-]{2,38}?)\s+",
        r"(?P<dose>\d+(?:\.\d+)?)\s*(?P<unit>mg|mcg|g|ml|iu|units?)(?P<rest>.*)$"
    ))
    .unwrap()
});
static START_DATE_RX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)start date\s*[:\-]?\s*([^\n]{4,30})").unwrap());
static STOP_DATE_RX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)stop(?:ped)? date\s*[:\-]?\s*([^\n]{4,30})").unwrap());
static STATUS_ACTIVE_RX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)\bstatus\s*[:\-]?\s*active\b|\bactive\b").unwrap());
static STATUS_STOPPED_RX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)\bstopped\b|\bdiscontinued\b|\bceased\b").unwrap());

fn med_fields(rest: &str) -> (Option<String>, Option<String>, Option<String>) {
    let r = rest.to_lowercase();
    let freq = FREQ_TERMS.iter().find(|f| r.contains(**f)).map(|s| s.to_string());
    let route = ROUTE_TERMS
        .iter()
        .find(|t| word_present(&r, t))
        .map(|s| s.to_string());
    let status = if STATUS_ACTIVE_RX.is_match(&r) {
        Some("ACTIVE".to_string())
    } else if STATUS_STOPPED_RX.is_match(&r) {
        Some("STOPPED".to_string())
    } else {
        None
    };
    (freq, route, status)
}

fn word_present(hay: &str, word: &str) -> bool {
    // \bword\b
    let mut from = 0;
    while let Some(rel) = hay[from..].find(word) {
        let s = from + rel;
        let e = s + word.len();
        let b_ok = s == 0 || !hay.as_bytes()[s - 1].is_ascii_alphanumeric();
        let a_ok = e == hay.len() || !hay.as_bytes()[e].is_ascii_alphanumeric();
        if b_ok && a_ok {
            return true;
        }
        from = s + 1;
    }
    false
}

pub fn extract_medications(
    page_text: &str,
    doc_type: DocType,
    default_date: &Option<String>,
) -> Vec<RawItem> {
    let mut items = Vec::new();
    let mut seen: std::collections::HashSet<(String, usize)> = std::collections::HashSet::new();

    let mut emit = |name: &str, dose: f64, unit: &str, freq: Option<String>, route: Option<String>,
                    status_in: Option<String>, line: &str, ls: usize, le: usize, conf: f64,
                    out: &mut Vec<RawItem>| {
        let norm = n::normalize_drug(name);
        if NON_DRUG_NAMES.contains(&norm.as_str()) || norm.len() < 3 {
            return;
        }
        let k = (norm.clone(), ls);
        if seen.contains(&k) {
            return;
        }
        seen.insert(k);
        let status = status_in.unwrap_or_else(|| {
            if matches!(doc_type, DocType::Prescription | DocType::MedicationList) {
                "ACTIVE".to_string()
            } else {
                "DOCUMENTED".to_string()
            }
        });
        let start_date = START_DATE_RX
            .captures(line)
            .and_then(|c| n::parse_date(&c[1]).0);
        let stop_date = STOP_DATE_RX
            .captures(line)
            .and_then(|c| n::parse_date(&c[1]).0);
        out.push(RawItem {
            source_text: line.trim().to_string(),
            char_start: ls,
            char_end: le,
            confidence: conf,
            payload: Payload::Medication(Med {
                drug_name: name.trim().to_string(),
                drug_norm: norm,
                dose: Some(dose),
                unit: Some(unit.to_string()),
                frequency: freq,
                route,
                status,
                start_date,
                stop_date,
                record_date: default_date.clone(),
            }),
        });
    };

    for m in MED_TABLE_RX.captures_iter(page_text) {
        let whole = m.get(0).unwrap();
        let ls = page_text[..whole.start()].rfind('\n').map(|i| i + 1).unwrap_or(0);
        let le = page_text[whole.start()..].find('\n').map(|i| whole.start() + i).unwrap_or(page_text.len());
        let line = &page_text[ls..le];
        let status = m["status"].to_uppercase();
        let status = if status.starts_with("ACTIV") { "ACTIVE".to_string() } else { status };
        emit(
            &m["name"],
            m["dose"].parse().unwrap_or(0.0),
            &m["unit"].to_lowercase(),
            Some(m["freq"].trim().to_string()),
            Some(m["route"].to_lowercase()),
            Some(status),
            line, ls, le, 0.96, &mut items,
        );
    }

    for m in MED_LINE_RX.captures_iter(page_text) {
        let whole = m.get(0).unwrap();
        let ls = page_text[..whole.start()].rfind('\n').map(|i| i + 1).unwrap_or(0);
        let le = page_text[whole.start()..].find('\n').map(|i| whole.start() + i).unwrap_or(page_text.len());
        let line = &page_text[ls..le];
        // Extend over indented continuation lines (Start date / Status).
        let mut block_end = le;
        let mut cursor = le;
        while cursor < page_text.len() {
            let nls = cursor + 1;
            if nls > page_text.len() {
                break;
            }
            let nle = page_text[nls.min(page_text.len())..]
                .find('\n')
                .map(|i| nls + i)
                .unwrap_or(page_text.len());
            let nxt = &page_text[nls.min(page_text.len())..nle];
            if !(nxt.starts_with("    ") || nxt.starts_with('\t')) || nxt.trim().is_empty() {
                break;
            }
            static CONT_STOP: Lazy<Regex> = Lazy::new(|| Regex::new(r"^\s*(?:\d+[.)]|-{5,})").unwrap());
            if CONT_STOP.is_match(nxt) {
                break;
            }
            block_end = nle;
            cursor = nle;
        }
        let block = &page_text[ls..block_end];
        let low = line.to_lowercase();
        if low.contains("reference") || low.contains("result") || low.contains("unit  ") {
            continue;
        }
        let (freq, route, status) = med_fields(block);
        emit(
            &m["name"],
            m["dose"].parse().unwrap_or(0.0),
            &m["unit"].to_lowercase(),
            freq, route, status,
            block, ls, le, 0.94, &mut items,
        );
    }

    items
}

// --- qualitative findings ---------------------------------------------------

static HYPOTHETICAL_RX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(concat!(
        r"(?i)\b(?:if|should|unless|to (?:exclude|rule out|assess for)|",
        r"query|\?|consider|suspected|possible|cannot exclude|",
        r"r/o|rule out|advise|recommend(?:ed)?|plan(?:ned)?\s+for|",
        r"awaiting|pending)\b"
    ))
    .unwrap()
});
static NEGATOR_RX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(concat!(
        r"(?i)\b(?:no|not|non|without|negative for|free of|absent|denies|",
        r"unremarkable for|ruled out)\s*$"
    ))
    .unwrap()
});
static ANCHOR_RX: Lazy<Regex> = Lazy::new(|| Regex::new(r"[:\-\u{2013}\u{2014}|\t]\s*$").unwrap());

fn negated(low: &str, pos: usize) -> bool {
    let from = pos.saturating_sub(24);
    NEGATOR_RX.is_match(&low[from..pos])
}

pub fn extract_findings(
    page_text: &str,
    default_date: &Option<String>,
    precision: &str,
) -> Vec<RawItem> {
    let mut items = Vec::new();
    let mut offset = 0usize;
    for line in page_text.split('\n') {
        let ls = offset;
        let le = offset + line.len();
        offset = le + 1;
        let low = line.to_lowercase();
        if low.trim().is_empty() {
            continue;
        }
        let hypothetical = HYPOTHETICAL_RX.is_match(&low);
        let mut taken: Vec<(usize, usize)> = Vec::new();
        // One documented finding per (concept, value class) per line: two phrases
        // for the same finding on one line ("bite cells" + "Heinz bodies") are
        // the same documentation instance, not two.
        let mut emitted: std::collections::HashSet<(&str, &str)> = std::collections::HashSet::new();
        for (phrase_low, concept, value_key, _phrase) in packs::status_phrases() {
            let mut start = 0usize;
            while let Some(rel) = low[start..].find(phrase_low.as_str()) {
                let pos = start + rel;
                let end = pos + phrase_low.len();
                start = end;
                let before = if pos > 0 { low.as_bytes()[pos - 1] } else { b' ' };
                let after = if end < low.len() { low.as_bytes()[end] } else { b' ' };
                if before.is_ascii_alphanumeric() || after.is_ascii_alphanumeric() {
                    continue;
                }
                let overlaps = taken.iter().any(|&(b, e)| {
                    (pos < b && e < end + 1 && e > pos) || (b <= pos && pos < e) || (b < end && end <= e)
                });
                if overlaps {
                    continue;
                }
                if packs::anchored_phrases().contains(phrase_low) && !ANCHOR_RX.is_match(&low[..pos]) {
                    continue;
                }
                if packs::status_value_positive(concept, value_key) == Some(true) && negated(&low, pos) {
                    continue;
                }
                taken.push((pos, end));
                if hypothetical {
                    continue;
                }
                if !emitted.insert((concept, value_key)) {
                    continue; // same finding already documented on this line
                }
                items.push(RawItem {
                    source_text: line.trim().to_string(),
                    char_start: ls,
                    char_end: le,
                    confidence: 0.92,
                    payload: Payload::Finding(Find {
                        concept: concept.to_string(),
                        surface_form: line[pos..end].trim().to_string(),
                        value_key: value_key.to_string(),
                        value_text: packs::status_value_label(concept, value_key),
                        date: default_date.clone(),
                        date_precision: precision.to_string(),
                    }),
                });
            }
        }
    }
    items
}

// --- references + follow-up events ------------------------------------------

/// Collapse whitespace runs to single spaces, keeping a map back to source byte
/// offsets, so a pattern written the human way still matches text broken by
/// layout while provenance points at the original characters.
fn flatten(text: &str) -> (String, Vec<usize>) {
    let mut out = String::new();
    let mut index_map: Vec<usize> = Vec::new();
    let mut prev_ws = false;
    for (i, ch) in text.char_indices() {
        if ch.is_whitespace() {
            if !prev_ws && !out.is_empty() {
                out.push(' ');
                index_map.push(i);
            }
            prev_ws = true;
        } else {
            out.push(ch);
            // record byte offset for each byte of this char
            for _ in 0..ch.len_utf8() {
                index_map.push(i);
            }
            prev_ws = false;
        }
    }
    (out, index_map)
}

fn line_span(text: &str, pos: usize) -> (usize, usize) {
    let ls = text[..pos.min(text.len())].rfind('\n').map(|i| i + 1).unwrap_or(0);
    let le = text[pos.min(text.len())..].find('\n').map(|i| pos + i).unwrap_or(text.len());
    (ls, le)
}

pub fn extract_references(page_text: &str) -> Vec<RawItem> {
    let mut items = Vec::new();
    let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::new();
    let (flat, imap) = flatten(page_text);
    for (pattern, label, hint) in packs::document_reference_patterns() {
        let rx = Regex::new(&format!("(?i){pattern}")).unwrap();
        if let Some(m) = rx.find(&flat) {
            if seen.contains(label) {
                continue;
            }
            seen.insert(label);
            let start = imap.get(m.start()).copied().unwrap_or(0);
            let (ls, le) = line_span(page_text, start);
            items.push(RawItem {
                source_text: page_text[ls..le].trim().to_string(),
                char_start: ls,
                char_end: le,
                confidence: 0.95,
                payload: Payload::Reference(Ref {
                    label: label.to_string(),
                    doc_type_hint: hint.to_string(),
                }),
            });
        }
    }
    items
}

pub fn extract_followup_events(page_text: &str, default_date: &Option<String>) -> Vec<RawItem> {
    let mut items = Vec::new();
    let (flat, imap) = flatten(page_text);
    for (pattern, label) in packs::followup_patterns() {
        let rx = Regex::new(&format!("(?i){pattern}")).unwrap();
        if let Some(m) = rx.find(&flat) {
            let start = imap.get(m.start()).copied().unwrap_or(0);
            let (ls, le) = line_span(page_text, start);
            items.push(RawItem {
                source_text: page_text[ls..le].trim().to_string(),
                char_start: ls,
                char_end: le,
                confidence: 0.9,
                payload: Payload::Event(Ev {
                    event_type: "EXPECTED_FOLLOWUP".to_string(),
                    label: label.to_string(),
                    date: default_date.clone(),
                    detail: Some(m.as_str().trim().to_string()),
                }),
            });
        }
    }
    items
}

// --- the shipped extractor --------------------------------------------------

/// Deterministic pattern/lexicon extractor. No model calls.
pub struct RuleExtractor;

impl Default for RuleExtractor {
    fn default() -> Self {
        RuleExtractor
    }
}

impl RuleExtractor {
    pub fn name(&self) -> &'static str {
        "rule-extractor/1.0"
    }

    pub fn classify(&self, full_text: &str, filename: &str) -> DocType {
        classify_text(full_text, filename)
    }

    pub fn document_date(&self, full_text: &str) -> (Option<String>, String) {
        let (iso, prec, _labelled) = n::find_document_date(full_text);
        (iso, prec.to_string())
    }

    pub fn extract_page(
        &self,
        page_text: &str,
        doc_type: DocType,
        doc_date: &Option<String>,
    ) -> Vec<RawItem> {
        // Only an explicitly LABELLED page date may override the document date.
        let (page_date, precision, labelled) = n::find_document_date(page_text);
        let (effective, prec): (Option<String>, String) = if labelled && page_date.is_some() {
            (page_date, precision.to_string())
        } else {
            (doc_date.clone(), "day".to_string())
        };
        let mut out = Vec::new();
        out.extend(extract_observations(page_text, &effective, &prec));
        out.extend(extract_claims(page_text, &effective, &prec));
        out.extend(extract_medications(page_text, doc_type, &effective));
        out.extend(extract_findings(page_text, &effective, &prec));
        out.extend(extract_references(page_text));
        out.extend(extract_followup_events(page_text, &effective));
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_lab_report() {
        let dt = classify_text("Complete Blood Count\nHemoglobin 9.2 g/dL", "cbc.pdf");
        assert_eq!(dt, DocType::LabReport);
    }

    #[test]
    fn extracts_observation_with_date() {
        let items = extract_observations(
            "Hemoglobin 9.2 g/dL\n",
            &Some("2026-05-04".to_string()),
            "day",
        );
        let obs: Vec<&Obs> = items
            .iter()
            .filter_map(|i| match &i.payload {
                Payload::Observation(o) => Some(o),
                _ => None,
            })
            .collect();
        assert!(obs.iter().any(|o| o.concept == "hemoglobin" && o.value == Some(9.2)));
    }

    #[test]
    fn extracts_medication_status_from_doc_role() {
        let items = extract_medications(
            "Ferrous sulfate 200 mg once daily oral\n",
            DocType::Prescription,
            &Some("2026-05-01".to_string()),
        );
        let med = items.iter().find_map(|i| match &i.payload {
            Payload::Medication(m) => Some(m),
            _ => None,
        });
        let med = med.expect("a medication");
        assert_eq!(med.drug_norm, "ferrous sulfate");
        assert_eq!(med.status, "ACTIVE");
    }

    #[test]
    fn extracts_claim_in_assessment() {
        let items = extract_claims(
            "Impression:\nIron deficiency confirmed on iron studies.\n",
            &Some("2026-05-04".to_string()),
            "day",
        );
        let cl = items.iter().find_map(|i| match &i.payload {
            Payload::Claim(c) => Some(c),
            _ => None,
        });
        assert_eq!(cl.unwrap().claim_type, "IRON_DEFICIENCY");
    }
}
