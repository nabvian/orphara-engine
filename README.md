# ORPHARA engine

A disease-blind reasoning engine for rare-disease decision *support*, written in
Rust. You give it a patient's extracted facts — findings from a doctor's note,
biomarkers from a report — and a compiled knowledge model, and it returns a
ranked, explainable shortlist of candidate disorders: what each one would need to
be confirmed, which findings point to it, and why. It does not make the decision.
A clinician does.

The point of this repository is the **engine**, not the medicine. The engine
knows nothing about any disease. All medical knowledge lives *outside* the code,
as a compiled model the engine reads through one frozen interface. Swap the model
and the same engine reasons about something else, with no code change. That
separation is the whole design, and it is what makes the engine safe to open
while the knowledge is still being built and reviewed.

Author: Koushik Das.

## Status, honestly

This is research software. It is **not clinically validated**, it is **not a
medical device**, and nothing here should be used for real diagnosis or
treatment. The model that ships with it (`knowledge/example/`) is a **synthetic,
made-up example** with two fictional disorders — it exists only so the engine can
build, test and demo end to end. It is not medical knowledge. Do not read it as
clinical guidance.

## Open engine, separate knowledge

This is the open half of an open-core project:

- **The engine is open**, here, under the GNU AGPL-3.0. Read it, run it, check
  it, build on it.
- **The disease knowledge is separate and, for now, private.** The real models —
  the sourced, reviewed reasoning graphs — are a large, ongoing body of work. I'm
  keeping them under a restricted licence while they're built and validated, and
  intend to open them too once that validation is done. The engine never needs
  them to run: it ships with the synthetic example, and reads any model you point
  it at.

So the funding-and-roadmap story is simple: the engine you see is the finished,
open foundation; the year ahead is knowledge — growing the private models toward
full hematology coverage — after which the knowledge opens as well.

## What's in here

A small Rust workspace. Each crate does one thing:

- `core-types` — the frozen contract: the `KnowledgeProvider` trait and the graph
  types. The engine depends only on this, never on a specific disease.
- `dsd` — the compiled binary knowledge format and its loader/verifier.
- `dsdc` — the compiler: `.dsdsrc` text → `.dsd` bytes.
- `reasoning` — the rules layer: matches findings, gates on confirmatory
  evidence, and classifies each candidate's outcome. This is the safety layer.
- `gnn` — a small graph model that supplies a *ranking* signal only. It never
  crosses a rule tier, so it can reorder within a tier but never override the
  rules.
- `engine` — ties rules + GNN into one hybrid assessment.
- `temporal` — a timing pass that flags a result whose reliability depends on
  when it was taken. Advisory; it never changes the ranking.
- `caretrace` — the extractor: turns PDFs / text / notes into structured facts,
  medications and findings, with provenance and a record audit
  (conflicts / changes / gaps). Optional standard-vocabulary coding.
- `concept-adapter` — the seam that maps extracted concepts to model nodes and
  builds the patient graph. Unmapped concepts are quarantined, never invented.
- `poc-server` — a small local web UI to try the whole pipeline in a browser.

## Build and run

You need Rust 1.80 or newer.

```bash
cargo test --workspace          # everything, green
cargo run -p engine --example example_model_demo    # the pipeline in the terminal
cargo run -p poc-server         # local web UI on http://127.0.0.1:8794
```

Nothing phones home; it runs entirely on your machine.

### Bring your own model

The server and the demos default to the bundled synthetic example. To run the
engine over a different compiled model, point it at one — no code change:

```bash
ORPHARA_MODEL=/path/to/your_model.dsdsrc cargo run -p poc-server
```

## How it stays honest

- **Disease-blind.** The engine reasons over a graph and knows nothing about any
  specific disease. There is no `check_g6pd()` anywhere, and there never will be —
  the contract in `core-types` is frozen.
- **Nothing is guessed.** A concept the adapter can't place is set aside, not
  invented. A confirmatory test that's missing makes a candidate *Possible* and
  the engine names what's missing — it never asserts the disorder.
- **Absent is not unknown.** A documented absence is carried as absent; a fact
  that simply isn't recorded stays unknown.
- **Rules gate; the model only ranks.** The learned signal can reorder
  candidates within a tier, never lift a weaker one over a stronger.

## Licence

GNU Affero General Public License v3.0 only. See [`LICENSE`](LICENSE). If you run
a modified version to provide a network service, the AGPL requires you to offer
that version's source to its users.

## Funding and contact

If you'd like to support the work, see [`funding.json`](funding.json). Questions
or collaboration: Koushik Das, daskoushik@protonmail.ch.
