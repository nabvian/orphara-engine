//! Offline local-index provider + importers (the user-activated path).
//!
//! The user imports an OFFICIAL release they are entitled to — RxNorm (public
//! domain), LOINC (free account), SNOMED CT (licensed) — and it is indexed
//! in-memory for exact / synonym / approximate lookup. Nothing is fabricated:
//! codes come ONLY from the imported data, so an unconfigured provider returns
//! nothing and reports its licence state (never a guessed code).

use std::collections::HashMap;

use super::{Coding, LicenceRequirement, MatchKind, ProviderHealth, ProviderState, TerminologyProvider};

/// One vocabulary concept: a code, a preferred display, and synonyms.
#[derive(Clone, Debug)]
pub struct VocabEntry {
    pub code: String,
    pub display: String,
    pub synonyms: Vec<String>,
}

/// An in-memory vocabulary index built from an imported release.
#[derive(Clone, Debug, Default)]
pub struct LocalVocab {
    pub version: Option<String>,
    entries: Vec<VocabEntry>,
    /// norm(term) -> (entry index, is the preferred display).
    index: HashMap<String, (usize, bool)>,
}

impl LocalVocab {
    fn push(&mut self, code: String, display: String, synonyms: Vec<String>) {
        let idx = self.entries.len();
        self.index.entry(norm(&display)).or_insert((idx, true));
        for s in &synonyms {
            self.index.entry(norm(s)).or_insert((idx, false));
        }
        self.entries.push(VocabEntry { code, display, synonyms });
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// RxNorm `RXNCONSO.RRF` (pipe-delimited). English rows only; the preferred
    /// term is the ingredient name (TTY=IN) or `ISPREF=Y`, others are synonyms.
    /// Fields: RXCUI|LAT|TS|LUI|STT|SUI|ISPREF|RXAUI|SAUI|SCUI|SDUI|SAB|TTY|CODE|STR|SRL|SUPPRESS|CVF
    pub fn rxnorm_from_rrf(text: &str) -> Self {
        let mut by_cui: HashMap<String, (Option<String>, Vec<String>)> = HashMap::new();
        let mut order: Vec<String> = Vec::new();
        for line in text.lines() {
            let f: Vec<&str> = line.split('|').collect();
            if f.len() < 17 {
                continue;
            }
            let (rxcui, lat, ispref, tty, str_, suppress) = (f[0], f[1], f[6], f[12], f[14], f[16]);
            if lat != "ENG" || suppress == "O" || suppress == "E" || str_.is_empty() {
                continue;
            }
            let entry = by_cui.entry(rxcui.to_string()).or_insert_with(|| {
                order.push(rxcui.to_string());
                (None, Vec::new())
            });
            let preferred = tty == "IN" || tty == "PIN" || ispref == "Y";
            if preferred && entry.0.is_none() {
                entry.0 = Some(str_.to_string());
            } else {
                entry.1.push(str_.to_string());
            }
        }
        let mut v = LocalVocab::default();
        for cui in order {
            let (pref, syns) = by_cui.remove(&cui).unwrap();
            let display = pref.or_else(|| syns.first().cloned()).unwrap_or_default();
            let syns: Vec<String> = syns.into_iter().filter(|s| *s != display).collect();
            if !display.is_empty() {
                v.push(cui, display, syns);
            }
        }
        v
    }

    /// LOINC `Loinc.csv`. Uses the header to locate LOINC_NUM, LONG_COMMON_NAME,
    /// SHORTNAME and DisplayName; other columns are ignored.
    pub fn loinc_from_csv(text: &str) -> Self {
        let mut v = LocalVocab::default();
        let mut lines = text.lines();
        let header = match lines.next() {
            Some(h) => csv_row(h),
            None => return v,
        };
        let col = |name: &str| header.iter().position(|h| h.eq_ignore_ascii_case(name));
        let (c_num, c_long) = match (col("LOINC_NUM"), col("LONG_COMMON_NAME")) {
            (Some(a), Some(b)) => (a, b),
            _ => return v,
        };
        let c_short = col("SHORTNAME");
        let c_disp = col("DisplayName");
        for line in lines {
            let r = csv_row(line);
            let get = |i: Option<usize>| i.and_then(|i| r.get(i)).cloned().unwrap_or_default();
            let code = r.get(c_num).cloned().unwrap_or_default();
            let display = r.get(c_long).cloned().unwrap_or_default();
            if code.is_empty() || display.is_empty() {
                continue;
            }
            let syns: Vec<String> = [get(c_short), get(c_disp)]
                .into_iter()
                .filter(|s| !s.is_empty() && *s != display)
                .collect();
            v.push(code, display, syns);
        }
        v
    }

    /// SNOMED CT description file (RF2, tab-delimited). Active rows only; the FSN
    /// (typeId 900000000000003001) is the display, synonyms are synonyms.
    /// Fields: id, effectiveTime, active, moduleId, conceptId, languageCode, typeId, term, caseSignificanceId
    pub fn snomed_from_rf2(text: &str) -> Self {
        const FSN: &str = "900000000000003001";
        let mut by_concept: HashMap<String, (Option<String>, Vec<String>)> = HashMap::new();
        let mut order: Vec<String> = Vec::new();
        for line in text.lines() {
            let f: Vec<&str> = line.split('\t').collect();
            if f.len() < 8 || f[0] == "id" {
                continue;
            }
            let (active, concept_id, type_id, term) = (f[2], f[4], f[6], f[7]);
            if active != "1" || term.is_empty() {
                continue;
            }
            let e = by_concept.entry(concept_id.to_string()).or_insert_with(|| {
                order.push(concept_id.to_string());
                (None, Vec::new())
            });
            if type_id == FSN && e.0.is_none() {
                e.0 = Some(term.to_string());
            } else {
                e.1.push(term.to_string());
            }
        }
        let mut v = LocalVocab::default();
        for c in order {
            let (fsn, syns) = by_concept.remove(&c).unwrap();
            let display = fsn.or_else(|| syns.first().cloned()).unwrap_or_default();
            let syns: Vec<String> = syns.into_iter().filter(|s| *s != display).collect();
            if !display.is_empty() {
                v.push(c, display, syns);
            }
        }
        v
    }

    fn lookup(&self, system: &str, system_uri: &str, text: &str, limit: usize) -> Vec<Coding> {
        let q = norm(text);
        if q.is_empty() {
            return Vec::new();
        }
        if let Some(&(idx, is_pref)) = self.index.get(&q) {
            let e = &self.entries[idx];
            return vec![Coding {
                system: system.to_string(),
                system_uri: system_uri.to_string(),
                code: e.code.clone(),
                display: e.display.clone(),
                queried_text: text.to_string(),
                match_kind: if is_pref { MatchKind::Exact } else { MatchKind::Synonym },
                score: None,
                version: self.version.clone(),
            }];
        }
        // Approximate: token-overlap (Jaccard) over display + synonyms.
        let qtok = tokens(&q);
        if qtok.is_empty() {
            return Vec::new();
        }
        let mut scored: Vec<(f64, usize)> = Vec::new();
        for (i, e) in self.entries.iter().enumerate() {
            let mut best = 0.0f64;
            for term in std::iter::once(&e.display).chain(e.synonyms.iter()) {
                let s = jaccard(&qtok, &tokens(&norm(term)));
                if s > best {
                    best = s;
                }
            }
            if best >= 0.34 {
                scored.push((best, i));
            }
        }
        scored.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)));
        scored
            .into_iter()
            .take(limit)
            .map(|(s, i)| {
                let e = &self.entries[i];
                Coding {
                    system: system.to_string(),
                    system_uri: system_uri.to_string(),
                    code: e.code.clone(),
                    display: e.display.clone(),
                    queried_text: text.to_string(),
                    match_kind: MatchKind::Approximate,
                    score: Some(s),
                    version: self.version.clone(),
                }
            })
            .collect()
    }
}

/// A vocabulary provider backed by an imported local release.
pub struct LocalIndexProvider {
    key: String,
    label: String,
    system: String,
    system_uri: String,
    kinds: Vec<&'static str>,
    licence: LicenceRequirement,
    vocab: Option<LocalVocab>,
}

impl LocalIndexProvider {
    /// RxNorm (public domain — no licence). `None` vocab = not yet imported.
    pub fn rxnorm(vocab: Option<LocalVocab>) -> Self {
        LocalIndexProvider {
            key: "rxnorm-local".into(),
            label: "RxNorm (local release)".into(),
            system: "RxNorm".into(),
            system_uri: "http://www.nlm.nih.gov/research/umls/rxnorm".into(),
            kinds: vec![super::KIND_MEDICATION],
            licence: LicenceRequirement {
                required: false,
                note: "RxNorm is public domain. Import an RXNCONSO.RRF release to activate.".into(),
                ..Default::default()
            },
            vocab,
        }
    }

    /// LOINC (free account required to download the release).
    pub fn loinc(vocab: Option<LocalVocab>) -> Self {
        LocalIndexProvider {
            key: "loinc-local".into(),
            label: "LOINC (local release)".into(),
            system: "LOINC".into(),
            system_uri: "http://loinc.org".into(),
            kinds: vec![super::KIND_OBSERVATION],
            licence: LicenceRequirement {
                required: true,
                licence_name: Some("LOINC licence (free account)".into()),
                obtain_url: Some("https://loinc.org/downloads/".into()),
                credential_fields: vec![],
                note: "LOINC is free of charge but requires accepting its licence. Import Loinc.csv to activate.".into(),
            },
            vocab,
        }
    }

    /// SNOMED CT (licence required; free in member territories for approved use).
    pub fn snomed(vocab: Option<LocalVocab>) -> Self {
        LocalIndexProvider {
            key: "snomed-local".into(),
            label: "SNOMED CT (local release)".into(),
            system: "SNOMED CT".into(),
            system_uri: "http://snomed.info/sct".into(),
            kinds: vec![super::KIND_DIAGNOSIS],
            licence: LicenceRequirement {
                required: true,
                licence_name: Some("SNOMED CT Affiliate Licence".into()),
                obtain_url: Some("https://www.snomed.org/get-snomed".into()),
                credential_fields: vec![],
                note: "SNOMED CT requires a licence (free in member territories for approved use). Import an RF2 description file to activate.".into(),
            },
            vocab,
        }
    }
}

impl TerminologyProvider for LocalIndexProvider {
    fn key(&self) -> &str {
        &self.key
    }
    fn system(&self) -> &str {
        &self.system
    }
    fn system_uri(&self) -> &str {
        &self.system_uri
    }
    fn licence(&self) -> &LicenceRequirement {
        &self.licence
    }
    fn codes_kinds(&self) -> &[&'static str] {
        &self.kinds
    }
    fn configured(&self) -> bool {
        self.vocab.as_ref().map(|v| !v.is_empty()).unwrap_or(false)
    }
    fn health(&self) -> ProviderHealth {
        let (state, detail) = match &self.vocab {
            Some(v) if !v.is_empty() => (
                ProviderState::Active,
                format!("{} concepts loaded.", v.len()),
            ),
            _ if self.licence.required => (
                ProviderState::Unlicensed,
                "No release imported. Codings are not produced; audit results are unaffected.".into(),
            ),
            _ => (
                ProviderState::Unknown,
                "No release imported yet.".into(),
            ),
        };
        ProviderHealth {
            key: self.key.clone(),
            label: self.label.clone(),
            state,
            detail,
            version: self.vocab.as_ref().and_then(|v| v.version.clone()),
        }
    }
    fn lookup(&self, text: &str, kind: &str, limit: usize) -> Vec<Coding> {
        if !self.kinds.contains(&kind) {
            return Vec::new();
        }
        match &self.vocab {
            Some(v) if !v.is_empty() => v.lookup(&self.system, &self.system_uri, text, limit),
            _ => Vec::new(),
        }
    }
}

// --- helpers ----------------------------------------------------------------

fn norm(s: &str) -> String {
    crate::normalize_ws(s).to_lowercase().trim_matches(|c| " .,:;".contains(c)).to_string()
}

fn tokens(s: &str) -> Vec<String> {
    s.split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|t| !t.is_empty())
        .map(|t| t.to_string())
        .collect()
}

fn jaccard(a: &[String], b: &[String]) -> f64 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let sa: std::collections::HashSet<&String> = a.iter().collect();
    let sb: std::collections::HashSet<&String> = b.iter().collect();
    let inter = sa.intersection(&sb).count() as f64;
    let uni = sa.union(&sb).count() as f64;
    inter / uni
}

/// Minimal CSV row split handling double-quoted fields with commas.
fn csv_row(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_q = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' => {
                if in_q && chars.peek() == Some(&'"') {
                    cur.push('"');
                    chars.next();
                } else {
                    in_q = !in_q;
                }
            }
            ',' if !in_q => {
                out.push(cur.trim().to_string());
                cur.clear();
            }
            _ => cur.push(c),
        }
    }
    out.push(cur.trim().to_string());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    // Synthetic fixtures (obviously fake codes — mechanics only, no real codes).
    #[test]
    fn rxnorm_rrf_import_and_lookup() {
        let rrf = "\
T100|ENG|P|L1|PF|S1|Y|A1|s|c|d|RXNORM|IN|T100|Ferrous sulfate|SRL|N|\n\
T100|ENG|S|L1|VC|S2|N|A2|s|c|d|RXNORM|SY|T100|Ferrous sulphate|SRL|N|\n";
        let v = LocalVocab::rxnorm_from_rrf(rrf);
        assert_eq!(v.len(), 1);
        let hit = v.lookup("RxNorm", "uri", "ferrous sulfate", 3);
        assert_eq!(hit[0].code, "T100");
        assert_eq!(hit[0].match_kind, MatchKind::Exact);
        let syn = v.lookup("RxNorm", "uri", "ferrous sulphate", 3);
        assert_eq!(syn[0].match_kind, MatchKind::Synonym);
    }

    #[test]
    fn loinc_csv_import_and_lookup() {
        let csv = "\
LOINC_NUM,COMPONENT,LONG_COMMON_NAME,SHORTNAME,DisplayName\n\
X-HB,Hemoglobin,\"Hemoglobin [Mass/volume] in Blood\",Hgb Bld,Hemoglobin\n";
        let v = LocalVocab::loinc_from_csv(csv);
        assert_eq!(v.len(), 1);
        let hit = v.lookup("LOINC", "uri", "Hemoglobin [Mass/volume] in Blood", 3);
        assert_eq!(hit[0].code, "X-HB");
        assert_eq!(hit[0].match_kind, MatchKind::Exact);
        // approximate on a partial phrase
        let approx = v.lookup("LOINC", "uri", "blood hemoglobin mass", 3);
        assert!(!approx.is_empty());
        assert_eq!(approx[0].match_kind, MatchKind::Approximate);
    }

    #[test]
    fn unconfigured_provider_returns_nothing_and_reports_state() {
        let p = LocalIndexProvider::loinc(None);
        assert!(!p.configured());
        assert_eq!(p.health().state, ProviderState::Unlicensed);
        assert!(p.lookup("Hemoglobin", super::super::KIND_OBSERVATION, 3).is_empty());
    }
}
