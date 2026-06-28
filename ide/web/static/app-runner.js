"use strict";
// app-runner.js — the "run" VIEW: renders effect-run output in the #view figure slot,
// exactly like query/verify — no separate panel, no extra button.
//
// Integration points:
//   openInteractiveView("run") in app-interactive.js calls initRunnerView() here.
//   scheduleRunnerIfActive() is called from scheduleAnalyze (app.js) so the output
//   refreshes at the same 350ms cadence as the analysis graphs.
//
// The view is offered for every FSM model (injected in renderViewTabs alongside
// "query"/"verify" by patches in app-history.js). It is purely frontend-driven:
// clicking the "run" chip sets activeInteractive = "run" and renders text output
// into #view with no PNG fetch.
//
// Public surface:
//   initRunnerView(view)       — called by openInteractiveView("run") in app-interactive.js
//   scheduleRunnerIfActive()   — called by scheduleAnalyze (app.js) on every editor change
//   initRunner()               — no-op init (called from app.js init sequence)

// --- state ------------------------------------------------------------
let _runTimer = null;   // debounce handle — mirrors the 350ms analyze timer

// --- editor source (strict-mode `const` doesn't go on window) --------
function _editorSource() {
  const el = document.getElementById("code");
  return el && el.env && el.env.editor ? el.env.editor.getValue() : "";
}

// --- render output into #view ----------------------------------------
// Follows the same pattern as _openQueryView / _openVerifyView in app-interactive.js:
// set innerHTML on #view, update the caption, hide auxiliary controls.
function initRunnerView(view) {
  if (!view) view = document.getElementById("view");
  if (!view) return;
  view.classList.remove("recomputing", "stale", "grabbing");
  view.innerHTML =
    '<div id="rview" class="iview">'
    + '<pre id="rview-output" class="runner-out">⟳ running…</pre>'
    + '<div id="rview-exit" class="dim"></div>'
    + '</div>';
  const cap = document.getElementById("view-caption");
  if (cap) cap.textContent = "live effect-run output — re-runs on every edit at the analysis cadence";
  const axCtl = document.getElementById("axes-ctl");
  if (axCtl) axCtl.hidden = true;
  const acCtl = document.getElementById("allcond-ctl");
  if (acCtl) acCtl.hidden = true;
  // Kick off the run immediately when the view opens.
  _doRun();
}

// --- the actual fetch -------------------------------------------------
async function _doRun() {
  const source = _editorSource();
  const outEl = document.getElementById("rview-output");
  const exitEl = document.getElementById("rview-exit");
  if (!outEl) return;   // view not currently open — nothing to update

  outEl.textContent = "⟳ running…";
  if (exitEl) exitEl.textContent = "";

  try {
    const res = await fetch("/api/run", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ source }),
    });
    if (!res.ok) {
      outEl.textContent = `backend error: HTTP ${res.status}`;
      if (exitEl) { exitEl.textContent = "error"; exitEl.className = "runner-exit-err"; }
      return;
    }
    const d = await res.json();
    const combined = [d.stdout, d.stderr].filter(Boolean).join("");
    outEl.textContent = combined || "(no output — program produced nothing)";

    const parts = [];
    if (d.timed_out) parts.push("timed out");
    else parts.push(`exit ${d.exit_code}`);
    parts.push(`(max steps: ${d.max_steps})`);
    if (d.note) parts.push("· " + d.note);
    if (exitEl) {
      exitEl.textContent = parts.join(" ");
      exitEl.className = d.exit_code === 0 && !d.timed_out ? "runner-exit-ok" : "runner-exit-err";
    }
    outEl.scrollTop = outEl.scrollHeight;
  } catch (e) {
    outEl.textContent = `Error: ${e}`;
    if (exitEl) { exitEl.textContent = "error"; exitEl.className = "runner-exit-err"; }
  }
}

// --- debounce: same 350ms as scheduleAnalyze -------------------------
function _scheduleRun() {
  clearTimeout(_runTimer);
  _runTimer = setTimeout(_doRun, 350);
}

// Public: called by scheduleAnalyze in app.js each time the editor changes.
// Only re-runs when the run view is currently the active interactive view.
function scheduleRunnerIfActive() {
  if (typeof activeInteractive === "undefined" || activeInteractive !== "run") return;
  _scheduleRun();
}

// Public: no-op init — view is registered purely via patches in app-history.js
// and the openInteractiveView dispatch in app-interactive.js.
function initRunner() {}
