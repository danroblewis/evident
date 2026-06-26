"use strict";

// ==============================================================================
// app-findreplace.js — a find & replace bar for the editor (#386).
//
// Ace's ext-searchbox (the stock find/replace widget) is NOT in this bundle — the
// `replace` command tries to lazy-load /static/vendor/ext-searchbox.js and 404s. Rather
// than vendor a new file, this is a small custom bar wired to Ace's CORE search APIs
// (editor.find / findNext / findPrevious / replace / replaceAll), which ARE in ace-ace.js.
// Standard find / next / prev / replace / replace-all over the buffer.
//
// openFindReplace() (Ctrl-H / the ⌘K "Find & replace" command) shows it in replace mode;
// openFind() (Ctrl-F) shows it find-only. initFindReplace() wires the editor chords + the
// bar's controls, called from initEditorInput(). Reuses `editor` (app.js) at call time.
// ==============================================================================

let _frBar = null;

// Build (once) the floating find/replace bar over the editor pane.
function _frEl() {
  if (_frBar) return _frBar;
  _frBar = document.createElement("div");
  _frBar.id = "findreplace"; _frBar.hidden = true;
  _frBar.innerHTML =
    '<div class="fr-row">'
    + '<input id="fr-find" placeholder="find" autocomplete="off" spellcheck="false">'
    + '<span id="fr-count" class="dim"></span>'
    + '<button id="fr-prev" title="previous match (Shift-Enter)">↑</button>'
    + '<button id="fr-next" title="next match (Enter)">↓</button>'
    + '<button id="fr-close" title="close (Esc)">✕</button>'
    + '</div>'
    + '<div class="fr-row" id="fr-replace-row">'
    + '<input id="fr-rep" placeholder="replace with" autocomplete="off" spellcheck="false">'
    + '<button id="fr-rep-one" title="replace this match">replace</button>'
    + '<button id="fr-rep-all" title="replace every match">replace all</button>'
    + '</div>';
  document.body.appendChild(_frBar);
  return _frBar;
}

// Run the live find for the current query — highlights all matches and reports the count.
function _frDoFind(backwards) {
  const q = $("#fr-find").value;
  if (!q) { $("#fr-count").textContent = ""; return; }
  editor.find(q, { backwards: !!backwards, wrap: true, caseSensitive: false, regExp: false, preventScroll: false });
  // count all matches for the indicator (ace's $search over the whole doc).
  const ranges = editor.findAll(q, { caseSensitive: false, regExp: false });
  $("#fr-count").textContent = ranges ? `${ranges} match${ranges === 1 ? "" : "es"}` : "no matches";
}

function findReplaceOpen() { return !!_frBar && !_frBar.hidden; }
function closeFindReplace() {
  if (_frBar) _frBar.hidden = true;
  editor.focus();
}

// Open the bar. `replace` true → show the replace row (Ctrl-H); false → find-only (Ctrl-F). Seeds the
// find field with the current selection (the table-stakes "select a word, hit find" behaviour).
function openFindReplace(replace) {
  const el = _frEl();
  el.hidden = false;
  $("#fr-replace-row").hidden = !replace;
  const sel = editor.getSelectedText();
  const find = $("#fr-find");
  if (sel && !sel.includes("\n")) find.value = sel;
  find.focus(); find.select();
  if (find.value) _frDoFind(false);
}
function openFind() { openFindReplace(false); }

function initFindReplace() {
  _frEl();
  $("#fr-find").addEventListener("input", () => _frDoFind(false));
  $("#fr-find").addEventListener("keydown", (e) => {
    if (e.key === "Enter") { e.preventDefault(); _frDoFind(e.shiftKey); }
    else if (e.key === "Escape") { e.preventDefault(); closeFindReplace(); }
  });
  $("#fr-rep").addEventListener("keydown", (e) => {
    if (e.key === "Enter") { e.preventDefault(); _frReplaceOne(); }
    else if (e.key === "Escape") { e.preventDefault(); closeFindReplace(); }
  });
  $("#fr-next").onclick = () => _frDoFind(false);
  $("#fr-prev").onclick = () => _frDoFind(true);
  $("#fr-close").onclick = () => closeFindReplace();
  $("#fr-rep-one").onclick = () => _frReplaceOne();
  $("#fr-rep-all").onclick = () => _frReplaceAll();
}

// Replace the current match, then advance to the next (the standard find-and-replace cadence).
function _frReplaceOne() {
  const q = $("#fr-find").value; if (!q) return;
  editor.find(q, { wrap: true, caseSensitive: false, regExp: false });   // ensure a match is selected
  editor.replace($("#fr-rep").value);
  _frDoFind(false);
}
// Replace EVERY match in the buffer (one undo group), then report how many changed.
function _frReplaceAll() {
  const q = $("#fr-find").value; if (!q) return;
  editor.find(q, { wrap: true, caseSensitive: false, regExp: false });
  const n = editor.replaceAll($("#fr-rep").value);
  $("#fr-count").textContent = `replaced ${n}`;
  if (typeof setStatus === "function") setStatus(`replaced ${n} occurrence${n === 1 ? "" : "s"} ✓`, "ok");
}
