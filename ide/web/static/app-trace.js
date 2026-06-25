"use strict";

// ==============================================================================
// app-trace.js — the scrubbable counterexample/witness TRACE + its diagram marker.
//
// The TLA+-Toolbox-style stepper (Ana #198/#120/#175) and the "trace lights up the
// explorer" diagram highlight (#231/#206 + the #354 ring/cursor). Split out of
// app-verify.js to keep both files under the CLAUDE.md ≤500-line convention — a single
// concern: stepping through a state path and marking the current step on the live
// diagram. Behaviour-preserving move.
//
// Consumers (checkInvariant / runTemporal / runQuery / explorePoint in app-verify.js,
// loadProgram in app-buffer.js, run() in app.js) call showTrace / clearTrace / _fmtTrace;
// those resolve at call time, so this file loading BEFORE them is safe. Cross-file globals
// referenced: `$`, `editor`, `fmtState` (app.js), `lastOverlay` (app-structure.js).
// ==============================================================================

// --- counterexample-trace pure formatters (Ana #173/#175/#198) --------------------
// A counterexample run as a compact trace: init → … → the offending state.
function _fmtTrace(trace) {
  if (!trace || trace.length < 2) return "";
  const steps = trace.map((s) => Object.entries(s).map(([k, v]) => `${k.split(".").pop()}=${v}`).join(" "));
  return steps.length > 8 ? steps.slice(0, 4).join(" → ") + " → … → " + steps[steps.length - 1]
                          : steps.join(" → ");
}
// Pure index/format helpers for the scrubbable stepper (the DOM stepper is below).
function _traceClamp(i, n) { return i < 0 ? 0 : (i > n - 1 ? n - 1 : i); }
function _traceStepLabel(i, n) { return `step ${i + 1} / ${n}`; }       // 1-based for humans
function _traceStateLine(state) {
  return Object.entries(state || {}).map(([k, v]) => `${k.split(".").pop()} = ${v}`).join("   ");
}

// --- trace → diagram highlight (#231/#206: the TLA+/Alloy "trace lights up the explorer") ----
// `lastOverlay` (app-structure.js) holds the live `.view-wrap` + its identifiable points; the
// scrubber rings the point matching the current step. A trace step is a state dict; a point carries
// the same shape as `point.state`. Match on the SHARED leaf keys: a point matches iff every key
// present in BOTH agrees (after the k.split('.').pop() leaf-name normalization the rest of the code
// uses). Pure + total → unit-testable headless. Returns the matching point, or null.
function _matchPoint(points, stepState) {
  if (!points || !points.length || !stepState) return null;
  const leaf = (o) => {
    const m = {};
    for (const k of Object.keys(o || {})) m[k.split(".").pop()] = o[k];
    return m;
  };
  const want = leaf(stepState);
  const valid = points.filter((p) => typeof p.fx === "number" && typeof p.fy === "number");
  // (1) EXACT match on shared leaves — the discrete case (state_graph / integer FSMs). Every shared key
  // must agree by string value; this is the original behaviour and stays exact for discrete states.
  for (const p of valid) {
    const have = leaf(p.state);
    let shared = 0, ok = true;
    for (const k of Object.keys(want)) {
      if (k in have) { shared++; if (String(have[k]) !== String(want[k])) { ok = false; break; } }
    }
    if (ok && shared > 0) return p;
  }
  // (2) #354: NEAREST match on shared NUMERIC leaves — the continuous case (phase_portrait / solution_space
  // over real-valued vars). A trace step's float (s=59.347…) will never string-equal a vector-field sample
  // point, so ring the closest sample by Euclidean distance over the shared numeric axes. Discrete views
  // never reach here (they exact-matched above); a view with no numeric overlap returns null (no ring).
  const numKeys = Object.keys(want).filter((k) => typeof want[k] === "number");
  let best = null, bestD = Infinity;
  for (const p of valid) {
    const have = leaf(p.state);
    let d = 0, shared = 0;
    for (const k of numKeys) {
      if (typeof have[k] === "number") { const dx = have[k] - want[k]; d += dx * dx; shared++; }
    }
    if (shared > 0 && d < bestD) { bestD = d; best = p; }
  }
  return best;
}
// Remove any prior trace-step marker from the live overlay (each scrub step replaces it). Covers both
// the round ring (scatter views) and the full-height vertical cursor (#354 time_series tick views).
function clearTraceRing() {
  if (lastOverlay && lastOverlay.wrap) {
    lastOverlay.wrap.querySelectorAll(".trace-ring, .trace-cursor").forEach((m) => m.remove());
  }
}
// Mark the current trace step on the live `.view-wrap`. No-op when there's no live overlay, no points,
// or no match — the stepper still works regardless. The marker is pointer-events:none (app.css) so it
// never blocks the underlying .pt-target hover targets. #354: a matched point carrying a `tick` field
// (time_series, which plots ticks on the x-axis) gets a full-height VERTICAL CURSOR at its fx instead of
// a point-ring; everything else (state_graph / phase_portrait / solution_space) keeps the round ring.
function highlightTraceStep(stepState) {
  clearTraceRing();
  if (!lastOverlay || !lastOverlay.wrap) return;
  const p = _matchPoint(lastOverlay.points, stepState);
  if (!p) return;
  const marker = document.createElement("div");
  if (p.tick !== undefined) {
    marker.className = "trace-cursor";
    marker.style.left = (p.fx * 100) + "%";        // a vertical line at the tick's x-fraction
  } else {
    marker.className = "trace-ring";
    marker.style.left = (p.fx * 100) + "%";
    marker.style.top = (p.fy * 100) + "%";
  }
  lastOverlay.wrap.appendChild(marker);
}

// --- scrubbable counterexample trace (TLA+-Toolbox style, Ana #198/#120/#175) ----------
// The trace array is the BFS path init→violation (safety) or the dodging/lasso run (liveness).
// Step through it one state at a time, reading the FULL assignment at each step — not the
// one-line collapse. Pure helpers (_traceClamp / _traceStepLabel / _traceStateLine, above) carry
// the index + format logic so they're unit-testable without a DOM.
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
