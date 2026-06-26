"use strict";

// ==============================================================================
// app-glossary.js — the VISIBLE, searchable glossary panel + the first-load "hover to
// learn" hint (#375 + #374). The newcomer-learning surface: three ways to reach the SAME
// term definitions (GLOSSARY in app-symbols.js, the one source of truth) —
//   • hover a keyword in the editor (app-editor.js, glossFor) — the inline card,
//   • ⌘K "Glossary: <term>" rows (app-palette.js) — the keyboard route,
//   • the visible "📖 glossary" toolbar button → this panel — for someone who doesn't know ⌘K.
//
// This panel reuses glossaryItems() (app-symbols.js) — it does NOT fork the definitions. A
// search box filters term + definition text. Loaded before app.js; initGlossary() wires the
// button + first-load hint, called from app.js's init sequence.
// ==============================================================================

let _glossPanel = null;

// Build (once) + return the floating glossary panel: a search box over the full {term, def} list.
function _glossEl() {
  if (_glossPanel) return _glossPanel;
  _glossPanel = document.createElement("div");
  _glossPanel.id = "glossary"; _glossPanel.hidden = true;
  _glossPanel.innerHTML =
    '<div class="gloss-head">glossary — what the words mean'
    + ' <span class="dim">(hover a keyword in the editor too · Esc closes)</span></div>'
    + '<input id="gloss-search" placeholder="search a term or its meaning…  (claim · Δ · := · cyclic)" autocomplete="off" spellcheck="false">'
    + '<div id="gloss-list"></div>';
  document.body.appendChild(_glossPanel);
  _glossPanel.querySelector("#gloss-search").addEventListener("input", (e) => _renderGlossList(e.target.value));
  return _glossPanel;
}

// Render the filtered term list. Each row: the term + its plain-English definition. The GLOSSARY
// values already lead with "term — …", so show the def text verbatim (its leading term is the heading).
function _renderGlossList(query) {
  const list = _glossPanel.querySelector("#gloss-list");
  const q = (query || "").trim().toLowerCase();
  const items = (typeof glossaryItems === "function" ? glossaryItems() : [])
    .filter(({ term, def }) => !q || term.toLowerCase().includes(q) || def.toLowerCase().includes(q));
  if (!items.length) { list.innerHTML = `<div class="gloss-none dim">no term matches “${escapeHtml(query)}”</div>`; return; }
  list.innerHTML = items.map(({ term, def }) =>
    `<div class="gloss-row"><span class="gloss-term">${escapeHtml(term)}</span>`
    + `<span class="gloss-def">${escapeHtml(def)}</span></div>`).join("");
}

// Open the glossary panel, optionally pre-filtered to `term` (the ⌘K "Glossary: X" route lands here).
function openGlossary(term) {
  const el = _glossEl();
  el.hidden = false;
  const box = el.querySelector("#gloss-search");
  box.value = term || "";
  _renderGlossList(box.value);
  box.focus(); box.select();
  _dismissGlossHint();   // opening the glossary satisfies the discoverability hint
}
function closeGlossary() { if (_glossPanel) _glossPanel.hidden = true; }
function glossaryOpen() { return !!_glossPanel && !_glossPanel.hidden; }

// #374: a ONE-TIME, subtle first-load hint that the editor keywords are hoverable AND that there's a
// glossary button — the hover is invisible until stumbled on, so point at it once. Dismissed on first
// interaction (or after a while) and remembered in localStorage so it never nags twice.
const _GLOSS_HINT_KEY = "evident-gloss-hint-seen";
let _glossHintEl = null;
function _glossHintSeen() { try { return localStorage.getItem(_GLOSS_HINT_KEY) === "1"; } catch (e) { return false; } }
function _dismissGlossHint() {
  try { localStorage.setItem(_GLOSS_HINT_KEY, "1"); } catch (e) {}
  if (_glossHintEl) { _glossHintEl.remove(); _glossHintEl = null; }
}
function maybeShowGlossHint() {
  if (_glossHintSeen()) return;
  // #374: don't collide with the guided TOUR — a true first-timer gets the tour (the richer onboarding);
  // the glossary hint is for the next visit, once the tour's been seen/skipped. Skip while the tour will
  // auto-run (tour flag unset) so the two don't stack. (TOUR_FLAG lives in app-palette.js.)
  try { if (typeof TOUR_FLAG !== "undefined" && localStorage.getItem(TOUR_FLAG) !== "1") return; } catch (e) {}
  _glossHintEl = document.createElement("div");
  _glossHintEl.id = "gloss-hint";
  _glossHintEl.innerHTML = '💡 New here? <b>Hover any keyword</b> (the dotted ones) to learn what it means'
    + ' — or open the <b>📖 glossary</b>. <span class="gloss-hint-x" title="dismiss">✕</span>';
  document.body.appendChild(_glossHintEl);
  _glossHintEl.querySelector(".gloss-hint-x").onclick = _dismissGlossHint;
  // auto-fade after a while so it never lingers; still counts as "seen".
  setTimeout(() => { if (_glossHintEl) _dismissGlossHint(); }, 12000);
}

// Wire the visible glossary button + Esc/outside-click dismissal + the first-load hint.
function initGlossary() {
  const btn = $("#glossary-btn");
  if (btn) btn.onclick = (e) => { e.stopPropagation(); glossaryOpen() ? closeGlossary() : openGlossary(); };
  document.addEventListener("keydown", (e) => { if (e.key === "Escape" && glossaryOpen()) closeGlossary(); });
  document.addEventListener("click", (e) => {
    if (glossaryOpen() && !_glossPanel.contains(e.target) && e.target.id !== "glossary-btn") closeGlossary();
  });
  maybeShowGlossHint();
}
