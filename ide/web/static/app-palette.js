"use strict";

// ==============================================================================
// app-palette.js — symbol palette / command-palette / guided-tour DATA and the pure
// helpers over them: fuzzy subsequence match, line-comment toggle, label highlight,
// and the static PALETTE / TOUR_STEPS tables (tasks #62/#182/#164).
//
// Pure data + hoisted functions only. The popover/overlay DOM, the keyboard chords,
// and all wiring that touches `editor` / `$` stay in app.js. Loaded before app.js.
// Behaviour-preserving move out of app.js.
// ==============================================================================

// --- symbol palette / cheat-sheet rows (Task #62) ---------------------------------
const PALETTE = [
  ["∈", "membership / typing",  "\\in  or  in"],
  ["⇒", "implies",              "\\imp / =>  or  implies"],
  ["⟸", "reverse-implies",      "\\when  or  when"],
  ["∀", "for-all",              "\\all  or  forall"],
  ["∃", "there-exists",         "\\exists  or  exists"],
  ["¬", "not",                  "\\neg  or  not"],
  ["∧", "and",                  "\\and  or  and"],
  ["∨", "or",                   "\\or  or  or"],
  ["≤", "less-or-equal",        "\\le  or  <="],
  ["≥", "greater-or-equal",     "\\ge  or  >="],
  ["≠", "not-equal",            "\\ne  or  !="],
  ["Δ", "forward difference",   "\\Delta  or  delta"],
  ["↦", "maps-to / rename",     "\\mapsto"],
  ["→", "to",                   "\\to"],
  ["⟨", "seq-literal open",     "\\langle"],
  ["⟩", "seq-literal close",    "\\rangle"],
  ["∪", "set union",            "\\cup"],
  ["∩", "set intersection",     "\\cap"],
];

// --- command-palette pure helpers (Task #182) -------------------------------------
// Subsequence fuzzy match: every char of `q` appears in `label` in order. Returns the
// matched index list (for highlighting) or null. Empty query matches with no highlight.
function fuzzyMatch(label, q) {
  const text = String(label).toLowerCase();
  const query = String(q || "").toLowerCase().replace(/\s+/g, "");
  if (!query) return [];
  const idx = [];
  let j = 0;
  for (let i = 0; i < text.length && j < query.length; i++) {
    if (text[i] === query[j]) { idx.push(i); j++; }
  }
  return j === query.length ? idx : null;
}

// Line-comment toggle over a block of source lines. Pure: takes/returns an array of
// strings, unit-testable without Ace. Mirrors the Evident `-- ` comment convention.
function toggleCommentLines(lines) {
  const prefix = "-- ";
  const codeLines = lines.filter((l) => l.trim() !== "");
  const allCommented = codeLines.length > 0
    && codeLines.every((l) => /^\s*-- ?/.test(l));
  return lines.map((l) => {
    if (l.trim() === "") return l;
    if (allCommented) return l.replace(/^(\s*)-- ?/, "$1");
    return l.replace(/^(\s*)/, "$1" + prefix);
  });
}

// Highlight the fuzzy-matched chars in a command label (returns escaped HTML).
function highlightLabel(label, idx) {
  if (!idx || !idx.length) return escapeHtml(label);
  let out = "", set = new Set(idx);
  for (let i = 0; i < label.length; i++) {
    const c = escapeHtml(label[i]);
    out += set.has(i) ? `<b>${c}</b>` : c;
  }
  return out;
}

// --- guided first-run walkthrough: flag + sample + steps (Task #164) ---------------
const TOUR_FLAG = "evident-tour-done";
const SUDOKU_SAMPLE = "4×4 sudoku · fill the grid (⊨ Solve)";
const TOUR_STEPS = [
  { sel: "#editor-pane", title: "1 · The editor",
    body: "Write constraints here. This is a live counter — try changing "
      + "<code>count = 0</code> to <code>count = 3</code>. Operators autocomplete: "
      + "type <code>\\le</code> → ≤." },
  { sel: "#banner", title: "2 · The dynamics panel",
    body: "Every edit re-solves instantly. The banner names your model's SHAPE; the "
      + "diagram below shows the solved behavior — hover the dots to inspect a state." },
  { sel: "#honesty", title: "3 · The honesty line",
    body: "Evident never hides a problem: a dropped constraint is flagged here AND "
      + "marked amber on its line in the editor. That's the silent bug, surfaced." },
  { sel: "#solve-btn", title: "4 · ⊨ Solve",
    body: "Some programs are claims, not machines — press ⊨ Solve for a witness "
      + "assignment (or UNSAT). Try it on the sudoku sample." },
];

// =============================================================================
// Symbol palette + command palette (⌘K) + guided tour — the interactive UI for
// the data/helpers above. The DOM elements, keyboard chords and document/window
// listeners are created inside initPalette(), which app.js calls AFTER `editor`
// and the core globals exist (this file loads before app.js). Behaviour matches
// the original top-level wiring: same listeners, same elements, same order.
// =============================================================================

// Shared element + state handles, populated by initPalette().
let palette = null, cmdk = null, cmdkInput = null, cmdkList = null;
let cmdkCommands = [], cmdkFiltered = [], cmdkActive = 0;
let tourIdx = 0, tourEls = null;

const $$ = (s) => Array.from(document.querySelectorAll(s));

// --- symbol palette / cheat-sheet (Task #62) --------------------------------------
function insertGlyph(glyph) {
  editor.session.insert(editor.getCursorPosition(), glyph);
  editor.focus();
}
function togglePalette(show) {
  const open = show != null ? show : palette.hidden;
  palette.hidden = !open;
  $("#symbols-btn").classList.toggle("on", open);
}

// --- command palette: comment toggle + command list (Task #182) -------------------
// Apply the comment toggle to the editor's currently selected rows (or the cursor's line).
function toggleCommentSelection() {
  const sess = editor.session;
  const range = editor.getSelectionRange();
  const startRow = range.start.row;
  // a selection ending exactly at column 0 of a later row shouldn't pull in that empty next line
  const endRow = range.end.column === 0 && range.end.row > startRow
    ? range.end.row - 1 : range.end.row;
  const rows = [];
  for (let r = startRow; r <= endRow; r++) rows.push(sess.getLine(r));
  const out = toggleCommentLines(rows);
  const Range = ace.require("ace/range").Range;
  for (let r = startRow; r <= endRow; r++) {
    const cur = sess.getLine(r), next = out[r - startRow];
    if (next !== cur) sess.replace(new Range(r, 0, r, cur.length), next);
  }
  editor.focus();
}

// Build the live command list. Each command: { label, run }. Targets are resolved at build
// time; a command whose target element is missing is omitted so .run never throws.
function buildCommands() {
  const cmds = [];
  const clickIf = (id) => { const el = $(id); if (el) el.click(); };
  // open a sample — same path as the #samples select onchange
  Object.keys(SAMPLES).forEach((name) => {
    cmds.push({ label: "Open sample: " + name, run: () => loadProgram(SAMPLES[name], null) });
  });
  // open one of your saved programs
  const slots = loadSlots();
  Object.keys(slots).sort().forEach((name) => {
    cmds.push({ label: "Open saved: " + name, run: () => loadProgram(slots[name], name) });
  });
  cmds.push({ label: "Save as… — keep this program in a named slot", run: () => saveAsPrompt() });
  if (Object.keys(slots).length) cmds.push({ label: "Delete saved…", run: () => deletePrompt() });
  cmds.push({ label: "Export .ev — download this buffer as a file", run: () => exportEv() });
  cmds.push({ label: "Export diagram as SVG — vector, for a paper or slide", run: () => exportSVG() });
  cmds.push({ label: "Copy share link — a URL that loads this program", run: () => copyShareLink() });
  cmds.push({ label: "Solve claim — ⊨ witness or UNSAT", run: () => solve(false) });
  if ($("#smtlib-btn")) cmds.push({ label: "Copy SMT-LIB encoding", run: () => clickIf("#smtlib-btn") });
  if ($("#pin-btn")) cmds.push({ label: pinnedA ? "Unpin compare (A)" : "Pin this result — compare next beside it", run: () => togglePin() });
  if (pinnedA && pinnedA.source) cmds.push({ label: "⇄ Model-diff — which reachable states appeared / vanished vs pinned A", run: () => runDiff() });
  if ($("#symbols-btn")) cmds.push({ label: "Symbols palette — how to type ∈ ⇒ Δ", run: () => togglePalette(true) });
  if ($("#tour-btn")) cmds.push({ label: "Guided tour", run: () => startTour() });
  // one command per live view tab (the #tabs strip is rebuilt by paint())
  $$("#tabs .tab").forEach((tab) => {
    const view = tab.textContent.trim().replace(/ /g, "_");
    cmds.push({ label: "View: " + tab.textContent.trim(), run: () => run(view) });
  });
  cmds.push({ label: "Verify — focus the ⊢ property field", run: () => { const f = $("#inv-prop"); if (f) f.focus(); } });
  cmds.push({ label: "Query — find a reachable state (⊨? ∃)", run: () => { const f = $("#query-prop"); if (f) f.focus(); } });
  // Searchable concept glossary (Sam #246): every language noun (claim/fsm/type/enum), operator
  // (∈/⇒/Δ), and dynamics term (cyclic/driven/fixed point) as a ⌘K entry — select one to read its
  // full definition. So a newcomer can look up "what IS a claim" without leaving for a manual.
  glossaryItems().forEach(({ def }) => {
    cmds.push({ label: "📖 " + def, run: () => showGlossCentered(def) });
  });
  return cmds;
}

// Show a glossary definition centered near the top, dismissed on the next click/key (Sam #246).
// Reuses #gloss (pointer-events:none, so the dismiss click passes through) and resets the positioning
// it borrows, so a later hover tooltip isn't left shifted.
function showGlossCentered(text) {
  const g = $("#gloss");
  if (!g) return;
  g.textContent = text;
  g.style.left = "50%"; g.style.top = "12%"; g.style.transform = "translateX(-50%)"; g.style.maxWidth = "560px";
  g.hidden = false;
  const hide = () => {
    g.hidden = true; g.style.transform = ""; g.style.maxWidth = "";
    document.removeEventListener("click", hide, true); document.removeEventListener("keydown", hide, true);
  };
  setTimeout(() => { document.addEventListener("click", hide, true); document.addEventListener("keydown", hide, true); }, 0);
}

function renderCmdk() {
  const q = cmdkInput.value;
  cmdkFiltered = [];
  cmdkCommands.forEach((c) => {
    const m = fuzzyMatch(c.label, q);
    if (m !== null) cmdkFiltered.push({ cmd: c, idx: m });
  });
  if (cmdkActive >= cmdkFiltered.length) cmdkActive = Math.max(0, cmdkFiltered.length - 1);
  if (!cmdkFiltered.length) {
    cmdkList.innerHTML = '<div class="cmdk-empty dim">no matching command</div>';
    return;
  }
  cmdkList.innerHTML = cmdkFiltered.map((f, i) =>
    `<div class="cmdk-row${i === cmdkActive ? " on" : ""}" data-i="${i}">${highlightLabel(f.cmd.label, f.idx)}</div>`
  ).join("");
  const on = cmdkList.querySelector(".cmdk-row.on");
  if (on) on.scrollIntoView({ block: "nearest" });
}

function runCmdk(i) {
  const f = cmdkFiltered[i];
  closeCmdk();
  if (f) { try { f.cmd.run(); } catch (e) { /* a stale target — never let the palette throw */ } }
}

function openCmdk() {
  togglePalette(false);                 // don't stack the symbols popover under the modal
  cmdkCommands = buildCommands();
  cmdkInput.value = ""; cmdkActive = 0;
  cmdk.hidden = false;
  renderCmdk();
  cmdkInput.focus();
}

function closeCmdk() { cmdk.hidden = true; editor.focus(); }
function cmdkOpen() { return !cmdk.hidden; }
function toggleCmdk() { cmdkOpen() ? closeCmdk() : openCmdk(); }

// --- guided first-run walkthrough — coachmark tour (Task #164) ---------------------
function buildTourDom() {
  if (tourEls) return tourEls;
  const backdrop = document.createElement("div");
  backdrop.id = "tour-backdrop"; backdrop.hidden = true;
  const ring = document.createElement("div");
  ring.id = "tour-ring"; ring.hidden = true;
  const card = document.createElement("div");
  card.id = "tour-card"; card.hidden = true;
  backdrop.appendChild(ring); backdrop.appendChild(card);
  document.body.appendChild(backdrop);
  // a backdrop click (outside the card) closes the tour without marking it "seen"-only
  backdrop.addEventListener("click", (e) => { if (e.target === backdrop) endTour(); });
  tourEls = { backdrop, ring, card };
  return tourEls;
}

// place the ring over `target` and the card just below/above it, clamped to the viewport.
function positionTour(target) {
  const { ring, card } = tourEls;
  const r = target.getBoundingClientRect();
  const pad = 4;
  ring.style.top = (r.top - pad) + "px";
  ring.style.left = (r.left - pad) + "px";
  ring.style.width = (r.width + pad * 2) + "px";
  ring.style.height = (r.height + pad * 2) + "px";
  const cw = card.offsetWidth || 300, ch = card.offsetHeight || 140;
  let top = r.bottom + 12;
  if (top + ch > window.innerHeight - 8) top = Math.max(8, r.top - ch - 12);
  let left = r.left;
  left = Math.min(left, window.innerWidth - cw - 8);
  left = Math.max(8, left);
  card.style.top = top + "px";
  card.style.left = left + "px";
}

function renderTourStep() {
  const total = TOUR_STEPS.length;
  // skip forward over any step whose target is missing from the DOM
  while (tourIdx < total && !$(TOUR_STEPS[tourIdx].sel)) tourIdx++;
  if (tourIdx >= total) { endTour(); return; }
  const step = TOUR_STEPS[tourIdx];
  const target = $(step.sel);
  if (!target) { endTour(); return; }
  const { ring, card } = tourEls;
  const last = tourIdx === total - 1;
  card.innerHTML =
    `<div class="tour-title">${step.title}</div>`
    + `<div class="tour-body">${step.body}</div>`
    + `<div class="tour-foot">`
    + `<span class="tour-step">${tourIdx + 1} / ${total}</span>`
    + `<span class="sp"></span>`
    + `<span class="tour-skip" data-act="skip">Skip</span>`
    + (tourIdx > 0 ? `<button data-act="back">Back</button>` : "")
    + `<button class="primary" data-act="next">${last ? "Done" : "Next"}</button>`
    + `</div>`;
  ring.hidden = false; card.hidden = false;
  positionTour(target);
}

function startTour() {
  buildTourDom();
  togglePalette(false);            // don't let the palette overlap the tour
  tourIdx = 0;
  tourEls.backdrop.hidden = false;
  renderTourStep();
}

function endTour() {
  try { localStorage.setItem(TOUR_FLAG, "1"); } catch (_) {}
  if (!tourEls) return;
  tourEls.backdrop.hidden = true;
  tourEls.ring.hidden = true;
  tourEls.card.hidden = true;
}

function tourActive() { return tourEls && !tourEls.backdrop.hidden; }

// auto-run once on first visit
function maybeAutoTour() {
  let seen = false;
  try { seen = localStorage.getItem(TOUR_FLAG) === "1"; } catch (_) { seen = true; }
  if (!seen) startTour();
}

// --- wiring: build the overlay DOM + attach all listeners --------------------------
// Called from app.js's bootstrap once `editor` and the core globals exist. Mirrors the
// original top-level wiring exactly (same elements, same listeners).
function initPalette() {
  // symbol palette / cheat-sheet popover
  palette = document.createElement("div");
  palette.id = "palette"; palette.hidden = true;
  palette.innerHTML =
    '<div class="palette-head">type these operators — click a row to insert it'
    + ' <span class="dim">(Esc closes)</span></div>'
    + PALETTE.map(([g, name, mn], i) =>
        `<div class="palette-row" data-i="${i}">`
        + `<span class="palette-glyph">${escapeHtml(g)}</span>`
        + `<span class="palette-name">${escapeHtml(name)}</span>`
        + `<span class="palette-mn dim">${escapeHtml(mn)}</span></div>`).join("");
  document.body.appendChild(palette);
  palette.addEventListener("click", (e) => {
    const row = e.target.closest(".palette-row");
    if (!row) return;
    insertGlyph(PALETTE[+row.dataset.i][0]);   // keep the palette open for multi-insert
  });
  $("#symbols-btn").onclick = (e) => { e.stopPropagation(); togglePalette(); };
  // dismiss: Esc anywhere, or a click outside the popover/button
  document.addEventListener("keydown", (e) => { if (e.key === "Escape" && !palette.hidden) togglePalette(false); });
  document.addEventListener("click", (e) => {
    if (!palette.hidden && !palette.contains(e.target) && e.target.id !== "symbols-btn") togglePalette(false);
  });

  // command palette (⌘K) overlay DOM
  cmdk = document.createElement("div");
  cmdk.id = "cmdk"; cmdk.hidden = true;
  cmdk.innerHTML =
    '<div id="cmdk-box">'
    + '<input id="cmdk-input" placeholder="Type a command…  (open a sample, solve, switch view)" autocomplete="off" spellcheck="false">'
    + '<div id="cmdk-list"></div>'
    + '<div id="cmdk-foot" class="dim">⌘K commands · ⌘⏎ solve · ⌘/ comment · ↑↓ move · ⏎ run · Esc close</div>'
    + '</div>';
  document.body.appendChild(cmdk);
  cmdkInput = $("#cmdk-input"); cmdkList = $("#cmdk-list");
  cmdkInput.addEventListener("input", () => { cmdkActive = 0; renderCmdk(); });
  cmdkInput.addEventListener("keydown", (e) => {
    if (e.key === "ArrowDown") { e.preventDefault(); cmdkActive = Math.min(cmdkActive + 1, cmdkFiltered.length - 1); renderCmdk(); }
    else if (e.key === "ArrowUp") { e.preventDefault(); cmdkActive = Math.max(cmdkActive - 1, 0); renderCmdk(); }
    else if (e.key === "Enter") { e.preventDefault(); runCmdk(cmdkActive); }
    else if (e.key === "Escape") { e.preventDefault(); closeCmdk(); }
  });
  cmdkList.addEventListener("click", (e) => {
    const row = e.target.closest(".cmdk-row");
    if (row) runCmdk(+row.dataset.i);
  });
  cmdk.addEventListener("mousedown", (e) => { if (e.target === cmdk) closeCmdk(); });  // backdrop click closes
  if ($("#cmdk-hint")) $("#cmdk-hint").onclick = (e) => { e.stopPropagation(); openCmdk(); };

  // editor-scoped chords (Ace owns keystrokes while the editor is focused)
  editor.commands.addCommand({
    name: "cmdkPalette", bindKey: { win: "Ctrl-K", mac: "Command-K" },
    exec: () => toggleCmdk(),
  });
  editor.commands.addCommand({
    name: "cmdkSolve", bindKey: { win: "Ctrl-Enter", mac: "Command-Enter" },
    exec: () => solve(false),
  });
  editor.commands.addCommand({
    name: "cmdkComment", bindKey: { win: "Ctrl-/", mac: "Command-/" },
    exec: () => toggleCommentSelection(),
  });

  // global chords (fire when focus is OUTSIDE the editor / inputs)
  document.addEventListener("keydown", (e) => {
    const mod = e.metaKey || e.ctrlKey;
    if (!mod) return;
    const k = e.key.toLowerCase();
    if (k === "k") { e.preventDefault(); toggleCmdk(); return; }
    if (cmdkOpen()) return;                       // the palette's own input owns the rest
    const inField = e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement
      || (e.target.closest && e.target.closest("#code"));   // Ace already handles its own chords
    if (inField) return;
    if (k === "enter") { e.preventDefault(); solve(false); }
    else if (k === "/") { e.preventDefault(); toggleCommentSelection(); }
  });

  // tour card-button delegation + Esc + resize + the ? tour button
  document.addEventListener("click", (e) => {
    const btn = e.target.closest("[data-act]");
    if (!btn || !tourActive()) return;
    const act = btn.dataset.act;
    if (act === "skip") { endTour(); return; }
    if (act === "back") { tourIdx = Math.max(0, tourIdx - 1); renderTourStep(); return; }
    if (act === "next") {
      if (tourIdx >= TOUR_STEPS.length - 1) {
        // last step "Done": open the sudoku sample so ⊨ Solve has something to chew on
        if (SAMPLES[SUDOKU_SAMPLE]) loadProgram(SAMPLES[SUDOKU_SAMPLE], null);
        endTour();
      } else { tourIdx++; renderTourStep(); }
    }
  });
  document.addEventListener("keydown", (e) => {
    if (e.key === "Escape" && tourActive()) { e.stopPropagation(); endTour(); }
  });
  window.addEventListener("resize", () => {
    if (!tourActive()) return;
    const t = $(TOUR_STEPS[tourIdx].sel);
    if (t) positionTour(t);
  });
  $("#tour-btn").onclick = (e) => { e.stopPropagation(); startTour(); };
}
