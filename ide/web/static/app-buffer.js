"use strict";

// ==============================================================================
// app-buffer.js — save / export / share-link / named-slots + the samples menu
// (Task #213). The pure localStorage-slot and share-link codecs (no DOM/editor) are
// headless-testable; the action handlers (Save as… / Delete / Export / Copy link)
// and the #samples dropdown wiring are attached by initBuffer(), called from app.js
// once `editor` exists. Loaded before app.js. Behaviour-preserving move.
//
// NOTE: the SHARED / SAVED consts and the editor.setValue bootstrap stay in app.js
// (they run at editor-init time); only the helpers/handlers live here. sharedFromHash
// is hoisted, so app.js's top-level `const SHARED = sharedFromHash(...)` still resolves.
// ==============================================================================

// The #samples <select>, populated by initBuffer().
let sel = null;

// --- named slots + share-link codecs (pure, no DOM/editor) -------------------------
const SLOTS_KEY = "evident-slots";
function loadSlots() {                              // corrupt / missing map → {} (never throw)
  try {
    const raw = localStorage.getItem(SLOTS_KEY);
    if (!raw) return {};
    const obj = JSON.parse(raw);
    if (!obj || typeof obj !== "object" || Array.isArray(obj)) return {};
    const out = {};                                // keep only string→string entries
    for (const k of Object.keys(obj)) if (typeof obj[k] === "string") out[k] = obj[k];
    return out;
  } catch (e) { return {}; }
}
function writeSlots(map) { try { localStorage.setItem(SLOTS_KEY, JSON.stringify(map)); } catch (e) {} }
function saveSlot(name, source) {                  // add/overwrite; returns the new map
  const map = loadSlots(); map[name] = source; writeSlots(map); return map;
}
function deleteSlot(name) {                         // remove one; returns the new map
  const map = loadSlots(); delete map[name]; writeSlots(map); return map;
}

// Share link: base64 of UTF-8 bytes, then URL-encoded. btoa wants a binary string, so the
// unescape(encodeURIComponent(…)) dance widens unicode to bytes first; decode reverses it.
// Any malformed input (bad base64, non-UTF-8) returns null — the caller falls back to a
// normal load instead of throwing.
function encodeShare(source) {
  return encodeURIComponent(btoa(unescape(encodeURIComponent(source))));
}
function decodeShare(token) {
  try {
    const src = decodeURIComponent(decodeURIComponent(token).replace(/^src=/, ""));
    return decodeURIComponent(escape(atob(src)));
  } catch (e) { return null; }
}
// Pull a shared program out of a location.hash like "#src=<token>"; null if absent/undecodable.
function sharedFromHash(hash) {
  const m = (hash || "").match(/^#src=(.+)$/);
  return m ? decodeShare(m[1]) : null;
}

// --- save / export / share actions (Task #213) ------------------------------------
// "Save as…" prompts for a slot name, stores {name → source} in the evident-slots map, makes
// it the active buffer name, and re-renders the dropdown so it's immediately re-openable.
function saveAsPrompt() {
  const suggested = currentSlotName || ($("#fname").textContent || "untitled.ev").replace(/\.ev$/, "");
  const name = (window.prompt("Save program as:", suggested) || "").trim();
  if (!name) return;
  saveSlot(name, editor.getValue());
  currentSlotName = name;
  $("#fname").textContent = name.replace(/\.ev$/, "") + ".ev";
  refreshSamplesMenu();
  setStatus("saved “" + name + "” ✓", "ok");
}
function deletePrompt() {
  const slots = loadSlots(), keys = Object.keys(slots).sort();
  if (!keys.length) { setStatus("no saved programs to delete", "dim"); return; }
  const name = (window.prompt("Delete which saved program?\n" + keys.join(", "), keys[0]) || "").trim();
  if (!name) return;
  if (slots[name] == null) { setStatus("no saved program named “" + name + "”", "err"); return; }
  deleteSlot(name);
  if (currentSlotName === name) currentSlotName = null;
  refreshSamplesMenu();
  setStatus("deleted “" + name + "” ✓", "ok");
}
function exportEv() {
  const name = ($("#fname").textContent || "model").replace(/\.ev$/, "") || "model";
  const a = document.createElement("a");
  a.href = URL.createObjectURL(new Blob([editor.getValue()], { type: "text/plain" }));
  a.download = name + ".ev";
  a.click(); URL.revokeObjectURL(a.href);
  setStatus("exported " + name + ".ev ✓", "ok");
}
// Download the current diagram as SVG (vector, publication-quality) — the figure half of Ana #244.
// Re-renders the active view server-side as SVG (same renderer, .svg out path) and saves it.
async function exportSVG() {
  if (typeof activeView === "undefined" || !activeView) { setStatus("no diagram to export yet", "dim"); return; }
  setStatus("rendering " + activeView + ".svg…", "dim");
  try {
    const res = await fetch("/api/figure", {
      method: "POST", headers: { "content-type": "application/json" },
      body: JSON.stringify({ source: editor.getValue(), view: activeView }),
    });
    const d = await res.json();
    if (!d.ok || !d.svg) { setStatus("svg export failed: " + (d.error || "?"), "err"); return; }
    const a = document.createElement("a");
    a.href = URL.createObjectURL(new Blob([d.svg], { type: "image/svg+xml" }));
    a.download = `evident-${activeView}.svg`;
    a.click(); URL.revokeObjectURL(a.href);
    setStatus("exported " + activeView + ".svg ✓", "ok");
  } catch (e) { setStatus("svg export error: " + e, "err"); }
}
async function copyShareLink() {
  const url = location.origin + location.pathname + "#src=" + encodeShare(editor.getValue());
  try {
    await navigator.clipboard.writeText(url);
    setStatus("share link copied ✓", "ok");
  } catch (_) {                                      // clipboard blocked → put it in the hash so it's at least visible
    location.hash = "src=" + encodeShare(editor.getValue());
    setStatus("share link in the address bar — copy the URL", "ok");
  }
}

// --- samples menu: open a worked example, or one of your saved programs ------------
// Built-in samples + the user's saved slots share one dropdown; saved slots live under
// a "── my saved ──" optgroup with slot:-prefixed values so they can't collide with a
// sample name. refreshSamplesMenu() rebuilds it after a save/delete.
function refreshSamplesMenu() {
  const slots = loadSlots();
  const slotKeys = Object.keys(slots).sort();
  let html = '<option value="">open sample…</option>' +
    Object.keys(SAMPLES).map((k) => `<option value="${escapeHtml(k)}">${escapeHtml(k)}</option>`).join("");
  if (slotKeys.length) {
    html += '<optgroup label="── my saved ──">' +
      slotKeys.map((k) => `<option value="slot:${escapeHtml(k)}">${escapeHtml(k)}</option>`).join("") +
      '</optgroup>';
  }
  sel.innerHTML = html;
}

// Load a program into the editor and clear every per-program panel that must not bleed
// across (pin, solve board, verify assertion + result, scrubber). Shared by samples,
// slots, and the shared-link loader.
function loadProgram(source, slotName, view) {
  currentSlotName = slotName || null;
  editor.setValue(source, -1);
  $("#solve-given").value = "";   // a fresh program must not inherit the last pin…
  $("#solve").hidden = true;       // …nor leave a stale UNSAT/witness over the new program
  $("#inv-prop").value = "";       // …nor a stale verify assertion (Sam #107)
  $("#inv-result").textContent = "";
  $("#query-prop").value = "";      // …nor a stale ad-hoc query or assumption stack (Ana #255):
  $("#query-result").textContent = "";   // the `light = Green` chip must not persist onto `counter`.
  if (typeof clearAssumptions === "function") clearAssumptions();
  clearTrace();
  run(view);                       // a sample jumps to its headline view; a slot/share → undefined → recommend
}

// --- wiring: save/export/share buttons + the #samples dropdown ---------------------
function initBuffer() {
  if ($("#save-btn"))   $("#save-btn").onclick   = () => saveAsPrompt();
  if ($("#export-btn")) $("#export-btn").onclick = () => exportEv();
  if ($("#share-btn"))  $("#share-btn").onclick  = () => copyShareLink();
  sel = $("#samples");
  refreshSamplesMenu();
  sel.onchange = () => {
    const v = sel.value;
    if (v.startsWith("slot:")) {
      const name = v.slice(5), slots = loadSlots();
      if (slots[name] != null) loadProgram(slots[name], name);
    } else if (SAMPLES[v]) {
      loadProgram(SAMPLES[v], null, headlineView(v, SAMPLES[v]));
    }
    sel.value = "";          // reset the label so the same entry can be re-opened
  };
}

// Entry-claim picker (#86): when the buffer declares MORE THAN ONE non-test claim, auto-pick can't
// choose between them (solve returns satisfied=null), so surface a dropdown to target ⊨ Solve at one.
// Hidden for single-claim / FSM files. Called from run() on every analyze; preserves the selection.
function updateClaimPicker(source) {
  const sel = document.querySelector("#claim-select");
  if (!sel) return;
  const claims = [...source.matchAll(/^\s*claim\s+([A-Za-z_]\w*)/gm)]
    .map((m) => m[1]).filter((n) => !/^(?:sat|unsat)_/.test(n));
  if (claims.length > 1) {
    const cur = sel.value;
    sel.innerHTML = claims.map((c) => `<option${c === cur ? " selected" : ""}>${c}</option>`).join("");
    sel.hidden = false;
  } else {
    sel.hidden = true; sel.innerHTML = "";
  }
}
