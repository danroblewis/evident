"use strict";

// app-structure.js — the #structure panel + the interactive diagram overlay.
//
// Two related "read the rendered result" concerns, split out of app.js (loaded before it):
//   • renderStructure(s)  — the SOLVER-computed structure of the whole model (verdict, fixed
//     points / rest states, solution-space bounds).
//   • fmtState / overlayPoints — drop transparent hover targets over the rendered scatter so
//     each point reveals its full state (hover) and can be explored from (click).
//
// These reference globals that live elsewhere by call time: $, gloss (app.js), VERDICTS
// (app-symbols.js), explorePoint (app-verify.js). They are referenced only inside function
// bodies, which run after every script has loaded — so load-order among the scripts is safe.

// The most recent diagram overlay (the live `.view-wrap` + its identifiable points), so the
// trace scrubber can locate the current step's state ON the diagram and ring it (#231/#206 —
// "the trace lights up the explorer"). Set when overlayPoints draws the live overlay; cleared
// by a render with no points so a stale ring never floats over a different picture.
// Read by app-verify.js's trace scrubber (loaded after this file).
let lastOverlay = null;

// The SOLVER-computed structure of the whole model (not a single run): a verdict, the
// rigorous fixed points / equilibria (states the solver proves map to themselves), and the
// exact boundary of the solution space (min..max each variable spans over the reachable set).
function renderStructure(s) {
  const el = $("#structure");
  // the invariant checker only makes sense for an FSM with a reachable set — not a raw claim
  $("#invariant").hidden = !s || !!s.claim;
  $("#query-row").hidden = !s || !!s.claim;            // ad-hoc query shares the verify row's gate
  if ($("#query-row").hidden) { $("#query-stack").hidden = true; $("#query-suggest").hidden = true; }  // hide the chips too
  if (!s) { el.hidden = true; return; }
  el.hidden = false;
  const [icon, name, note] = VERDICTS[s.verdict] || ["·", s.verdict, ""];
  // title= tooltips teach the verification concepts in place — the words appear in the panel,
  // not the editor, so the editor glossary can't reach them (Sam #163).
  const vhelp = "the model's GLOBAL behaviour, solved from the transition relation over the whole "
    + "reachable set — not one simulated run.";
  let html = `<span class="verdict v-${s.verdict}" title="${vhelp}">${icon} ${name}</span>`
    + (note ? `<span class="dim">${note}</span>` : "");
  if (s.fixed_points && s.fixed_points.length) {
    const fp = s.fixed_points.slice(0, 3).map(
      (f) => "(" + Object.entries(f).map(([k, v]) => `${k}=${v}`).join(", ") + ")").join("  ");
    const more = s.fixed_points.length > 3 ? ` +${s.fixed_points.length - 3}` : "";
    const label = s.verdict === "nondeterministic" ? "rest states" : "fixed point";
    const fhelp = "a REACHABLE state the system maps to itself — once here it stays. Found by "
      + "solving T(s,s), then intersected with the reachable set so it's never a phantom.";
    html += `<span class="struct-fp" title="${fhelp}">● ${label}: ${fp}${more}</span>`;
  }
  const b = s.bounds || {}, keys = Object.keys(b);
  if (keys.length) {
    const bstr = keys.map((k) => `${k} ∈ [${b[k][0]}, ${b[k][1]}]`).join("   ");
    const bhelp = "the exact range each variable spans over the solution space — z3-proven "
      + "(Optimize over the unrolled transition), not the min/max of one run.";
    html += `<span class="struct-bounds" title="${bhelp}">⊏ boundary${s.capped ? " (≥, capped)" : ""}: ${bstr}</span>`;
  }
  // #338: the pairwise forced-equal / forced-different decomposition as interrogable text beside the
  // chart (not just baked into the PNG); the relations panel below adds the affine implied ones.
  const _sn = (n) => String(n).split(".").pop();
  if (s.equalities && s.equalities.length) {
    html += `<span class="struct-relations" title="variables forced EQUAL in every solution (claim ∧ a≠b is UNSAT)">= forced equal: ${s.equalities.map(([a, b]) => `${_sn(a)}=${_sn(b)}`).join(", ")}</span>`;
  }
  if (s.inequalities && s.inequalities.length) {
    html += `<span class="struct-relations" title="variables forced DIFFERENT in every solution (claim ∧ a=b is UNSAT)">≠ forced different: ${s.inequalities.map(([a, b]) => `${_sn(a)}≠${_sn(b)}`).join(", ")}</span>`;
  }
  // #341: implied relations are CLICKABLE — each reveals its unsat-core PROOF (which claim constraints
  // force it, "the claim ∧ ¬relation is UNSAT"). Ana's interrogability ask — don't just trust green text.
  if (s.relations && s.relations.length) {
    const rhelp = "affine relations forced in every solution, implied by the claim's constraints. "
      + "Click one to see WHICH constraints force it (the Z3 unsat-core proof).";
    const rels = s.relations.map((r, i) =>
      `<span class="struct-rel" data-i="${i}" title="click for the proof — which constraints force this">${r.eq}</span>`).join("  ");
    html += `<span class="struct-relations" title="${rhelp}">⊢ implied: ${rels}</span><span id="rel-proof" class="dim"></span>`;
  }
  // #334: a witnessing lasso means NOT every run rests — offer to REPLAY the dodging loop in the step
  // scrubber (the same one verify/liveness uses), ringing each state on the live view as you step.
  const lasso = s.rest_cycle && s.rest_cycle.length >= 2 ? s.rest_cycle : null;
  if (lasso) {
    html += ` <button id="replay-lasso" class="struct-replay" title="step through a run that loops forever among non-rest states, never reaching the absorbing set — the witness that not every run rests">▶ replay a dodging loop</button>`;
  }
  // #326: a reachable terminal means a run DOES reach rest — REPLAY the path init→terminal (the concrete
  // witness behind 'TERMINATES at {…}': HOW it gets to rest, not just WHERE).
  const reach = s.reach_path && s.reach_path.length >= 2 ? s.reach_path : null;
  if (reach) {
    html += ` <button id="replay-reach" class="struct-replay" title="step through the path from the initial state to where this run comes to rest — the concrete trajectory into the absorbing set">▶ replay path to rest</button>`;
  }
  // #332: on-demand soundness — cross-check this model's abstract verdict against brute-force enumeration.
  html += ` <button id="verify-snd" class="struct-replay" title="cross-check the abstract verdict (terminal set / reachable box) against a brute-force enumeration of THIS model — a fabrication self-check (#332/#330)">⛨ verify soundness</button><span id="snd-result" class="dim"></span>`;
  el.innerHTML = html;
  if (lasso) {
    const rb = el.querySelector("#replay-lasso");
    if (rb) rb.onclick = () => showTrace(lasso, "a run looping forever — never resting (#333/#334)", "violation", 0);
  }
  if (reach) {
    const pb = el.querySelector("#replay-reach");
    if (pb) pb.onclick = () => showTrace(reach, "the path from init to where the run comes to rest (#326)", "info", -1);
  }
  const vb = el.querySelector("#verify-snd");
  if (vb) vb.onclick = async () => {
    const out = $("#snd-result"); out.textContent = " checking…";
    try {
      const body = JSON.stringify({ source: editor.getValue(), verify_soundness: true });
      const d = await (await fetch("/api/analyze", { method: "POST", headers: { "content-type": "application/json" }, body })).json();
      out.textContent = " " + _fmtSoundness(d.soundness);
    } catch (e) { out.textContent = " ✕ " + e; }
  };
  // #341/#345: click a relation → show its proof. Prefer the #345 Farkas DERIVATION (how the constraints
  // combine to yield it); fall back to the #341 unsat-core list (which constraints) when no combo (reals).
  el.querySelectorAll(".struct-rel").forEach((sp) => {
    sp.onclick = () => {
      const r = s.relations[+sp.dataset.i];
      const why = r.combo ? `derived as  ${r.combo}` : `forced by: ${(r.core || []).join("  ∧  ") || "the claim"}`;
      $("#rel-proof").textContent = ` ⊢ ${r.eq} — ${why}  (Z3-proven: claim ∧ ¬(${r.eq}) is UNSAT)`;
    };
  });
}


function _fmtSoundness(snd) {
  if (!snd) return "no result";
  if (snd.verdict === "n/a" || !snd.applicable) return "soundness: n/a — " + (snd.detail || "not exactly enumerable here");
  if (snd.verdict === "mismatch") return "⚠ SOUNDNESS MISMATCH — " + (snd.detail || "abstract ≠ brute-force; please report");
  if (snd.verdict === "sound") return "✓ cross-checked — the abstract verdict matches a brute-force enumeration of this model";
  return "inconclusive — neither check could run (Z3 undecided + box unbounded); no claim made (#335)";
}

// Interactive diagram overlay (#184): drop transparent hover targets over the rendered
// solution_space scatter — hover → tooltip of that point's full state; click → pin it until
// the next hover. fx/fy are figure fractions from the top-left, so they map directly to a
// wrapper sized exactly to the image. No-op (and clears) when there are no points.
function fmtState(st) {
  // Round float values to ~4 significant figures — a raw f64 like -0.012319949034860988 is noise
  // in a hover tooltip (Marek #228). Integers/bools/enums pass through unchanged.
  const fmtVal = (v) => (typeof v === "number" && !Number.isInteger(v)) ? Number(v.toPrecision(4)) : v;
  return Object.entries(st || {})
    .map(([k, v]) => `${k}=${fmtVal(v)}`).join("  ");
}
function overlayPoints(wrap, points) {
  if (!wrap) { lastOverlay = null; return; }
  // Record the live overlay so the trace scrubber can ring the current step's state on it.
  // Only a wrap WITH plottable points is a useful target; otherwise clear (no ring possible).
  lastOverlay = (points && points.length) ? { wrap, points } : null;
  let pinned = false;
  const show = (txt, x, y) => {
    gloss.textContent = txt; gloss.hidden = false;
    gloss.style.left = Math.min(x + 12, window.innerWidth - 380) + "px";
    gloss.style.top = (y + 18) + "px";
  };
  const hide = () => { if (!pinned) gloss.hidden = true; };
  if (!points || !points.length) return;
  points.forEach((p) => {
    if (typeof p.fx !== "number" || typeof p.fy !== "number") return;
    const t = document.createElement("div");
    t.className = "pt-target";
    t.style.left = (p.fx * 100) + "%";
    t.style.top = (p.fy * 100) + "%";
    const txt = fmtState(p.state);
    t.title = txt + " — click to explore from here";   // native tooltip + explore hint (#242)
    t.addEventListener("mouseenter", (e) => { pinned = false; show(txt, e.clientX, e.clientY); });
    t.addEventListener("mousemove", (e) => { if (!pinned) show(txt, e.clientX, e.clientY); });
    t.addEventListener("mouseleave", hide);
    // Click → pin the tooltip AND explore from this state (#242): "assume the machine is HERE",
    // what's reachable forward + what run leads here. explorePoint lives in app-verify.js.
    t.addEventListener("click", (e) => {
      e.stopPropagation(); pinned = true; show(txt, e.clientX, e.clientY);
      if (typeof explorePoint === "function" && p.state) explorePoint(p.state);
    });
    wrap.appendChild(t);
  });
}
