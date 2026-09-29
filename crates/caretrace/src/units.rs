//! A generated UCUM-shaped unit grammar.
//!
//! The extractor finds measurements by recognising the UNIT first, then the
//! adjacent number, then the label — which is what makes extraction independent
//! of any laboratory's page layout. The unit vocabulary is GENERATED from an
//! atom table crossed with the SI prefix table and combined by UCUM's operators
//! (`/`, `.`, `^`), not hand-listed per report format.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use once_cell::sync::Lazy;
use regex::Regex;

use crate::text::{char_at, char_before};

// --- prefix + atom tables ---------------------------------------------------

const PREFIXES: &[(&str, i32)] = &[
    ("Y", 24), ("Z", 21), ("E", 18), ("P", 15), ("T", 12), ("G", 9), ("M", 6),
    ("k", 3), ("h", 2), ("da", 1), ("d", -1), ("c", -2), ("m", -3), ("u", -6),
    ("µ", -6), ("μ", -6), ("n", -9), ("p", -12), ("f", -15), ("a", -18),
    ("z", -21), ("y", -24),
];

const METRIC_ATOMS: &[(&str, &str)] = &[
    ("g", "mass"), ("m", "length"), ("s", "time"), ("L", "volume"), ("l", "volume"),
    ("mol", "amount"), ("eq", "amount"), ("osm", "amount"), ("K", "temperature"),
    ("Pa", "pressure"), ("Hz", "frequency"), ("U", "catalytic"), ("kat", "catalytic"),
    ("cal", "energy"), ("J", "energy"), ("V", "electric"), ("A", "electric"),
    ("Ohm", "electric"), ("Sv", "dose"), ("Gy", "dose"), ("Bq", "radioactivity"),
    ("t", "mass"), ("bar", "pressure"),
];

const NON_METRIC_ATOMS: &[(&str, &str)] = &[
    ("%", "fraction"), ("min", "time"), ("h", "time"), ("d", "time"), ("wk", "time"),
    ("mo", "time"), ("a", "time"), ("degC", "temperature"), ("Cel", "temperature"),
    ("[degF]", "temperature"), ("mm[Hg]", "pressure"), ("[iU]", "arbitrary"),
    ("[IU]", "arbitrary"), ("10*", "count"), ("10^", "count"), ("1", "unity"),
    ("[pH]", "acidity"), ("cm[H2O]", "pressure"),
    ("IU", "arbitrary"), ("iU", "arbitrary"), ("mIU", "arbitrary"),
    ("kIU", "arbitrary"), ("uIU", "arbitrary"), ("µIU", "arbitrary"),
    ("mU", "arbitrary"), ("kU", "arbitrary"), ("uU", "arbitrary"),
    ("Eq", "amount"), ("mEq", "amount"), ("uEq", "amount"), ("µEq", "amount"),
    ("nEq", "amount"), ("Osm", "amount"), ("mOsm", "amount"),
];

const NEVER_UNITS: &[&str] = &["am", "pm", "AM", "PM", "Am", "Pm"];

const COUNT_MANTISSA: &str = r"(?:[xX×]\s?)?10\s?[\*\^]\s?-?\d{1,2}";

fn atom_forms() -> &'static HashMap<String, String> {
    static F: OnceLock<HashMap<String, String>> = OnceLock::new();
    F.get_or_init(|| {
        let mut out: HashMap<String, String> = HashMap::new();
        for (atom, dim) in METRIC_ATOMS {
            out.entry(atom.to_string()).or_insert_with(|| dim.to_string());
            for (pfx, _) in PREFIXES {
                out.entry(format!("{pfx}{atom}"))
                    .or_insert_with(|| dim.to_string());
            }
        }
        for (atom, dim) in NON_METRIC_ATOMS {
            out.insert(atom.to_string(), dim.to_string());
        }
        out
    })
}

/// Runtime-registered non-UCUM spellings contributed by packs (`bpm` -> `/min`).
fn aliases() -> &'static Mutex<HashMap<String, String>> {
    static A: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();
    A.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Teach the grammar a non-UCUM spelling used by real reports.
pub fn register_alias(surface: &str, ucum: &str) {
    let key = surface.trim().to_lowercase().replace(' ', "");
    if key.is_empty() {
        return;
    }
    aliases()
        .lock()
        .unwrap()
        .entry(key)
        .or_insert_with(|| ucum.to_string());
}

// --- compiled patterns ------------------------------------------------------

static UNIT_RX: Lazy<Regex> = Lazy::new(|| {
    // Atoms longest-first so 'mmol' wins over 'mol', 'dL' over 'L'.
    let mut atoms: Vec<&String> = atom_forms().keys().collect();
    atoms.sort_by_key(|a| std::cmp::Reverse(a.len()));
    let alt = atoms
        .iter()
        .map(|a| regex::escape(a))
        .collect::<Vec<_>>()
        .join("|");
    let term = format!(r"(?:{COUNT_MANTISSA}|{alt})(?:\s?[\*\^]\s?-?\d{{1,2}})?");
    // A UCUM term combined by '/' or '.'. Boundaries are enforced in code.
    Regex::new(&format!(r"{term}(?:\s?[/.]\s?{term}){{0,3}}")).unwrap()
});

static COUNT_MANTISSA_RX: Lazy<Regex> =
    Lazy::new(|| Regex::new(COUNT_MANTISSA).unwrap());
static COUNT_MANTISSA_FULL: Lazy<Regex> =
    Lazy::new(|| Regex::new(&format!(r"^(?:{COUNT_MANTISSA})$")).unwrap());
static NUMBER_RX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"-?\d{1,7}(?:[.,]\d{1,4})?").unwrap());
static BARE_NUM_FULL: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^-?\d+(?:[.,]\d+)?$").unwrap());
static CLOCK_RX: Lazy<Regex> = Lazy::new(|| Regex::new(r"\d{1,2}:\d{2}").unwrap());
static DATE_LIKE_RX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"\d{1,4}[-/]\d{1,2}[-/]\d{1,4}").unwrap());

static CLINICAL_HINT_RX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(concat!(
        r"^(?:g/d?[Ll]|mg/d?[Ll]|µ?u?g/d?[Ll]|ng/m?[Ll]|pg|fL|fl|mmol/[Ll]|",
        r"µ?umol/[Ll]|mEq/[Ll]|m?[Ii][Uu]/[Ll]|%|10[\*\^]\d+/[Ll]|",
        r"mm\[?Hg\]?|U/[Ll]|k?U/m?[Ll]|s|ms)"
    ))
    .unwrap()
});

const AMBIGUOUS_ALONE: &[&str] = &[
    "a", "d", "h", "m", "s", "t", "1", "A", "K", "M", "P", "T", "U", "V", "min",
    "mo", "wk", "da", "cal", "pa", "Pa", "dam", "as", "at", "in", "is", "no",
    "on", "or", "to", "kat", "eq", "yd", "am", "pm",
];

// --- canonicalisation + dimension -------------------------------------------

/// Normalise a surface spelling toward UCUM without discarding meaning.
pub fn canonical(unit: &str) -> String {
    let mut u = unit.trim().replace(' ', "");
    if let Some(a) = aliases().lock().unwrap().get(&u.to_lowercase()) {
        return a.clone();
    }
    u = u.replace(['µ', 'μ'], "u");
    // strip a leading x/X/×
    static LEAD_X: Lazy<Regex> = Lazy::new(|| Regex::new(r"^[xX×]").unwrap());
    u = LEAD_X.replace(&u, "").into_owned();
    u = u.replace("10^", "10*");
    static GM: Lazy<Regex> = Lazy::new(|| Regex::new(r"gm/").unwrap());
    u = GM.replace_all(&u, "g/").into_owned();
    // Reports capitalise UCUM's lowercase 'eq'/'osm'. Case-fold, keeping the
    // grammar generated rather than alias-listed.
    static EQ_MID: Lazy<Regex> = Lazy::new(|| Regex::new(r"([a-zA-Z])Eq\b").unwrap());
    u = EQ_MID.replace_all(&u, "${1}eq").into_owned();
    static EQ_START: Lazy<Regex> = Lazy::new(|| Regex::new(r"^Eq\b").unwrap());
    u = EQ_START.replace(&u, "eq").into_owned();
    static OSM_MID: Lazy<Regex> = Lazy::new(|| Regex::new(r"([a-zA-Z])Osm\b").unwrap());
    u = OSM_MID.replace_all(&u, "${1}osm").into_owned();
    static OSM_START: Lazy<Regex> = Lazy::new(|| Regex::new(r"^Osm\b").unwrap());
    u = OSM_START.replace(&u, "osm").into_owned();
    // Uppercase the litre atom only (mg must not become MG). Lookbehind/ahead
    // are unavailable, so walk the string and fix a standalone 'l'.
    u = uppercase_litre(&u);
    u
}

/// Uppercase an 'l' that denotes a litre: standalone, or after a metric prefix
/// (d/c/m/u/n/p) with no adjacent letter (standalone and prefixed litre atom).
fn uppercase_litre(u: &str) -> String {
    let bytes: Vec<char> = u.chars().collect();
    let mut out = String::with_capacity(u.len());
    for (i, &c) in bytes.iter().enumerate() {
        if c == 'l' {
            let prev = if i > 0 { Some(bytes[i - 1]) } else { None };
            let next = bytes.get(i + 1).copied();
            let next_is_alpha = next.map(|n| n.is_ascii_alphabetic()).unwrap_or(false);
            let prev_is_alpha = prev.map(|p| p.is_ascii_alphabetic()).unwrap_or(false);
            if !next_is_alpha {
                let standalone = !prev_is_alpha;
                let after_prefix = matches!(prev, Some('d' | 'c' | 'm' | 'u' | 'n' | 'p'));
                if standalone || after_prefix {
                    out.push('L');
                    continue;
                }
            }
        }
        out.push(c);
    }
    out
}

fn strip_exp(head: &str) -> String {
    static EXP: Lazy<Regex> = Lazy::new(|| Regex::new(r"[\*\^]\s?-?\d+$").unwrap());
    EXP.replace(head, "").into_owned()
}

/// Quantity kind of a composite, or `None` if the grammar rejects it.
pub fn dimension(unit: &str) -> Option<String> {
    let u = canonical(unit);
    if COUNT_MANTISSA_FULL.is_match(&u) {
        return Some("count".into());
    }
    let forms = atom_forms();
    if let Some(rest) = u.strip_prefix('/') {
        let first = rest.split(['/', '.']).next().unwrap_or("");
        let tail_head = strip_exp(first);
        let tail_dim = forms.get(&tail_head);
        if tail_dim.is_none() && !matches!(tail_head.as_str(), "L" | "dL" | "mL") {
            return None;
        }
        return Some(format!(
            "count_per_{}",
            tail_dim.map(|s| s.as_str()).unwrap_or("unit")
        ));
    }
    let head0 = u.split(['/', '.']).next().unwrap_or("");
    let head = strip_exp(head0);
    if COUNT_MANTISSA_FULL.is_match(&head) {
        return Some("count".into());
    }
    let dim = if head == "10" && COUNT_MANTISSA_RX.find(&u).map(|m| m.start()) == Some(0) {
        Some("count".to_string())
    } else {
        forms.get(&head).cloned()
    };
    let dim = dim?;
    if let Some((_, tail)) = u.split_once('/') {
        let tail_first = tail.split(['/', '.']).next().unwrap_or("");
        let tail_head = strip_exp(tail_first);
        let tail_dim = forms.get(&tail_head);
        if matches!(tail_dim.map(|s| s.as_str()), Some("volume") | Some("time"))
            || matches!(tail_head.as_str(), "L" | "dL" | "mL")
        {
            return Some(format!(
                "{dim}_per_{}",
                tail_dim.map(|s| s.as_str()).unwrap_or("unit")
            ));
        }
    }
    Some(dim)
}

/// Whether a grammar match should be treated as a real clinical unit.
pub fn is_plausible_unit(unit: &str, require_hint: bool) -> bool {
    let raw = unit.trim();
    if raw.is_empty() || AMBIGUOUS_ALONE.contains(&raw) || NEVER_UNITS.contains(&raw) {
        return false;
    }
    static PLAIN_NUM: Lazy<Regex> = Lazy::new(|| Regex::new(r"^-?\d+(?:[.,]\d+)?$").unwrap());
    if PLAIN_NUM.is_match(raw) {
        return false;
    }
    if dimension(raw).is_none() && COUNT_MANTISSA_RX.find(raw).map(|m| m.start()) != Some(0) {
        return false;
    }
    if require_hint {
        return CLINICAL_HINT_RX.is_match(&canonical(raw));
    }
    true
}

/// `(start, end, surface)` byte spans of every plausible unit token.
pub fn find_units(text: &str, require_hint: bool) -> Vec<(usize, usize, String)> {
    let mut out = Vec::new();
    for m in UNIT_RX.find_iter(text) {
        // Enforce token boundaries (this regex engine has no lookaround):
        //   (?<![A-Za-z0-9_]) ... (?![A-Za-z0-9_%])
        if let Some(c) = char_before(text, m.start()) {
            if c.is_alphanumeric() || c == '_' {
                continue;
            }
        }
        if let Some(c) = char_at(text, m.end()) {
            if c.is_alphanumeric() || c == '_' || c == '%' {
                continue;
            }
        }
        let u = m.as_str().to_string();
        if is_plausible_unit(&u, require_hint) {
            out.push((m.start(), m.end(), u));
        }
    }
    out
}

/// One located measurement, found unit-first.
#[derive(Clone, Debug)]
pub struct RawMeasure {
    pub value: f64,
    pub unit: String,
    pub unit_canonical: String,
    pub dimension: Option<String>,
    pub value_span: (usize, usize),
    pub unit_span: (usize, usize),
}

/// Every `(value, unit)` pair in `text`, found unit-first. Layout-blind: no
/// analyte label is involved.
pub fn find_measurements(text: &str, max_gap: usize) -> Vec<RawMeasure> {
    let mut blocked: Vec<(usize, usize)> = Vec::new();
    for m in CLOCK_RX.find_iter(text) {
        blocked.push((m.start(), m.end()));
    }
    for m in DATE_LIKE_RX.find_iter(text) {
        blocked.push((m.start(), m.end()));
    }
    let is_blocked =
        |a: usize, b: usize| blocked.iter().any(|&(s, e)| a < e && s < b);

    let mut out = Vec::new();
    for (u_start, u_end, surface) in find_units(text, false) {
        if is_blocked(u_start, u_end) {
            continue;
        }
        let left = &text[..u_start];
        let trimmed_len = left.trim_end_matches([' ', '\t']).len();
        let gap = left.len() - trimmed_len;
        if gap > max_gap {
            continue;
        }
        let left_stripped = &left[..trimmed_len];
        // The LAST number immediately before the unit.
        let last = NUMBER_RX.find_iter(left_stripped).last();
        let m = match last {
            Some(m) if m.end() == left_stripped.len() => m,
            _ => continue,
        };
        if is_blocked(m.start(), m.end()) {
            continue;
        }
        let value: f64 = match m.as_str().replace(',', ".").parse() {
            Ok(v) => v,
            Err(_) => continue,
        };
        out.push(RawMeasure {
            value,
            unit: surface.clone(),
            unit_canonical: canonical(&surface),
            dimension: dimension(&surface),
            value_span: (m.start(), m.end()),
            unit_span: (u_start, u_end),
        });
    }
    out
}

/// Whether a text is a bare number (used by the cellwise extractor).
pub fn is_bare_number(text: &str) -> bool {
    BARE_NUM_FULL.is_match(text.trim())
}

/// Whether two units measure the same kind of thing.
pub fn same_dimension(a: Option<&str>, b: Option<&str>) -> bool {
    let (a, b) = match (a, b) {
        (Some(a), Some(b)) => (a, b),
        _ => return false,
    };
    if canonical(a) == canonical(b) {
        return true;
    }
    match (dimension(a), dimension(b)) {
        (Some(da), Some(db)) => da == db,
        _ => false,
    }
}

const CONVERSIONS: &[((&str, &str), f64)] = &[
    (("g/dL", "g/L"), 10.0),
    (("g/L", "g/dL"), 0.1),
    (("mg/dL", "mg/L"), 10.0),
    (("mg/L", "mg/dL"), 0.1),
    (("ng/mL", "ug/L"), 1.0),
    (("ug/L", "ng/mL"), 1.0),
    (("10*9/L", "10*3/uL"), 1.0),
    (("10*3/uL", "10*9/L"), 1.0),
    (("10*12/L", "10*6/uL"), 1.0),
    (("10*6/uL", "10*12/L"), 1.0),
];

/// Exact scale conversion, or `None` when no published factor applies.
pub fn convert(value: f64, frm: &str, to: &str) -> Option<f64> {
    let (f, t) = (canonical(frm), canonical(to));
    if f == t {
        return Some(value);
    }
    CONVERSIONS
        .iter()
        .find(|((a, b), _)| *a == f && *b == t)
        .map(|(_, factor)| value * factor)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dimensions() {
        assert_eq!(dimension("g/dL").as_deref(), Some("mass_per_volume"));
        assert_eq!(dimension("fL").as_deref(), Some("volume"));
        assert_eq!(dimension("pg").as_deref(), Some("mass"));
        assert_eq!(dimension("%").as_deref(), Some("fraction"));
        assert_eq!(dimension("10*9/L").as_deref(), Some("count_per_volume"));
    }

    #[test]
    fn canonicalises() {
        assert_eq!(canonical("gm/dl"), "g/dL");
        assert_eq!(canonical("x10^9/l"), "10*9/L");
        assert_eq!(canonical("G/DL"), "G/DL"); // only the litre atom is cased
    }

    #[test]
    fn finds_measurement_unit_first() {
        let t = "Hemoglobin 9.2 g/dL (ref 13.0-17.0)";
        let ms = find_measurements(t, 2);
        assert!(ms.iter().any(|m| (m.value - 9.2).abs() < 1e-9
            && m.unit_canonical == "g/dL"));
    }

    #[test]
    fn clock_and_date_are_not_measurements() {
        assert!(find_measurements("collected at 09:30", 2).is_empty());
        assert!(find_measurements("dated 2026-01-18 today", 2).is_empty());
    }
}
