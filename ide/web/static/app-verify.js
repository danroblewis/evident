"use strict";

// ==============================================================================
// app-verify.js — the VERIFY / TEMPORAL / QUERY console: safety + liveness invariant
// checking against the reachable set (□ proof + counterexample), the ∃ ad-hoc query +
// assumption stack, and explore-from-a-clicked-state.
//
// Two sibling concerns were split out to keep this under the CLAUDE.md ≤500-line
// convention: the solve/witness result rendering + domain pictures → app-solve.js, and
// the scrubbable trace + diagram marker → app-trace.js (both loaded before this file).
// This file calls into them at call time — renderSolve/parsePins (app-solve.js) via
// app.js's solve(); showTrace/clearTrace/_fmtTrace (app-trace.js) on every verdict.
//
// Hoisted functions only — they reference $ / editor / scopeBound / lastDropped at CALL
// time, so loading this before app.js is safe. Listeners are attached by initVerify(),
// called from app.js once the core globals exist. Behaviour-preserving.
// ==============================================================================

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
    body: JSON.stringify({ source: editor.getValue(), var: varName, op, value, scope: scopeBound }),
  });
  return res.json();
}
// Strip a trailing WEAK-FAIRNESS suffix (`… WF` / `… under fairness`, Ana #269) from a liveness
// property and OR it with the WF checkbox. Returns { text, fair }: `text` is the property with the
// suffix removed, `fair` true iff either the suffix is present or the checkbox is ticked. Applied
// only on the liveness branches — fairness is meaningless for a □ safety invariant.
function _fairFlag(raw) {
  const m = raw.match(/^(.*?)\s*(?:\bunder\s+fairness\b|\bweak\s+fairness\b|\bWF\b)\s*$/i);
  const checked = (() => { const el = $("#fair-in"); return !!(el && el.checked); })();
  return m ? { text: m[1].trim(), fair: true } : { text: raw, fair: checked };
}

// #451: a verify PASS over a BROKEN model (dropped>0) is a proof about the TRUNCATED program, not what
// the user wrote — so the result card must inherit the BROKEN flag, never read as a clean green proof.
// Sets the success text, then (when dropped>0) de-rates green→amber and appends the caveat.
function _verifyGood(out, text) {
  const dropped = (typeof lastDropped !== "undefined") ? lastDropped : 0;
  if (dropped > 0) {
    out.className = "broken";
    out.textContent = `⚠ ${text.replace(/^✓\s*/, "")} — but over a BROKEN model: ${dropped} constraint(s) dropped; this proof is about the truncated program, not what you wrote.`;
  } else {
    out.className = "good";
    out.textContent = text;
  }
}
async function checkInvariant() {
  const out = $("#inv-result");
  clearTrace();                              // a new check invalidates the old scrubber
  let rawIn = $("#inv-prop").value.trim();
  // #437: in the verify VIEW, the modality comes from the picker, not a typed glyph — prepend it so the
  // existing parser below sees ◇/□◇ (safety + leads-to are bare; the user types P ⤳ Q for leads-to).
  if (typeof activeInteractive !== "undefined" && activeInteractive === "verify" && typeof _verifyModality !== "undefined") {
    const pfx = { eventually: "◇ ", infinitely_often: "□◇ " }[_verifyModality] || "";
    if (pfx && rawIn) rawIn = pfx + rawIn;
  }
  if (!rawIn) { out.textContent = ""; return; }
  // A `WF` / `under fairness` suffix (or the WF checkbox) requests WEAK-FAIRNESS liveness (#269):
  // exclude unfair lassos. Strip the suffix before the modality parse; `fair` rides the temporal body.
  const { text: raw, fair } = _fairFlag(rawIn);
  // LIVENESS first: P ⤳ Q (leads-to), or ◇/□◇ Q. Q and P are CONJUNCTIONS — ◇(timer = 0 ∧ light = Red)
  // — parsed via _parseTerms (the same ∧-splitter the ⊨? query uses), so they're not limited to one
  // var op value (Ana #258/#142).
  const lt = raw.split(/\s*(?:⤳|~>|\bleads to\b)\s*/);
  if (lt.length === 2) {
    const P = _parseTerms(lt[0]), Q = _parseTerms(lt[1]);
    if (P.error || Q.error || !(P.terms || []).length || !(Q.terms || []).length) {
      out.className = "bad"; out.textContent = "✕ leads-to: write  P ⤳ Q  (e.g. mode = Coining ⤳ mode = Idle)"; return;
    }
    return runTemporal(out, { terms: Q.terms, modality: "leads_to", p_terms: P.terms, fair });
  }
  // STRONG liveness □◇Q (infinitely often) — checked BEFORE plain ◇ so the □ prefix isn't
  // swallowed by the ◇ branch. Holds iff no run gets permanently trapped in ¬Q (Ana #260).
  const io = raw.match(/^\s*(?:□◇|◻◇|\[\]<>|infinitely(?:\s+often)?)\s+(.+)$/i);
  if (io) {
    const Q = _parseTerms(io[1]);
    if (Q.error || !(Q.terms || []).length) { out.className = "bad"; out.textContent = "✕ infinitely-often: write  □◇ var op value  (e.g. □◇ light = Yellow)"; return; }
    return runTemporal(out, { terms: Q.terms, modality: "infinitely_often", fair });
  }
  const ev = raw.match(/^\s*(?:◇|<>|eventually)\s+(.+)$/i);
  if (ev) {
    const Q = _parseTerms(ev[1]);
    if (Q.error || !(Q.terms || []).length) { out.className = "bad"; out.textContent = "✕ eventually: write  ◇ var op value  (e.g. ◇ done = true)"; return; }
    return runTemporal(out, { terms: Q.terms, modality: "eventually", fair });
  }
  // WF on a non-liveness property is a user error — fairness only changes a liveness verdict.
  if (fair) { out.className = "bad"; out.textContent = "✕ fairness (WF) applies to LIVENESS only — write  □◇ P  ·  ◇ P  ·  P ⤳ Q"; return; }
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
    _verifyGood(out, (exhaustive ? "✓ proven" : "✓ holds (bounded)")
      + ` — ${texts.join(" ∧ ")} on all ${checked} reachable states`);
  } catch (e) { out.className = "bad"; out.textContent = "✕ " + e; }
}

// The scrubbable counterexample/witness TRACE + its diagram marker (_matchPoint / clearTraceRing /
// highlightTraceStep / clearTrace / _renderTrace / exportTraceCSV / showTrace + the _fmtTrace formatter)
// moved to app-trace.js (loaded before this file). The verify/temporal/query/explore paths below call
// showTrace / clearTrace / _fmtTrace at call time.

// Liveness check (◇ / ⤳) against /api/temporal, with the dodging-run trace on failure.
async function runTemporal(out, body) {
  clearTrace();
  out.className = "dim"; out.textContent = "checking…";
  try {
    const res = await fetch("/api/temporal", {
      method: "POST", headers: { "content-type": "application/json" },
      body: JSON.stringify({ source: editor.getValue(), scope: scopeBound, ...body }),
    });
    const d = await res.json();
    if (!d.ok) { out.className = "bad"; out.textContent = "✕ " + (d.error || "check failed"); return; }
    if (d.holds && d.fair) {
      // (b) — the WHOLE POINT of #269. Under WEAK FAIRNESS the property HOLDS: the goal is reachable
      // from every reachable (P-)state. WITHOUT fairness an unfair lasso dodges P forever; every
      // such dodging run that ignores the always-available path to P is excluded. Make it
      // unmistakable that this verdict depended on fairness — it is NOT an unconditional proof.
      _verifyGood(out, (d.exhaustive ? "✓ holds UNDER FAIRNESS" : "✓ holds UNDER FAIRNESS (bounded)")
        + ` — ${d.predicate} on all ${d.checked} reachable states.`
        + " Without fairness an unfair lasso dodges it forever; every dodging run that ignores the"
        + " always-available path to the goal is excluded.");
    } else if (d.holds) {
      // (a) — HOLDS even WITHOUT fairness (the lasso search found no dodging run at all).
      // For ◇ (AF: every run reaches Q at least once), distinguish RECURRENT (□◇ also holds —
      // Q recurs forever) from TRANSIENT (Q reached, then the system can settle into ¬Q forever).
      // The bare "✓ proven" on a transient ◇ invites a false recurrence reading (Ana #260).
      let note = "";
      if (body.modality === "eventually" && d.recurrent !== undefined) {
        note = d.recurrent
          ? " — and recurs (infinitely often)"
          : " — but TRANSIENT: reached, does not recur (the system settles into ¬Q)";
      }
      _verifyGood(out, (d.exhaustive ? "✓ proven" : "✓ holds (bounded)")
        + ` — ${d.predicate} on all ${d.checked} reachable states` + note);
    } else if (d.trap) {
      // (c) — FAILS even under fairness: a reachable TRAP (a state from which the goal is UNREACHABLE).
      // No escaping cycle to show — the witness is the trap state + the init→trap run.
      const tr = _fmtTrace(d.trace);
      const cex = Object.entries(d.counterexample || {}).map(([k, v]) => `${k.split(".").pop()}=${v}`).join(", ");
      const verdict = `a reachable TRAP (no path to the goal): ${cex} — fairness can't save it`;
      out.className = "bad";
      out.textContent = `✗ violated even UNDER FAIRNESS — ${d.predicate}; ${verdict}` + (tr ? `.  run: ${tr}` : "");
      if (d.trace && d.trace.length >= 2) showTrace(d.trace, verdict, "violation", null);
    } else {
      // Lasso (Ana #239): the run is a STEM into a CYCLE that dodges Q forever, classified by
      // fairness. forced ⇒ the cycle literally can't escape to Q (a counterexample even under
      // weak fairness); !forced ⇒ some cycle state has a fair successor that reaches Q, so the
      // dodge survives only WITHOUT fairness (re-run with WF to confirm it holds under fairness).
      const tr = _fmtTrace(d.trace);
      const verdict = d.cycle && d.cycle.length
        ? (d.forced
            ? "a run dodges it forever — forced cycle, no escape to Q"
            : "a run can dodge it — but under fairness the cycle escapes to Q; only a counterexample without fairness (tick WF)")
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
function _parseTerms(raw) {
  const parts = _stripOuterParens(raw).split(/\s*(?:∧|\/\\|\band\b)\s*/).filter((p) => p.trim());
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
  // optimize row (⤒ max / ⤓ min) — the quantitative query; Enter maximizes by default.
  $("#opt-max").onclick = () => runOptimize("max");
  $("#opt-min").onclick = () => runOptimize("min");
  $("#opt-var").addEventListener("keydown", (e) => { if (e.key === "Enter") runOptimize("max"); });
}
