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

// Strip the carried-var read/delta prefixes (ΔΔ, Δ, __, _, ¬) so the BASE name is what we
// store and match against — `Δcount`, `_count`, `¬done` all reduce to `count`/`done`, and a
// prefix typed at the call site (`Δcoun`) still completes the declared base (`count`).
function stripIdentPrefix(name) {
  return (name || "").replace(/^(?:ΔΔ|Δ|__|_|¬)+/, "");
}

// Parse the buffer for declared/carried names → {type, line}: the LHS of `name ∈ Type` and
// chained-membership decls (`0 ≤ count ∈ Int ≤ 5` → count:Int; `x, y, z ∈ Int`; first-line params
// `type IVec2(x, y ∈ Int)`), plus `name = expr` carried/assignment LHS (type inferred → null).
// `line` is 1-based. First declaration wins. Pure (text → Map) so it's unit-testable headless.
function parseScopeDecls(text) {
  const decls = new Map();
  const IDENT = "[A-Za-zΔ¬_][A-Za-z0-9_]*";
  const lineOf = (idx) => text.slice(0, idx).split("\n").length;
  // (1) `… name(, name)* ∈ Type` — grab the ident run left of ∈ AND the type token right of it
  //     (an identifier with an optional `(…)` for Seq(Int)/IVec2). Chained `0 ≤ count ∈ Int ≤ 5`
  //     leaves `count` as the token before ∈ and `Int` as the type after it.
  const memb = new RegExp("(" + IDENT + "(?:\\s*,\\s*" + IDENT + ")*)\\s*∈\\s*([A-Za-z_]\\w*(?:\\s*\\([^)]*\\))?)", "g");
  let m;
  while ((m = memb.exec(text)) !== null) {
    const ln = lineOf(m.index), type = m[2].replace(/\s+/g, "");
    for (const raw of m[1].split(",")) {
      const base = stripIdentPrefix(raw.trim());
      if (base && !/^[0-9]/.test(base) && !decls.has(base)) decls.set(base, { type, line: ln });
    }
  }
  // (2) `name = expr` assignment / carried LHS (not `==`); type inferred from the RHS → null.
  const asg = new RegExp("^\\s*(" + IDENT + ")\\s*=(?!=)", "gm");
  while ((m = asg.exec(text)) !== null) {
    const base = stripIdentPrefix(m[1]);
    if (base && !/^[0-9]/.test(base) && !decls.has(base)) decls.set(base, { type: null, line: lineOf(m.index) });
  }
  return decls;
}

// Names only (the completer's view), de-duplicated, prefixes stripped.
function parseScopeIdents(text) { return [...parseScopeDecls(text).keys()]; }

// Cached decls for the hover / go-to-def handlers — the hover fires on every mousemove, so we
// re-parse only when the buffer text actually changed.
let _declsCache = { src: null, decls: null };
function scopeDecls() {
  const src = editor.session.getValue();
  if (src !== _declsCache.src) _declsCache = { src, decls: parseScopeDecls(src) };
  return _declsCache.decls;
}

// The Ace completer. Offers the three groups, prefix-matched. The prefix Ace hands us is the
// word under the cursor; we ALSO strip its Δ/_/¬ prefix so `Δcoun` matches the declared `count`
// (the whole point — typo-defense against the silent Δcountt drop).
const evidentCompleter = {
  getCompletions(ed, session, pos, prefix, callback) {
    const base = stripIdentPrefix(prefix || "");
    const lower = base.toLowerCase();
    const matches = (name) => !lower || name.toLowerCase().startsWith(lower);
    const items = [];
    for (const k of COMPLETE_KEYWORDS) if (matches(k)) items.push({ caption: k, value: k, meta: "keyword", score: 900 });
    for (const t of COMPLETE_TYPES) if (matches(t)) items.push({ caption: t, value: t, meta: "type", score: 800 });
    // in-scope vars score HIGHEST — they're the typo-defense and the most specific to this buffer.
    for (const v of parseScopeIdents(session.getValue())) {
      if (v !== base && matches(v)) items.push({ caption: v, value: v, meta: "var", score: 1000 });
    }
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
      const raw = (tok.value || "").trim();
      const g = glossFor(raw);
      // Not a keyword? If it's a user identifier we can resolve, show its declared type + line
      // (hover-for-type, Marek #282). ⌘/Ctrl-click then jumps to the declaration (below).
      const base = stripIdentPrefix(raw);
      const d = !g && (tok.type === "identifier" || tok.type === "variable.parameter") ? scopeDecls().get(base) : null;
      const html = d
        ? `<b>${base}</b>${d.type ? "  ∈  " + d.type : "  <span style='opacity:.6'>(type inferred)</span>"}`
          + `<span style="opacity:.6">  ·  line ${d.line}  ·  ⌘-click to jump</span>`
        : null;
      if (g || html) {
        if (g) gloss.textContent = g; else gloss.innerHTML = html;
        gloss.hidden = false;
        gloss.style.left = Math.min(e.clientX + 12, window.innerWidth - 380) + "px";
        gloss.style.top = (e.clientY + 18) + "px";
        return;
      }
    }
    gloss.hidden = true;
  });
  editorEl.addEventListener("mouseleave", () => { gloss.hidden = true; });

  // ⌘/Ctrl-click a user identifier → jump to its declaration (go-to-definition, Marek #282).
  editorEl.addEventListener("mousedown", (e) => {
    if (!(e.metaKey || e.ctrlKey)) return;
    const pos = editor.renderer.screenToTextCoordinates(e.clientX, e.clientY);
    if (!pos) return;
    const tok = editor.session.getTokenAt(pos.row, pos.column + 1);
    if (!tok || (tok.type !== "identifier" && tok.type !== "variable.parameter")) return;
    const d = scopeDecls().get(stripIdentPrefix((tok.value || "").trim()));
    if (d) {
      e.preventDefault();
      editor.gotoLine(d.line, 0, true);     // 1-based; flash the decl line so the jump is visible
      editor.selection.selectLine();
      gloss.hidden = true;
    }
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
