# Using the ORPHARA engine

A step-by-step walkthrough: from a fresh clone to running the whole pipeline and
reading the structured analysis. It takes about ten minutes the first time.

A reminder first: this is research software, not a medical device, and the model
that ships with it is a **synthetic, made-up example**. Do not put real,
identifiable patient data into it, and do not use its output for real care. It is
decision *support* for evaluation.

If you just want to try it in the browser with nothing to install, skip to
[section 8](#8-google-colab-no-install) — the Colab notebook.

## 1. What you need

- **Rust**, version 1.80 or newer. If you don't have it, install `rustup` from
  https://rustup.rs and it sets up the toolchain.
- **git**.
- A terminal — macOS, Linux, or Windows all work.

Once built, it runs fully offline. Nothing phones home.

## 2. Get the code and build it

```bash
git clone https://github.com/nabvian/orphara-engine.git
cd orphara-engine
cargo test --workspace
```

The first build pulls a few crates and takes a couple of minutes. You should see
every test group report `ok`. If it does, you're ready.

## 3. See the pipeline in the terminal

```bash
cargo run -p engine --example example_model_demo
```

This compiles the bundled example model, trains the ranking model on the graph
itself, and reasons about a full case and then the same case with the
confirmatory concept removed — showing how a candidate drops from
`StronglySupported` to `Possible` and the engine names what's missing.

## 4. Analyze a note (and reports) from the command line

The quickest way to feed in your own patient history and note:

```bash
# the bundled example note:
cargo run -p concept-adapter --example analyze_note

# your own files — any mix of reports and notes, PDF or text:
cargo run -p concept-adapter --example analyze_note -- history_note.txt cbc_report.pdf
```

It prints the full structured analysis: the documents it read, the extracted
facts (with direction and provenance), anything quarantined because it couldn't
be mapped, the record audit, the ranked candidates with their outcome and what's
still needed to confirm each, and any timeline flags.

There are ready-made synthetic inputs in [`samples/`](samples/) to try.

## 5. Run the web UI

```bash
cargo run -p poc-server
```

Open http://127.0.0.1:8794. You'll see the ORPHARA page: a disclaimer bar, inputs
on the left, results on the right after you analyze. Stop it with Ctrl-C.

To analyze:

- **Upload report(s):** choose one or more files (PDF, text, or image). These
  contribute biomarkers.
- **Doctor's note:** type or paste the clinical note. This contributes findings.
- Use either or both, then click **Analyze**. To paste several documents into the
  note box at once, separate them with a header line like `=== cbc.txt ===`.

## 6. Reading the structured analysis

The result is a set of panels:

- **Summary** — counts for the run and the top candidate's outcome.
- **Documents** — each file, how it was classified, and its date.
- **Extracted facts** — what was pulled out, each with a direction (LOW / HIGH /
  NORMAL, taken from the report's *own* reference range or flag) and a link back
  to the source document and page.
- **Record audit** — conflicts (two documents disagreeing, shown side by side,
  never adjudicated), documented changes over time, and evidence gaps (a claim
  whose supporting test isn't in the records).
- **Diagnosis support** — the ranked candidates. Click one to open its detail:
  inheritance, confirmatory tests, triggers to avoid, dose-free management, and
  features. A ✓ marks what this patient documents; "still needed to confirm"
  lists what's missing.
- **Timeline validity** — flags a result whose reliability depends on when it was
  taken.

Outcome labels:

- **StronglySupported / Supported** — supportive findings with confirmatory
  evidence present.
- **Possible** — supportive findings, but a required test is missing. The panel
  says what's needed.
- **ConflictingEvidence / Contradicted** — the record points both ways, or
  against it.
- **InsufficientEvidence / Abstain** — not enough to say anything.

None of these is a diagnosis. They are an ordering with the reasons attached.

## 7. Run the engine over a different model

Everything above uses the bundled synthetic example. To run the same engine over
a different compiled model — with no code change — point it at a `.dsdsrc` file:

```bash
ORPHARA_MODEL=/path/to/your_model.dsdsrc cargo run -p poc-server
ORPHARA_MODEL=/path/to/your_model.dsdsrc cargo run -p concept-adapter --example analyze_note -- note.txt
```

This is how a granted partner runs the engine over a private knowledge model: the
model file stays on their machine; the engine code is unchanged.

## 8. Google Colab (no install)

If you'd rather not install Rust locally, open the notebook at
[`notebooks/orphara_colab.ipynb`](notebooks/orphara_colab.ipynb) in Google Colab
(File → Open notebook → GitHub → `nabvian/orphara-engine`). It installs Rust,
clones this repo, runs the tests, and runs the analysis on a note you can edit in
a cell — all in the browser.

## 9. Optional: vocabulary coding (LOINC / RxNorm / SNOMED)

The extractor can attach standard vocabulary codes to what it extracts. This is
off by default and never changes the audit or the result — it is advisory. It
needs an official release you're entitled to (RxNorm is public; LOINC and SNOMED
need their own licences), and it's a library step rather than a UI button. Ask if
you want to try it and I'll point you at the importer functions in the `caretrace`
terminology module.

## 10. If something goes wrong

- **"address already in use" on start:** something is already on port 8794. Stop
  it, or change the port in `crates/poc-server/src/main.rs`.
- **Build errors:** run `rustup update`; make sure you're on Rust 1.80 or newer.
- **A number in a report wasn't picked up:** it usually needs a recognisable unit
  next to it. Check how the report prints it.
- **"Nothing mapped":** with the bundled example, only the example's own concepts
  resolve; a value it can't place is quarantined (shown, never invented) rather
  than forced into a match.

## Feedback

Found something wrong, or something that should be there? Please open an issue on
this repository, or email engikd1993@gmail.com.
