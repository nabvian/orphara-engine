# GitHub Pages demo

**https://nabvian.github.io/orphara-engine/**

The page runs the engine itself in the browser. `site/web` is a thin C-ABI
bridge that links the unmodified engine crates (`caretrace`, `concept-adapter`,
`dsd`, `dsdc`, `engine`, `gnn`, `reasoning`, `temporal`) into one WebAssembly
module and makes the same calls, in the same order, as `crates/poc-server`.
The page loads the bundled synthetic example model and the synthetic documents
in `samples/`; the private knowledge base is never part of it.

```bash
rustup target add wasm32-unknown-unknown
cargo test --manifest-path site/web/Cargo.toml   # bridge tests over the samples
python3 site/build.py                            # writes _site/
python3 -m http.server -d _site 8000             # http://localhost:8000
```

`site/web` is deliberately not a member of the engine workspace, so
`cargo test --workspace` and the engine's dependency set are unchanged. Its one
extra dependency is `serde_json`, for the JSON handed to the page. The PDF
reader's `getrandom` dependency is served by the page's
`crypto.getRandomValues` (see `site/web/.cargo/config.toml`).

`.github/workflows/pages.yml` rebuilds and publishes the site to the
`gh-pages` branch on pushes to `main`.

`static/samples/enzyme_report_synthetic.pdf` is a synthetic lab report, the
PDF form of `samples/g6pd_enzyme_report.txt`, for trying PDF upload.
