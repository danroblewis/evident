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

// --- guided first-run walkthrough: flag + samples + steps (Tasks #164 / #63) -------
// The tour now teaches the IDEAS end-to-end, not just where the panels are: it loads the
// counter sample so the hands-on steps ("write a delta, watch the ramp, break it, see the
// dropped-constraint count") land on a concrete program a newcomer can poke. TOUR_SEED
// is opened when the tour starts; SUDOKU_SAMPLE is opened at "Done" so ⊨ Solve has a claim.
const TOUR_FLAG = "evident-tour-done";
const TOUR_SEED = "counter · a terminating clock (FSM)";
const SUDOKU_SAMPLE = "4×4 sudoku · fill the grid (⊨ Solve)";
const TOUR_STEPS = [
  { sel: "#editor-pane", title: "1 · A claim is a state machine",
    body: "This is the <code>counter</code> sample — an <b>fsm</b>: a claim that carries state "
      + "across ticks. You don't write a loop; you write the RELATION between one tick and the "
      + "next, and the solver replays it. <code>_count</code> reads the previous tick's value." },
  { sel: "#editor-pane", title: "2 · Δ is the change each tick",
    body: "Find the line <code>Δcount = (_count &lt; 5 ? 1 : 0)</code>. <code>Δcount</code> means "
      + "<code>count − _count</code> — the <i>change</i> per tick. So this reads \"rise by 1 while "
      + "below 5, else 0\". <b>Try it:</b> change the <code>1</code> to a <code>2</code> and watch the "
      + "ramp climb twice as fast in the diagram." },
  { sel: "#banner", title: "3 · Watch the ramp",
    body: "Every edit re-solves instantly. The banner names your model's SHAPE — here it "
      + "<b>Terminates</b>, because count reaches 5 and never leaves (a <i>fixed point</i>). Open "
      + "the <code>time_series</code> view below to literally see the ramp rise and flatten." },
  { sel: "#explainer", title: "4 · 'How this works'",
    body: "Stuck on what a keyword MEANS? This collapsible note explains every sample in plain "
      + "English — what fsm / Δ / is_first_tick are, and WHY this code produces what you see. Click "
      + "it open now." },
  { sel: "#honesty", title: "5 · Break it on purpose",
    body: "Evident never silently ignores a mistake. <b>Try this:</b> delete the line "
      + "<code>count ∈ Int := 0</code> — that's the SEED, the start value. The honesty line "
      + "here, and an amber mark on the editor line, flag any constraint the solver had to DROP. "
      + "That surfaced count is the silent bug, made loud." },
  { sel: "#solve-btn", title: "6 · ⊨ Solve a claim",
    body: "Not every program is a machine. A plain <b>claim</b> is a relation — press ⊨ Solve for a "
      + "witness assignment (or UNSAT if none exists). At Done we'll open the sudoku sample so you "
      + "can try it: the solver fills the grid with no algorithm written." },
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
  cmds.push({ label: "Copy share link — a URL that loads this program", run: () => copyShareLink() });
  cmds.push({ label: "Solve claim — ⊨ witness or UNSAT", run: () => solve(false) });
  if ($("#smtlib-btn")) cmds.push({ label: "Copy SMT-LIB encoding", run: () => clickIf("#smtlib-btn") });
  if ($("#pin-btn")) cmds.push({ label: pinnedA ? "Unpin compare (A)" : "Pin this result — compare next beside it", run: () => togglePin() });
  if (pinnedA && pinnedA.source) cmds.push({ label: "⇄ Model-diff — which reachable states appeared / vanished vs pinned A", run: () => runDiff() });
  if ($("#symbols-btn")) cmds.push({ label: "Symbols palette — how to type ∈ ⇒ Δ", run: () => togglePalette(true) });
  if ($("#tour-btn")) cmds.push({ label: "Guided tour", run: () => startTour() });
  if (typeof openHelp === "function") cmds.push({ label: "help: what do these mean? — verdict + interrogate terms", run: () => openHelp() });
  // one command per live view tab (the #tabs strip is rebuilt by paint())
  $$("#tabs .tab").forEach((tab) => {
    const view = tab.textContent.trim().replace(/ /g, "_");
    cmds.push({ label: "View: " + tab.textContent.trim(), run: () => run(view) });
  });
  cmds.push({ label: "Verify — focus the ⊢ property field", run: () => { const f = $("#inv-prop"); if (f) f.focus(); } });
  cmds.push({ label: "Query — find a reachable state (⊨? ∃)", run: () => { const f = $("#query-prop"); if (f) f.focus(); } });
  return cmds;
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
  // Seed the editor with the counter sample so the hands-on steps ("change the 1 to a 2",
  // "delete the is_first_tick seed") land on the exact lines they reference. Guarded so the
  // tour still runs if SAMPLES/loadProgram somehow aren't present.
  if (typeof loadProgram === "function" && SAMPLES[TOUR_SEED]) loadProgram(SAMPLES[TOUR_SEED], null);
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

// auto-run once on first visit. Skipped when a shared link (#src=…) loaded a program — the
// link is a deliberate override, and the tour now seeds the counter sample, which would
// clobber it. The user can still launch the tour by hand from the ? button.
function maybeAutoTour() {
  let seen = false;
  try { seen = localStorage.getItem(TOUR_FLAG) === "1"; } catch (_) { seen = true; }
  if (!seen && (typeof SHARED === "undefined" || SHARED == null)) startTour();
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

  // #372: while the palette is open, NOTHING may leak into the editor. ⌘K opens via Ace's command, so
  // there's a focus-transition window where the next fast keystroke can still land in Ace before
  // #cmdk-input takes focus. A CAPTURE-phase guard closes that window: any keydown whose target isn't the
  // palette input is stopped before Ace sees it, focus is pulled back to the input, and a printable char is
  // forwarded INTO the input (so the keystroke is captured, not just dropped). A newcomer reaching for the
  // glossary can never accidentally edit their program.
  document.addEventListener("keydown", (e) => {
    if (!cmdkOpen() || e.target === cmdkInput) return;
    // let the palette's own modifier chords (⌘K toggle, etc.) be handled by the global handler below
    if (e.metaKey || e.ctrlKey || e.altKey) return;
    e.preventDefault(); e.stopPropagation();          // never reaches Ace
    cmdkInput.focus();
    if (e.key && e.key.length === 1) {                // a printable char — capture it into the input
      const i = cmdkInput.selectionStart ?? cmdkInput.value.length;
      cmdkInput.value = cmdkInput.value.slice(0, i) + e.key + cmdkInput.value.slice(cmdkInput.selectionEnd ?? i);
      cmdkInput.setSelectionRange(i + 1, i + 1);
      cmdkActive = 0; renderCmdk();
    } else if (e.key === "Escape") { closeCmdk(); }
  }, true);   // capture phase — beats Ace's own keydown handling

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
