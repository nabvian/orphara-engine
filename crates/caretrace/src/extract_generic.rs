//! Layout-agnostic extraction.
//!
//! No template, vendor or page geometry is encoded. Three layout-independent
//! signals: (1) measurements found unit-first by the UCUM grammar and labelled
//! by POSITION; (2) labels associated geometrically (row / column / inline),
//! with reference ranges never read as values; (3) narrative sections segmented
//! by heading SHAPE, not heading text.

use once_cell::sync::Lazy;
use regex::Regex;

use crate::text::line_bounds;
use crate::units;

/// One numeric measurement with its layout-derived label.
#[derive(Clone, Debug)]
pub struct Measurement {
    pub label: String,
    pub value: f64,
    pub unit: String,
    pub unit_canonical: String,
    pub dimension: Option<String>,
    pub line: String,
    pub line_start: usize,
    pub span: (usize, usize),
    pub reference_range: Option<String>,
    pub flag: Option<String>,
    pub layout: &'static str,
    pub confidence: f64,
    pub row_date: Option<String>,
}

impl Measurement {
    /// Whether the unit denotes a plain amount (a dose/quantity) rather than a
    /// concentration — used to keep a prescription's '100 mg' from being read
    /// as an observation of the patient.
    pub fn amount_like(&self) -> bool {
        let d = self.dimension.clone().unwrap_or_default();
        !d.contains("_per_")
            && matches!(
                d.as_str(),
                "mass" | "volume" | "amount" | "arbitrary" | "count" | "catalytic"
            )
    }
}

/// A narrative section located by heading shape.
#[derive(Clone, Debug)]
pub struct Section {
    pub role: &'static str,
    pub heading: String,
    pub text: String,
    pub start: usize,
    pub end: usize,
}

static REF_RANGE_RX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(concat!(
        r"(?i)(?P<range>\(?\s*(?:ref(?:erence)?(?:\s*(?:range|interval|value)s?)?\s*[:=]?\s*)?",
        r"(?:-?\d+(?:\.\d+)?\s*(?:-|--|–|—|to)\s*-?\d+(?:\.\d+)?",
        r"|[<>≤≥]\s*-?\d+(?:\.\d+)?)\s*\)?)"
    ))
    .unwrap()
});

// Flag body without the surrounding non-letter boundaries (checked in code).
static FLAG_BODY_RX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(H{1,2}|L{1,2}|HIGH|LOW|ABN(?:ORMAL)?|CRIT(?:ICAL)?|\*)").unwrap()
});

static LABEL_TRIM: Lazy<Regex> = Lazy::new(|| Regex::new(r"^[\s\-*>•\u{2022}\|]+|[\s:\-=\.]+$").unwrap());
static SERIAL: Lazy<Regex> = Lazy::new(|| Regex::new(r"^\d{1,3}[).]\s*").unwrap());
static MULTISPACE: Lazy<Regex> = Lazy::new(|| Regex::new(r"\s{2,}").unwrap());
static CELL_SPLIT: Lazy<Regex> = Lazy::new(|| Regex::new(r"\s{2,}|\t+|\s*\|\s*").unwrap());
static DATE_CELL: Lazy<Regex> = Lazy::new(|| {
    Regex::new(concat!(
        r"^(?:\d{1,2}[-/\s](?:\d{1,2}|[A-Za-z]{3,9})\.?[-/\s]\d{2,4}",
        r"|\d{4}-\d{2}-\d{2}",
        r"|[A-Za-z]{3,9}\.?\s+\d{1,2},?\s+\d{4})$"
    ))
    .unwrap()
});

const FURNITURE: &[&str] = &[
    "page", "report", "report no", "report number", "reg no", "mrn", "uhid",
    "date", "dated", "sample", "specimen", "collected", "received", "reported",
    "age", "sex", "gender", "dob", "patient", "name", "ref by", "referred by",
    "doctor", "physician", "consultant", "technician", "lab", "laboratory",
    "address", "phone", "tel", "email", "invoice", "bill", "barcode",
    "accession", "case ref", "case", "visit", "encounter", "admission",
    "discharge", "start date", "stop date", "list date", "report date",
    "printed", "signature", "verified by", "authorised by", "authorized by",
    "method", "instrument", "analyser", "analyzer", "units", "unit",
    "reference", "reference range", "normal range", "range", "result",
    "results", "value", "flag", "comment", "comments", "interpretation",
    "test", "tests", "investigation", "investigations", "parameter",
];

const FURNITURE_TOKENS: &[&str] = &[
    "page", "mrn", "uhid", "reg", "barcode", "accession", "invoice", "bill",
    "signature", "technician", "physician", "consultant", "address", "phone",
    "tel", "email", "printed", "analyser", "analyzer", "instrument",
];

const FURNITURE_JOINERS: &[&str] = &["by", "on", "at", "no", "number", "id", "of"];

static NON_LETTER: Lazy<Regex> = Lazy::new(|| Regex::new(r"[^a-z ]").unwrap());

pub fn clean_label(raw: &str) -> String {
    let lab = LABEL_TRIM.replace_all(raw, "");
    let lab = SERIAL.replace(&lab, "");
    MULTISPACE
        .replace_all(&lab, " ")
        .trim_matches(|c| " :-=.\t|".contains(c))
        .to_string()
}

pub fn is_furniture(label: &str) -> bool {
    let lowered = label.to_lowercase();
    let key = NON_LETTER.replace_all(&lowered, "");
    let key = MULTISPACE.replace_all(key.trim(), " ").to_string();
    if key.is_empty() || FURNITURE.contains(&key.as_str()) {
        return true;
    }
    let toks: Vec<&str> = key.split_whitespace().collect();
    if !toks.is_empty()
        && toks.iter().all(|t| {
            FURNITURE.contains(t) || FURNITURE_TOKENS.contains(t) || FURNITURE_JOINERS.contains(t)
        })
    {
        return true;
    }
    if toks.iter().any(|t| FURNITURE_TOKENS.contains(t)) {
        return true;
    }
    if key.len() < 2 || (key.len() == 1 && key.chars().all(|c| c.is_ascii_alphabetic())) {
        return true;
    }
    false
}

fn is_date_cell(text: &str) -> bool {
    DATE_CELL.is_match(text.trim().trim_end_matches(','))
}

/// `(byte_offset_within_line, trimmed_text)` cells split on 2+ spaces, tabs or
/// pipes.
fn cells(line: &str) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    let mut pos = 0usize;
    for part in CELL_SPLIT.split(line) {
        // Locate this piece from the current cursor, from the current cursor.
        if let Some(rel) = line[pos..].find(part) {
            let idx = pos + rel;
            if !part.trim().is_empty() {
                out.push((idx, part.trim().to_string()));
            }
            pos = idx + part.len();
        }
    }
    out
}

/// The header cell whose column best aligns with `col`, within `tol`.
fn header_label_for(header: &str, col: usize, tol: usize) -> Option<String> {
    let cs = cells(header);
    if cs.is_empty() {
        return None;
    }
    let mut best: Option<(usize, &str)> = None;
    for (start, txt) in &cs {
        let d = start.abs_diff(col);
        if best.map(|(bd, _)| d < bd).unwrap_or(true) {
            best = Some((d, txt));
        }
    }
    best.filter(|(d, _)| *d <= tol).map(|(_, t)| t.to_string())
}

fn parse_float(s: &str) -> Option<f64> {
    s.replace(',', ".").parse().ok()
}

pub fn extract_cellwise(page_text: &str) -> Vec<Measurement> {
    let mut out = Vec::new();
    let mut pos = 0usize;
    for line in page_text.split('\n') {
        let ls = pos;
        pos += line.len() + 1;
        let cs = cells(line);
        if cs.len() < 3 {
            continue;
        }
        let unit_cell = cs
            .iter()
            .find(|(_, c)| units::is_plausible_unit(c, false) && units::dimension(c).is_some());
        let unit_cell = match unit_cell {
            Some(u) => u,
            None => continue,
        };
        let num_cells: Vec<&(usize, String)> =
            cs.iter().filter(|(_, c)| units::is_bare_number(c)).collect();
        if num_cells.is_empty() {
            continue;
        }
        let before: Vec<&(usize, String)> = num_cells
            .iter()
            .copied()
            .filter(|(o, _)| *o < unit_cell.0)
            .collect();
        let (v_off, v_txt) = match before.last() {
            Some(x) => (x.0, x.1.clone()),
            None => continue,
        };
        let text_cells_all: Vec<&(usize, String)> = cs
            .iter()
            .filter(|(o, c)| !units::is_bare_number(c) && *o < v_off && !units::is_plausible_unit(c, false))
            .collect();
        let row_date = text_cells_all
            .iter()
            .find(|(_, c)| is_date_cell(c))
            .map(|(_, c)| c.clone());
        let text_cells: Vec<&&(usize, String)> =
            text_cells_all.iter().filter(|(_, c)| !is_date_cell(c)).collect();
        if text_cells.is_empty() {
            continue;
        }
        let label = clean_label(&text_cells[0].1);
        if label.is_empty() || is_furniture(&label) {
            continue;
        }
        let after_unit = unit_cell.0 + unit_cell.1.len();
        let rng = REF_RANGE_RX
            .captures(&line[after_unit.min(line.len())..])
            .and_then(|c| c.name("range"))
            .map(|m| m.as_str().trim().to_string());
        let value = match parse_float(&v_txt) {
            Some(v) => v,
            None => continue,
        };
        out.push(Measurement {
            label,
            value,
            unit: unit_cell.1.clone(),
            unit_canonical: units::canonical(&unit_cell.1),
            dimension: units::dimension(&unit_cell.1),
            line: line.to_string(),
            line_start: ls,
            span: (ls + v_off, ls + after_unit),
            reference_range: rng,
            flag: None,
            layout: "COLUMN",
            confidence: 0.88,
            row_date,
        });
    }
    out
}

pub fn extract_measurements(page_text: &str) -> Vec<Measurement> {
    let mut out: Vec<Measurement> = Vec::new();
    let mut seen: std::collections::HashSet<(usize, usize)> = std::collections::HashSet::new();

    for m in units::find_measurements(page_text, 2) {
        let (v_start, _v_end) = m.value_span;
        let (_u_start, u_end) = m.unit_span;
        let (ls, le) = line_bounds(page_text, v_start);
        let line = &page_text[ls..le];
        let rel_v = v_start - ls;

        let ranges: Vec<(usize, usize)> = REF_RANGE_RX
            .captures_iter(line)
            .filter_map(|c| c.name("range").map(|mm| (mm.start(), mm.end())))
            .collect();
        if ranges.iter().any(|&(a, b)| a <= rel_v && rel_v < b) {
            continue;
        }
        if seen.contains(&(v_start, u_end)) {
            continue;
        }

        let left = &line[..rel_v];
        // Row-date detection: first cell before a 2+-space gap that is a date.
        let mut row_date: Option<String> = None;
        let line_cells = cells(line);
        if line_cells.len() >= 2 && line_cells[0].1.chars().count() >= 2 && is_date_cell(&line_cells[0].1) {
            row_date = Some(line_cells[0].1.clone());
        }

        // Cut point: after the last delimiter or preceding unit in `left`.
        let mut cut = 0usize;
        for d in ["|", ";", ",", "\u{2022}", "  "] {
            if let Some(i) = left.rfind(d) {
                cut = cut.max(i + d.len());
            }
        }
        for pm in units::find_units(left, false) {
            cut = cut.max(pm.1);
        }
        let mut label = {
            let a = clean_label(&left[cut.min(left.len())..]);
            if a.is_empty() { clean_label(left) } else { a }
        };
        let mut layout = "ROW";

        if is_date_cell(&label) {
            row_date = row_date.or(Some(label.clone()));
            let lcells = cells(left);
            label = lcells.get(1).map(|(_, t)| clean_label(t)).unwrap_or_default();
        }

        // No usable label on this line: try a column header above.
        if label.is_empty() || is_furniture(&label) {
            let mut hdr_line: Option<String> = None;
            let mut probe = ls;
            for _ in 0..4 {
                if probe == 0 {
                    break;
                }
                let (p_ls, p_le) = line_bounds(page_text, probe - 1);
                let cand = &page_text[p_ls..p_le];
                probe = p_ls;
                if cand.trim().is_empty() {
                    continue;
                }
                if !units::find_measurements(cand, 2).is_empty() {
                    continue; // another data row, not a header
                }
                hdr_line = Some(cand.to_string());
                break;
            }
            if let Some(h) = hdr_line {
                if let Some(header) = header_label_for(&h, rel_v, 6) {
                    if is_furniture(&clean_label(&header)) {
                        // header names the value column ('Result'); analyte is
                        // the leftmost cell of the data row.
                        if let Some((_, first)) = cells(line).first() {
                            label = clean_label(first);
                            layout = "COLUMN";
                        }
                    } else {
                        label = clean_label(&header);
                        layout = "COLUMN";
                    }
                }
            }
        }

        // Still nothing usable: keep it with the best available label.
        if label.is_empty() || is_furniture(&label) {
            let cand = cells(line)
                .first()
                .map(|(_, t)| clean_label(t))
                .unwrap_or_default();
            if !cand.is_empty() && !is_furniture(&cand) {
                label = cand;
            } else {
                label = String::new();
                layout = "INLINE";
            }
        }

        let rng = ranges.first().map(|&(a, b)| line[a..b].trim().to_string());

        // Flag in the short tail after the measurement.
        let tail_start = rel_v + (u_end - v_start);
        let tail = &line[tail_start.min(line.len())..];
        let tail14: String = tail.chars().take(14).collect();
        let flag = FLAG_BODY_RX.captures(&tail14).and_then(|c| {
            let mm = c.get(1).unwrap();
            let before_ok = tail14[..mm.start()]
                .chars()
                .next_back()
                .map(|ch| !ch.is_ascii_alphabetic())
                .unwrap_or(true);
            let after_ok = tail14[mm.end()..]
                .chars()
                .next()
                .map(|ch| !ch.is_ascii_alphabetic())
                .unwrap_or(true);
            if before_ok && after_ok {
                Some(mm.as_str().to_string())
            } else {
                None
            }
        });

        let mut conf: f64 = if !label.is_empty() { 0.92 } else { 0.55 };
        if layout == "COLUMN" {
            conf = 0.85;
        }
        if rng.is_some() {
            conf = (conf + 0.04).min(0.97);
        }

        seen.insert((v_start, u_end));
        out.push(Measurement {
            label,
            value: m.value,
            unit: m.unit.clone(),
            unit_canonical: m.unit_canonical.clone(),
            dimension: m.dimension.clone(),
            line: line.to_string(),
            line_start: ls,
            span: (v_start, u_end),
            reference_range: rng,
            flag,
            layout,
            confidence: conf,
            row_date,
        });
    }

    // Second pass for units in their own column.
    let covered: std::collections::HashSet<usize> = out.iter().map(|m| m.line_start).collect();
    for m in extract_cellwise(page_text) {
        if !covered.contains(&m.line_start) {
            out.push(m);
        }
    }
    out.sort_by_key(|x| x.span.0);
    out
}

// --- narrative sections -----------------------------------------------------

const ROLE_CUES: &[(&str, &[&str])] = &[
    ("INDICATION", &["indication", "clinical history", "history", "reason", "clinical details", "referred for", "complaint"]),
    ("TECHNIQUE", &["technique", "protocol", "procedure", "method", "sequences", "contrast", "acquisition", "study"]),
    ("COMPARISON", &["comparison", "prior study", "previous study"]),
    ("FINDINGS", &["findings", "observation", "observations", "description", "report", "examination", "gross", "microscopy"]),
    ("IMPRESSION", &["impression", "conclusion", "opinion", "summary", "interpretation", "diagnosis", "assessment"]),
    ("RECOMMENDATION", &["recommendation", "advice", "plan", "follow up", "follow-up", "suggestion"]),
];

const HEADING_MAX_WORDS: usize = 7;
const HEADING_MAX_CHARS: usize = 60;

fn is_heading(line: &str) -> bool {
    let s = line.trim();
    if s.is_empty() || s.chars().count() > HEADING_MAX_CHARS {
        return false;
    }
    let body = s.trim_end_matches(':').trim();
    if body.is_empty() || body.split_whitespace().count() > HEADING_MAX_WORDS {
        return false;
    }
    let first = body.chars().next().unwrap();
    if first.is_ascii_digit() && !s.ends_with(':') {
        return false;
    }
    if !units::find_measurements(s, 2).is_empty() {
        return false;
    }
    if s.ends_with('.') {
        return false;
    }
    let is_caps = body.to_uppercase() == body && body.chars().any(|c| c.is_alphabetic());
    let is_title = body
        .split_whitespace()
        .all(|w| w.chars().next().map(|c| c.is_uppercase() || !c.is_alphabetic()).unwrap_or(true));
    s.ends_with(':') || is_caps || is_title
}

fn role_for(heading: &str) -> &'static str {
    let h = heading.to_lowercase();
    let h = h.trim_matches(|c| c == ':' || c == ' ');
    for (role, cues) in ROLE_CUES {
        if cues.iter().any(|c| h.contains(c)) {
            return role;
        }
    }
    "OTHER"
}

pub fn extract_sections(page_text: &str) -> Vec<Section> {
    let mut lines: Vec<(usize, &str)> = Vec::new();
    let mut pos = 0usize;
    for ln in page_text.split('\n') {
        lines.push((pos, ln));
        pos += ln.len() + 1;
    }
    let heads: Vec<(usize, usize, &str)> = lines
        .iter()
        .enumerate()
        .filter(|(_, (_, ln))| is_heading(ln))
        .map(|(i, (off, ln))| (i, *off, *ln))
        .collect();

    let mut out = Vec::new();
    for (n, &(i, off, ln)) in heads.iter().enumerate() {
        let body_start = off + ln.len() + 1;
        let mut stop = page_text.len();
        for &(j, off2, _) in &heads[n + 1..] {
            if j > i {
                stop = off2;
                break;
            }
        }
        if body_start >= stop {
            continue;
        }
        let body = page_text[body_start..stop].trim_matches('\n');
        if body.trim().is_empty() || body.split_whitespace().count() < 2 {
            continue;
        }
        out.push(Section {
            role: role_for(ln),
            heading: ln.trim().trim_end_matches(':').to_string(),
            text: body.to_string(),
            start: body_start,
            end: stop,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn row_layout_measurement_is_labelled() {
        let ms = extract_measurements("Hemoglobin        9.2 g/dL   13.0-17.0\n");
        let hb = ms.iter().find(|m| (m.value - 9.2).abs() < 1e-9).unwrap();
        assert_eq!(hb.label.to_lowercase(), "hemoglobin");
        assert!(hb.reference_range.is_some());
    }

    #[test]
    fn reference_range_bounds_are_not_values() {
        // 13.0 and 17.0 are the printed range, not results.
        let ms = extract_measurements("Hemoglobin 9.2 g/dL (13.0 - 17.0)\n");
        assert_eq!(ms.iter().filter(|m| m.label.to_lowercase() == "hemoglobin").count(), 1);
    }

    #[test]
    fn sections_by_shape() {
        let t = "IMPRESSION:\nNormocytic anaemia, likely of chronic disease.\n";
        let secs = extract_sections(t);
        assert!(secs.iter().any(|s| s.role == "IMPRESSION"));
    }
}
