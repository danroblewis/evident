"use strict";

// ==============================================================================
// app-symhint.js — the inline backslash symbol hint (#215).
//
// Teach the `\name → glyph` shortcut AT THE CURSOR, the moment a `\` token is being
// typed — no palette trip. As you type `\g`, a small popup near the cursor lists the
// matching entries (`\ge → ≥`, `\geq → ≥`, …); typing narrows; ↑/↓ move the selection;
// Enter/Tab inserts the glyph (replacing the typed `\name`); Esc dismisses; a click
// inserts a row. Suppressed inside strings/comments.
//
// SINGLE SOURCE OF TRUTH: the entries are drawn from the SAME `UNI` table (app-data.js)
// that drives the auto-replacement (applyTokenInput, app-editor.js) and the palette —
// referenced, never forked. Split out of app-editor.js to keep both files under the
// CLAUDE.md ≤500-line convention; behaviour-preserving move.
//
// Cross-file globals referenced (all at call time, so load order is safe — this file
// loads BEFORE app-editor.js, like the other app-*.js): `editor`, `UNI`, plus
// `_replacing` / `spliceReplace` (the undo-grouped splice helpers, app-editor.js).
// initSymHint() wires the cursor + keydown listeners; it's called from
// initEditorInput() once `editor` exists.
// ==============================================================================

let _bsHint = null;          // the popup element (created lazily)
let _bsItems = [];           // [{name, glyph}] currently shown, best-prefix-first
let _bsSel = 0;              // selected index
let _bsAnchor = null;        // {row, col} of the leading backslash, for the replace range

function _bsEl() {
  if (_bsHint) return _bsHint;
  _bsHint = document.createElement("div");
  _bsHint.id = "bs-hint"; _bsHint.hidden = true;
  document.body.appendChild(_bsHint);
  return _bsHint;
}

function hideBackslashHint() { if (_bsHint) _bsHint.hidden = true; _bsItems = []; _bsAnchor = null; }
function backslashHintOpen() { return !!_bsHint && !_bsHint.hidden && _bsItems.length > 0; }

// Is the cursor inside a string/comment token? (don't hijack a literal backslash there.)
function _bsInStringOrComment(row, col) {
  const tok = editor.session.getTokenAt(row, col);
  return !!tok && /string|comment/.test(tok.type || "");
}

// Recompute the hint from the text just before the cursor. Shows it when a `\` + word-prefix sits
// immediately left of the caret (and we're not in a string/comment); hides it otherwise.
function updateBackslashHint() {
  if (_replacing) return;
  const pos = editor.getCursorPosition();
  const line = editor.session.getLine(pos.row);
  const before = line.slice(0, pos.column);
  const m = before.match(/\\([a-zA-Z]*)$/);      // a backslash + zero-or-more letters, right at the caret
  if (!m || _bsInStringOrComment(pos.row, pos.column)) { hideBackslashHint(); return; }
  const frag = m[1];
  const lower = frag.toLowerCase();
  // matching UNI names: prefix matches first (then keep declaration order), de-dup by name. With an
  // empty fragment (just `\`) show the whole table so the bare backslash already teaches the set.
  const names = Object.keys(UNI).filter((n) => !lower || n.toLowerCase().startsWith(lower));
  if (!names.length) { hideBackslashHint(); return; }
  // exact-name first, then shortest (the canonical alias), so `\ge` leads with ge not geq
  names.sort((a, b) => (a.toLowerCase() === lower ? -1 : b.toLowerCase() === lower ? 1 : 0) || a.length - b.length || a.localeCompare(b));
  _bsItems = names.slice(0, 8).map((n) => ({ name: n, glyph: UNI[n] }));
  _bsSel = 0;
  _bsAnchor = { row: pos.row, col: pos.column - m[0].length };   // the leading backslash
  renderBackslashHint();
}

function renderBackslashHint() {
  const el = _bsEl();
  el.innerHTML = _bsItems.map((it, i) =>
    `<div class="bs-row${i === _bsSel ? " on" : ""}" data-i="${i}">`
    + `<span class="bs-glyph">${it.glyph}</span>`
    + `<span class="bs-name">\\${it.name}</span></div>`).join("");
  // anchor under the caret using Ace's pixel position
  const r = editor.renderer;
  const cur = editor.getCursorPosition();
  const coords = r.textToScreenCoordinates(cur.row, cur.column);
  el.style.left = coords.pageX + "px";
  el.style.top = (coords.pageY + r.lineHeight + 2) + "px";
  el.hidden = false;
  // #215: the backslash hint takes precedence over Ace's live completer while a `\` token is typed —
  // dismiss any open autocomplete popup so the two don't stack at the cursor.
  if (editor.completer && editor.completer.popup && editor.completer.popup.isOpen) editor.completer.detach();
  // mouse: click a row to insert it
  el.querySelectorAll(".bs-row").forEach((row) => {
    row.onmousedown = (e) => { e.preventDefault(); _bsSel = +row.dataset.i; commitBackslashHint(); };
  });
}

// Insert the selected glyph, replacing the typed `\name` fragment. Single undo group (spliceReplace).
function commitBackslashHint() {
  if (!backslashHintOpen() || !_bsAnchor) return false;
  const it = _bsItems[_bsSel];
  const cur = editor.getCursorPosition();
  spliceReplace(_bsAnchor.row, _bsAnchor.col, cur.column, it.glyph);   // [\…fragment) → glyph
  hideBackslashHint();
  return true;
}

function moveBackslashHint(d) {
  if (!backslashHintOpen()) return;
  _bsSel = (_bsSel + d + _bsItems.length) % _bsItems.length;
  renderBackslashHint();
}

// Wire the cursor-move + keyboard-precedence listeners. Called from initEditorInput() once `editor`
// exists. The `change` handler in app-editor.js calls updateBackslashHint() directly (so the hint
// refreshes in the same place the auto-replacement runs); these two are the hint-only listeners.
function initSymHint() {
  // a cursor move OFF the `\` token (click / arrow without typing) must dismiss the hint too.
  editor.selection.on("changeCursor", () => { if (!_replacing) updateBackslashHint(); });

  // keyboard precedence while the backslash hint is open — intercept Enter/Tab (insert the selected
  // glyph), ↑/↓ (move), Esc (dismiss) in the CAPTURE phase on Ace's own text input, so we pre-empt
  // Ace's newline/indent/autocomplete bindings only while a `\` token is being typed.
  const ti = editor.textInput && editor.textInput.getElement();
  if (ti) ti.addEventListener("keydown", (e) => {
    if (!backslashHintOpen()) return;
    if (e.key === "Enter" || e.key === "Tab") {
      e.preventDefault(); e.stopPropagation(); commitBackslashHint();
    } else if (e.key === "ArrowDown") {
      e.preventDefault(); e.stopPropagation(); moveBackslashHint(1);
    } else if (e.key === "ArrowUp") {
      e.preventDefault(); e.stopPropagation(); moveBackslashHint(-1);
    } else if (e.key === "Escape") {
      e.preventDefault(); e.stopPropagation(); hideBackslashHint();
    }
  }, true);
}
