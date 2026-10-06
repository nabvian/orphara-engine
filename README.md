# ORPHARA engine

A disease-blind reasoning engine for rare-disease decision *support*, written in
Rust. This repository is the **open engine**. The medical knowledge it reasons
over lives outside it, and is described below.

Author: Koushik Das.

**[Try it in your browser →](https://nabvian.github.io/orphara-engine/)** The open engine, compiled to
WebAssembly, over the synthetic example model and sample documents. No install;
nothing leaves the browser tab.

## What ORPHARA is

ORPHARA is decision support for rare disease. The full picture it is built
toward:

1. **You give it the patient.** Upload the patient's history — lab reports,
   discharge summaries, prior results (PDF or text) — and write a short
   free-text clinical note.
2. **It extracts the structured facts.** Biomarkers (with the direction taken
   from each report's own printed reference range, never a guessed cutoff),
   findings, medications, dates — each carried back to the exact source text it
   came from. Conflicting or changed values across documents are surfaced, not
   silently merged.
3. **It reasons.** Against a curated, sourced knowledge base, it produces a
   **full structured analysis**: a ranked shortlist of candidate disorders, and
   for each one — the findings that point to it, the confirmatory tests still
   needed, the triggers to avoid, dose-free management, and inheritance. A
   timeline pass flags results whose reliability depends on when they were taken.
4. **It hands the decision back.** Every line traces to a source. Nothing is
   asserted without its confirmatory evidence; where that evidence is missing,
   ORPHARA says what is missing rather than guessing. The clinician decides.

That is the outcome ORPHARA is designed to give — today for a small proof set,
in future across a full rare-disease knowledge base.

## Scope: the proof, the year, and the whole inventory

Three horizons, smallest to largest:

- **This proof build covers 3 disorders.** The current knowledge base holds three
  red-cell enzyme disorders (G6PD deficiency, pyruvate kinase deficiency, and
  autosomal-recessive methaemoglobinaemia), end to end. Three is not the ambition
  — it is the **proof** that the whole pipeline works: real reports and a real
  note in, a full, sourced, structured analysis out.
- **Year one: the rare-disease Hematology family.** The plan for the coming year
  is to build out the whole Hematology family — **230 clinical entities** in the
  Orphanet Knowledge Base (release 2025-12-09), and still growing as the family
  and sub-family classification is completed (6,258 entities in the inventory are
  not yet family-classified, so some will resolve into Hematology).
- **The whole inventory, ultimately: every rare disease.** The end goal is to
  cover the full Orphanet rare-disease inventory. That inventory, in the release
  above, is **11,456 clinical entities**, of which **10,288 are active** (1,024
  obsolete and 144 not-rare-in-Europe are excluded). Orphanet counts disorders,
  disease groups and subtypes separately; ORPHARA resolves each entity's level as
  part of the build, so the disorder-level total is being established rather than
  guessed.

Counts are from the Orphanet Knowledge Base, release 2025-12-09, used under
CC BY 4.0 (data © Orphanet / INSERM). They are a moving target — Orphanet updates
the inventory, and ORPHARA's own classification refines the family and level
figures as the work proceeds.

**The architecture is built so growth is knowledge work, not engine work.** The
engine in this repository does not change as diseases are added; only the
knowledge base does. That is exactly why the engine can be open now while the
knowledge is still being built.

## What is in this repository, and what is not

This is an **open-core** project.

- **Open (here):** the **engine** — the disease-blind reasoning core, the
  knowledge format and compiler, the extractor, the concept adapter, the timeline
  pass, and a local web UI. Licensed under the GNU AGPL-3.0.
- **Bundled with it:** a **synthetic, made-up example model** (`knowledge/example/`)
  with two fictional disorders, so the engine builds, tests and demos end to end.
  It is **not medical knowledge** and must not be read as clinical guidance.
- **Not here (private for now):** the real disease knowledge base — the sourced,
  reviewed reasoning models (the 3-disorder proof, and the Hematology family work
  underway — see Scope). The engine does not need it to run; it reads whatever
  model you give it.

### Why the knowledge layer is not open yet — and will be

The knowledge base is **not clinically validated yet**. Open-sourcing an
unvalidated clinical knowledge base invites it to be used as if it were finished,
which is exactly what a decision-support tool must not allow before review.
**After clinical validation, the full suite — knowledge included — will be
open-sourced.** Open is the destination; the private phase is about getting there
safely. The engine is open now because the engine is finished and disease-blind:
opening it carries none of that risk.

## Status, honestly

Research software. **Not clinically validated, not a medical device.** Do not use
it for real diagnosis or treatment. Every example here uses synthetic data.

## Quick start

You need Rust 1.80 or newer ([rustup.rs](https://rustup.rs)).

```bash
cargo test --workspace                              # everything, green
cargo run -p engine --example example_model_demo    # the pipeline in the terminal
cargo run -p concept-adapter --example analyze_note  # a note -> full structured analysis
cargo run -p poc-server                             # local web UI on http://127.0.0.1:8794
```

Nothing phones home; it runs entirely on your machine. A full walkthrough —
building, using the web UI, uploading a patient history plus a doctor's note, and
reading the structured analysis — is in [`USAGE.md`](USAGE.md). To try it in the
browser with no local setup, see the Colab notebook in
[`notebooks/`](notebooks/orphara_colab.ipynb).

### Bring your own model

The server and the demos default to the bundled synthetic example. To run the
engine over a different compiled model, point it at one — no code change:

```bash
ORPHARA_MODEL=/path/to/your_model.dsdsrc cargo run -p poc-server
```

## The crates

- `core-types` — the frozen contract: the `KnowledgeProvider` trait and the graph
  types. The engine depends only on this, never on a specific disease.
- `dsd` / `dsdc` — the compiled binary knowledge format, its loader/verifier, and
  the compiler (`.dsdsrc` text → `.dsd` bytes).
- `reasoning` — the rules layer: matches findings, gates on confirmatory
  evidence, classifies each candidate's outcome. The safety layer.
- `gnn` — a small graph model that supplies a *ranking* signal only; it never
  crosses a rule tier, so it can reorder within a tier but never override rules.
- `engine` — ties rules + GNN into one hybrid assessment.
- `temporal` — an advisory timing pass; it flags but never changes the ranking.
- `caretrace` — the extractor: PDFs / text / notes → structured facts,
  medications, findings, with provenance and a record audit (conflicts / changes
  / gaps). Optional standard-vocabulary coding (LOINC / RxNorm / SNOMED).
- `concept-adapter` — maps extracted concepts to model nodes and builds the
  patient graph. Unmapped concepts are quarantined, never invented.
- `poc-server` — a small local web UI for the whole pipeline.

## How it stays honest

- **Disease-blind.** No `check_g6pd()` anywhere, and there never will be — the
  contract in `core-types` is frozen. Adding diseases changes the knowledge, not
  the code.
- **Nothing is guessed.** A concept it can't place is set aside, not invented. A
  missing confirmatory test makes a candidate *Possible*, and the engine names
  what's missing.
- **Absent is not unknown.** A documented absence is carried as absent; a fact not
  recorded stays unknown.
- **Rules gate; the model only ranks.**

## Licence

GNU Affero General Public License v3.0 only — see [`LICENSE`](LICENSE). If you run
a modified version as a network service, the AGPL requires you to offer that
version's source to its users.

## Support and contact

To support the work, see [`funding.json`](funding.json). Questions or
collaboration: open an issue on this repository, or email engikd1993@gmail.com.
