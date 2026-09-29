//! Ingestion: the only format-aware stage. Everything after it works on text.
//!
//! PDF text uses the `pdf-extract` crate (behind the `pdf` feature). A document
//! with no recoverable text layer is reported as extraction-incomplete; no text
//! is invented.

use std::path::Path;

use crate::hash::sha256_hex;
use crate::models::{new_id, Document, DocumentPage, DocType, ExtractionMethod, RawExtraction};

/// A document as ingested: the record, its pages, and the write-once raw
/// extraction that anchors every later provenance claim.
pub struct Ingested {
    pub document: Document,
    pub pages: Vec<DocumentPage>,
    pub raw: RawExtraction,
}

/// The page-separator used when concatenating pages into the raw extraction: a
/// form feed makes an explicit, greppable page boundary.
const PAGE_SEP: &str = "\n\u{0c}\n";

/// What a format reader returns: page texts, the extraction method, an optional
/// note, and the `(tool_name, tool_version)` that produced the text.
type ReadResult = (Vec<String>, ExtractionMethod, Option<String>, Option<(&'static str, String)>);

/// Read a file's pages of text by extension. Unknown extensions yield no pages.
fn read_pages(path: &Path, bytes: &[u8]) -> ReadResult {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
    match ext.as_str() {
        "txt" => (
            vec![String::from_utf8_lossy(bytes).into_owned()],
            ExtractionMethod::PlainText,
            None,
            Some(("builtin-text", "1.0".to_string())),
        ),
        "pdf" => read_pdf(bytes),
        "png" | "jpg" | "jpeg" => (
            vec![String::new()],
            ExtractionMethod::Ocr,
            None,
            None,
        ),
        _ => (
            vec![],
            ExtractionMethod::None,
            Some(format!("Unsupported file type '.{ext}'. No text was extracted.")),
            None,
        ),
    }
}

#[cfg(feature = "pdf")]
fn read_pdf(bytes: &[u8]) -> ReadResult {
    let tool = Some(("pdf-extract", env!("CARGO_PKG_VERSION").to_string()));
    // Hardening: `pdf-extract` can panic on malformed / unusual PDFs. Catch it
    // so one bad upload degrades to extraction-incomplete instead of taking the
    // process down. A silenced hook keeps the panic noise out of the log.
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let extracted = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        pdf_extract::extract_text_from_mem(bytes)
    }));
    std::panic::set_hook(prev);

    let incomplete = || {
        (
            vec![String::new()],
            ExtractionMethod::PdfTextLayer,
            Some("No text layer was recovered from this document. No text is invented; the document is reported as extraction-incomplete.".into()),
            tool.clone(),
        )
    };
    match extracted {
        Ok(Ok(text)) if !text.trim().is_empty() => {
            // `pdf-extract` emits a form feed at page boundaries for many PDFs;
            // split on it so per-page provenance survives when it is present.
            let pages: Vec<String> = if text.contains('\u{0c}') {
                text.split('\u{0c}').map(|s| s.to_string()).collect()
            } else {
                vec![text]
            };
            (pages, ExtractionMethod::PdfTextLayer, None, tool)
        }
        _ => incomplete(),
    }
}

#[cfg(not(feature = "pdf"))]
fn read_pdf(_bytes: &[u8]) -> ReadResult {
    (
        vec![String::new()],
        ExtractionMethod::PdfTextLayer,
        Some("PDF support is not compiled in (enable the `pdf` feature).".into()),
        None,
    )
}

/// Ingest raw bytes as one document. `display_name` is what the operator called
/// it; `ordinal` orders documents within a case.
pub fn ingest_bytes(display_name: &str, ordinal: usize, bytes: &[u8]) -> Ingested {
    let path = Path::new(display_name);
    let (pages_text, method, note, tool) = read_pages(path, bytes);

    let doc_id = new_id();
    let status = if pages_text.is_empty() { "MISSING" } else { "INGESTED" };
    let document = Document {
        id: doc_id.clone(),
        filename: display_name.to_string(),
        doc_type: DocType::Other,
        doc_date: None,
        page_count: pages_text.len(),
        byte_size: bytes.len(),
        sha256: sha256_hex(bytes),
        stored_path: None,
        processing_status: status.to_string(),
        processing_note: note.clone(),
        ordinal,
    };

    let pages: Vec<DocumentPage> = pages_text
        .iter()
        .enumerate()
        .map(|(i, txt)| DocumentPage {
            id: new_id(),
            document_id: doc_id.clone(),
            page_number: i + 1,
            text: txt.clone(),
        })
        .collect();

    // Write-once raw extraction: concatenated text + per-page byte offsets.
    let mut offsets = Vec::new();
    let mut cursor = 0usize;
    let mut parts = Vec::new();
    for txt in &pages_text {
        parts.push(txt.clone());
        offsets.push((cursor, cursor + txt.len()));
        cursor += txt.len() + PAGE_SEP.len();
    }
    let full = parts.join(PAGE_SEP);
    let has_layer = !full.trim().is_empty();
    let raw = RawExtraction {
        id: new_id(),
        document_id: doc_id,
        text: full.clone(),
        text_sha256: sha256_hex(full.as_bytes()),
        char_count: full.len(),
        page_offsets: offsets,
        method,
        tool_name: tool.as_ref().map(|(n, _)| n.to_string()),
        tool_version: tool.as_ref().map(|(_, v)| v.clone()),
        confidence: if method == ExtractionMethod::Ocr { None } else { Some(1.0) },
        has_text_layer: has_layer,
        note: if has_layer {
            note
        } else {
            note.or_else(|| Some(
                "No text layer was recovered from this document. No text is invented; the document is reported as extraction-incomplete.".into(),
            ))
        },
        sidecar_path: None,
    };

    Ingested { document, pages, raw }
}

/// Ingest a file already on disk.
pub fn ingest_path(path: &Path, ordinal: usize) -> std::io::Result<Ingested> {
    let bytes = std::fs::read(path)?;
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("document");
    Ok(ingest_bytes(name, ordinal, &bytes))
}

#[cfg(all(test, feature = "pdf"))]
mod pdf_tests {
    use super::*;

    #[test]
    fn malformed_pdf_degrades_without_panicking() {
        // A byte stream that claims to be a PDF but is garbage must not crash the
        // ingester; it is reported as extraction-incomplete, no text invented.
        let bad = b"%PDF-1.4\nthis is not a real pdf body \xff\xfe\x00 garbage";
        let ing = ingest_bytes("bad.pdf", 0, bad);
        assert!(!ing.raw.has_text_layer);
        assert!(ing.raw.note.is_some());
        assert_eq!(ing.document.sha256.len(), 64);
    }
}
