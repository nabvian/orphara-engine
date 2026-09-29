# Engine invariants

Rules the engine is built to keep. Breaking one is a design regression. Read
before changing anything under `crates/`.

## 1. The core does not change when knowledge changes

The core reasons; the DSD carries what the diseases mean. Reasoning crates are
written only against the `KnowledgeProvider` trait and nothing disease-specific.
Adding a disease, a family, or the whole corpus changes the data behind the
trait, never the trait and never the core.

Never allowed in a reasoning crate:

```
if disease == sickle_cell        // no
check_g6pd()                     // no
get_thalassemia_features()       // no
```

If a change to the core seems needed for new knowledge, the design is wrong: the
capability belongs in the DSD or in the contract.

## 2. Hybrid GNN + rules

- deterministic rules (`reasoning`) gate acceptance and abstention;
- the GNN (`gnn`) gives a ranking signal only and never crosses a rule tier;
- `engine` composes them; `temporal` adds a timeline validity pass.

The GNN's weights are data, not code, so the core stays disease-blind. A live
evidence sidecar and a cross-family router are out of scope; if added they come
as new crates and optional DSD sections, without altering the contract.

## 3. Global NodeId allocation

`NodeId`s come from one global namespace, never per-package, so packages compiled
separately compose without id collisions.

## 4. Missing is not absent

`EvidenceState` keeps `Unknown`, `NotAssessed` and `Indeterminate` distinct from
`Absent`, end to end. Collapsing them manufactures false contradictions.

## 5. No medical knowledge or dosing in code

The engine is generic computation only. Thresholds, criteria, drugs and doses
live in the DSD as sourced data, never as constants or `if` branches. Ranking is
not a diagnosis; abstention is a first-class result.

## 6. Dependency discipline

`core-types` is std-only and stays that way. Other crates add dependencies only
when they need them.

## 7. Version is 0.1.0

Edits, reviews and audits do not bump it. A version change is an explicit
decision, not a side effect of editing.
