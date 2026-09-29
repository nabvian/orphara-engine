//! The terminology layer is optional and additive: importing a local vocab
//! attaches codings to facts/medications, and doing so changes no audit count.

use caretrace::ingest::ingest_bytes;
use caretrace::pipeline::process_case;
use caretrace::terminology::local_index::{LocalIndexProvider, LocalVocab};
use caretrace::terminology::{resolve_case, TerminologyProvider};

fn case() -> caretrace::pipeline::CaseResult {
    let note = "\
Complete Blood Count
Report Date: 1 June 2026
Hemoglobin 8.1 g/dL
Ferrous sulfate 200 mg once daily oral
";
    process_case(vec![ingest_bytes("labs.txt", 0, note.as_bytes())])
}

#[test]
fn codings_attach_when_a_local_vocab_is_imported() {
    let c = case();

    // Synthetic local releases (fake codes — mechanics only).
    let rxnorm = LocalVocab::rxnorm_from_rrf(
        "T1|ENG|P|L|PF|S|Y|A|s|c|d|RXNORM|IN|T1|Ferrous sulfate|SRL|N|\n",
    );
    let loinc = LocalVocab::loinc_from_csv(
        "LOINC_NUM,LONG_COMMON_NAME,SHORTNAME\nX-HB,Hemoglobin,Hgb\n",
    );
    let providers: Vec<Box<dyn TerminologyProvider>> = vec![
        Box::new(LocalIndexProvider::rxnorm(Some(rxnorm))),
        Box::new(LocalIndexProvider::loinc(Some(loinc))),
    ];

    let (codings, report) = resolve_case(&c, &providers);
    assert!(report.coded > 0, "expected some codings");
    assert!(codings.iter().any(|k| k.system == "RxNorm" && k.assertable));
    assert!(codings.iter().any(|k| k.system == "LOINC"));
    // The document's own wording is preserved on the coding.
    assert!(codings.iter().any(|k| k.queried_text.to_lowercase().contains("hemoglobin")
        || k.queried_text.to_lowercase().contains("ferrous")));
}

#[test]
fn no_providers_means_no_codings_and_the_audit_is_identical() {
    let a = case();
    let b = case();
    // Resolving with nothing configured yields zero codings...
    let (codings, _r) = resolve_case(&a, &[]);
    assert!(codings.is_empty());
    // ...and the audit counts are the same whether or not resolution ran.
    assert_eq!(a.counts.get("facts"), b.counts.get("facts"));
    assert_eq!(a.counts.get("conflicts"), b.counts.get("conflicts"));
    assert_eq!(a.counts.get("evidence_gaps"), b.counts.get("evidence_gaps"));
}
