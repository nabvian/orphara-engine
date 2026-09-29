//! Local HTTP UI for the ORPHARA engine.
//!
//! Paste a doctor's note or upload reports and see the extraction, the record
//! audit (conflicts / changes / gaps), the diagnosis-support ranking, and any
//! timeline validity flags. A small blocking server over `std::net`, one client
//! at a time — enough for a local tester.
//!
//! The model is disease-blind: the server knows nothing about any disease. It
//! reasons over whatever compiled model it is given. By default that is the
//! bundled synthetic example (`knowledge/example/example_disorder.dsdsrc`); set
//! `ORPHARA_MODEL=/path/to/model.dsdsrc` to run it over a different model
//! without changing any code.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;

use caretrace::ingest::{ingest_bytes, Ingested};
use caretrace::pipeline::{process_case, CaseResult};
use concept_adapter::{adapt, ConceptMap};
use core_types::{EdgeType, EvidenceOutcome, EvidenceState, KnowledgeProvider, NodeId};
use dsd::Dsd;
use gnn::GnnModel;

const PAGE: &str = include_str!("page.html");
/// The bundled synthetic example model, compiled in as the default so the
/// server runs with no setup. Not medical knowledge.
const EXAMPLE_SRC: &str = include_str!("../../../knowledge/example/example_disorder.dsdsrc");
const ADDR: &str = "127.0.0.1:8794";

struct Model {
    dsd: Dsd,
    keys: HashMap<String, NodeId>,
    gnn: GnnModel,
}

fn main() {
    // Default to the bundled synthetic example; an operator can point the server
    // at a different model file with ORPHARA_MODEL, with no code change.
    let (src, origin) = match std::env::var("ORPHARA_MODEL") {
        Ok(path) if !path.is_empty() => match std::fs::read_to_string(&path) {
            Ok(text) => (text, path),
            Err(e) => {
                eprintln!("could not read ORPHARA_MODEL='{path}': {e}");
                std::process::exit(1);
            }
        },
        _ => (EXAMPLE_SRC.to_string(), "bundled synthetic example".to_string()),
    };
    let (bytes, keys) = dsdc::source::compile_source_with_keys(&src).expect("model compiles");
    let dsd = Dsd::from_bytes(&bytes).expect("model loads");
    let gnn = train(&dsd);
    let model = Model { dsd, keys, gnn };

    let listener = TcpListener::bind(ADDR).expect("bind");
    println!("ORPHARA engine listening on http://{ADDR}  (model: {origin})");
    for stream in listener.incoming() {
        let mut stream = match stream {
            Ok(s) => s,
            Err(_) => continue,
        };
        if let Err(e) = handle(&mut stream, &model) {
            eprintln!("request error: {e}");
        }
    }
}

fn handle(stream: &mut std::net::TcpStream, model: &Model) -> std::io::Result<()> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut request_line = String::new();
    if reader.read_line(&mut request_line)? == 0 {
        return Ok(());
    }
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("").to_string();
    let path = parts.next().unwrap_or("/").to_string();

    // Headers → content length + content type.
    let mut content_length = 0usize;
    let mut content_type = String::new();
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 {
            break;
        }
        let t = line.trim_end();
        if t.is_empty() {
            break;
        }
        let lower = t.to_lowercase();
        if let Some(v) = lower.strip_prefix("content-length:") {
            content_length = v.trim().parse().unwrap_or(0);
        } else if lower.starts_with("content-type:") {
            // Preserve original case: the multipart boundary is case-sensitive.
            content_type = t[t.find(':').map(|i| i + 1).unwrap_or(0)..].trim().to_string();
        }
    }

    let body = if content_length > 0 {
        let mut buf = vec![0u8; content_length];
        reader.read_exact(&mut buf)?;
        buf
    } else {
        Vec::new()
    };

    let (input_echo, results) = match (method.as_str(), path.as_str()) {
        ("POST", "/analyze") => {
            // Multipart (file uploads + note) or urlencoded (note only) fallback.
            let (files, note) = if content_type.contains("multipart/form-data") {
                let boundary = boundary_of(&content_type);
                parse_multipart(&body, &boundary)
            } else {
                (Vec::new(), form_field(&String::from_utf8_lossy(&body), "note"))
            };
            // Uploaded files are documents; the note may itself hold === docs.
            let mut docs: Vec<(String, Vec<u8>)> = files;
            for (name, text) in split_docs(&note) {
                docs.push((name, text.into_bytes()));
            }
            (note, analyze(&docs, model))
        }
        _ => (String::new(), String::new()),
    };

    let page = PAGE
        .replacen("{{INPUT}}", &html_escape(&input_echo), 1)
        .replacen("{{RESULTS}}", &results, 1);
    write_html(stream, &page)
}

fn write_html(stream: &mut std::net::TcpStream, body: &str) -> std::io::Result<()> {
    let resp = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    stream.write_all(resp.as_bytes())
}

// ---------------------------------------------------------------------------
// The analysis, rendered to HTML.
// ---------------------------------------------------------------------------

fn analyze(docs: &[(String, Vec<u8>)], model: &Model) -> String {
    if docs.is_empty() {
        return String::new();
    }
    let ingested: Vec<Ingested> = docs
        .iter()
        .enumerate()
        .map(|(i, (name, bytes))| ingest_bytes(name, i, bytes))
        .collect();
    let case = process_case(ingested);
    let k: &dyn KnowledgeProvider = &model.dsd;
    let out = adapt(&case, &model.keys, &ConceptMap::identity());
    let assessment = engine::run(&out.graph, k, Some(&model.gnn));
    let flags = temporal::analyze(&out.graph, k);

    let mut h = String::new();
    h.push_str(&kpis(&case, &assessment, &flags));
    h.push_str(&documents_section(&case));
    h.push_str(&facts_section(&case));
    h.push_str(&audit_section(&case));
    h.push_str(&diagnosis_section(&out, &assessment, k));
    h.push_str(&timeline_section(&flags));
    h
}

/// An ONCOVA-style panel: icon + title (+ optional subtitle) head, then body.
/// `accent` tints the icon: "" (purple) / "blue" / "green" / "pink".
fn panel(icon: &str, title: &str, sub: &str, accent: &str, body: &str) -> String {
    let subhtml = if sub.is_empty() {
        String::new()
    } else {
        format!("<div class=\"h2sub\">{}</div>", html_escape(sub))
    };
    let ico_cls = if accent.is_empty() { "ico".to_string() } else { format!("ico {accent}") };
    format!(
        "<div class=\"panel\"><div class=\"panel-head\"><span class=\"{ico_cls}\">{icon}</span>\
         <div><h2>{}</h2>{subhtml}</div></div><div class=\"panel-body\">{body}</div></div>",
        html_escape(title)
    )
}

fn kpis(case: &CaseResult, a: &engine::HybridAssessment, flags: &[temporal::TemporalFlag]) -> String {
    let g = |k: &str| case.counts.get(k).copied().unwrap_or(0.0) as i64;
    let top = a.candidates.first().map(|c| outcome_str(c.rule.outcome)).unwrap_or("—");
    let body = format!(
        "<div class=\"kpis\">\
         <div class=\"kpi\"><b>{}</b><span>documents</span></div>\
         <div class=\"kpi\"><b>{}</b><span>facts</span></div>\
         <div class=\"kpi\"><b>{}</b><span>conflicts</span></div>\
         <div class=\"kpi\"><b>{}</b><span>changes</span></div>\
         <div class=\"kpi\"><b>{}</b><span>gaps</span></div>\
         <div class=\"kpi\"><b>{}</b><span>timeline flags</span></div>\
         </div><div style=\"margin-top:12px\">Top candidate outcome: <span class=\"badge b-{top}\">{top}</span></div>",
        g("documents"), g("facts"), g("conflicts"), g("changes"), g("evidence_gaps"), flags.len()
    );
    panel("📊", "Summary", "one audit run", "", &body)
}

fn documents_section(case: &CaseResult) -> String {
    let mut rows = String::new();
    for d in &case.documents {
        rows.push_str(&format!(
            "<tr><td><code>{}</code></td><td>{}</td><td>{}</td><td>{}</td></tr>",
            html_escape(&d.filename),
            html_escape(d.doc_type.label()),
            d.doc_date.clone().unwrap_or_else(|| "—".into()),
            html_escape(&d.processing_status),
        ));
    }
    let body = format!(
        "<table><tr><th>File</th><th>Classified as</th><th>Date</th><th>Status</th></tr>{rows}</table>"
    );
    panel("📄", "Documents", "", "", &body)
}

fn facts_section(case: &CaseResult) -> String {
    let prov = provenance_map(case);
    let mut rows = String::new();
    for f in &case.facts {
        let value = match (f.value_num, &f.value_text) {
            (Some(v), _) => trim_f(v),
            (None, Some(t)) => html_escape(t),
            _ => "—".into(),
        };
        let dir = match f.interpretation.as_deref() {
            Some(d) => {
                let cls = match d {
                    "LOW" => "b-Contradicted",
                    "HIGH" => "b-Possible",
                    _ => "b-InsufficientEvidence",
                };
                format!("<span class=\"badge {cls}\">{}</span>", html_escape(d))
            }
            None => "—".into(),
        };
        rows.push_str(&format!(
            "<tr><td class=\"mono\">{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td class=\"prov\">{}</td></tr>",
            f.display_id,
            html_escape(&f.concept),
            value,
            html_escape(&f.unit.clone().unwrap_or_default()),
            dir,
            f.obs_date.clone().unwrap_or_else(|| "—".into()),
            prov.get(&f.source_id).cloned().unwrap_or_default(),
        ));
    }
    if rows.is_empty() {
        rows = "<tr><td colspan=7 class=\"empty\">No structured facts extracted.</td></tr>".into();
    }
    let body = format!(
        "<table><tr><th>ID</th><th>Concept</th><th>Value</th><th>Unit</th><th>Dir</th><th>Date</th><th>Source</th></tr>{rows}</table>"
    );
    panel("🔬", "Extracted facts", "biomarkers + findings, with provenance", "green", &body)
}

fn audit_section(case: &CaseResult) -> String {
    let mut body = String::new();

    body.push_str("<h3 class=\"sub-h\">Conflicts</h3>");
    if case.conflicts.is_empty() {
        body.push_str("<p class=\"empty\">None.</p>");
    } else {
        for c in &case.conflicts {
            body.push_str(&format!(
                "<div class=\"flagbox stop\"><b>{}</b> — {} vs {}<br><span class=\"prov\">{}</span></div>",
                html_escape(&c.concept_label),
                html_escape(&c.left_summary),
                html_escape(&c.right_summary),
                html_escape(&c.basis),
            ));
        }
    }

    body.push_str("<h3 class=\"sub-h\">Documented changes</h3>");
    if case.changes.is_empty() {
        body.push_str("<p class=\"empty\">None.</p>");
    } else {
        for ch in &case.changes {
            body.push_str(&format!("<div class=\"flagbox\">{}</div>", html_escape(&ch.basis)));
        }
    }

    body.push_str("<h3 class=\"sub-h\">Evidence gaps</h3>");
    if case.gaps.is_empty() {
        body.push_str("<p class=\"empty\">None.</p>");
    } else {
        for g in &case.gaps {
            let not = if g.evidence_not_located.is_empty() {
                String::new()
            } else {
                format!("<br><span class=\"prov\">not located: {}</span>",
                        html_escape(&g.evidence_not_located.join(", ")))
            };
            body.push_str(&format!(
                "<div class=\"flagbox\"><b>{}</b> — {}{}</div>",
                html_escape(&g.title),
                html_escape(&g.basis),
                not
            ));
        }
    }
    panel("⚖️", "Record audit", "conflicts · changes · gaps", "pink", &body)
}

fn diagnosis_section(
    out: &concept_adapter::AdapterResult,
    a: &engine::HybridAssessment,
    k: &dyn KnowledgeProvider,
) -> String {
    let mut body = String::new();

    body.push_str("<h3 class=\"sub-h\">Concept adapter</h3>");
    if out.mapped.is_empty() {
        body.push_str("<p class=\"empty\">Nothing mapped to the DSD registry.</p>");
    } else {
        let chips: Vec<String> = out
            .mapped
            .iter()
            .map(|m| {
                let cls = if format!("{:?}", m.state) == "Absent" { "chip absent" } else { "chip" };
                format!("<span class=\"{cls}\">{} · {:?}</span>", html_escape(&m.extractor_concept), m.state)
            })
            .collect();
        body.push_str(&format!("<div class=\"chips\">{}</div>", chips.join("")));
    }
    if !out.quarantined.is_empty() {
        let q: Vec<String> = out
            .quarantined
            .iter()
            .map(|q| format!("<code>{}</code> <span class=\"prov\">({})</span>",
                             html_escape(&q.extractor_concept), html_escape(&q.reason)))
            .collect();
        body.push_str(&format!("<p class=\"prov\" style=\"margin-top:8px\">Quarantined (never invented): {}</p>", q.join("; ")));
    }

    body.push_str("<h3 class=\"sub-h\">Diagnosis support · Blind Core</h3>");
    body.push_str("<p class=\"prov\" style=\"margin:-2px 0 8px\">Click a candidate to see its inheritance, confirmatory tests, triggers to avoid, dose-free management and features (from the DSD). ✓ = documented in this patient.</p>");
    if a.candidates.is_empty() {
        body.push_str("<p class=\"empty\">No candidate diseases — nothing in the inputs maps to a linked concept.</p>");
    } else {
        // Present concepts in this patient, for ✓ marking.
        let present: std::collections::HashSet<NodeId> = out
            .graph
            .observations
            .iter()
            .filter(|o| o.state == EvidenceState::Present)
            .map(|o| o.concept)
            .collect();
        for (i, c) in a.candidates.iter().enumerate() {
            let o = outcome_str(c.rule.outcome);
            let open = if i == 0 { " open" } else { "" };
            let score = c.rule.evidence.local_support;
            let discs = c.rule.evidence.strong_discriminators;
            let gnn = c.gnn_score.map(|g| format!(" · gnn {g:+.1}")).unwrap_or_default();
            let missing: Vec<&str> = c.rule.missing_required.iter().map(|&m| k.name(m).unwrap_or("?")).collect();
            let missing_html = if missing.is_empty() {
                String::new()
            } else {
                format!(
                    "<div class=\"dgroup conf\"><div class=\"dlbl\">Still needed to confirm</div>{}</div>",
                    missing.iter().map(|m| format!("<span class=\"chip avoid\">{}</span>", html_escape(m))).collect::<String>()
                )
            };
            body.push_str(&format!(
                "<details class=\"cand\"{open}><summary><span class=\"rank\">#{}</span>\
                 <span class=\"cname\">{}</span><span class=\"badge b-{o}\">{o}</span>\
                 <span class=\"prov\">support {score:.1} · {discs} discrim.{gnn}</span>\
                 <span class=\"chev\">▸</span></summary><div class=\"detail\">{}{}</div></details>",
                i + 1,
                html_escape(k.name(c.rule.evidence.disease).unwrap_or("?")),
                disease_detail(k, c.rule.evidence.disease, &present),
                missing_html,
            ));
        }
        if a.abstained {
            body.push_str("<p class=\"prov\">The engine abstains: nothing reaches acceptance.</p>");
        }
    }
    panel("🧠", "Diagnosis support", "adapter -> Blind Core (rules gate · GNN ranks)", "pur", &body)
}

/// Everything the DSD knows about a disease, grouped for the expandable card:
/// inheritance, confirmatory tests, discriminators, features, triggers to avoid,
/// and dose-free management. `present` marks what this patient documents.
fn disease_detail(k: &dyn KnowledgeProvider, disease: NodeId, present: &std::collections::HashSet<NodeId>) -> String {
    let (mut gene, mut conf, mut disc, mut feat, mut trig, mut mgmt) =
        (Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new());
    for e in k.neighbors(disease) {
        let name = k.name(e.target).unwrap_or("?");
        let here = present.contains(&e.target);
        let base = if here { "chip ok" } else { "chip" };
        let chip = format!("<span class=\"{base}\">{}{}</span>", html_escape(name), if here { " ✓" } else { "" });
        match e.edge_type {
            EdgeType::CausedBy => gene.push(html_escape(name)),
            EdgeType::Requires => conf.push(chip),
            EdgeType::Discriminates => disc.push(chip),
            EdgeType::HasFinding | EdgeType::HasPhenotype | EdgeType::HasPattern | EdgeType::AssociatedWith => feat.push(chip),
            EdgeType::TriggeredBy => trig.push(format!("<span class=\"chip avoid\">{}{}</span>", html_escape(name), if here { " ✓" } else { "" })),
            EdgeType::TreatedBy => mgmt.push(chip),
            _ => {}
        }
    }
    let group = |cls: &str, label: &str, chips: &[String]| -> String {
        if chips.is_empty() {
            String::new()
        } else {
            format!("<div class=\"dgroup {cls}\"><div class=\"dlbl\">{label}</div>{}</div>", chips.join(""))
        }
    };
    let mut out = String::new();
    if !gene.is_empty() {
        out.push_str(&format!(
            "<div class=\"dgroup\"><div class=\"dlbl\">Inheritance / gene</div><span class=\"chip\">{}</span></div>",
            gene.join("</span> <span class=\"chip\">")
        ));
    }
    out.push_str(&group("conf", "Confirmatory / required tests", &conf));
    out.push_str(&group("", "Discriminating features", &disc));
    out.push_str(&group("", "Associated features", &feat));
    out.push_str(&group("avoid", "Triggers to avoid", &trig));
    out.push_str(&group("mgmt", "Management (dose-free)", &mgmt));
    if out.is_empty() {
        out.push_str("<p class=\"empty\">No further detail in the DSD for this concept.</p>");
    }
    out
}

fn timeline_section(flags: &[temporal::TemporalFlag]) -> String {
    let body = if flags.is_empty() {
        "<p class=\"empty\">No timing concerns.</p>".to_string()
    } else {
        flags
            .iter()
            .map(|f| format!("<div class=\"flagbox\">⚠ <b>[{:?}]</b> {}</div>", f.timing, html_escape(&f.message)))
            .collect::<Vec<_>>()
            .join("")
    };
    panel("⏱️", "Timeline validity", "assay-timing / confounding", "", &body)
}

// ---------------------------------------------------------------------------
// GNN trained once at startup — disease-blind. For each disease in the model,
// its discriminating + confirmatory concepts form one labelled positive case.
// Nothing here names a disease; it reads whatever model was loaded.
// ---------------------------------------------------------------------------

fn train(dsd: &Dsd) -> GnnModel {
    let k: &dyn KnowledgeProvider = dsd;
    let examples: Vec<(std::collections::HashSet<NodeId>, NodeId)> = dsd
        .diseases()
        .iter()
        .filter_map(|&disease| {
            let present: std::collections::HashSet<NodeId> = k
                .neighbors(disease)
                .iter()
                .filter(|e| matches!(e.edge_type, EdgeType::Discriminates | EdgeType::Requires))
                .map(|e| e.target)
                .collect();
            (!present.is_empty()).then_some((present, disease))
        })
        .collect();
    let mut model = gnn::init(42);
    if !examples.is_empty() {
        model.train(&examples, k, 400, 0.2);
    }
    model
}

// ---------------------------------------------------------------------------
// Helpers.
// ---------------------------------------------------------------------------

fn outcome_str(o: EvidenceOutcome) -> &'static str {
    match o {
        EvidenceOutcome::StronglySupported => "StronglySupported",
        EvidenceOutcome::Supported => "Supported",
        EvidenceOutcome::Possible => "Possible",
        EvidenceOutcome::ConflictingEvidence => "ConflictingEvidence",
        EvidenceOutcome::InsufficientEvidence => "InsufficientEvidence",
        EvidenceOutcome::Contradicted => "Contradicted",
        EvidenceOutcome::Abstain => "Abstain",
    }
}

fn provenance_map(case: &CaseResult) -> HashMap<String, String> {
    let doc: HashMap<&str, &str> = case.documents.iter().map(|d| (d.id.as_str(), d.filename.as_str())).collect();
    case.sources
        .iter()
        .map(|s| {
            let name = doc.get(s.document_id.as_str()).copied().unwrap_or("?");
            (s.id.clone(), format!("{}:p{}", html_escape(name), s.page_number))
        })
        .collect()
}

fn trim_f(v: f64) -> String {
    if v == v.trunc() && v.abs() < 1e15 {
        format!("{}", v as i64)
    } else {
        format!("{v}")
    }
}

/// Split the textarea into documents on `=== name ===` header lines.
fn split_docs(input: &str) -> Vec<(String, String)> {
    let mut docs: Vec<(String, String)> = Vec::new();
    let mut name = String::from("note.txt");
    let mut buf = String::new();
    let mut started = false;
    for line in input.lines() {
        if let Some(h) = header_name(line) {
            if started && !buf.trim().is_empty() {
                docs.push((name.clone(), std::mem::take(&mut buf)));
            } else {
                buf.clear();
            }
            name = h;
            started = true;
        } else {
            buf.push_str(line);
            buf.push('\n');
        }
    }
    if !buf.trim().is_empty() {
        docs.push((name, buf));
    }
    if docs.is_empty() && !input.trim().is_empty() {
        docs.push(("note.txt".into(), input.to_string()));
    }
    docs
}

/// `=== name ===` (2+ '=' on each side) → Some(name).
fn header_name(line: &str) -> Option<String> {
    let t = line.trim();
    let inner = t.strip_prefix("==")?.strip_suffix("==")?;
    let name = inner.trim_matches('=').trim();
    if name.is_empty() {
        None
    } else {
        Some(name.to_string())
    }
}

// --- multipart/form-data parsing (binary-safe, for file uploads) ------------

fn boundary_of(content_type: &str) -> String {
    content_type
        .split(';')
        .find_map(|p| p.trim().strip_prefix("boundary="))
        .map(|b| b.trim().trim_matches('"').to_string())
        .unwrap_or_default()
}

/// Returns `(files: [(filename, bytes)], note_text)`.
fn parse_multipart(body: &[u8], boundary: &str) -> (Vec<(String, Vec<u8>)>, String) {
    let mut files = Vec::new();
    let mut note = String::new();
    if boundary.is_empty() {
        return (files, note);
    }
    let delim = format!("--{boundary}").into_bytes();
    for seg in split_bytes(body, &delim) {
        let seg = seg.strip_prefix(b"\r\n").unwrap_or(seg);
        if seg.is_empty() || seg.starts_with(b"--") {
            continue; // preamble or closing "--"
        }
        let hp = match find_subslice(seg, b"\r\n\r\n") {
            Some(p) => p,
            None => continue,
        };
        let headers = String::from_utf8_lossy(&seg[..hp]);
        let mut part_body = &seg[hp + 4..];
        if part_body.ends_with(b"\r\n") {
            part_body = &part_body[..part_body.len() - 2];
        }
        let filename = header_attr(&headers, "filename");
        let name = header_attr(&headers, "name");
        match filename {
            Some(fname) if !fname.is_empty() && !part_body.is_empty() => {
                files.push((fname, part_body.to_vec()));
            }
            _ => {
                if name.as_deref() == Some("note") {
                    note = String::from_utf8_lossy(part_body).into_owned();
                }
            }
        }
    }
    (files, note)
}

fn split_bytes<'a>(data: &'a [u8], delim: &[u8]) -> Vec<&'a [u8]> {
    let mut out = Vec::new();
    let mut start = 0;
    while let Some(pos) = find_subslice(&data[start..], delim) {
        out.push(&data[start..start + pos]);
        start += pos + delim.len();
    }
    out.push(&data[start..]);
    out
}

fn find_subslice(hay: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || hay.len() < needle.len() {
        return None;
    }
    (0..=hay.len() - needle.len()).find(|&i| &hay[i..i + needle.len()] == needle)
}

/// Extract `attr="value"` from a Content-Disposition header.
fn header_attr(headers: &str, attr: &str) -> Option<String> {
    let key = format!("{attr}=\"");
    let start = headers.find(&key)? + key.len();
    let rest = &headers[start..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

fn form_field(body: &str, field: &str) -> String {
    for pair in body.split('&') {
        if let Some(v) = pair.strip_prefix(&format!("{field}=")) {
            return url_decode(v);
        }
    }
    String::new()
}

fn url_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b'%' if i + 2 < bytes.len() => {
                let hi = hex(bytes[i + 1]);
                let lo = hex(bytes[i + 2]);
                match (hi, lo) {
                    (Some(h), Some(l)) => {
                        out.push(h * 16 + l);
                        i += 3;
                    }
                    _ => {
                        out.push(b'%');
                        i += 1;
                    }
                }
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn html_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}
