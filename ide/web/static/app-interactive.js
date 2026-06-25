"use strict";

// ==============================================================================
// app-interactive.js — the INTERACTIVE VIEW primitive (#436/#437, web-ide-shell.md
// §R.5). A gallery view that renders an input+result CELL in the #view region
// instead of a PNG: "query" (∃ a reachable state) and "verify" (□ a property over
// all runs). They re-parent the live query/verify controls into the cell, so the
// working /api/query · /api/invariant · /api/temporal + showTrace logic is reused
// verbatim (no fork, no backend touch). Split out of app.js (behaviour-preserving).
// Loaded before app.js. The chip dispatch (app-history.js) calls openInteractiveView;
// app.js's run() reads activeInteractive; app-verify.js reads _verifyModality.
// ==============================================================================

// the active INTERACTIVE view, or null when a normal PNG view is showing. While one is active, an edit
// re-renders IT (re-runs against the new model) rather than snapping to a PNG. Cleared when a normal view
// chip is clicked (run() with an explicit view).
let activeInteractive = null;
let lastViews = [], lastClaim = false;   // the last model's available views + claim flag (so an interactive view can re-render the strip)

// re-parenting bookkeeping: each borrowed control's original parent (#interrogate-stash), for restore-on-leave.
const _interrogateHome = {};
function _stash(id) { const el = $("#" + id); if (el && !_interrogateHome[id]) _interrogateHome[id] = el.parentNode; return el; }
function _restoreInterrogate() {
  // park any borrowed controls back in their off-view home (#interrogate-stash, hidden) — the bottom
  // INTERROGATE panel is gone, so this is "detach to the stash," not "return to a visible panel."
  ["invariant", "query-row", "query-stack", "query-suggest", "inv-trace"].forEach((id) => {
    const el = $("#" + id), home = _interrogateHome[id];
    if (el && home && el.parentNode !== home) home.appendChild(el);
  });
}

function openInteractiveView(v) {
  if (!INTERACTIVE_VIEWS.has(v)) return;
  activeInteractive = v;
  const view = $("#view");
  view.classList.remove("recomputing", "stale", "grabbing");
  _restoreInterrogate();    // park any borrowed controls in the stash BEFORE wiping #view, so innerHTML= can't orphan them
  $("#axes-ctl").hidden = true; $("#allcond-ctl").hidden = true;
  if (v === "query") _openQueryView(view);
  else if (v === "verify") _openVerifyView(view);
  renderViewTabs({ views: lastViews, claim: lastClaim }, v, run);   // highlight the chip + sync its family
}

function _openQueryView(view) {
  view.innerHTML =
    `<div id="qview" class="iview">
       <div class="iview-head">⊨? ∃ a reachable state satisfying your condition — a conjunction (Enter or assert ⊢+ stacks it)</div>
       <div id="qview-slot" class="iview-slot"></div>
       <div id="qview-trace" class="iview-trace"></div>
     </div>`;
  const slot = $("#qview-slot"), traceSlot = $("#qview-trace");
  const qrow = _stash("query-row"), qstack = _stash("query-stack"), qsug = _stash("query-suggest"), qtrace = _stash("inv-trace");
  if (qrow) { qrow.hidden = false; slot.appendChild(qrow); }
  if (qstack) slot.appendChild(qstack);          // visibility managed by renderAssumptions (hidden when empty)
  if (qsug) slot.appendChild(qsug);
  if (qtrace) traceSlot.appendChild(qtrace);     // showTrace renders the init→witness path here
  $("#view-caption").textContent = "the reachable state(s) satisfying your query — a witness + the path that reaches one";
  const inp = $("#query-prop"); if (inp) inp.focus();
}

// #437: the verify view — the same primitive as query, with a richer modality-picker input (Mira R.2b).
// Re-parents the live #invariant row (property field + WF toggle) + #inv-trace into the cell, and adds a
// modality <select> (□ safety / ◇ eventually / □◇ infinitely-often / ⤳ leads-to). On check it prepends the
// chosen modality to the property and calls the existing checkInvariant() — so /api/invariant + /api/temporal
// + showTrace are reused verbatim (zero backend change). Result: the proof card OR the counterexample-trace
// scrubber, both in the cell.
function _openVerifyView(view) {
  view.innerHTML =
    `<div id="vview" class="iview">
       <div class="iview-head">⊢ verify — PROVE a property holds over EVERY reachable state (□ safety) or EVERY run (liveness). A failure gives a counterexample run you can step through.</div>
       <div id="vview-moderow" class="iview-moderow">
         <select id="vview-modality" title="the property shape">
           <option value="safety">□ safety (always)</option>
           <option value="eventually">◇ eventually</option>
           <option value="infinitely_often">□◇ infinitely often</option>
           <option value="leads_to">⤳ leads-to (P ⤳ Q)</option>
         </select>
       </div>
       <div id="vview-slot" class="iview-slot"></div>
       <div id="vview-trace" class="iview-trace"></div>
     </div>`;
  const slot = $("#vview-slot"), traceSlot = $("#vview-trace");
  const inv = _stash("invariant"), itrace = _stash("inv-trace");
  if (inv) { inv.hidden = false; slot.appendChild(inv); }   // the property field + WF + check button, reused
  if (itrace) traceSlot.appendChild(itrace);                // the counterexample / proof scrubber renders here
  // the modality picker drives the property placeholder + prepends the modality glyph at check time
  const modSel = $("#vview-modality");
  modSel.onchange = () => _applyVerifyModality(modSel.value);
  _applyVerifyModality(modSel.value);
  $("#view-caption").textContent = "whether the property holds over ALL runs — a proof card, or a scrubbable counterexample trace";
  const inp = $("#inv-prop"); if (inp) inp.focus();
}

// Set the verify property field's placeholder + the modality the check uses. Safety = bare comparison;
// ◇/□◇ = a conjunction Q; ⤳ = the full "P ⤳ Q" typed in the field. checkInvariant (app-verify.js) reads
// _verifyModality to prepend the modality glyph.
let _verifyModality = "safety";
function _applyVerifyModality(m) {
  _verifyModality = m;
  const inp = $("#inv-prop"); if (!inp) return;
  const ph = {
    safety: "always true — a comparison:  count ≤ 5   ·   0 ≤ timer ≤ 6",
    eventually: "◇ eventually — a conjunction:  done = true   ·   light = Green ∧ timer = 0",
    infinitely_often: "□◇ infinitely often — a conjunction:  light = Yellow",
    leads_to: "P ⤳ Q — both conjunctions:  mode = Coining ⤳ mode = Idle",
  }[m] || "";
  inp.placeholder = ph;
  // WF (fairness) only applies to liveness — disable it for safety
  const wf = $("#fair-ctl"); if (wf) { wf.style.opacity = m === "safety" ? "0.4" : ""; const cb = $("#fair-in"); if (cb) cb.disabled = m === "safety"; }
}
