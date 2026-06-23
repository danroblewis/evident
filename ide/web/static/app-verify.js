"use strict";

// ==============================================================================
// app-verify.js — solve/witness rendering: the SAT/UNSAT/enumeration view plus the
// domain-picture renderers for Seq witnesses (board / grid / record table / cell
// strip, Task #68/#196) and the pure counterexample-trace formatters (Ana #173/#198).
//
// Hoisted functions only — they reference escapeHtml / editor / seqViz at CALL time, so
// loading this before app.js is safe (all scripts are parsed before any handler fires).
// The /api/solve fetch, the trace stepper DOM, and the verify console wiring stay in
// app.js. Behaviour-preserving move out of app.js.
// ==============================================================================

// --- solve/query: pin parsing + result rendering ----------------------------------
function parsePins(s) {
  const given = {};
  (s || "").split(",").forEach((pair) => {
    const eq = pair.indexOf("=");
    if (eq > 0) { const k = pair.slice(0, eq).trim(); if (k) given[k] = pair.slice(eq + 1).trim(); }
  });
  return given;
}

// escapeHtml lives in app-data.js (first-loaded), shared by every concern file.

// One conflicting core as a line-number/text table.
function coreTable(core) {
  return `<table>${core.map((c) =>
    `<tr><td class="k">line ${c.line}</td><td class="v">${escapeHtml(c.text)}</td></tr>`).join("")}</table>`;
}

// The UNSAT body: every minimal core. `d.cores` (a list of cores) is preferred; we fall back to
// the single `d.core` for older responses. One core → "remove any one"; N independent cores →
// "fix one constraint from EACH", each core its own visually-separated group.
function renderCores(d, pinned) {
  const cores = (d.cores && d.cores.length) ? d.cores
              : (d.core && d.core.length ? [d.core] : []);
  if (!cores.length) {
    return `<span class="dim">no assignment satisfies the constraints${
      pinned.length ? " under those pins — try different ones." : "."}</span>`;
  }
  if (cores.length === 1) {
    return `<div class="dim">conflicting core — removing any one of these makes it solvable:</div>`
      + coreTable(cores[0]);
  }
  const more = d.cores_complete === false ? " (at least)" : "";
  return `<div class="dim">${cores.length}${more} independent contradictions — `
    + `fix one constraint from EACH to make it solvable:</div>`
    + cores.map((core, i) =>
        `<div class="core-group"><div class="dim">contradiction #${i + 1}</div>${coreTable(core)}</div>`
      ).join("");
}

function renderSolve(d, given) {
  const head = $("#solve-head"), body = $("#solve-body");
  body.classList.remove("stale");                          // fresh result — undim (Sam #211)
  const pinned = Object.keys(given || {});
  if (!d.ok) { head.innerHTML = `<span class="bad">✕ ${escapeHtml(d.error || "query failed")}</span>`; body.innerHTML = ""; return; }

  // enumeration — a list of distinct witnesses, with exhaustive/▸limit honesty
  if (d.solutions) {
    const n = d.count != null ? d.count : d.solutions.length;
    if (!n) { head.innerHTML = `<span class="unsat">⊭ UNSAT</span> — <b>${d.claim || "claim"}</b> has no solutions`; body.innerHTML = ""; return; }
    head.innerHTML = `<span class="sat">⊨ ${d.complete ? "all " + n : "≥ " + n}</span> distinct witness${n === 1 ? "" : "es"} of <b>${d.claim || "claim"}</b>`
      + (d.complete ? ` <span class="dim">(complete — the solver exhausted the space)</span>`
                    : ` <span class="dim">(showing ${n}; stopped at the limit, more may exist)</span>`);
    body.innerHTML = d.solutions.map((s, i) =>
      `<div class="sol"><span class="dim">#${i + 1}</span> `
      + Object.keys(s).sort().map((k) => `${k}=${escapeHtml(JSON.stringify(s[k]))}`).join("&nbsp;&nbsp;") + `</div>`).join("");
    return;
  }

  // UNSAT — with the minimal conflicting cores (which constraints conflict). One core renders
  // as before ("remove any one"); multiple INDEPENDENT cores each render as their own group, so
  // a user fixing an over-constrained model sees every contradiction at once (Ana #205).
  if (d.satisfied === false) {
    head.innerHTML = `<span class="unsat">⊭ UNSAT</span> — <b>${d.claim || "claim"}</b> has no solution`
      + (pinned.length ? ` <span class="dim">with ${pinned.join(", ")} pinned</span>` : "");
    body.innerHTML = renderCores(d, pinned);
    return;
  }

  // single SAT witness
  head.innerHTML = `<span class="sat">⊨ SAT</span> — <b>${d.claim || "claim"}</b> has a witness`
    + (pinned.length ? ` <span class="dim">(pinned: ${pinned.join(", ")})</span>` : "");
  const keys = Object.keys(d.bindings || {}).sort();
  if (!keys.length) { body.innerHTML = '<span class="dim">satisfiable (no free variables to report)</span>'; return; }
  // Domain picture(s): any Seq binding draws as a board / cell strip ABOVE the raw table
  // (Task #68) — a beginner can't read positional arrays as a solution.
  const src = (typeof editor !== "undefined") ? editor.getValue() : "";
  // A var that draws as a domain picture (board / grid / record table) is shown ONLY as that
  // picture — the raw JSON row underneath read like a debug dump (Marek #204). Scalars (no
  // picture) keep their row; the picture IS the source of truth for the rest.
  const vizByKey = {};
  keys.forEach((k) => { const v = seqViz(k, d.bindings[k], src); if (v) vizByKey[k] = v; });
  const viz = keys.map((k) => vizByKey[k]).filter(Boolean).join("");
  const rawKeys = keys.filter((k) => !vizByKey[k]);
  body.innerHTML = (viz ? `<div class="viz-wrap">${viz}</div>` : "")
    + (rawKeys.length ? `<table>${rawKeys.map((k) => `<tr><td class="k">${k}${pinned.includes(k) ? " 📌" : ""}</td>`
        + `<td class="v">${escapeHtml(JSON.stringify(d.bindings[k]))}</td></tr>`).join("")}</table>` : "");
  // The board IS the domain answer — lead with it. When the witness draws as a board/grid, scroll the
  // solve panel into view so the filled answer isn't missed below the abstract feasibility heatmap (Sam #247).
  if (viz) requestAnimationFrame(() => { const p = $("#solve"); if (p) p.scrollIntoView({ behavior: "smooth", block: "nearest" }); });
}

// --- domain-picture rendering for Seq witnesses (Task #68 / #196) -----------------
// --- domain-picture rendering for Seq witnesses (Task #68 / #196) -----------------
// A Seq witness is hard to read as a flat array. Draw it as the domain shape it is:
//   • record-Seq (array of objects)            → a small TABLE, one row per element
//   • sudoku-shaped Int-Seq (length K², 1..K)  → a K×K filled grid
//   • N-queens column-Seq (permutation + name) → a chessboard with pieces
//   • anything else                            → the honest index→value cell strip
// Shapes are detected from the witness itself plus the source (`#name = N`), generically —
// no sample names are hardcoded.
function seqViz(name, val, source) {
  if (!Array.isArray(val) || !val.length) return null;
  const n = val.length;

  // record-Seq: every element is a plain object (a record). Render columns = field names.
  if (val.every((v) => v && typeof v === "object" && !Array.isArray(v))) {
    return recordTable(name, val);
  }

  // only primitive-Int seqs get a numeric picture; mixed/non-int seqs fall through.
  if (!val.every((v) => typeof v === "number" && Number.isInteger(v))) return null;

  // A queens board needs TWO honest signals, not just "values in 0..N-1" — that also matches a
  // topological order (pos=[0,1,2,3,4]) and a sudoku row, which would draw a wrong chessboard
  // (Marek #68/#92). Require: a queens-like variable NAME *and* a true permutation of 0..N-1
  // (one queen per row AND column).
  const queensName = /^(col|cols|queen|queens|row|rows|board)$/.test(name.toLowerCase());
  const isPermutation = n >= 4 && new Set(val).size === n && val.every((v) => v >= 0 && v < n);
  if (queensName && isPermutation) return queensBoard(name, val);

  // sudoku-shaped: a flat Int-Seq whose length is a perfect square K² (4, 9, 16, 25),
  // with every value a single symbol in 1..K (or 0..K-1). Reshape it into the K×K grid the
  // values already imply — Sam shouldn't reshape 16 index=value lines in his head.
  const k = Math.round(Math.sqrt(n));
  if (k >= 2 && k * k === n) {
    const min = Math.min(...val), max = Math.max(...val);
    const oneBased = min >= 1 && max <= k;        // 1..K (the canonical sudoku numbering)
    const zeroBased = min >= 0 && max <= k - 1;   // 0..K-1
    if (oneBased || zeroBased) return sudokuGrid(name, val, k);
  }

  return cellStrip(name, val);
}

// One row per element, one column per record field. Replaces a raw-JSON array of objects with a
// scannable table (subset-sum's {weight, take} items, toposort's {from, to} edges, sudoku boxes).
function recordTable(name, rows) {
  // union of field names across rows, in first-seen order (rows are homogeneous in practice).
  const cols = [];
  rows.forEach((r) => Object.keys(r).forEach((c) => { if (!cols.includes(c)) cols.push(c); }));
  const fmt = (v) =>
    typeof v === "boolean" ? (v ? "✓" : "·")
      : (v && typeof v === "object") ? escapeHtml(JSON.stringify(v))
      : escapeHtml(String(v));
  const head = `<tr><th>#</th>${cols.map((c) => `<th>${escapeHtml(c)}</th>`).join("")}</tr>`;
  const trs = rows.map((r, i) =>
    `<tr><td class="rt-i">${i}</td>`
    + cols.map((c) => `<td>${c in r ? fmt(r[c]) : ""}</td>`).join("") + `</tr>`).join("");
  return `<div class="viz"><div class="viz-label">${escapeHtml(name)} `
    + `<span class="dim">(${rows.length} × {${cols.map(escapeHtml).join(", ")}})</span></div>`
    + `<table class="rec-table">${head}${trs}</table></div>`;
}

// A flat Int-Seq reshaped into the K×K grid its values imply (sudoku / latin-square style).
function sudokuGrid(name, vals, k) {
  let cells = "";
  for (let r = 0; r < k; r++) {
    for (let c = 0; c < k; c++) {
      // subgrid shading when K is itself a perfect square (4→2×2 boxes, 9→3×3) — purely visual.
      const sub = Math.round(Math.sqrt(k));
      const boxed = sub * sub === k && (Math.floor(r / sub) + Math.floor(c / sub)) % 2 === 1;
      cells += `<div class="scell${boxed ? " box" : ""}">${escapeHtml(String(vals[r * k + c]))}</div>`;
    }
  }
  return `<div class="viz"><div class="viz-label">${escapeHtml(name)} — ${k}×${k} grid`
    + ` <span class="dim">(${escapeHtml(name)}[r·${k}+c] → cell at row r, col c)</span></div>`
    + `<div class="sgrid" style="grid-template-columns:repeat(${k},1fr)">${cells}</div></div>`;
}

// `#name = N` in the source → N (the pinned Seq length), else null.
function pinnedLen(source, name) {
  const m = (source || "").match(new RegExp("#\\s*" + name.replace(/[.*+?^${}()|[\\]\\\\]/g, "\\$&") + "\\s*=\\s*(\\d+)"));
  return m ? parseInt(m[1], 10) : null;
}

function queensBoard(name, cols) {
  const n = cols.length;
  let cells = "";
  for (let r = 0; r < n; r++) {
    for (let c = 0; c < n; c++) {
      const dark = (r + c) % 2 === 1;
      const q = cols[r] === c;
      cells += `<div class="qsq${dark ? " dark" : ""}${q ? " q" : ""}">${q ? "♛" : ""}</div>`;
    }
  }
  return `<div class="viz"><div class="viz-label">${escapeHtml(name)} — ${n}×${n} board`
    + ` <span class="dim">(row i → queen at column ${escapeHtml(name)}[i])</span></div>`
    + `<div class="qboard" style="grid-template-columns:repeat(${n},1fr)">${cells}</div></div>`;
}

function cellStrip(name, arr) {
  const cells = arr.map((v, i) =>
    `<div class="cell"><div class="cell-idx">${i}</div><div class="cell-val">${escapeHtml(String(v))}</div></div>`).join("");
  return `<div class="viz"><div class="viz-label">${escapeHtml(name)} `
    + `<span class="dim">(index → value)</span></div><div class="strip">${cells}</div></div>`;
}

// --- counterexample-trace pure formatters (Ana #173/#175/#198) --------------------
// A counterexample run as a compact trace: init → … → the offending state.
function _fmtTrace(trace) {
  if (!trace || trace.length < 2) return "";
  const steps = trace.map((s) => Object.entries(s).map(([k, v]) => `${k.split(".").pop()}=${v}`).join(" "));
  return steps.length > 8 ? steps.slice(0, 4).join(" → ") + " → … → " + steps[steps.length - 1]
                          : steps.join(" → ");
}
// Pure index/format helpers for the scrubbable stepper (the DOM stepper is in app.js).
function _traceClamp(i, n) { return i < 0 ? 0 : (i > n - 1 ? n - 1 : i); }
function _traceStepLabel(i, n) { return `step ${i + 1} / ${n}`; }       // 1-based for humans
function _traceStateLine(state) {
  return Object.entries(state || {}).map(([k, v]) => `${k.split(".").pop()} = ${v}`).join("   ");
}

// ==============================================================================
// Verify console — safety/liveness invariant checking against the reachable set,
// plus the scrubbable counterexample-trace stepper DOM (Ana #198/#173/#156/#142).
// The pure trace formatters live above; this is the stateful half. Listeners are
// attached by initVerify(), called from app.js once the core globals exist. Moved
// verbatim out of app.js (same shared global scope) — behaviour-preserving.
// ==============================================================================

// Assert-and-check a safety invariant over the reachable set — verify `var op value` holds on
// EVERY reachable state (a proof when the set is finite & fully explored), or get a reachable
// counterexample. The other half of the relational pitch: not just "watch", but "prove".
const _INV_RE = /^\s*([A-Za-z_]\w*(?:\.\w+)?)\s*(<=|>=|!=|<|>|=|≤|≥|≠)\s*(.+?)\s*$/;
// two-sided range — lo (<|≤) var (<|≤) hi — the canonical invariant shape (Marek #156)
const _INV_RANGE = /^\s*(-?\d+(?:\.\d+)?)\s*(<=|<|≤)\s*([A-Za-z_]\w*(?:\.\w+)?)\s*(<=|<|≤)\s*(-?\d+(?:\.\d+)?)\s*$/;
function _coerce(s) {
  if (/^-?\d+$/.test(s)) return parseInt(s, 10);
  if (/^-?\d*\.\d+$/.test(s)) return parseFloat(s);
  if (s === "true" || s === "false") return s === "true";
  return s;
}
async function _checkOne(varName, op, value) {
  const res = await fetch("/api/invariant", {
    method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify({ source: editor.getValue(), var: varName, op, value }),
  });
  return res.json();
}
async function checkInvariant() {
  const out = $("#inv-result");
  clearTrace();                              // a new check invalidates the old scrubber
  const raw = $("#inv-prop").value.trim();
  if (!raw) { out.textContent = ""; return; }
  // LIVENESS first: P ⤳ Q (leads-to), or ◇/□◇ Q. Q and P are CONJUNCTIONS — ◇(timer = 0 ∧ light = Red)
  // — parsed via _parseTerms (the same ∧-splitter the ⊨? query uses), so they're not limited to one
  // var op value (Ana #258/#142).
  const lt = raw.split(/\s*(?:⤳|~>|\bleads to\b)\s*/);
  if (lt.length === 2) {
    const P = _parseTerms(lt[0]), Q = _parseTerms(lt[1]);
    if (P.error || Q.error || !(P.terms || []).length || !(Q.terms || []).length) {
      out.className = "bad"; out.textContent = "✕ leads-to: write  P ⤳ Q  (e.g. mode = Coining ⤳ mode = Idle)"; return;
    }
    return runTemporal(out, { terms: Q.terms, modality: "leads_to", p_terms: P.terms });
  }
  // STRONG liveness □◇Q (infinitely often) — checked BEFORE plain ◇ so the □ prefix isn't
  // swallowed by the ◇ branch. Holds iff no run gets permanently trapped in ¬Q (Ana #260).
  const io = raw.match(/^\s*(?:□◇|◻◇|\[\]<>|infinitely(?:\s+often)?)\s+(.+)$/i);
  if (io) {
    const Q = _parseTerms(io[1]);
    if (Q.error || !(Q.terms || []).length) { out.className = "bad"; out.textContent = "✕ infinitely-often: write  □◇ var op value  (e.g. □◇ light = Yellow)"; return; }
    return runTemporal(out, { terms: Q.terms, modality: "infinitely_often" });
  }
  const ev = raw.match(/^\s*(?:◇|<>|eventually)\s+(.+)$/i);
  if (ev) {
    const Q = _parseTerms(ev[1]);
    if (Q.error || !(Q.terms || []).length) { out.className = "bad"; out.textContent = "✕ eventually: write  ◇ var op value  (e.g. ◇ done = true)"; return; }
    return runTemporal(out, { terms: Q.terms, modality: "eventually" });
  }
  // SAFETY (□): a two-sided range becomes TWO predicates (var ≥ lo ∧ var ≤ hi); else a single comparison.
  let preds;
  const rg = raw.match(_INV_RANGE);
  if (rg) {
    const [, lo, lop, varName, hop, hi] = rg;
    preds = [[varName, lop === "<" ? ">" : ">=", _coerce(lo)], [varName, hop, _coerce(hi)]];
  } else {
    const mt = raw.match(_INV_RE);
    if (!mt) { out.className = "bad"; out.textContent = "✕ write  var op value  (e.g. count ≤ 5  or  0 ≤ x ≤ 6)"; return; }
    preds = [[mt[1], mt[2], _coerce(mt[3])]];
  }
  out.className = "dim"; out.textContent = "checking…";
  try {
    let checked = 0, exhaustive = true; const texts = [];
    for (const [varName, op, value] of preds) {
      const d = await _checkOne(varName, op, value);
      if (!d.ok) { out.className = "bad"; out.textContent = "✕ " + (d.error || "check failed"); return; }
      texts.push(d.predicate); checked = Math.max(checked, d.checked || 0); exhaustive = exhaustive && d.exhaustive;
      if (!d.holds) {
        const cex = Object.entries(d.counterexample || {}).map(([k, v]) => `${k}=${v}`).join(", ");
        const tr = _fmtTrace(d.trace);
        out.className = "bad";
        out.textContent = `✗ violated (${d.predicate}) — counterexample  ${cex}` + (tr ? `   ·   trace: ${tr}` : "");
        if (d.trace && d.trace.length >= 2) showTrace(d.trace, d.predicate);
        return;
      }
    }
    out.className = "good";
    out.textContent = (exhaustive ? "✓ proven" : "✓ holds (bounded)")
      + ` — ${texts.join(" ∧ ")} on all ${checked} reachable states`;
  } catch (e) { out.className = "bad"; out.textContent = "✕ " + e; }
}
// A counterexample run as a compact trace: init → … → the offending state (Ana #173/#175).

// --- scrubbable counterexample trace (TLA+-Toolbox style, Ana #198/#120/#175) ----------
// The trace array is the BFS path init→violation (safety) or the dodging/lasso run (liveness).
// Step through it one state at a time, reading the FULL assignment at each step — not the
// one-line collapse. Pure helpers (_traceClamp / _traceStepLabel / _traceStateLine) carry the
// index + format logic so they're unit-testable without a DOM.
// --- trace → diagram highlight (#231/#206: the TLA+/Alloy "trace lights up the explorer") ----
// `lastOverlay` (app.js) holds the live `.view-wrap` + its identifiable points; the scrubber rings
// the point matching the current step. A trace step is a state dict; a point carries the same shape
// as `point.state`. Match on the SHARED leaf keys: a point matches iff every key present in BOTH
// agrees (after the k.split('.').pop() leaf-name normalization the rest of the code uses). Pure +
// total → unit-testable headless. Returns the matching point, or null (no points / no agreement).
function _matchPoint(points, stepState) {
  if (!points || !points.length || !stepState) return null;
  const leaf = (o) => {
    const m = {};
    for (const k of Object.keys(o || {})) m[k.split(".").pop()] = String(o[k]);
    return m;
  };
  const want = leaf(stepState);
  for (const p of points) {
    if (typeof p.fx !== "number" || typeof p.fy !== "number") continue;
    const have = leaf(p.state);
    let shared = 0, ok = true;
    for (const k of Object.keys(want)) {
      if (k in have) { shared++; if (have[k] !== want[k]) { ok = false; break; } }
    }
    if (ok && shared > 0) return p;
  }
  return null;
}
// Remove any prior trace-step ring from the live overlay (each scrub step replaces it).
function clearTraceRing() {
  if (lastOverlay && lastOverlay.wrap) {
    const old = lastOverlay.wrap.querySelector(".trace-ring");
    if (old) old.remove();
  }
}
// Ring the diagram point matching the current trace step, over the live `.view-wrap`. No-op when
// there's no live overlay, no points, or no match — the stepper still works regardless. The ring
// is pointer-events:none (app.css) so it never blocks the underlying .pt-target hover targets.
function highlightTraceStep(stepState) {
  clearTraceRing();
  if (!lastOverlay || !lastOverlay.wrap) return;
  const p = _matchPoint(lastOverlay.points, stepState);
  if (!p) return;
  const ring = document.createElement("div");
  ring.className = "trace-ring";
  ring.style.left = (p.fx * 100) + "%";
  ring.style.top = (p.fy * 100) + "%";
  lastOverlay.wrap.appendChild(ring);
}

const _trace = { states: [], i: 0, label: "", kind: "violation", cycleStart: null };
function clearTrace() {
  _trace.states = []; _trace.i = 0; _trace.label = ""; _trace.kind = "violation"; _trace.cycleStart = null;
  const el = $("#inv-trace"); el.hidden = true; el.innerHTML = "";
  clearTraceRing();                          // drop any diagram highlight from the old scrubber (#231/#206)
}
function _renderTrace() {
  const el = $("#inv-trace"), n = _trace.states.length;
  if (n < 2) { el.hidden = true; el.innerHTML = ""; clearTraceRing(); return; }
  const i = _trace.i, last = i === n - 1;
  el.hidden = false;
  el.innerHTML = "";
  const head = document.createElement("div"); head.className = "trace-head";
  if (_trace.label) { const lab = document.createElement("span"); lab.className = "trace-label"; lab.textContent = _trace.label; head.appendChild(lab); }
  const prev = document.createElement("button"); prev.className = "trace-nav"; prev.textContent = "◀"; prev.disabled = i === 0;
  prev.onclick = () => { _trace.i = _traceClamp(_trace.i - 1, n); _renderTrace(); };
  const step = document.createElement("span"); step.className = "trace-step"; step.textContent = _traceStepLabel(i, n);
  const next = document.createElement("button"); next.className = "trace-nav"; next.textContent = "▶"; next.disabled = last;
  next.onclick = () => { _trace.i = _traceClamp(_trace.i + 1, n); _renderTrace(); };
  head.appendChild(prev); head.appendChild(step); head.appendChild(next);
  // The final step is the WITNESS for an existential query (a goal — good), the VIOLATION for a
  // refuted safety check (bad), or — for a liveness lasso — every step from cycle_start on is the
  // dodging CYCLE ("↻ loops here, never reaches Q", amber). Don't paint a query's goal red (#237/#239).
  const goal = _trace.kind === "goal";
  const cs = _trace.cycleStart;
  const inCycle = cs != null && i >= cs;
  if (inCycle) { const flag = document.createElement("span"); flag.className = "trace-flag cycle"; flag.textContent = "↻ loops here — never reaches Q"; head.appendChild(flag); }
  else if (last) { const flag = document.createElement("span"); flag.className = "trace-flag" + (goal ? " good" : ""); flag.textContent = goal ? "● witness here" : "● violation here"; head.appendChild(flag); }
  // Export the whole trace as a state table — so a counterexample / witness run can leave for a
  // spreadsheet, a paper, or a regression fixture (Ana #244).
  const dl = document.createElement("button");
  dl.className = "trace-nav trace-dl"; dl.textContent = "↧ csv"; dl.title = "download this trace as a CSV state table";
  dl.onclick = exportTraceCSV;
  head.appendChild(dl);
  el.appendChild(head);
  const line = document.createElement("div");
  line.className = "trace-state" + (inCycle ? " cycle" : (last ? (goal ? " good" : " bad") : ""));
  line.textContent = _traceStateLine(_trace.states[i]);
  el.appendChild(line);
  highlightTraceStep(_trace.states[i]);      // ring this step's state on the diagram (#231/#206)
}
// Download the current trace as a CSV state table (Ana #244): a header of leaf var names + one row per
// step. So a counterexample / witness run can leave for a spreadsheet, a paper, or a regression fixture.
function exportTraceCSV() {
  const states = _trace.states;
  if (!states || !states.length) return;
  const keys = Object.keys(states[0]);
  const esc = (v) => { const s = typeof v === "string" ? v : JSON.stringify(v); return /[",\n]/.test(s) ? '"' + s.replace(/"/g, '""') + '"' : s; };
  const rows = [["step", ...keys.map((k) => k.split(".").pop())].join(",")];
  states.forEach((s, i) => rows.push([i, ...keys.map((k) => esc(s[k]))].join(",")));
  const blob = new Blob([rows.join("\n")], { type: "text/csv" });
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url; a.download = "evident-trace.csv"; document.body.appendChild(a); a.click(); a.remove();
  URL.revokeObjectURL(url);
}
// Open the stepper on a fresh trace, parked at the final step. `kind` is "goal" for a query witness
// (else a violation); `cycleStart`, when given, marks where a liveness lasso's dodging cycle begins
// (steps ≥ it are the loop).
function showTrace(trace, label, kind, cycleStart) {
  if (!trace || trace.length < 2) { clearTrace(); return; }
  _trace.states = trace; _trace.i = trace.length - 1; _trace.label = label || "";
  _trace.kind = kind || "violation";
  _trace.cycleStart = (cycleStart != null && cycleStart >= 0) ? cycleStart : null;
  _renderTrace();
}
// Liveness check (◇ / ⤳) against /api/temporal, with the dodging-run trace on failure.
async function runTemporal(out, body) {
  clearTrace();
  out.className = "dim"; out.textContent = "checking…";
  try {
    const res = await fetch("/api/temporal", {
      method: "POST", headers: { "content-type": "application/json" },
      body: JSON.stringify({ source: editor.getValue(), ...body }),
    });
    const d = await res.json();
    if (!d.ok) { out.className = "bad"; out.textContent = "✕ " + (d.error || "check failed"); return; }
    if (d.holds) {
      out.className = "good";
      // For ◇ (AF: every run reaches Q at least once), distinguish RECURRENT (□◇ also holds —
      // Q recurs forever) from TRANSIENT (Q reached, then the system can settle into ¬Q forever).
      // The bare "✓ proven" on a transient ◇ invites a false recurrence reading (Ana #260).
      let note = "";
      if (body.modality === "eventually" && d.recurrent !== undefined) {
        note = d.recurrent
          ? " — and recurs (infinitely often)"
          : " — but TRANSIENT: reached, does not recur (the system settles into ¬Q)";
      }
      out.textContent = (d.exhaustive ? "✓ proven" : "✓ holds (bounded)")
        + ` — ${d.predicate} on all ${d.checked} reachable states` + note;
    } else {
      // Lasso (Ana #239): the run is a STEM into a CYCLE that dodges Q forever, classified by
      // fairness. forced ⇒ the cycle literally can't escape to Q (a counterexample even under
      // weak fairness); !forced ⇒ some cycle state has a fair successor that reaches Q, so the
      // dodge survives only WITHOUT fairness.
      const tr = _fmtTrace(d.trace);
      const verdict = d.cycle && d.cycle.length
        ? (d.forced
            ? "a run dodges it forever — forced cycle, no escape to Q"
            : "a run can dodge it — but under fairness the cycle escapes to Q; only a counterexample without fairness")
        : "a run gets stuck in a ¬Q sink — never reaches Q";
      out.className = "bad";
      out.textContent = `✗ violated — ${d.predicate}; ${verdict}` + (tr ? `:  ${tr}` : "");
      if (d.trace && d.trace.length >= 2) showTrace(d.trace, verdict, "violation", d.cycle_start);
    }
  } catch (e) { out.className = "bad"; out.textContent = "✕ " + e; }
}

// --- ad-hoc query (⊨? / ∃): the EXISTENTIAL dual of ⊢ verify's □ (Ana #195) -------------
// `var op value ∧ …` — find a REACHABLE state satisfying the conjunction (sat-witness + count +
// trace), instead of checking it holds everywhere. Reuses _INV_RE/_coerce to parse each term,
// the same split as the editor, and showTrace/_fmtTrace to render the path init→witness.

// Parse a conjunction string into a list of `[var, op, value]` terms (the /api/query payload).
// Returns { terms } on success or { error: "<bad term>" } on the first unparseable term — the
// single source of truth for both the one-shot query and an asserted assumption (Ana #240).
function _parseTerms(raw) {
  const parts = (raw || "").split(/\s*(?:∧|\/\\|\band\b)\s*/).filter((p) => p.trim());
  const terms = [];
  for (const part of parts) {
    const m = part.match(_INV_RE);
    if (!m) return { error: part.trim() };
    terms.push([m[1], m[2], _coerce(m[3])]);
  }
  return { terms };
}
// A `[var, op, value]` term rendered back to source text (chip label / readable conjunction).
function _termText(t) { return `${t[0]} ${t[1]} ${t[2]}`; }

// Run /api/query for `terms` and render the verdict into `out`. `nAssume` ≥ 0 tunes the UNSAT
// copy to name the assumption stack ("the last one made it unsat"). Shared by one-shot + stack.
async function _execQuery(out, terms, nAssume) {
  // Busy signal: the search can take ~1.5s on a real-valued model; without it the row looks frozen
  // (Sam #249). Pulse the result + disable the query buttons until it returns.
  out.className = "dim searching"; out.textContent = "⋯ searching…";
  const btns = ["#query-btn", "#query-assert", "#query-clear"].map((s) => $(s)).filter(Boolean);
  btns.forEach((b) => { b.disabled = true; });
  try {
    const res = await fetch("/api/query", {
      method: "POST", headers: { "content-type": "application/json" },
      body: JSON.stringify({ source: editor.getValue(), terms }),
    });
    const d = await res.json();
    if (!d.ok) { out.className = "bad"; out.textContent = "✕ " + (d.error || "query failed"); return; }
    if (!d.satisfiable) {
      out.className = "bad";
      const lead = nAssume
        ? `✗ no reachable state satisfies all ${nAssume} assumption${nAssume === 1 ? "" : "s"} — the last one made it unsat`
        : `✗ no reachable state satisfies it`;
      out.textContent = `${lead} — ${d.predicate}`
        + (d.exhaustive ? ` (searched all ${d.checked} reachable states)` : ` (searched ${d.checked}; capped)`);
      return;
    }
    const w = Object.entries(d.witness || {}).map(([k, v]) => `${k.split(".").pop()}=${v}`).join(" ");
    out.className = "good";
    const under = nAssume ? `⊨ under ${nAssume} assumption${nAssume === 1 ? "" : "s"} — ` : "";
    out.textContent = `✓ ${under}reachable — ${w} (${d.count} of ${d.checked} state${d.checked === 1 ? "" : "s"})`;
    if (d.trace && d.trace.length >= 2) showTrace(d.trace, `a run reaching: ${d.predicate}`, "goal");
    renderMatchWalker(out, d);                 // walk every matching reachable state (Ana #241)
  } catch (e) { out.className = "bad"; out.textContent = "✕ " + e; }
  finally { btns.forEach((b) => { b.disabled = false; }); }
}

// Walk ALL matching reachable states (Ana #241) — the SAT dual of the all-cores enumeration (Alloy's
// "every instance"). When the query matches >1 state, append a ◀ k/N ▶ stepper that cycles through
// them, ringing each on the diagram. fmtState (app.js) + highlightTraceStep are reused.
const _matchWalk = { list: [], i: 0 };
function renderMatchWalker(out, d) {
  const list = (d && d.matches) || [];
  if (list.length < 2) { _matchWalk.list = []; return; }
  _matchWalk.list = list; _matchWalk.i = 0;
  const nav = document.createElement("span");
  nav.className = "match-walk";
  const draw = () => {
    nav.innerHTML = "";
    const m = _matchWalk.list[_matchWalk.i];
    const prev = document.createElement("button");
    prev.className = "trace-nav"; prev.textContent = "◀"; prev.disabled = _matchWalk.i === 0;
    prev.onclick = () => { _matchWalk.i--; draw(); };
    const lab = document.createElement("span");
    lab.className = "match-lab";
    lab.textContent = ` walk matches ${_matchWalk.i + 1}/${_matchWalk.list.length}${d.matches_capped ? "+" : ""}: ${fmtState(m)} `;
    const next = document.createElement("button");
    next.className = "trace-nav"; next.textContent = "▶"; next.disabled = _matchWalk.i >= _matchWalk.list.length - 1;
    next.onclick = () => { _matchWalk.i++; draw(); };
    nav.append(prev, lab, next);
    highlightTraceStep(m);                     // ring this match on the diagram
  };
  draw();
  out.appendChild(nav);
}

// One-shot ⊨? query: parse the whole conjunction in #query-prop and search, leaving the stack
// untouched (the additive original behaviour — a bare conjunction still works, Ana #240).
async function runQuery() {
  const out = $("#query-result");
  clearTrace();                                          // a new query invalidates the old scrubber
  const raw = $("#query-prop").value.trim();
  if (!raw) { out.textContent = ""; return; }
  const p = _parseTerms(raw);
  if (p.error) { out.className = "bad"; out.textContent = `✕ bad term “${p.error}” — write  var op value  (e.g. timer = 2)`; return; }
  await _execQuery(out, p.terms, 0);
}

// --- assumption stack (Z3 push/pop interrogation, Ana #240) -----------------------------
// An accumulated list of asserted terms. Assert pushes onto it; ✕ on a chip retracts; every
// push/pop re-queries under the FULL stack via the same /api/query primitive. Pure list ops
// (`_pushAssumption` / `_popAssumption`) return NEW arrays so they're unit-testable DOM-free.
const _assumptions = [];
function _pushAssumption(list, term) { return list.concat([term]); }
function _popAssumption(list, idx) { return list.filter((_, i) => i !== idx); }

// Re-render the chip row from `_assumptions`; ✕ retracts that chip. Hidden when empty.
function renderAssumptions() {
  const row = $("#query-stack"), chips = $("#query-chips");
  row.hidden = _assumptions.length === 0;
  chips.innerHTML = "";
  _assumptions.forEach((t, i) => {
    const chip = document.createElement("span"); chip.className = "assume-chip";
    const lab = document.createElement("span"); lab.textContent = _termText(t); chip.appendChild(lab);
    const x = document.createElement("button"); x.className = "assume-x"; x.textContent = "✕";
    x.title = "retract this assumption"; x.onclick = () => retractAssumption(i);
    chip.appendChild(x); chips.appendChild(chip);
  });
}
// Search under the current stack (or clear the result + trace when the stack empties).
async function queryUnderStack() {
  const out = $("#query-result");
  clearTrace();
  if (!_assumptions.length) { out.className = "dim"; out.textContent = ""; return; }
  await _execQuery(out, _assumptions.slice(), _assumptions.length);
}
// Assert (push): parse #query-prop's conjunction, push each term, clear the input, re-query.
async function assertAssumption() {
  const inp = $("#query-prop"), out = $("#query-result");
  const raw = inp.value.trim();
  if (!raw) return;
  const p = _parseTerms(raw);
  if (p.error) { out.className = "bad"; out.textContent = `✕ bad term “${p.error}” — write  var op value  (e.g. timer = 2)`; return; }
  let next = _assumptions;
  for (const t of p.terms) next = _pushAssumption(next, t);
  _assumptions.length = 0; _assumptions.push(...next);
  inp.value = "";                                        // cleared, ready for the next assertion
  renderAssumptions();
  await queryUnderStack();
}
// Retract (pop): drop assumption `idx` and re-query under the remaining stack.
async function retractAssumption(idx) {
  const next = _popAssumption(_assumptions, idx);
  _assumptions.length = 0; _assumptions.push(...next);
  renderAssumptions();
  await queryUnderStack();
}
// Clear all: reset the stack and the result.
async function clearAssumptions() {
  _assumptions.length = 0;
  renderAssumptions();
  await queryUnderStack();
}

// Example-query chips so a newcomer isn't typing blind, guessing var names (Sam #248). Built from a
// REAL reachable state when the lead view carries sample points (clicking is then a guaranteed hit
// that shows a witness + trace), else from the bare var names. Called by paint() with the analyze data.
function renderQuerySuggestions(data) {
  const row = $("#query-suggest");
  if (!row) return;
  if (!data || $("#query-row").hidden) { row.hidden = true; row.innerHTML = ""; return; }
  const sample = (data.points && data.points[0] && data.points[0].state) || null;
  let chips = [];
  if (sample) chips = Object.entries(sample).slice(0, 4).map(([k, v]) => `${k.split(".").pop()} = ${v}`);
  else if (data.vars && data.vars.length) chips = data.vars.slice(0, 4).map((v) => `${v} = `);
  if (!chips.length) { row.hidden = true; row.innerHTML = ""; return; }
  row.hidden = false;
  row.innerHTML = `<span class="suggest-label">try:</span>`
    + chips.map((c) => `<button class="suggest-chip" type="button">${escapeHtml(c)}</button>`).join("");
  row.querySelectorAll(".suggest-chip").forEach((btn) => {
    btn.onclick = () => {
      const f = $("#query-prop");
      f.value = btn.textContent.trim();
      f.focus();
      if (!/=\s*$/.test(f.value)) runQuery();   // a complete "var = value" chip → run it; a bare "var =" waits for input
    };
  });
}

// --- explore-from-a-clicked-state (#242): "assume the machine is HERE" --------------
// A diagram point (overlayPoints in app.js) is clicked → POST /api/explore for that point's
// `state` and answer Ana's two reachability questions: what runs FORWARD from here (count +
// a csv of the set) and what run LEADS here (init→state, scrubbed onto the diagram via the
// same showTrace ring the query/verify paths use). Renders into the shared #query-result line.
async function explorePoint(state) {
  const out = $("#query-result");
  if (!out) return;
  clearTrace();                                          // a new explore invalidates the old scrubber
  const here = fmtState(state);
  out.className = "dim"; out.textContent = `▸ from ${here} — exploring…`;
  try {
    const res = await fetch("/api/explore", {
      method: "POST", headers: { "content-type": "application/json" },
      body: JSON.stringify({ source: editor.getValue(), state }),
    });
    const d = await res.json();
    if (!d.ok) { out.className = "bad"; out.textContent = "✕ " + (d.error || "explore failed"); return; }
    const fwd = `${d.forward_count}${d.forward_capped ? "+" : ""} state${d.forward_count === 1 ? "" : "s"} reachable forward`;
    const back = d.is_initial ? "0 steps from init (this is the start)"
      : (d.trace_to ? `${d.trace_to.length - 1} steps from init` : "unreachable from init");
    const cyc = d.reaches_init && !d.is_initial ? " · ↺ returns to init" : "";
    out.className = "good";
    out.innerHTML = "";
    out.appendChild(document.createTextNode(`▸ from ${here} — ${fwd} · ${back}${cyc}  `));
    if (d.forward && d.forward.length) out.appendChild(_exploreCsvLink(d.forward, state));
    if (d.trace_to && d.trace_to.length >= 2) {
      out.appendChild(_exploreScrubLink(d.trace_to));
      showTrace(d.trace_to, "a run reaching this state");
    }
  } catch (e) { out.className = "bad"; out.textContent = "✕ " + e; }
}

// "↧ csv" download of the forward-reachable set (the states explore found from the click).
function _exploreCsvLink(forward, start) {
  const cols = Object.keys(start || forward[0] || {});
  const esc = (v) => /[",\n]/.test(String(v)) ? `"${String(v).replace(/"/g, '""')}"` : String(v);
  const rows = [cols.join(","), ...forward.map((s) => cols.map((c) => esc(s[c])).join(","))];
  const a = document.createElement("a");
  a.className = "explore-link"; a.textContent = "↧ csv"; a.title = "download the forward-reachable set";
  a.href = URL.createObjectURL(new Blob([rows.join("\n")], { type: "text/csv" }));
  a.download = "reachable-forward.csv";
  return a;
}

// "↧ scrub" — re-open the init→state stepper if the user dismissed it (showTrace already opened it).
function _exploreScrubLink(trace) {
  const a = document.createElement("a");
  a.className = "explore-link"; a.textContent = "↧ scrub run";
  a.title = "step through the run that leads here"; a.href = "#";
  a.onclick = (e) => { e.preventDefault(); showTrace(trace, "a run reaching this state"); };
  return a;
}

// --- wiring: attach the verify console + field-shortcut listeners ------------------
// Mirrors the original top-level wiring (same elements, same listeners).
function initVerify() {
  $("#inv-btn").onclick = checkInvariant;
  // The ⊢ verify box accepts the SAME typable shortcuts as the editor — a newcomer who learned
  // `\ge → ≥` / `>=` shouldn't get bounced when they reuse it here (Sam #212/#160).
  $("#inv-prop").addEventListener("input", () => expandFieldSymbols($("#inv-prop")));
  $("#solve-given").addEventListener("input", () => expandFieldSymbols($("#solve-given")));
  $("#inv-prop").addEventListener("keydown", (e) => { if (e.key === "Enter") checkInvariant(); });
  $("#solve-close").onclick = () => { $("#solve").hidden = true; };
  $("#solve-given").addEventListener("keydown", (e) => { if (e.key === "Enter") solve(false); });
  // ad-hoc query row (⊨?) — same field-shortcut expansion (Ana #195). "find" runs a one-shot
  // query (bare conjunction, original behaviour); "assert ⊢+" / Enter pushes onto the assumption
  // stack and re-queries under the FULL stack; "clear" resets it (the push/pop loop, Ana #240).
  $("#query-btn").onclick = runQuery;
  $("#query-assert").onclick = assertAssumption;
  $("#query-clear").onclick = clearAssumptions;
  $("#query-prop").addEventListener("input", () => expandFieldSymbols($("#query-prop")));
  $("#query-prop").addEventListener("keydown", (e) => { if (e.key === "Enter") assertAssumption(); });
}
