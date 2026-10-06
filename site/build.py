"""Build the ORPHARA GitHub Pages site into ``_site/``.

    python3 site/build.py

Compiles ``site/web`` (a thin C-ABI bridge over the unmodified engine crates)
to WebAssembly, copies the static page, and writes the bundled synthetic
example model, the synthetic sample documents and the case presets to
``data/``. Needs Rust with the ``wasm32-unknown-unknown`` target
(``rustup target add wasm32-unknown-unknown``). Standard library only.
"""
from __future__ import annotations

import hashlib
import json
import re
import shutil
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SITE = ROOT / "site"
OUT = ROOT / "_site"
WEB = SITE / "web"
WASM = WEB / "target" / "wasm32-unknown-unknown" / "release" / "orphara_web.wasm"

# Each preset is a set of the synthetic documents in samples/, in upload order.
PRESETS = [
    {"id": "missing", "label": "Confirmatory test missing", "docs": ["doctor_note_g6pd.txt"],
     "point": "Supportive findings, but the required test is not in the record. The candidate stays Possible and the engine names the missing test."},
    {"id": "added", "label": "…then add the lab report", "docs": ["doctor_note_g6pd.txt", "g6pd_enzyme_report.txt"],
     "point": "The same note plus an enzyme report. The direction comes from the report's own printed reference range, the confirmatory test is now present, and the outcome rises."},
    {"id": "confirmed", "label": "Fully documented case", "docs": ["g6pd_confirmed.txt"],
     "point": "Discriminating findings and the confirmatory test in one note."},
    {"id": "timing", "label": "Assay timing trap", "docs": ["g6pd_assay_timing_trap.txt"],
     "point": "A normal result recorded during a confounding state. The timeline pass flags it as unreliable; it never changes the ranking."},
    {"id": "cyanotic", "label": "Second disorder", "docs": ["methaemoglobinaemia.txt"],
     "point": "The other fictional disorder in the example model. Concepts the model does not contain are quarantined, not invented."},
    {"id": "record", "label": "Multi-document record", "docs": ["cbc_visit1.txt", "cbc_visit2.txt", "discharge_summary.txt"],
     "point": "Two lab reports that disagree, and a claim with no supporting test. The record audit shows both; nothing maps, so the engine abstains."},
    {"id": "unmapped", "label": "Mostly outside the model", "docs": ["pk_deficiency.txt"],
     "point": "Most findings are outside the two-disorder example model, so they are set aside rather than forced into a match."},
]


def main() -> None:
    subprocess.run(["cargo", "build", "--release", "--target", "wasm32-unknown-unknown"], cwd=WEB, check=True)
    if OUT.exists():
        shutil.rmtree(OUT)
    shutil.copytree(SITE / "static", OUT)
    (OUT / ".nojekyll").touch()
    shutil.copy2(WASM, OUT / "orphara.wasm")

    data = OUT / "data"
    data.mkdir()
    samples = {p.name: p.read_text() for p in sorted((ROOT / "samples").glob("*.txt"))}
    for preset in PRESETS:
        missing = [d for d in preset["docs"] if d not in samples]
        if missing:
            raise SystemExit(f"preset {preset['id']} names missing samples: {missing}")
    (data / "samples.json").write_text(json.dumps({"samples": samples, "presets": PRESETS}, indent=1))
    shutil.copy2(ROOT / "knowledge" / "example" / "example_disorder.dsdsrc", data / "example_disorder.dsdsrc")

    version = re.search(r'^version\s*=\s*"([^"]+)"', (ROOT / "crates" / "engine" / "Cargo.toml").read_text(), re.M)
    commit = subprocess.run(["git", "rev-parse", "--short", "HEAD"], cwd=ROOT, capture_output=True, text=True).stdout.strip()
    digest = hashlib.sha256((OUT / "orphara.wasm").read_bytes()).hexdigest()
    (data / "build.json").write_text(json.dumps({
        "engine": version.group(1) if version else "0.1.0", "commit": commit, "wasm_sha256": digest,
        "wasm_bytes": (OUT / "orphara.wasm").stat().st_size,
    }))
    print(f"built {OUT.relative_to(ROOT)}/ · orphara.wasm {digest[:12]} · {(OUT / 'orphara.wasm').stat().st_size // 1024} KiB")


if __name__ == "__main__":
    main()
