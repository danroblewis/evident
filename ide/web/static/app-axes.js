"use strict";

// ==============================================================================
// app-axes.js — the #421 AXIS SELECTOR. The "axes ▾" control that rides under the
// figure for the axis-taking views (phase_portrait / nullcline / orbit / occupancy
// / cobweb), letting the user pick which variable goes on each axis. Split out of
// app.js (behaviour-preserving). Loaded before app.js; initAxes() wires the events
// once the core (run, $) exists. paint() calls renderAxesCtl; run() calls
// axisParamsFor() to put the override on the /api/analyze body.
// ==============================================================================

// #421 AXIS OVERRIDE: per-view {x, y} the user explicitly picked, keyed by view name. Sent on the
// request for that view (so editing the code keeps the chosen axes, like preferredView); absent ⇒ the
// backend auto-picks. A different axis-view has its own (usually empty) entry, so switching views echoes
// that view's defaults rather than forcing the last view's axes onto it.
let axisOverride = {};
let _axesView = null;    // the active axis-taking view, so the onchange handlers know which view to re-run
let _axesOpen = false;   // the picker's open/closed state, preserved across re-renders

// The x_var/y_var the request for `view` should carry: the user's override for it, or {} (auto-pick).
function axisParamsFor(view) {
  const ax = (view && axisOverride[view]) || {};
  return { x_var: ax.x || null, y_var: ax.y || null };
}

// Reflect data.axes onto the control, populated from data.vars. Hidden unless the view is axis-taking
// (data.axes != null). cobweb is 1-D → hide the y select. A fell_back flag (the requested var wasn't
// usable) shows a subtle note. The toggle stays open across re-renders if the user opened it.
function renderAxesCtl(data) {
  const ctl = $("#axes-ctl");
  if (!ctl) return;
  if (!data.axes || !data.png) { ctl.hidden = true; _axesView = null; return; }
  _axesView = data.view;
  const vars = data.vars || [];
  const oneD = data.view === "cobweb" || data.axes.y == null || data.axes.y === data.axes.x;
  const fill = (sel, cur) => {
    sel.innerHTML = vars.map((v) => `<option value="${v}"${v === cur ? " selected" : ""}>${v}</option>`).join("");
  };
  fill($("#axes-x"), data.axes.x);
  fill($("#axes-y"), data.axes.y);
  $("#axes-y-wrap").style.display = oneD ? "none" : "";
  const note = $("#axes-note");
  if (data.axes.fell_back) { note.hidden = false; note.textContent = "— requested var unavailable; using auto-pick"; }
  else { note.hidden = true; note.textContent = ""; }
  ctl.hidden = false;
  // keep the picker's open/closed state across re-renders (don't snap it shut on every analyze)
  $("#axes-pick").hidden = !_axesOpen;
  $("#axes-toggle").textContent = _axesOpen ? "axes ▴" : "axes ▾";
}

// pick a new axis var → record the override for THIS view and re-render it (sticky like preferredView)
function _applyAxes() {
  if (!_axesView) return;
  const x = $("#axes-x").value;
  const y = $("#axes-y-wrap").style.display === "none" ? x : $("#axes-y").value;
  axisOverride[_axesView] = { x, y };
  run(_axesView);          // explicit view ⇒ the override for it is sent; sticky across later edits
}

// Wire the toggle + the x/y selects. Called from app.js once `run` + the DOM exist.
function initAxes() {
  $("#axes-toggle").addEventListener("click", () => {
    _axesOpen = !_axesOpen;
    $("#axes-pick").hidden = !_axesOpen;
    $("#axes-toggle").textContent = _axesOpen ? "axes ▴" : "axes ▾";
  });
  $("#axes-x").addEventListener("change", _applyAxes);
  $("#axes-y").addEventListener("change", _applyAxes);
}
