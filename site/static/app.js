// ORPHARA GitHub Pages demo. The analysis runs the unmodified engine crates,
// compiled to WebAssembly (see site/web/src/lib.rs); this file only moves bytes
// in and renders the JSON that comes back.

const $ = (s) => document.querySelector(s);
const esc = (s) => String(s ?? "").replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c]);
const num = (v, d = 2) => (v == null ? "—" : Number.isInteger(v) ? String(v) : Number(v).toFixed(d));

// ---------- engine ----------
const enc = new TextEncoder(), dec = new TextDecoder();
let wasmModule = null, X = null, modelSrc = "";

async function instantiate() {
  let memory = null;
  const inst = await WebAssembly.instantiate(wasmModule, {
    env: { orphara_random_fill: (ptr, len) => crypto.getRandomValues(new Uint8Array(memory.buffer, ptr, len)) },
  });
  X = inst.exports;
  memory = X.memory;
}

function call(fn, bytes) {
  const ptr = X.orphara_alloc(bytes.length);
  new Uint8Array(X.memory.buffer, ptr, bytes.length).set(bytes);
  const n = X[fn](ptr, bytes.length);
  const out = JSON.parse(dec.decode(new Uint8Array(X.memory.buffer, X.orphara_out_ptr(), n)));
  X.orphara_free(ptr, bytes.length);
  return out;
}

// A Rust panic traps the whole instance in WebAssembly (there is no unwinding
// to catch it, unlike the native build). Restart it with the current model.
async function guarded(fn, bytes) {
  try {
    return call(fn, bytes);
  } catch (e) {
    await instantiate();
    if (modelSrc) call("orphara_load_model", enc.encode(modelSrc));
    throw e;
  }
}

function frame(docs) {
  const parts = [u32(docs.length)];
  for (const d of docs) {
    const name = enc.encode(d.name);
    parts.push(u32(name.length), name, u32(d.bytes.length), d.bytes);
  }
  const out = new Uint8Array(parts.reduce((a, p) => a + p.length, 0));
  let o = 0;
  for (const p of parts) { out.set(p, o); o += p.length; }
  return out;
}
function u32(n) { const b = new Uint8Array(4); new DataView(b.buffer).setUint32(0, n, true); return b; }

// The same split as crates/poc-server: `=== name ===` header lines start a new document.
function splitDocs(input) {
  const docs = []; let name = "note.txt", buf = [], started = false;
  for (const line of input.split(/\r?\n/)) {
    const t = line.trim(), m = t.startsWith("==") && t.endsWith("==") ? t.slice(2, -2).replace(/^=+|=+$/g, "").trim() : "";
    if (m) {
      if (!(started && buf.join("\n").trim())) buf = [];
      else { docs.push({ name, text: buf.join("\n") + "\n" }); buf = []; }
      name = m; started = true;
    } else buf.push(line);
  }
  if (buf.join("\n").trim()) docs.push({ name, text: buf.join("\n") + "\n" });
  if (!docs.length && input.trim()) docs.push({ name: "note.txt", text: input });
  return docs;
}

// ---------- static content ----------
const EDGE = {
  CausedBy: ["gene", "var(--e-gene)"], Requires: ["requires (confirmatory)", "var(--e-req)"], Discriminates: ["discriminates", "var(--e-disc)"],
  HasFinding: ["has finding", "var(--e-feat)"], AssociatedWith: ["associated with", "var(--e-feat)"], HasPhenotype: ["has phenotype", "var(--e-feat)"], HasPattern: ["has pattern", "var(--e-feat)"],
  TriggeredBy: ["triggered by (avoid)", "var(--e-trig)"], TreatedBy: ["treated by", "var(--e-treat)"], ConfoundedBy: ["confounded by (timeline)", "var(--e-conf)"],
};
const ORDER = ["CausedBy", "Requires", "Discriminates", "HasFinding", "HasPhenotype", "HasPattern", "AssociatedWith", "TriggeredBy", "TreatedBy"];

const OUTCOMES = [
  ["StronglySupported", "Supportive findings with confirmatory evidence present."],
  ["Supported", "Supportive findings with confirmatory evidence present."],
  ["Possible", "Supportive findings, but a required test is missing. The panel says what is needed."],
  ["ConflictingEvidence", "The record points both ways."],
  ["Contradicted", "The record points against it."],
  ["InsufficientEvidence", "Not enough to say anything."],
  ["Abstain", "Nothing reaches acceptance, and the engine says so."],
];
$("#outcome-legend").innerHTML = OUTCOMES.map(([o, d]) => `<div><span class="badge o-${o}">${o}</span><span>${esc(d)}</span></div>`).join("");

const FLOW = [
  { crates: ["caretrace"], title: "Extract", text: "PDFs, text and notes into facts, medications and findings with provenance, plus a record audit of conflicts, changes and gaps." },
  { crates: ["concept-adapter"], title: "Map", text: "Extracted concepts to model nodes and a patient graph. Anything it cannot place is quarantined, never invented." },
  { crates: ["dsd", "dsdc"], title: "Knowledge", text: "The compiled binary knowledge format, its loader and verifier, and the compiler from .dsdsrc text." },
  { crates: ["reasoning"], title: "Gate", text: "The safety layer: matches findings, gates on confirmatory evidence and classifies each candidate's outcome.", safety: true },
  { crates: ["gnn", "engine"], title: "Rank", text: "A small graph model supplies a ranking signal only. It can reorder within a tier, never cross one." },
  { crates: ["temporal"], title: "Timeline", text: "An advisory pass that flags results whose timing makes them unreliable. It never changes the ranking." },
];
$("#flow").innerHTML = FLOW.map((s) => `<div class="flow-step${s.safety ? " safety" : ""}">${s.crates.map((c) => `<span class="crate">${c}</span>`).join("")}<b>${s.title}</b><p>${esc(s.text)}</p></div>`).join("");

// ---------- load ----------
const runtime = $("#runtime"), runtimeText = $("#runtime-text");
let data, build, files = [], currentPreset = null, lastMs = null, lastFacts = [];
try {
  const [wasmBytes, d, src, b] = await Promise.all([
    fetch("orphara.wasm").then((r) => { if (!r.ok) throw new Error(`orphara.wasm: HTTP ${r.status}`); return r.arrayBuffer(); }),
    fetch("data/samples.json").then((r) => r.json()),
    fetch("data/example_disorder.dsdsrc").then((r) => r.text()),
    fetch("data/build.json").then((r) => r.json()),
  ]);
  data = d; build = b;
  wasmModule = await WebAssembly.compile(wasmBytes);
  await instantiate();
  data.exampleModel = src;
  $("#model-src").value = src;
  $("#build-hash").textContent = `engine ${b.engine} · commit ${b.commit} · orphara.wasm sha256 ${b.wasm_sha256.slice(0, 16)}… · ${Math.round(b.wasm_bytes / 1024)} KiB`;
  const m = compileModel(src);
  if (!m.ok) throw new Error(m.error);
  runtime.classList.add("ready");
  runtimeText.textContent = `Ready · engine ${b.engine} running in this tab · synthetic example model`;
  $("#analyze").disabled = false; $("#compile").disabled = false; $("#model-try").disabled = false;
} catch (e) {
  runtime.classList.add("error");
  runtimeText.textContent = `The engine could not load in this browser: ${e.message}`;
  throw e;
}

// ---------- stats ----------
function renderStats() {
  $("#stats").innerHTML = [
    ["10 crates", "in the open engine, AGPL-3.0; the core contract is standard-library only"],
    ["0 bytes", "uploaded: extraction and reasoning run in this browser tab"],
    [lastMs == null ? "—" : `${lastMs} ms`, "for the last analysis here, from documents to ranked candidates"],
    ["7 outcomes", "including Possible with the missing test named, and Abstain"],
  ].map(([v, l]) => `<div class="stat"><strong>${esc(v)}</strong><span>${esc(l)}</span></div>`).join("");
}
renderStats();

// ---------- presets ----------
const docsEl = $("#docs");
$("#presets").innerHTML = data.presets.map((p) => `<button type="button" role="tab" aria-selected="false" data-id="${p.id}">${esc(p.label)}</button>`).join("");
function loadPreset(id) {
  currentPreset = data.presets.find((p) => p.id === id);
  document.querySelectorAll("#presets button").forEach((b) => b.setAttribute("aria-selected", String(b.dataset.id === id)));
  docsEl.value = currentPreset.docs.map((n) => `=== ${n} ===\n${data.samples[n].trimEnd()}\n`).join("\n");
  $("#preset-point").textContent = currentPreset.point;
  files = []; renderFiles();
}
$("#presets").addEventListener("click", (e) => { const b = e.target.closest("button"); if (b) { loadPreset(b.dataset.id); analyze(); } });
docsEl.addEventListener("input", () => {
  if (currentPreset) { currentPreset = null; document.querySelectorAll("#presets button").forEach((b) => b.setAttribute("aria-selected", "false")); $("#preset-point").textContent = "Edited. Press Analyze to re-run."; }
});
docsEl.addEventListener("keydown", (e) => { if ((e.ctrlKey || e.metaKey) && e.key === "Enter") { e.preventDefault(); analyze(); } });
$("#analyze").addEventListener("click", () => analyze());

// ---------- files ----------
$("#file-input").addEventListener("change", async (e) => {
  for (const f of e.target.files) files.push({ name: f.name, bytes: new Uint8Array(await f.arrayBuffer()) });
  e.target.value = ""; renderFiles(); analyze();
});
function renderFiles() {
  $("#files").innerHTML = files.map((f, i) => `<span>${esc(f.name)} · ${Math.max(1, Math.round(f.bytes.length / 1024))} KiB<button type="button" data-i="${i}" aria-label="Remove ${esc(f.name)}">×</button></span>`).join("");
}
$("#files").addEventListener("click", (e) => { const b = e.target.closest("button"); if (b) { files.splice(Number(b.dataset.i), 1); renderFiles(); analyze(); } });

// ---------- analysis ----------
async function analyze() {
  if (!X) return;
  // Uploaded files first, then the documents in the text box: the local UI's order.
  const docs = [...files, ...splitDocs(docsEl.value).map((d) => ({ name: d.name, bytes: enc.encode(d.text) }))];
  const out = $("#output");
  if (!docs.length) { out.innerHTML = `<div class="card empty-state"><p class="muted">Add a document or pick a case.</p></div>`; return; }
  let r; const t0 = performance.now();
  try {
    r = await guarded("orphara_analyze", frame(docs));
  } catch (e) {
    out.innerHTML = `<div class="card"><h3>This input could not be read</h3><p class="muted">The engine stopped while reading it, most likely on a PDF it cannot parse. In the browser that cannot be caught the way the native build catches it, so the engine was restarted. Remove the file and try again, or run the engine locally.</p><p class="muted small">${esc(e.message)}</p></div>`;
    return;
  }
  lastMs = Math.max(1, Math.round(performance.now() - t0)); renderStats();
  if (!r.ok) { out.innerHTML = `<div class="card"><p class="err">${esc(r.error)}</p></div>`; return; }
  lastFacts = r.facts;
  out.innerHTML = [summary(r), diagnosis(r), adapter(r), factsPanel(r), audit(r), timeline(r), documents(r)].join("");
}

const panel = (title, sub, body) => `<div class="card"><div class="panel-head"><h3>${esc(title)}</h3>${sub ? `<span>${esc(sub)}</span>` : ""}</div>${body}</div>`;

function summary(r) {
  const c = r.counts, top = r.candidates[0];
  const k = (v, l) => `<div class="kpi"><b>${v ?? 0}</b><span>${l}</span></div>`;
  return panel("Summary", `analysed in ${lastMs} ms`, `<div class="kpis">${k(c.documents, "documents")}${k(c.facts, "facts")}${k(r.mapped.length, "mapped to the model")}${k(r.quarantined.length, "quarantined")}${k((c.conflicts ?? 0) + (c.evidence_gaps ?? 0), "conflicts + gaps")}${k(r.flags.length, "timeline flags")}</div>
    <p style="margin:12px 0 0">Top candidate: ${top ? `<span class="badge o-${top.outcome}">${top.outcome}</span> ${esc(top.disease)}` : `<span class="badge o-Abstain">Abstain</span> nothing in the input maps to a linked concept`}</p>`);
}

function diagnosis(r) {
  if (!r.candidates.length) return panel("Diagnosis support", "rules gate · graph model ranks", `<p class="empty">No candidate disorders: nothing in the input maps to a concept the model links to a disorder. The engine abstains rather than guess.</p>`);
  const groups = [["gene", "Inheritance / gene"], ["confirmatory", "Confirmatory / required tests"], ["discriminating", "Discriminating features"], ["features", "Associated features"], ["triggers", "Triggers to avoid"], ["management", "Management (dose-free)"]];
  const cards = r.candidates.map((c, i) => {
    const detail = groups.filter(([g]) => c.detail[g]?.length).map(([g, label]) => `<div class="dgroup"><div class="dlbl">${label}</div><div class="chips">${c.detail[g].map((x) => `<span class="chip${g === "triggers" ? " avoid" : x.present ? " ok" : ""}">${esc(x.name)}${x.present ? " ✓" : ""}</span>`).join("")}</div></div>`).join("");
    const missing = c.missing.length ? `<div class="missing-box"><b>Still needed to confirm:</b> ${c.missing.map(esc).join(", ")}</div>` : "";
    return `<details class="cand"${i === 0 ? " open" : ""}><summary><span class="rank">#${i + 1}</span><span class="cname">${esc(c.disease)}</span><span class="badge o-${c.outcome}">${c.outcome}</span><span class="meta">support ${num(c.support, 1)} · ${c.discriminators} discriminator${c.discriminators === 1 ? "" : "s"}${c.gnn != null ? ` · graph-model score ${c.gnn >= 0 ? "+" : ""}${num(c.gnn, 1)}` : ""}</span></summary><div class="detail">${missing}${detail}</div></details>`;
  }).join("");
  return panel("Diagnosis support", "rules gate · graph model ranks", `<p class="muted small" style="margin:-4px 0 10px">✓ marks what this patient's documents show. The detail comes from the knowledge model.</p>${cards}${r.abstained ? `<p class="muted small">The engine abstains: nothing reaches acceptance.</p>` : ""}`);
}

function adapter(r) {
  const mapped = r.mapped.length ? `<div class="chips">${r.mapped.map((m) => `<span class="chip${m.state === "Absent" ? " absent" : " ok"}" title="${esc(m.name)}">${esc(m.concept)} · ${esc(m.state)}</span>`).join("")}</div>` : `<p class="empty">Nothing mapped to the model.</p>`;
  const q = r.quarantined.length ? `<h4 class="sub-h">Quarantined: shown, never invented</h4><div class="chips">${r.quarantined.map((x) => `<span class="chip q" title="${esc(x.reason)}">${esc(x.concept)}</span>`).join("")}</div><p class="muted small" style="margin:6px 0 0">${esc(r.quarantined[0].reason)}</p>` : "";
  return panel("Concept adapter", "extracted concepts → model nodes", `<h4 class="sub-h">Mapped</h4>${mapped}${q}`);
}

function factsPanel(r) {
  if (!r.facts.length) return panel("Extracted facts", "", `<p class="empty">No facts extracted.</p>`);
  const rows = r.facts.map((f, i) => {
    const value = f.value != null ? num(f.value) : f.text ?? "—";
    const dir = f.direction ? `<span class="badge d-${esc(f.direction)}">${esc(f.direction)}</span>` : "—";
    return `<tr class="fact-row" data-i="${i}" tabindex="0" aria-expanded="false"><td class="mono">${esc(f.id)}</td><td>${esc(f.concept)}</td><td class="num">${esc(value)}</td><td>${esc(f.unit ?? "")}</td><td>${dir}</td><td class="nowrap">${esc(f.date ?? "—")}</td><td class="mono">${esc(f.document ?? "?")}:p${f.page ?? "?"}</td></tr>`;
  }).join("");
  return panel("Extracted facts", "click a row for its source text", `<div class="table-wrap"><table id="facts-table"><thead><tr><th>ID</th><th>Concept</th><th>Value</th><th>Unit</th><th>Direction</th><th>Date</th><th>Source</th></tr></thead><tbody>${rows}</tbody></table></div>`);
}

function audit(r) {
  const conf = r.conflicts.length ? r.conflicts.map((c) => `<div class="flagbox stop"><b>${esc(c.concept)}</b>: ${esc(c.left)} vs ${esc(c.right)}<span class="prov">${esc(c.basis)}</span></div>`).join("") : `<p class="empty">None.</p>`;
  const ch = r.changes.length ? r.changes.map((c) => `<div class="flagbox">${esc(c.basis)}</div>`).join("") : `<p class="empty">None.</p>`;
  const gaps = r.gaps.length ? r.gaps.map((g) => `<div class="flagbox warn"><b>${esc(g.title)}</b>: ${esc(g.basis)}${g.not_located.length ? `<span class="prov">not located: ${esc(g.not_located.join(", "))}</span>` : ""}</div>`).join("") : `<p class="empty">None.</p>`;
  return panel("Record audit", "conflicts are shown side by side, never adjudicated", `<h4 class="sub-h">Conflicts</h4>${conf}<h4 class="sub-h">Documented changes</h4>${ch}<h4 class="sub-h">Evidence gaps</h4>${gaps}`);
}

function timeline(r) {
  const body = r.flags.length ? r.flags.map((f) => `<div class="flagbox warn"><b>${esc(f.timing)}</b> · ${esc(f.message)}</div>`).join("") : `<p class="empty">No timing concerns.</p>`;
  return panel("Timeline validity", "advisory: never changes the ranking", body);
}

function documents(r) {
  const rows = r.documents.map((d) => `<tr><td class="mono">${esc(d.filename)}</td><td>${esc(d.type)}</td><td class="nowrap">${esc(d.date ?? "—")}</td><td>${esc(d.status)}${d.note ? `<br><span class="muted small">${esc(d.note)}</span>` : ""}</td><td class="mono">${esc(d.sha256.slice(0, 10))}…</td></tr>`).join("");
  const stages = r.stages.map((s) => `<span title="${esc(s.counts.map(([k, v]) => `${k} ${v}`).join(", "))}">${esc(s.name)}</span>`).join("");
  return panel("Documents and pipeline", "", `<div class="table-wrap"><table><thead><tr><th>File</th><th>Classified as</th><th>Date</th><th>Status</th><th>SHA-256</th></tr></thead><tbody>${rows}</tbody></table></div><h4 class="sub-h">Pipeline stages recorded</h4><div class="stages">${stages}</div>`);
}

$("#output").addEventListener("click", (e) => toggleSource(e.target.closest("tr.fact-row")));
$("#output").addEventListener("keydown", (e) => { if (e.key === "Enter" || e.key === " ") { const tr = e.target.closest("tr.fact-row"); if (tr) { e.preventDefault(); toggleSource(tr); } } });
function toggleSource(tr) {
  if (!tr) return;
  const next = tr.nextElementSibling;
  if (next?.classList.contains("src-row")) { next.remove(); tr.setAttribute("aria-expanded", "false"); return; }
  const f = lastFacts[Number(tr.dataset.i)];
  if (!f) return;
  tr.insertAdjacentHTML("afterend", `<tr class="src-row"><td colspan="7">Source text, ${esc(f.document)} page ${f.page}: <code>${esc(f.source_text ?? "")}</code> · surface form “${esc(f.surface)}” · confidence ${num(f.confidence)}</td></tr>`);
  tr.setAttribute("aria-expanded", "true");
}

// ---------- knowledge model ----------
function compileModel(src) {
  let m;
  try { m = call("orphara_load_model", enc.encode(src)); } catch (e) { m = { ok: false, error: e.message }; }
  if (m.ok) { modelSrc = src; renderModel(m); $("#model-status").textContent = "compiled"; }
  else { $("#model-status").innerHTML = `<span class="err">${esc(m.error)}</span>`; $("#model-summary").innerHTML = `<span class="err">Compile error: ${esc(m.error)}. The previous model is still loaded.</span>`; }
  return m;
}

function renderModel(m) {
  const byKey = Object.fromEntries(m.concepts.map((c) => [c.key, c]));
  $("#model-summary").textContent = `${m.diseases.length} disorder${m.diseases.length === 1 ? "" : "s"} · ${m.concepts.length} concepts · ${m.edges.length} relationships`;
  const dEdges = m.edges.filter((e) => m.diseases.includes(e.from));
  const others = m.edges.filter((e) => !m.diseases.includes(e.from));
  const right = [];
  for (const t of ORDER) for (const e of dEdges) if (e.type === t && !right.includes(e.to)) right.push(e.to);
  for (const e of others) for (const k of [e.from, e.to]) if (!right.includes(k) && !m.diseases.includes(k)) right.push(k);
  const RH = 30, W = 560, LX = 4, LW = 170, RX = 250, RW = 306, top = 10;
  const H = Math.max(m.diseases.length * 80, right.length * RH) + top * 2;
  const ry = (k) => top + right.indexOf(k) * RH + RH / 2;
  const span = right.length * RH, dStep = span / Math.max(1, m.diseases.length);
  const dy = (k) => top + m.diseases.indexOf(k) * dStep + dStep / 2;
  const short = (s, n) => (s.length > n ? s.slice(0, n - 1) + "…" : s);
  const paths = dEdges.map((e) => `<path d="M${LX + LW},${dy(e.from)} C${(LX + LW + RX) / 2},${dy(e.from)} ${(LX + LW + RX) / 2},${ry(e.to)} ${RX},${ry(e.to)}" stroke="${EDGE[e.type]?.[1] ?? "var(--e-feat)"}"><title>${esc(byKey[e.from]?.name)} — ${esc(EDGE[e.type]?.[0] ?? e.type)} → ${esc(byKey[e.to]?.name)} (${num(e.weight)})</title></path>`).join("")
    + others.map((e) => { const y1 = ry(e.from), y2 = ry(e.to), x = RX + RW; return `<path d="M${x},${y1} C${x + 40},${y1} ${x + 40},${y2} ${x},${y2}" stroke="${EDGE[e.type]?.[1] ?? "var(--e-feat)"}" stroke-dasharray="4 3"><title>${esc(byKey[e.from]?.name)} — ${esc(EDGE[e.type]?.[0] ?? e.type)} → ${esc(byKey[e.to]?.name)} (window ${num(e.weight)} days)</title></path>`; }).join("");
  const dNodes = m.diseases.map((k) => { const y = dy(k), lines = wrap(byKey[k]?.name ?? k, 20).slice(0, 3); return `<g class="dn"><rect x="${LX}" y="${y - 10 - lines.length * 8}" width="${LW}" height="${20 + lines.length * 16}" rx="8"/>${lines.map((l, i) => `<text x="${LX + 10}" y="${y - lines.length * 7 + 15 + i * 16}">${esc(l)}</text>`).join("")}<title>${esc(byKey[k]?.name)}</title></g>`; }).join("");
  const cNodes = right.map((k) => { const y = ry(k), c = byKey[k]; return `<g class="cn"><rect x="${RX}" y="${y - 12}" width="${RW}" height="24" rx="6"/><text x="${RX + 8}" y="${y + 4.5}">${esc(short(c?.name ?? k, 38))}</text><title>${esc(c?.name)} · ${esc(c?.kind)} · ${esc(k)}</title></g>`; }).join("");
  const used = [...new Set(m.edges.map((e) => e.type))];
  $("#model-graph").innerHTML = `<div class="model-legend">${used.map((t) => `<span><i style="background:${EDGE[t]?.[1] ?? "var(--e-feat)"}"></i>${esc(EDGE[t]?.[0] ?? t)}</span>`).join("")}</div><svg viewBox="0 0 ${W + 50} ${H}" role="img" aria-label="Knowledge model graph: disorders on the left, linked concepts on the right">${paths}${dNodes}${cNodes}</svg>`;
  $("#model-table").innerHTML = `<div class="table-wrap"><table><thead><tr><th>From</th><th>Relationship</th><th>To</th><th>Weight</th></tr></thead><tbody>${m.edges.map((e) => `<tr><td>${esc(byKey[e.from]?.name ?? e.from)}</td><td>${esc(EDGE[e.type]?.[0] ?? e.type)}</td><td>${esc(byKey[e.to]?.name ?? e.to)}</td><td class="num">${num(e.weight)}</td></tr>`).join("")}</tbody></table></div>`;
}
function wrap(s, n) { const out = []; let line = ""; for (const w of s.split(" ")) { if ((line + " " + w).trim().length > n && line) { out.push(line); line = w; } else line = (line + " " + w).trim(); } if (line) out.push(line); return out; }

$("#compile").addEventListener("click", () => { if (compileModel($("#model-src").value).ok) analyze(); });
$("#model-try").addEventListener("click", () => {
  const el = $("#model-src");
  if (!/^rel demo_hemolytic requires reticulocyte_count/m.test(el.value)) el.value = el.value.trimEnd() + "\n\n# added in the browser\nconcept test reticulocyte_count Reticulocyte count (example)\nrel demo_hemolytic requires reticulocyte_count 1.0\n";
  if (!compileModel(el.value).ok) return;
  loadPreset("confirmed");
  $("#preset-point").textContent = "Re-run against your edited model: the same documents, a new required test, a different outcome. Reset the model to undo.";
  analyze();
  $("#playground").scrollIntoView({ behavior: "smooth" });
});
$("#model-reset").addEventListener("click", () => { $("#model-src").value = data.exampleModel; if (compileModel(data.exampleModel).ok) analyze(); });

loadPreset(data.presets[0].id);
analyze();
