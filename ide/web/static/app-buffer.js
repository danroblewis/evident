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

// #364: the PRISTINE source last loaded (sample / slot / shared link / default), plus how to re-load it
// (its slot name + headline view). "Revert" re-loads this baseline, discarding any edits since. Set by
// loadProgram on every load; app.js's editor bootstrap seeds it for the first buffer too.
let _loadedBaseline = null, _loadedSlot = null, _loadedView = undefined;

// #364: a minimal newcomer starter for "new file" — an empty fsm reads better than a blank buffer (it
// shows the shape to fill in) without prescribing a model. analyze() on it just nudges "write a constraint".
const NEW_BUFFER_STUB =
`fsm machine
    -- carried state — e.g.  count ∈ Int := 0
    -- the rule each tick — e.g.  Δcount = 1
`;

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
  _loadV++;                        // #359: bump the load token so any in-flight debounced analyze skips
  currentSlotName = slotName || null;
  // #364: remember this load as the revert baseline (the pristine source + how to re-load it).
  _loadedBaseline = source; _loadedSlot = slotName || null; _loadedView = view;
  editor.setValue(source, -1);
  $("#solve-given").value = "";   // a fresh program must not inherit the last pin…
  $("#solve").hidden = true;       // …nor leave a stale UNSAT/witness over the new program
  $("#inv-prop").value = "";       // …nor a stale verify assertion (Sam #107)
  $("#inv-result").textContent = "";
  $("#query-prop").value = "";      // …nor a stale ad-hoc query or assumption stack (Ana #255):
  $("#query-result").textContent = "";   // the `light = Green` chip must not persist onto `counter`.
  if (typeof clearAssumptions === "function") clearAssumptions();
  clearTrace();
  // #447: a load is a definitive model change — close any open verdict dossier so it can't show the
  // previous program's verdict during the gap before the new analyze lands (renderStructure re-binds it).
  if (typeof closeModal === "function" && !$("#ck-modal").hidden && $("#ck-modal-title").textContent.startsWith("Verdict")) closeModal();
  run(view);                       // a sample jumps to its headline view (run() also refreshes the explainer)
}

// #364: REVERT — discard edits and re-load the source we last loaded (sample / slot / shared / default).
// Destructive (it throws away the current buffer), so confirm first UNLESS there's nothing to lose (the
// buffer already equals the baseline). Re-loading through loadProgram re-derives #fname/explainer and
// clears the stale solve/verify/query/trace state, same as opening a sample. No baseline ⇒ clear instead.
function revertBuffer() {
  if (_loadedBaseline == null) { newBuffer(); return; }
  if (editor.getValue() === _loadedBaseline) {       // already pristine — re-running is the most it can do
    setStatus("already the loaded version — nothing to revert", "dim");
    return;
  }
  if (!window.confirm("Revert to the loaded version? Your edits will be discarded.")) return;
  loadProgram(_loadedBaseline, _loadedSlot, _loadedView);
  setStatus("reverted to the loaded version ✓", "ok");
}

// #364: NEW — clear to a minimal starter stub (an empty fsm — the shape to fill in). Destructive when the
// buffer holds real work, so confirm unless it's already the stub / empty / pure whitespace. Routes through
// loadProgram so all the per-program panels reset exactly as a sample load would.
function newBuffer() {
  const cur = editor.getValue().trim();
  const trivial = cur === "" || cur === NEW_BUFFER_STUB.trim();
  if (!trivial && !window.confirm("Start a new empty file? Your current program will be discarded.")) return;
  currentSlotName = null;
  loadProgram(NEW_BUFFER_STUB, null);
  $("#fname").textContent = "untitled.ev";
  setStatus("new file ✓ — write a constraint to see the dynamics", "ok");
}

// --- wiring: save/export/share buttons + the #samples dropdown ---------------------
function initBuffer() {
  // #364: the FIRST buffer is loaded by app.js's editor bootstrap (not through loadProgram), so seed the
  // revert baseline to whatever bootstrapped — a shared link reverts to the shared source, a persisted /
  // default buffer to what's on screen. initBuffer() runs after the bootstrap, so the editor value is set.
  if (_loadedBaseline == null && typeof editor !== "undefined") _loadedBaseline = editor.getValue();
  if ($("#new-btn"))    $("#new-btn").onclick    = () => newBuffer();      // #364
  if ($("#revert-btn")) $("#revert-btn").onclick = () => revertBuffer();   // #364
  if ($("#save-btn"))   $("#save-btn").onclick   = () => saveAsPrompt();
  if ($("#export-btn")) $("#export-btn").onclick = () => exportEv();
  if ($("#share-btn"))  $("#share-btn").onclick  = () => copyShareLink();
  sel = $("#samples");
  refreshSamplesMenu();
  sel.onchange = () => {
    // #449: capture the picked value AT EVENT TIME (before the `sel.value=""` reset below) and dispatch on
    // that captured `v`, so a re-entrant change fired by the reset can never re-read a stale value. The
    // load itself (loadProgram → editor.setValue) is fully SYNCHRONOUS, so the last selection always wins
    // the editor — JS runs each change handler to completion before the next; an older pick can't override
    // a newer one through this path. (The analyze RESULT is separately guarded by the #449 _loadV check in
    // app.js's run(), so a stale in-flight analysis can't paint the wrong verdict over the new program.)
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

// All top-level ENTRIES the file declares, in source-declaration order: every `fsm` plus every
// genuine `claim` (sat_/unsat_ test claims excluded). The LAST one is the default entry the
// runtime renders (#290) — helper claims/types come first, the headline entry last.
function topLevelEntries(source) {
  return [...source.matchAll(/^\s*(?:fsm|claim)\s+([A-Za-z_]\w*)/gm)]
    .map((m) => m[1]).filter((n) => !/^(?:sat|unsat)_/.test(n));
}

// Entry picker (#86/#290): when the buffer declares MORE THAN ONE top-level entry (fsm or claim),
// surface a dropdown so the user can choose which one to RENDER and ⊨ Solve — the runtime defaults
// to the LAST-DEFINED, so the dropdown defaults there too. Hidden for single-entry files. Called
// from run() on every analyze; preserves an explicit selection across edits.
function updateClaimPicker(source) {
  const sel = document.querySelector("#claim-select");
  if (!sel) return;
  const entries = topLevelEntries(source);
  if (entries.length > 1) {
    // Keep a still-valid selection; otherwise default to the LAST-DEFINED entry (the runtime's default).
    const cur = entries.includes(sel.value) ? sel.value : entries[entries.length - 1];
    sel.innerHTML = entries.map((c) => `<option${c === cur ? " selected" : ""}>${c}</option>`).join("");
    sel.value = cur;
    sel.hidden = false;
  } else {
    sel.hidden = true; sel.innerHTML = "";
  }
}
