//! Deterministic normalization: concepts, units, dates.
//!
//! Every function is pure. The original surface wording is always returned
//! alongside the normalized key, so provenance survives normalization.

use once_cell::sync::Lazy;
use regex::Regex;

use crate::date;
use crate::packs;

/// How a surface label was mapped to a concept.
pub const EXACT: &str = "EXACT";
pub const CONTAINED: &str = "CONTAINED";
pub const UNRECOGNISED: &str = "UNRECOGNISED";

static NON_CONCEPT_CHARS: Lazy<Regex> = Lazy::new(|| Regex::new(r"[^a-z0-9 %/\-]").unwrap());
static WS: Lazy<Regex> = Lazy::new(|| Regex::new(r"\s+").unwrap());
static DOTTED: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?:[A-Za-z]\s*\.\s*){2,}[A-Za-z]?").unwrap());
static NON_SLUG: Lazy<Regex> = Lazy::new(|| Regex::new(r"[^a-z0-9]+").unwrap());

const STOPWORDS: &[&str] = &[
    "the", "a", "an", "of", "was", "is", "were", "are", "with", "and", "on", "at",
    "in", "for", "patient", "patients", "his", "her", "their", "s", "shows",
    "showed", "showing", "noted", "recorded", "reported", "records", "summary",
    "report", "value", "result", "level", "levels", "count", "serum", "plasma",
    "blood", "whole", "venous", "arterial", "capillary", "total", "measured",
    "admission", "discharge", "today", "repeat",
];

fn clean_concept(surface: &str) -> String {
    let lower = surface.to_lowercase();
    let s = NON_CONCEPT_CHARS.replace_all(&lower, " ");
    let s = WS.replace_all(&s, " ");
    s.trim().trim_matches(|c| c == ' ' || c == '-' || c == ':').to_string()
}

/// Map a surface label ('Hb', 'PLT', 'Packed Cell Volume') to a concept key.
pub fn normalize_concept(surface: &str) -> Option<String> {
    let s = clean_concept(surface);
    if s.is_empty() {
        return None;
    }
    if let Some(k) = packs::synonym_index().get(&s) {
        return Some(k.clone());
    }
    for (syn, key) in packs::synonyms_by_length() {
        if &s == syn || s.starts_with(&format!("{syn} ")) || s.ends_with(&format!(" {syn}")) {
            return Some(key.clone());
        }
    }
    None
}

fn undot(surface: &str) -> String {
    let joined = DOTTED.replace_all(surface, |m: &regex::Captures| {
        let g = m.get(0).unwrap().as_str();
        let compact: String = g.chars().filter(|c| *c != '.' && !c.is_whitespace()).collect();
        format!("{compact} ")
    });
    WS.replace_all(&joined, " ").trim().to_string()
}

/// Resolve any surface label to `(concept_key, display_label, method)`.
///
/// A label the vocabulary does not know is NOT discarded: it is kept under an
/// `unmapped:` slug so the measurement is still counted, traceable, and
/// comparable with itself over time — just not asserted to be a known concept.
pub fn resolve_label(surface: &str) -> (Option<String>, String, &'static str) {
    if let Some(key) = normalize_concept(surface) {
        let label = packs::concept_label(&key);
        return (Some(key), label, EXACT);
    }
    let undotted = undot(surface);
    if undotted != surface {
        if let Some(key) = normalize_concept(&undotted) {
            let label = packs::concept_label(&key);
            return (Some(key), label, EXACT);
        }
    }

    let lower = undotted.to_lowercase();
    let cleaned = NON_CONCEPT_CHARS.replace_all(&lower, " ");
    let words: Vec<&str> = cleaned
        .split_whitespace()
        .filter(|w| !w.is_empty() && !STOPWORDS.contains(w))
        .collect();
    if words.is_empty() {
        return (None, surface.trim().to_string(), UNRECOGNISED);
    }

    let reduced = words.join(" ");
    for (syn, k) in packs::synonyms_by_length() {
        if syn == &reduced || contains_word(&reduced, syn) {
            return (Some(k.clone()), packs::concept_label(k), CONTAINED);
        }
    }

    // Not in the vocabulary — stable unmapped slug.
    let slug: String = {
        let joined = words.join(" ");
        let s = NON_SLUG.replace_all(&joined, "_");
        let s = s.trim_matches('_');
        s.chars().take(48).collect()
    };
    let display = {
        let d = WS.replace_all(surface.trim(), " ").to_string();
        if d.is_empty() { slug.clone() } else { d }
    };
    if slug.is_empty() {
        (None, display, UNRECOGNISED)
    } else {
        (Some(format!("unmapped:{slug}")), display, UNRECOGNISED)
    }
}

/// Whether a concept key came from an analyte outside the vocabulary.
pub fn is_unmapped(concept: &str) -> bool {
    concept.starts_with("unmapped:")
}

pub fn normalize_unit(concept: &str, unit: Option<&str>) -> Option<String> {
    match unit {
        None => packs::concepts().get(concept).map(|cd| cd.canonical_unit.to_string()),
        Some(u) => {
            let u = u.trim();
            if let Some(alias) = packs::unit_alias_index().get(concept) {
                if let Some(canon) = alias.get(&u.to_lowercase()) {
                    return Some(canon.clone());
                }
            }
            if let Some(cd) = packs::concepts().get(concept) {
                if u.eq_ignore_ascii_case(cd.canonical_unit) {
                    return Some(cd.canonical_unit.to_string());
                }
            }
            Some(u.to_string())
        }
    }
}

static DRUG_TRAIL: Lazy<Regex> = Lazy::new(|| Regex::new(r"[.,;:]+$").unwrap());

pub fn normalize_drug(name: &str) -> String {
    let n = WS.replace_all(name.trim(), " ").to_lowercase();
    let n = DRUG_TRAIL.replace(&n, "").to_string();
    packs::medication_synonyms().get(&n).cloned().unwrap_or(n)
}

// --- dates ------------------------------------------------------------------

const MONTHS: &[&str] = &[
    "jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec",
];

fn month_num(name: &str) -> Option<i64> {
    let key: String = name.chars().take(3).collect::<String>().to_lowercase();
    MONTHS.iter().position(|m| *m == key).map(|i| i as i64 + 1)
}

static DMY: Lazy<Regex> = Lazy::new(|| Regex::new(r"\b(\d{1,2})\s+([A-Za-z]{3,9})\.?\s+(\d{4})\b").unwrap());
static MDY: Lazy<Regex> = Lazy::new(|| Regex::new(r"\b([A-Za-z]{3,9})\.?\s+(\d{1,2}),?\s+(\d{4})\b").unwrap());
static ISO: Lazy<Regex> = Lazy::new(|| Regex::new(r"\b(\d{4})-(\d{2})-(\d{2})\b").unwrap());
static DMY_NUM: Lazy<Regex> = Lazy::new(|| Regex::new(r"\b(\d{1,2})/(\d{1,2})/(\d{4})\b").unwrap());
static MY: Lazy<Regex> = Lazy::new(|| Regex::new(r"\b([A-Za-z]{3,9})\.?\s+(\d{4})\b").unwrap());

/// Return `(ISO date or None, precision)`.
pub fn parse_date(text: &str) -> (Option<String>, &'static str) {
    if let Some(m) = DMY.captures(text) {
        if let (Ok(d), Some(mon), Ok(y)) = (
            m[1].parse::<i64>(),
            month_num(&m[2]),
            m[3].parse::<i64>(),
        ) {
            if let Some(e) = date::to_epoch_day(y, mon, d) {
                return (Some(date::iso_from_epoch(e)), "day");
            }
        }
    }
    if let Some(m) = MDY.captures(text) {
        if let (Some(mon), Ok(d), Ok(y)) = (
            month_num(&m[1]),
            m[2].parse::<i64>(),
            m[3].parse::<i64>(),
        ) {
            if let Some(e) = date::to_epoch_day(y, mon, d) {
                return (Some(date::iso_from_epoch(e)), "day");
            }
        }
    }
    if let Some(m) = ISO.captures(text) {
        if let (Ok(y), Ok(mon), Ok(d)) = (
            m[1].parse::<i64>(),
            m[2].parse::<i64>(),
            m[3].parse::<i64>(),
        ) {
            if let Some(e) = date::to_epoch_day(y, mon, d) {
                return (Some(date::iso_from_epoch(e)), "day");
            }
        }
    }
    if let Some(m) = DMY_NUM.captures(text) {
        if let (Ok(d), Ok(mon), Ok(y)) = (
            m[1].parse::<i64>(),
            m[2].parse::<i64>(),
            m[3].parse::<i64>(),
        ) {
            if let Some(e) = date::to_epoch_day(y, mon, d) {
                return (Some(date::iso_from_epoch(e)), "day");
            }
        }
    }
    if let Some(m) = MY.captures(text) {
        if let (Some(mon), Ok(y)) = (month_num(&m[1]), m[2].parse::<i64>()) {
            if let Some(e) = date::to_epoch_day(y, mon, 1) {
                return (Some(date::iso_from_epoch(e)), "month");
            }
        }
    }
    (None, "unknown")
}

static DATE_LABEL: Lazy<Regex> = Lazy::new(|| {
    Regex::new(concat!(
        r"(?i)(?:report date|reported on|date of report|consultation date|",
        r"follow-?up date|list date|discharge date|admission date|compiled|",
        r"sample date|date)\s*[:\-]\s*([^\n]{4,40})"
    ))
    .unwrap()
});

/// Find a document/page date. `labelled` is true only when the date came from an
/// explicit label ('Report Date:') — an unlabelled fallback is the first date in
/// the text and must not override a real document date.
pub fn find_document_date(text: &str) -> (Option<String>, &'static str, bool) {
    let mut best: Option<(String, &'static str)> = None;
    for m in DATE_LABEL.captures_iter(text) {
        let (iso, prec) = parse_date(&m[1]);
        if let Some(iso) = iso {
            let label = m[0].to_lowercase();
            if best.is_none() || label.contains("discharge date") || label.contains("report date") {
                best = Some((iso, prec));
            }
        }
    }
    if let Some((iso, prec)) = best {
        return (Some(iso), prec, true);
    }
    let (iso, prec) = parse_date(text);
    (iso, prec, false)
}

pub fn confidence_band(value: f64) -> &'static str {
    if value >= 0.90 {
        "High"
    } else if value >= 0.75 {
        "Moderate"
    } else {
        "Low"
    }
}

/// Whole-word containment: `needle` bounded by non-alphanumeric on both sides.
fn contains_word(hay: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return false;
    }
    let hb = hay.as_bytes();
    let mut from = 0;
    while let Some(rel) = hay[from..].find(needle) {
        let start = from + rel;
        let end = start + needle.len();
        let before_ok = start == 0 || !is_word_byte(hb[start - 1]);
        let after_ok = end == hb.len() || !is_word_byte(hb[end]);
        if before_ok && after_ok {
            return true;
        }
        from = start + 1;
    }
    false
}

fn is_word_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_abbreviations_and_prose() {
        assert_eq!(normalize_concept("Hb").as_deref(), Some("hemoglobin"));
        assert_eq!(normalize_concept("PLT").as_deref(), Some("platelets"));
        let (c, _disp, method) = resolve_label("M.C.V.");
        assert_eq!(c.as_deref(), Some("mcv"));
        assert_eq!(method, EXACT);
    }

    #[test]
    fn unmapped_is_kept_not_dropped() {
        // An analyte outside the vocabulary is kept under an `unmapped:` slug.
        let (c, _d, method) = resolve_label("Haptoglobin level");
        assert!(c.as_deref().unwrap().starts_with("unmapped:"));
        assert_eq!(method, UNRECOGNISED);
    }

    #[test]
    fn dates() {
        assert_eq!(parse_date("18 January 2026").0.as_deref(), Some("2026-01-18"));
        assert_eq!(parse_date("2026-05-04").0.as_deref(), Some("2026-05-04"));
        let (iso, _p, labelled) = find_document_date("Report Date: 4 May 2026\n...");
        assert_eq!(iso.as_deref(), Some("2026-05-04"));
        assert!(labelled);
    }
}
