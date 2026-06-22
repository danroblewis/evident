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

  // UNSAT — with a delta-debugged core (which constraints conflict)
  if (d.satisfied === false) {
    head.innerHTML = `<span class="unsat">⊭ UNSAT</span> — <b>${d.claim || "claim"}</b> has no solution`
      + (pinned.length ? ` <span class="dim">with ${pinned.join(", ")} pinned</span>` : "");
    body.innerHTML = (d.core && d.core.length)
      ? `<div class="dim">conflicting core — removing any one of these makes it solvable:</div>`
        + `<table>${d.core.map((c) => `<tr><td class="k">line ${c.line}</td><td class="v">${escapeHtml(c.text)}</td></tr>`).join("")}</table>`
      : `<span class="dim">no assignment satisfies the constraints${pinned.length ? " under those pins — try different ones." : "."}</span>`;
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
  // LIVENESS first: P ⤳ Q (leads-to), or ◇/eventually Q — routed to the temporal checker (#142).
  const lt = raw.split(/\s*(?:⤳|~>|\bleads to\b)\s*/);
  if (lt.length === 2) {
    const P = lt[0].match(_INV_RE), Q = lt[1].match(_INV_RE);
    if (!P || !Q) { out.className = "bad"; out.textContent = "✕ leads-to: write  P ⤳ Q  (e.g. mode = Coining ⤳ mode = Idle)"; return; }
    return runTemporal(out, { var: Q[1], op: Q[2], value: _coerce(Q[3]), modality: "leads_to",
                              p_var: P[1], p_op: P[2], p_value: _coerce(P[3]) });
  }
  const ev = raw.match(/^\s*(?:◇|eventually)\s+(.+)$/i);
  if (ev) {
    const Q = ev[1].match(_INV_RE);
    if (!Q) { out.className = "bad"; out.textContent = "✕ eventually: write  ◇ var op value  (e.g. ◇ done = true)"; return; }
    return runTemporal(out, { var: Q[1], op: Q[2], value: _coerce(Q[3]), modality: "eventually" });
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
const _trace = { states: [], i: 0, label: "" };
function clearTrace() {
  _trace.states = []; _trace.i = 0; _trace.label = "";
  const el = $("#inv-trace"); el.hidden = true; el.innerHTML = "";
}
function _renderTrace() {
  const el = $("#inv-trace"), n = _trace.states.length;
  if (n < 2) { el.hidden = true; el.innerHTML = ""; return; }
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
  if (last) { const flag = document.createElement("span"); flag.className = "trace-flag"; flag.textContent = "● violation here"; head.appendChild(flag); }
  el.appendChild(head);
  const line = document.createElement("div");
  line.className = "trace-state" + (last ? " bad" : "");
  line.textContent = _traceStateLine(_trace.states[i]);
  el.appendChild(line);
}
// Open the stepper on a fresh trace, parked at the violating (final) step.
function showTrace(trace, label) {
  if (!trace || trace.length < 2) { clearTrace(); return; }
  _trace.states = trace; _trace.i = trace.length - 1; _trace.label = label || "";
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
      out.textContent = (d.exhaustive ? "✓ proven" : "✓ holds (bounded)")
        + ` — ${d.predicate} on all ${d.checked} reachable states`;
    } else {
      const tr = _fmtTrace(d.trace);
      out.className = "bad";
      out.textContent = `✗ violated — ${d.predicate}; a run dodges it forever`
        + (tr ? `:  ${tr}` : "");
      if (d.trace && d.trace.length >= 2) showTrace(d.trace, "a run that dodges it forever");
    }
  } catch (e) { out.className = "bad"; out.textContent = "✕ " + e; }
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
}
