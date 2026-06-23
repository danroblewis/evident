"use strict";

// ==============================================================================
// Pan / zoom the diagram surface (#233). Split out of app.js — a self-contained
// concern over #view. A reachable graph at hundreds of states is a hairball under
// fit-to-box; scroll-zoom + drag-pan + dbl-click-reset is table stakes.
//
// We transform the WHOLE `.view-wrap` (image + .pt-target hover targets + .trace-ring),
// not just the <img>, so the interactive overlay stays aligned — the points live at
// fx*100%/fy*100% INSIDE the wrap, so one transform moves them together. transform-origin
// 0 0 makes the translate/scale math exact (top-left anchored).
//
// State + setupPanZoom()/resetPanZoom() live at script-global scope (no module system here);
// app.js calls setupPanZoom() once at boot, and renderLiveView (app-history.js) calls
// resetPanZoom() on every render so a zoom never carries across programs.
// ==============================================================================

const PZ_MIN = 1, PZ_MAX = 8;
let _panzoom = { scale: 1, tx: 0, ty: 0 };

// Pure, headless-testable: new {scale,tx,ty} after zooming by `factor` about (cx,cy) — the
// cursor in wrap-local pixels. Clamp scale to [PZ_MIN, PZ_MAX]; keep the content point under
// the cursor fixed: with origin 0 0, screen = content*scale + t, so holding `cx` fixed gives
// t' = cx - (cx - t) * (scale'/scale). When the clamp bites, scale' = scale → t unchanged.
function zoomAt(state, cx, cy, factor) {
  const scale = Math.min(PZ_MAX, Math.max(PZ_MIN, state.scale * factor));
  const r = scale / state.scale;
  return { scale, tx: cx - (cx - state.tx) * r, ty: cy - (cy - state.ty) * r };
}

function applyPanZoom(wrap) {
  if (!wrap) return;
  wrap.style.transformOrigin = "0 0";
  wrap.style.transform = `translate(${_panzoom.tx}px, ${_panzoom.ty}px) scale(${_panzoom.scale})`;
}

// Reset to identity and re-apply, so a zoom from one program never carries into the next render
// (called by renderLiveView on every paint). At scale 1, tx/ty 0 the transform is a no-op.
function resetPanZoom(wrap) {
  wrap = wrap || $("#view").querySelector(".view-wrap");
  let scale = 1, tx = 0, ty = 0;
  const view = $("#view");
  // Single-view only (the wrap sits directly in #view, which flex-centers it): scale a SMALL diagram
  // UP to fill a larger pane (#176) so it doesn't float in empty space on a big monitor. CSS max-width
  // already handles the down-scale case, so only act when fit > 1. Overlays live in the wrap and scale
  // with it (stay aligned). transform-origin is 0 0 and flex pre-centers the unscaled wrap, so the
  // scaled wrap re-centers with tx = ww·(1−s)/2. Clamp the up-scale so a tiny figure doesn't over-blur.
  if (wrap && view && wrap.parentElement === view) {
    const vb = view.getBoundingClientRect(), ww = wrap.offsetWidth, wh = wrap.offsetHeight;
    if (ww > 0 && wh > 0 && vb.width > 0 && vb.height > 0) {
      const fit = Math.min((vb.width - 16) / ww, (vb.height - 16) / wh);
      if (fit > 1.02) { scale = Math.min(fit, 2.5); tx = ww * (1 - scale) / 2; ty = wh * (1 - scale) / 2; }
    }
  }
  _panzoom = { scale, tx, ty };
  applyPanZoom(wrap);
}

// Wire wheel-zoom / drag-pan / dbl-click-reset ONCE to #view; every handler operates on the
// CURRENT `.view-wrap` (rebuilt on each render), looked up live. Cursor position is taken
// relative to the wrap's box so zoomAt's fixed-point holds regardless of where #view sits.
function setupPanZoom() {
  const view = $("#view");
  if (!view) return;
  const wrap = () => view.querySelector(".view-wrap");
  const localPt = (w, e) => {
    const b = w.getBoundingClientRect();
    return { x: e.clientX - b.left, y: e.clientY - b.top };
  };
  view.addEventListener("wheel", (e) => {
    const w = wrap();
    if (!w) return;
    e.preventDefault();                         // stop the page scrolling under the diagram
    const { x, y } = localPt(w, e);
    // wheel up (deltaY<0) zooms in. Per-notch factor; trackpads send small deltas → small steps.
    _panzoom = zoomAt(_panzoom, x, y, Math.exp(-e.deltaY * 0.0015));
    applyPanZoom(w);
  }, { passive: false });

  let drag = null;
  view.addEventListener("mousedown", (e) => {
    const w = wrap();
    if (!w || e.button !== 0) return;
    drag = { x: e.clientX, y: e.clientY, tx: _panzoom.tx, ty: _panzoom.ty };
    view.classList.add("grabbing");
  });
  window.addEventListener("mousemove", (e) => {
    if (!drag) return;
    _panzoom.tx = drag.tx + (e.clientX - drag.x);
    _panzoom.ty = drag.ty + (e.clientY - drag.y);
    applyPanZoom(wrap());
  });
  window.addEventListener("mouseup", () => { drag = null; view.classList.remove("grabbing"); });
  view.addEventListener("dblclick", () => resetPanZoom(wrap()));
}

// Draggable splitter — drag #splitter to set --split (the editor pane's width), persisted so the
// layout survives a reload (Marek #277). Self-contained IIFE; the DOM exists by the time this loads.
(function initSplitter() {
  const sp = document.querySelector("#splitter"), app = document.querySelector("#app");
  if (!sp || !app) return;
  try { const s = localStorage.getItem("evident-split"); if (s) app.style.setProperty("--split", s); } catch (e) {}
  let dragging = false;
  sp.addEventListener("mousedown", (e) => {
    dragging = true; sp.classList.add("dragging");
    document.body.style.cursor = "col-resize"; document.body.style.userSelect = "none"; e.preventDefault();
  });
  window.addEventListener("mousemove", (e) => {
    if (!dragging) return;
    app.style.setProperty("--split", Math.max(220, Math.min(window.innerWidth - 260, e.clientX)) + "px");
  });
  window.addEventListener("mouseup", () => {
    if (!dragging) return;
    dragging = false; sp.classList.remove("dragging");
    document.body.style.cursor = ""; document.body.style.userSelect = "";
    try { localStorage.setItem("evident-split", getComputedStyle(app).getPropertyValue("--split").trim()); } catch (e) {}
  });
})();
