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
// Send a MULTI-TERM safety check (#381): a CONJUNCTION (`{ terms }`) or an IMPLICATION
// (`{ antecedent, consequent }`), each a list of [var, op, value]. Same /api/invariant
// endpoint, same response contract as _checkOne.
async function _checkPredicate(body) {
  const res = await fetch("/api/invariant", {
    method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify({ source: editor.getValue(), scope: scopeBound, ...body }),
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
  // SAFETY (□), #381: an IMPLICATION  A ⇒ C  (each side a ∧-conjunction), or a CONJUNCTION
  // A ∧ B …, or a two-sided range, or a single comparison. The first split that matches wins;
  // ⇒ is tried before ∧ so `a ∧ b ⇒ c` reads as (a∧b)⇒c, not a∧(b⇒c).
  const imp = raw.split(/\s*(?:⇒|=>|→|⟹)\s*/);
  if (imp.length === 2) {
    const A = _parseTerms(imp[0]), C = _parseTerms(imp[1]);
    if (A.error || C.error || !(A.terms || []).length || !(C.terms || []).length) {
      out.className = "bad";
      out.textContent = "✕ implication: write  A ⇒ C  (e.g. count = 5 ⇒ done = true)";
      return;
    }
    return _runSafety(out, { antecedent: A.terms, consequent: C.terms });
  }
  // Multi-term CONJUNCTION (an explicit ∧, not the lo ≤ var ≤ hi range, which has no ∧).
  if (/(?:∧|\/\\|\band\b)/.test(raw)) {
    const P = _parseTerms(raw);
    if (P.error || !(P.terms || []).length) {
      out.className = "bad";
      out.textContent = `✕ conjunction: each term must be  var op value  (bad: ${P.error || raw})`;
      return;
    }
    return _runSafety(out, { terms: P.terms });
  }
  // Two-sided range  lo (<|≤) var (<|≤) hi  → a conjunction of two predicates.
  const rg = raw.match(_INV_RANGE);
  if (rg) {
    const [, lo, lop, varName, hop, hi] = rg;
    const terms = [[varName, lop === "<" ? ">" : ">=", _coerce(lo)], [varName, hop, _coerce(hi)]];
    return _runSafety(out, { terms });
  }
  // Single comparison.
  const mt = raw.match(_INV_RE);
  if (!mt) { out.className = "bad"; out.textContent = "✕ write  var op value  (e.g. count ≤ 5  ·  0 ≤ x ≤ 6  ·  a ∧ b ⇒ c)"; return; }
  return _runSafety(out, { var: mt[1], op: mt[2], value: _coerce(mt[3]) });
}

// Run ONE safety check (#381) against /api/invariant for `body` (a single-var, conjunction, or
// implication payload) and render the proof/counterexample verdict. The whole safety property is
// now one server scan, so the verdict is HOLDS/VIOLATED for the property as a whole — not a loop
// of per-term checks.
async function _runSafety(out, body) {
  out.className = "dim"; out.textContent = "checking…";
  try {
    const d = (body.var !== undefined)
      ? await _checkOne(body.var, body.op, body.value)
      : await _checkPredicate(body);
    if (!d.ok) { out.className = "bad"; out.textContent = "✕ " + (d.error || "check failed"); return; }
    if (!d.holds) {
      const cex = Object.entries(d.counterexample || {}).map(([k, v]) => `${k.split(".").pop()}=${v}`).join(", ");
      const tr = _fmtTrace(d.trace);
      out.className = "bad";
      out.textContent = `✗ violated (${d.predicate}) — counterexample  ${cex}` + (tr ? `   ·   trace: ${tr}` : "");
      if (d.trace && d.trace.length >= 2) showTrace(d.trace, d.predicate);
      return;
    }
    _verifyGood(out, (d.exhaustive ? "✓ proven" : "✓ holds (bounded)")
      + ` — ${d.predicate} on all ${d.checked} reachable states`);
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
