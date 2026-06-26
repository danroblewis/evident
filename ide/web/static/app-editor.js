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

// --- Evident syntax-highlighting Ace mode -----------------------------------------
// A code editor with no language mode shows undifferentiated grey text. This Ace mode
// tokenizes Evident: keywords, the Unicode/ASCII operators, comments, strings, numbers,
// _prev reads, Type/Variant capitals, and booleans — mapped to dracula token classes.
ace.define("ace/mode/evident", [
  "require", "exports", "module",
  "ace/lib/oop", "ace/mode/text", "ace/mode/text_highlight_rules",
], function (require, exports) {
  const oop = require("ace/lib/oop");
  const TextMode = require("ace/mode/text").Mode;
  const TextHighlightRules = require("ace/mode/text_highlight_rules").TextHighlightRules;

  const KEYWORDS =
    "claim|type|enum|fsm|schema|import|assert|match|matches|subclaim|in|is" +
    "_first_tick|coindexed|edges";
  // The Unicode/ASCII operator glyphs. Escaped for use inside a character class.
  const OPS = "∈∉∀∃⇒⟸↦→⟨⟩≤≥≠Δ¬∧∨∪∩×·⊆∅=<>+\\-*/?:.,#|";

  function EvidentHighlightRules() {
    this.$rules = {
      start: [
        { token: "comment.line", regex: "--.*$" },
        { token: "string", regex: '"(?:\\\\.|[^"\\\\])*"' },
        { token: "constant.numeric", regex: "\\b\\d+(?:\\.\\d+)?\\b" },
        // booleans (lowercase) — capital True/False are unbound names, left as identifiers
        { token: "constant.language.boolean", regex: "\\b(?:true|false)\\b" },
        // keywords (word-boundary; is_first_tick handled by the regex alternation)
        { token: "keyword", regex: "\\b(?:" + KEYWORDS + ")\\b" },
        // previous-tick read: _foo
        { token: "variable.parameter", regex: "_[A-Za-z]\\w*\\b" },
        // Type name / enum Variant — Capitalized identifier
        { token: "entity.name.type", regex: "\\b[A-Z]\\w*\\b" },
        // plain identifiers
        { token: "identifier", regex: "\\b[a-z_]\\w*\\b" },
        // operators (Unicode + ASCII)
        { token: "keyword.operator", regex: "[" + OPS + "]" },
      ],
    };
  }
  oop.inherits(EvidentHighlightRules, TextHighlightRules);

  function Mode() {
    this.HighlightRules = EvidentHighlightRules;
    this.lineCommentStart = "--";
  }
  oop.inherits(Mode, TextMode);
  exports.Mode = Mode;
});

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

// --- autocomplete (Marek #276/#279) ----------------------------------------------
// The editor had NO completion: a typo'd `Δcountt` silently dropped its constraint and
// nothing offered `count`. Three completion groups feed Ace's Autocomplete:
//   keyword — the language words (fsm, claim, type, …)
//   type    — the built-in type names (Int, Bool, Seq, …)
//   var     — IN-SCOPE identifiers parsed from the CURRENT buffer (the typo-defense:
//             when you type `Δcoun`, `count` is offered so the drop never happens).
// keyword/type lists are the explicit name-sets below; their gloss text still lives in
// the GLOSSARY (single source) — these lists are just which keys belong to which group.
const COMPLETE_KEYWORDS = [
  "fsm", "claim", "type", "enum", "schema", "subclaim",
  "match", "matches", "import", "is_first_tick",
];
const COMPLETE_TYPES = ["Int", "Bool", "Nat", "Real", "String", "Seq", "Set"];

// The buffer SYMBOL SCAN + resolution (stripIdentPrefix / parseScopeDecls / parseScopeIdents /
// bufferDecls / parseBufferSymbols / scopeDecls / declAtToken / declHoverHtml / _gotoDecl /
// gotoDefinitionAtCursor) moved to app-symbols-scan.js (loaded before this file). The completer +
// the hover + go-to-def wiring below call them at call time.

// The Ace completer. Offers the three groups, prefix-matched. The prefix Ace hands us is the
// word under the cursor; we ALSO strip its Δ/_/¬ prefix so `Δcoun` matches the declared `count`
// (the whole point — typo-defense against the silent Δcountt drop).
const evidentCompleter = {
  getCompletions(ed, session, pos, prefix, callback) {
    // #389/#215: a `\name` token belongs to the backslash SYMBOL hint (app-symhint.js), not this
    // identifier completer — if a backslash sits immediately before the prefix, stand down so the two
    // never stack at the cursor (the symhint owns `\`-tokens; this completer owns word-tokens).
    const line = session.getLine(pos.row);
    if (line.slice(0, pos.column).match(/\\[A-Za-z]*$/)) { callback(null, []); return; }
    const base = stripIdentPrefix(prefix || "");
    const lower = base.toLowerCase();
    const matches = (name) => !lower || name.toLowerCase().startsWith(lower);
    const items = [], seen = new Set();
    const add = (name, meta, score) => {
      if (name === base || seen.has(name) || !matches(name)) return;
      seen.add(name); items.push({ caption: name, value: name, meta, score });
    };
    // in-scope vars score HIGHEST — they're the typo-defense and the most specific to this buffer.
    for (const v of parseScopeIdents(session.getValue())) add(v, "var", 1000);
    // #389: the buffer's declared NAMES — claim/fsm names (call them), declared types (∈ them), enum
    // variants (use them in a match / a literal). Scored just below in-scope vars, above built-ins.
    const sym = parseBufferSymbols(session.getValue());
    for (const c of sym.claims) add(c, "claim", 950);
    for (const t of sym.types) add(t, "type", 940);
    for (const vv of sym.variants) add(vv, "variant", 930);
    for (const k of COMPLETE_KEYWORDS) add(k, "keyword", 900);
    for (const t of COMPLETE_TYPES) add(t, "type", 800);   // built-in Int/Bool/… (deduped against buffer types)
    callback(null, items);
  },
};

// Wire autocomplete: load the language_tools ext (provides the Autocomplete machinery +
// the enable*Autocompletion options), then point the editor at ONLY our completer (drop the
// default text/keyword completers so live-complete stays quiet and never fights `\`-input).
function initAutocomplete() {
  try { ace.require("ace/ext/language_tools"); } catch (e) { /* ext not present — Ctrl-Space no-op */ }
  editor.setOptions({
    enableBasicAutocompletion: true,        // Ctrl-Space
    enableLiveAutocompletion: true,         // as-you-type (unobtrusive: only our completer feeds it)
    enableSnippets: false,
    liveAutocompletionThreshold: 2,         // need ≥2 chars before the live popup — keeps single
                                            // keystrokes (incl. the `\` of a `\in`→∈ mnemonic) quiet.
  });
  editor.completers = [evidentCompleter];
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
  if (ln == null || ln < 0 || ln >= editor.session.getLength()) return;
  const Range = ace.require("ace/range").Range;
  editor.session.addGutterDecoration(ln, "error-gutter");
  _errLine = ln;
  // Token-level squiggle when the parser gave a column: underline just the offending token (the
  // non-space run at col), not the whole line (Marek #194). Fall back to full-line without a col.
  if (loc && Number.isInteger(loc.col) && loc.col >= 1) {
    const lineText = editor.session.getLine(ln);
    const c0 = Math.min(Math.max(0, loc.col - 1), lineText.length);
    const tok = lineText.slice(c0).match(/^\S+/);
    const c1 = c0 + (tok ? tok[0].length : 1);
    _errMarker = editor.session.addMarker(new Range(ln, c0, ln, c1), "ace-error-token", "text");
  } else {
    _errMarker = editor.session.addMarker(new Range(ln, 0, ln, Infinity), "ace-error-line", "fullLine");
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
  // #386: find & replace — Ace's ext-searchbox isn't in this bundle (its `replace` lazy-loads a 404'd
  // file), so bind Ctrl-F / Ctrl-H to our own bar (app-findreplace.js), wired to Ace's core search APIs.
  if (typeof initFindReplace === "function") initFindReplace();
  editor.commands.addCommand({ name: "evidentFind", bindKey: { win: "Ctrl-F", mac: "Command-F" },
    exec: () => { if (typeof openFind === "function") openFind(); } });
  editor.commands.addCommand({ name: "evidentReplace", bindKey: { win: "Ctrl-H", mac: "Command-Option-F" },
    exec: () => { if (typeof openFindReplace === "function") openFindReplace(true); } });

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

  // F2 → rename the identifier at the cursor, and its _/__/Δ/ΔΔ/¬ forms, everywhere (Ana #276).
  // Find-references already works (Ace highlights every occurrence of the selected word). The token
  // boundaries (lookbehind/ahead on identifier chars, with the prefix captured + preserved) keep
  // `discount`/`count5` safe and rename `_count`→`_new` alongside `count`→`new`.
  editor.commands.addCommand({
    name: "renameSymbol",
    bindKey: { win: "F2", mac: "F2" },
    exec: function (ed) {
      const pos = ed.getCursorPosition();
      const base = stripIdentPrefix((ed.session.getTextRange(ed.session.getWordRange(pos.row, pos.column)) || "").trim());
      if (!/^[A-Za-z_]\w*$/.test(base)) { setStatus("put the cursor on an identifier to rename (F2)", "dim"); return; }
      const next = window.prompt(`Rename "${base}" (and its _ / Δ forms) to:`, base);
      if (!next || next === base) return;
      if (!/^[A-Za-z_]\w*$/.test(next)) { setStatus(`"${next}" isn't a valid identifier`, "err"); return; }
      const esc = base.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
      const re = new RegExp("(?<![A-Za-z0-9_])((?:ΔΔ|Δ|__|_|¬)*)" + esc + "(?![A-Za-z0-9_])", "g");
      ed.setValue(ed.getValue().replace(re, "$1" + next), -1);   // one undo group reverts the whole rename
      setStatus(`renamed ${base} → ${next} (everywhere, prefixes preserved)`, "ok");
    },
  });

  // #429: ⌘D / Ctrl-D → add the NEXT occurrence of the selection to a multi-cursor (Sublime/VSCode's
  // selectMore). Ace ships the multiselect machinery (ed.selectMore) but its default keymap doesn't bind
  // ⌘D to it, and without an explicit binding the browser's native ⌘D (bookmark) hijacks the chord — so
  // we bind it here. With no selection, selectMore first selects the word at the cursor, then each press
  // adds the following match. We mark it readOnly:false so it's active in the editable buffer.
  editor.commands.addCommand({
    name: "selectNextOccurrence",
    bindKey: { win: "Ctrl-D", mac: "Command-D" },
    exec: function (ed) { ed.selectMore(1); },
    readOnly: false,
  });

  // typable-token input + the debounced analyze, both driven off the one change handler.
  editor.session.on("change", (delta) => {
    applyTokenInput(delta);
    updateBackslashHint();    // #215: refresh the inline `\name → glyph` hint as the token is typed (app-symhint.js)
    scheduleAnalyze();
  });
  initSymHint();   // #215: the backslash-hint cursor + keyboard-precedence listeners (app-symhint.js)

  // hover-to-learn glossary: resolve the token under the cursor and show its gloss.
  const editorEl = $("#code");
  editorEl.addEventListener("mousemove", (e) => {
    const pos = editor.renderer.screenToTextCoordinates(e.clientX, e.clientY);
    if (!pos) { gloss.hidden = true; return; }
    const tok = editor.session.getTokenAt(pos.row, pos.column + 1);
    if (tok) {
      const raw = (tok.value || "").trim();
      // #388: a DECLARED symbol (var/claim/type/enum/variant — incl. its _x/Δx form) shows its kind +
      // type + decl line, and wins over the generic glossary so `_count` reads "_count : Int — prev-tick
      // value of count · line 7" (the rich type card), not just the bare "previous-tick read" gloss.
      const html = declHoverHtml(raw);
      // #366: otherwise a keyword/operator teaches its MEANING. A multi-char op (`:=`, `++`) spanning the
      // cursor wins over its single-char token (`:`/`=`), which the Ace mode tokenizes apart.
      const g = html ? null : (glossAtCursor(editor.session.getLine(pos.row), pos.column) || glossFor(raw));
      if (g || html) {
        if (html) gloss.innerHTML = html; else gloss.textContent = g;
        gloss.hidden = false;
        gloss.style.left = Math.min(e.clientX + 12, window.innerWidth - 380) + "px";
        gloss.style.top = (e.clientY + 18) + "px";
        return;
      }
    }
    gloss.hidden = true;
  });
  editorEl.addEventListener("mouseleave", () => { gloss.hidden = true; });

  // #387: ⌘/Ctrl-click any DECLARED symbol → jump to its declaration line. Covers vars, claim/type/enum
  // names, and variants (declAtToken resolves the _/Δ form to its base var). Marek #282 + #387.
  editorEl.addEventListener("mousedown", (e) => {
    if (!(e.metaKey || e.ctrlKey)) return;
    const pos = editor.renderer.screenToTextCoordinates(e.clientX, e.clientY);
    if (!pos) return;
    const tok = editor.session.getTokenAt(pos.row, pos.column + 1);
    const r = tok ? declAtToken((tok.value || "").trim()) : null;
    if (r) { e.preventDefault(); e.stopPropagation(); _gotoDecl(r.decl); }
  }, true);   // capture phase: run BEFORE Ace's own mousedown so stopPropagation pre-empts its caret-place

  // #387: F12 (and the ⌘K "Go to definition") jumps from the identifier AT THE CURSOR to its decl line.
  editor.commands.addCommand({
    name: "gotoDefinition", bindKey: { win: "F12", mac: "F12" },
    exec: function (ed) {
      const pos = ed.getCursorPosition();
      const tok = ed.session.getTokenAt(pos.row, pos.column + 1);
      const r = tok ? declAtToken((tok.value || "").trim()) : null;
      if (r) _gotoDecl(r.decl);
      else setStatus("put the cursor on a declared symbol to jump to its definition (F12)", "dim");
    },
  });

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
