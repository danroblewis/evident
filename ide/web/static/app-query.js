"use strict";

// ==============================================================================
// app-query.js — the EXISTENTIAL/INTERROGATION half of the verify console: the
// ad-hoc ∃ query (⊨?), the Z3 push/pop ASSUMPTION STACK, the verify placeholder +
// query suggestions, and explore-from-a-clicked-state (#242).
//
// Split out of app-verify.js to keep both files under the CLAUDE.md ≤500-line
// convention (#381 grew the safety parser). Hoisted functions in the SAME shared
// global scope — they reference $ / editor / scopeBound and the _INV_RE/_coerce
// parsers (app-verify.js) + showTrace/_fmtTrace (app-trace.js) at CALL time, so
// load order only requires this file be present before initVerify() runs.
// Behaviour-preserving — moved verbatim.
// ==============================================================================

// --- ad-hoc query (⊨? / ∃): the EXISTENTIAL dual of ⊢ verify's □ (Ana #195) -------------
// `var op value ∧ …` — find a REACHABLE state satisfying the conjunction (sat-witness + count +
// trace), instead of checking it holds everywhere. Reuses _INV_RE/_coerce to parse each term,
// the same split as the editor, and showTrace/_fmtTrace to render the path init→witness.

// Parse a conjunction string into a list of `[var, op, value]` terms (the /api/query payload).
// Returns { terms } on success or { error: "<bad term>" } on the first unparseable term — the
// single source of truth for both the one-shot query and an asserted assumption (Ana #240).
function _stripOuterParens(s) {
  // Drop ONE matched outer paren: `(P ∧ Q)` → `P ∧ Q`. A verification engineer parenthesizes the
  // operand by reflex — `□◇ (P ∧ Q)` must parse like `□◇ P ∧ Q` (Ana #263). Only strips when the
  // leading `(` closes at the very end (balanced), so `(a) ∧ (b)` is left alone.
  let t = (s || "").trim();
  while (t.startsWith("(") && t.endsWith(")")) {
    let depth = 0, matched = true;
    for (let i = 0; i < t.length; i++) {
      if (t[i] === "(") depth++;
      else if (t[i] === ")") { depth--; if (depth === 0 && i < t.length - 1) { matched = false; break; } }
    }
    if (!matched || depth !== 0) break;
    t = t.slice(1, -1).trim();
  }
  return t;
}
// #483: flip a comparison operator when the term is written value-op-var ("0 ≤ count" means "count ≥ 0").
const _FLIP_OP = { "<=": ">=", ">=": "<=", "<": ">", ">": "<", "≤": "≥", "≥": "≤", "=": "=", "≠": "≠", "!=": "!=" };
// value-op-var order: a literal (number/true/false/identifier value) then an op then the VAR.
const _INV_RE_FLIPPED = /^\s*(-?\d+(?:\.\d+)?|true|false|[A-Za-z_]\w*)\s*(<=|>=|!=|<|>|=|≤|≥|≠)\s*([A-Za-z_]\w*(?:\.\w+)?)\s*$/;
// a bare Bool term: `done` ⇒ [done,'=','true']; `¬done` / `!done` ⇒ [done,'=','false'] (#483).
const _BARE_BOOL_RE = /^\s*(¬|!|not\s+)?([A-Za-z_]\w*(?:\.\w+)?)\s*$/;

// Parse ONE conjunct into a [var, op, value] term, accepting (1) var-op-value (the original), (2) the
// flipped value-op-var order, and (3) a bare Bool var (optionally ¬-prefixed). Returns null on no match.
function _parseTerm(part) {
  const m = part.match(_INV_RE);
  if (m) return [m[1], m[2], _coerce(m[3])];
  const f = part.match(_INV_RE_FLIPPED);                       // #483: "0 ≤ count" → [count, ≥, 0]
  if (f && _FLIP_OP[f[2]]) return [f[3], _FLIP_OP[f[2]], _coerce(f[1])];
  const b = part.match(_BARE_BOOL_RE);                         // #483: bare Bool → name = true/false
  if (b) return [b[2], "=", _coerce(b[1] ? "false" : "true")]; // a leading ¬/!/not negates it (coerce → Bool)
  return null;
}
function _parseTerms(raw) {
  const parts = _stripOuterParens(raw).split(/\s*(?:∧|\/\\|\band\b)\s*/).filter((p) => p.trim());
  const terms = [];
  for (const part of parts) {
    const term = _parseTerm(part);
    if (!term) return { error: part.trim() };
    terms.push(term);
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
      body: JSON.stringify({ source: editor.getValue(), terms, scope: scopeBound }),
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

// The ⊢verify placeholder, made concrete from the model's OWN vars + a real reachable value (#155):
// an enum carried var becomes a "◇ light = Green" liveness example a newcomer can actually run; a
// numeric var a "timer ≤ 5" safety one — instead of the abstract "var ≤ 5". Falls back to the generic
// hint when there's no sampled state (e.g. a continuous/over-fanned model with no points).
function updateVerifyPlaceholder(data) {
  const el = $("#inv-prop");
  if (!el) return;
  const state = (data && data.points && data.points[0] && data.points[0].state) || null;
  const entries = state ? Object.entries(state).map(([k, v]) => [k.split(".").pop(), v]) : [];
  const en = entries.find(([, v]) => typeof v === "string");      // a real enum var + value (light = Green)
  const nu = entries.find(([, v]) => typeof v === "number");      // a real numeric var (timer)
  if (!en && !nu) {
    el.placeholder = "verify (use your own vars) — safety:  var ≤ 5  ·  0 ≤ var ≤ 10     liveness:  ◇ var = 5  ·  □◇ var = 5  ·  P ⤳ Q  (tick WF / add ‘WF’ for under-fairness)";
    return;
  }
  const safety = nu ? `${nu[0]} ≤ 5` : `${en[0]} = ${en[1]}`;
  const live = en ? `${en[0]} = ${en[1]}` : `${nu[0]} = 5`;
  el.placeholder = `verify your vars — safety:  ${safety}     liveness:  ◇ ${live}  ·  □◇ ${live}  ·  □◇ ${live} WF`;
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
