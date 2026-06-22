"use strict";

// ==============================================================================
// app-editor.js — editor interaction: auto-indent on Enter, typable-token input
// (\\word / bare-mnemonic / ASCII-pair auto-replacement, Task #34), the hover-to-learn
// glossary + banner-concept + view-caption tooltips, and the inline error /
// dropped-constraint line markers.
//
// Hoisted functions + module state only; every top-level listener (the Ace newline
// command, the session change handler, the hover listeners) is attached by
// initEditorInput(), called from app.js once `editor` / `gloss` / the core exist.
// Loaded before app.js. Behaviour-preserving move out of app.js.
// ==============================================================================

// --- typable-token input (backslash + bare mnemonic auto-replacement) -------------
// Driven off session 'change'. Inspect the just-inserted char + the word/op-pair before
// the cursor, then splice in the glyph (single undo group, so Ctrl-Z reverts one at a time).
let _replacing = false;        // guard against re-entrancy from our own splice
function applyTokenInput(delta) {
  if (_replacing) return;
  if (!delta || delta.action !== "insert") return;
  const inserted = delta.lines.length === 1 ? delta.lines[0] : null;
  if (!inserted || inserted.length !== 1) return;     // only single-char keystrokes trigger
  const ch = inserted;
  const row = delta.end.row, col = delta.end.column;  // cursor sits just after the inserted char
  const line = editor.session.getLine(row);
  const before = line.slice(0, col);                  // text up to and including the trigger char

  // (1) ASCII operator pair — convert the instant the 2nd char lands.
  const pair = before.slice(-2);
  if (OP_PAIRS[pair]) {
    spliceReplace(row, col - 2, col, OP_PAIRS[pair]);
    return;
  }

  // (2) backslash LaTeX input: \word + a non-letter committed it.
  if (!/[a-zA-Z\\]/.test(ch)) {
    const bs = before.match(/\\([a-zA-Z]+)(.)$/);
    if (bs && UNI[bs[1]]) {
      // replace "\word<trigger>" with "<glyph><trigger>"
      spliceReplace(row, col - bs[0].length, col, UNI[bs[1]] + bs[2]);
      return;
    }
  }

  // (3) bare word mnemonic — convert when a non-word char follows a COMPLETE word that is
  //     a mnemonic. Word-boundary safe: the char before the word must be a non-word char
  //     (or start of line), so `Int`/`min`/`Coining` never convert — only a standalone word.
  if (!/[A-Za-z0-9_]/.test(ch)) {
    const wm = before.match(/(^|[^A-Za-z0-9_])([A-Za-z]+)(.)$/);
    if (wm && WORD_MNEMONICS[wm[2]]) {
      const wordStart = col - wm[3].length - wm[2].length;   // start of the matched word
      // Replace "word<trigger>" with "<glyph><trigger>" (keep the boundary char and land
      // the cursor AFTER it) — otherwise the cursor sits before the trigger space and the
      // next keystroke wedges between glyph and space (`in `+`Int` → `∈Int `).
      spliceReplace(row, wordStart, col, WORD_MNEMONICS[wm[2]] + wm[3]);
    }
  }
}

// Replace the [startCol, endCol) range on `row` with `text`, keeping the cursor after the
// inserted text and the operation in the same undo group as the triggering keystroke (so a
// single Ctrl-Z reverts exactly one replacement).
function spliceReplace(row, startCol, endCol, text) {
  _replacing = true;
  const Range = ace.require("ace/range").Range;
  editor.session.replace(new Range(row, startCol, row, endCol), text);
  editor.moveCursorTo(row, startCol + text.length);
  _replacing = false;
}

// --- inline error line marker -----------------------------------------------------
let _errLine = null;
function clearErrorLine() {
  if (_errLine != null) {
    editor.session.removeGutterDecoration(_errLine, "error-gutter");
    if (_errMarker != null) { editor.session.removeMarker(_errMarker); _errMarker = null; }
    _errLine = null;
  }
}
let _errMarker = null;
// Mark the offending line. Prefer the structured {line, col} from /api/analyze
// (parser now emits it); fall back to scraping "line N" out of the message text.
function markErrorLine(err, loc) {
  clearErrorLine();
  let ln = null;
  if (loc && Number.isInteger(loc.line)) {
    ln = loc.line - 1;
  } else {
    const m = (err || "").match(/line (\d+)/i);
    if (m) ln = parseInt(m[1], 10) - 1;
  }
  if (ln != null && ln >= 0 && ln < editor.session.getLength()) {
    const Range = ace.require("ace/range").Range;
    _errMarker = editor.session.addMarker(
      new Range(ln, 0, ln, Infinity), "ace-error-line", "fullLine");
    editor.session.addGutterDecoration(ln, "error-gutter");
    _errLine = ln;
  }
}

// --- dropped-constraint line markers ----------------------------------------------
// A DROPPED constraint is Evident's signature silent bug: the line parsed, but couldn't
// translate to a Z3 Bool, so it was discarded — the variable it constrained is left FREE,
// and the model is under-constrained while looking valid. Surface that AT the line the
// user wrote it. Distinct AMBER style from the red parse-error marker (a parse error
// blocks; a dropped constraint runs but silently lies). The gutter cell carries an Ace
// warning annotation whose tooltip = the desugared dropped-constraint text.
let _droppedRows = [];
function clearDroppedLines() {
  for (const d of _droppedRows) {
    editor.session.removeMarker(d.marker);
    editor.session.removeGutterDecoration(d.row, "warn-gutter");
  }
  _droppedRows = [];
  editor.session.clearAnnotations();
}
// locs: 1-based source lines; warnings: the raw `warning: dropped …` block (for tooltips).
function markDroppedLines(locs, warnings) {
  clearDroppedLines();
  if (!Array.isArray(locs) || !locs.length) return;
  const Range = ace.require("ace/range").Range;
  const pretties = (warnings || "")
    .split("\n")
    .map((l) => (l.match(/couldn't translate to Bool\):\s*(.+)$/) || [])[1])
    .filter(Boolean);
  const annotations = [];
  locs.forEach((line, i) => {
    const row = line - 1;
    if (!Number.isInteger(row) || row < 0 || row >= editor.session.getLength()) return;
    const marker = editor.session.addMarker(
      new Range(row, 0, row, Infinity), "ace-warn-line", "fullLine");
    editor.session.addGutterDecoration(row, "warn-gutter");
    _droppedRows.push({ row, marker });
    annotations.push({
      row, column: 0, type: "warning",
      text: pretties[i]
        ? "dropped constraint (left FREE — not translated to a Z3 Bool):\n  " + pretties[i]
        : "dropped constraint — couldn't translate to a Z3 Bool (variable left free)",
    });
  });
  if (annotations.length) editor.session.setAnnotations(annotations);
}

// --- wiring: auto-indent + token-input + hover tooltips + (markers are call-only) --
// initEditorInput() mirrors the original top-level editor wiring exactly.
function initEditorInput() {
  // auto-indent on Enter: copy the line's leading whitespace, +1 level after a block opener.
  editor.commands.addCommand({
    name: "evidentNewline",
    bindKey: { win: "Enter", mac: "Enter" },
    exec: function (ed) {
      const cursor = ed.getCursorPosition();
      const line = ed.session.getLine(cursor.row);
      const indent = (line.match(/^[ \t]*/) || [""])[0];
      const opener = /^\s*(fsm|claim|type|enum|schema)\b/.test(line) || /⇒\s*$/.test(line);
      ed.insert("\n" + indent + (opener ? "    " : ""));
    },
  });

  // typable-token input + the debounced analyze, both driven off the one change handler.
  editor.session.on("change", (delta) => {
    applyTokenInput(delta);
    scheduleAnalyze();
  });

  // hover-to-learn glossary: resolve the token under the cursor and show its gloss.
  const editorEl = $("#code");
  editorEl.addEventListener("mousemove", (e) => {
    const pos = editor.renderer.screenToTextCoordinates(e.clientX, e.clientY);
    if (!pos) { gloss.hidden = true; return; }
    const tok = editor.session.getTokenAt(pos.row, pos.column + 1);
    if (tok) {
      const g = glossFor((tok.value || "").trim());
      if (g) {
        gloss.textContent = g; gloss.hidden = false;
        gloss.style.left = Math.min(e.clientX + 12, window.innerWidth - 380) + "px";
        gloss.style.top = (e.clientY + 18) + "px";
        return;
      }
    }
    gloss.hidden = true;
  });
  editorEl.addEventListener("mouseleave", () => { gloss.hidden = true; });

  // concept hover in the banner (Sam #163/#165) — same #gloss tooltip, delegated.
  document.addEventListener("mouseover", (e) => {
    const c = e.target.closest && e.target.closest(".concept");
    if (c && c.dataset.gloss) {
      gloss.textContent = c.dataset.gloss; gloss.hidden = false;
      gloss.style.left = Math.min(e.clientX + 12, window.innerWidth - 380) + "px";
      gloss.style.top = (e.clientY + 18) + "px";
    }
  });
  document.addEventListener("mouseout", (e) => {
    if (e.target.closest && e.target.closest(".concept")) gloss.hidden = true;
  });

  // per-view caption hover on the #tabs strip (Sam #189) — same #gloss delegation.
  document.addEventListener("mouseover", (e) => {
    const t = e.target.closest && e.target.closest("#tabs .tab");
    if (t && t.dataset.gloss) {
      gloss.textContent = t.dataset.gloss; gloss.hidden = false;
      gloss.style.left = Math.min(e.clientX + 12, window.innerWidth - 380) + "px";
      gloss.style.top = (e.clientY + 18) + "px";
    }
  });
  document.addEventListener("mouseout", (e) => {
    if (e.target.closest && e.target.closest("#tabs .tab")) gloss.hidden = true;
  });
}
