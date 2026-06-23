"use strict";


const $ = (s) => document.querySelector(s);

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

// --- editor construction ----------------------------------------------------------
const editor = ace.edit("code");
editor.setTheme("ace/theme/dracula");
editor.session.setMode("ace/mode/evident");
editor.session.setUseWrapMode(true);          // line wrapping on
editor.session.setTabSize(4);
editor.session.setUseSoftTabs(true);
editor.setOptions({
  fontSize: "14px",
  showPrintMargin: false,
  highlightActiveLine: true,
  useWorker: false,                            // no built-in linter; analyze() is our diagnostics
  newLineMode: "unix",
});
editor.renderer.setShowGutter(true);

// --- buffer bootstrap (save / export / share / samples live in app-buffer.js) ------
// The named-slot / export / share-link helpers + the #samples menu were split into
// app-buffer.js (loaded before this file); initBuffer() wires their buttons below. The
// editor's initial value still loads here (it's the editor's own bootstrap).

// Persist the buffer across reloads — losing your work on an accidental refresh is the
// fastest way to lose a user's trust. A shared link (#src=…) takes precedence over the
// auto-persisted buffer: the whole point of the link is to override what's already there.
const SHARED = sharedFromHash(location.hash);
const SAVED = (() => { try { return localStorage.getItem("evident-buffer"); } catch (e) { return null; } })();
editor.setValue(SHARED != null ? SHARED : (SAVED != null ? SAVED : DEFAULT_PROGRAM), -1);   // -1 = cursor to start
// The buffer is saved on every edit (scheduleAnalyze), but a refresh during the 350ms analyze
// debounce — or before the first edit — could miss the latest keystrokes; flush on unload too so
// reload NEVER loses work (Marek #174).
window.addEventListener("beforeunload", () => {
  try { localStorage.setItem("evident-buffer", editor.getValue()); } catch (e) {}
});

// --- shared tooltip + solving badge elements --------------------------------------
// The #gloss tooltip is shared by the editor hover glossary, banner-concept and view-caption
// tooltips (all wired in app-editor.js via initEditorInput) and by the diagram point overlay
// (overlayPoints below). The #solving badge is the "still computing" overlay used by run()/paint().
const gloss = document.createElement("div");
gloss.id = "gloss"; gloss.hidden = true; document.body.appendChild(gloss);
// A PROMINENT solving badge centered over the diagram — the top-bar status counter was too easy to
// miss, so a >0.4s analyze read as "frozen" (Marek #202/#132). This sits where the user is looking.
const solving = document.createElement("div");
solving.id = "solving"; solving.hidden = true; $("#dynamics").appendChild(solving);

// --- the live loop ----------------------------------------------------------------
let timer = null, activeView = null, lastSource = "", _dimTimer = null, _elapsedTimer = null;

// The most recent diagram overlay (the live `.view-wrap` + its identifiable points), so the
// trace scrubber can locate the current step's state ON the diagram and ring it (#231/#206 —
// "the trace lights up the explorer"). Set when overlayPoints draws the live overlay; cleared
// by a render with no points so a stale ring never floats over a different picture.
let lastOverlay = null;

// --- run-history + pin/compare (tasks #209, #207) ---------------------------------
// `history` is a ring buffer of the last good analyses (newest first), so you can flip
// back to "what did this look like 3 edits ago" (#209). `pinnedA` holds one snapshot
// captured by the 📌 button so the live result renders BESIDE it (#207). `pastView`,
// when set, means we're looking at a past snapshot read-only — the next edit returns live.
const HISTORY_CAP = 8;
let history = [];
let pinnedA = null;
let pastView = null;
let currentSlotName = null;   // the active saved-slot name; overrides the derived #fname (Task #213)

// Push a snapshot onto a newest-first ring buffer, capping length. Pure (returns the
// array) so it's unit-testable headless; mutates in place for the module array.

function setStatus(text, cls) { const s = $("#status"); s.textContent = text; s.className = cls || "dim"; }

// Translate parser jargon into something a newcomer can act on. The raw error stays
// (it's precise); we append a plain-language hint for the common footguns.
// Rust lexer token names → the literal the user actually typed (Sam #195: "got Eq" is jargon).

// The SOLVER-computed structure of the whole model (not a single run): a verdict, the
// rigorous fixed points / equilibria (states the solver proves map to themselves), and the
// exact boundary of the solution space (min..max each variable spans over the reachable set).
function renderStructure(s) {
  const el = $("#structure");
  // the invariant checker only makes sense for an FSM with a reachable set — not a raw claim
  $("#invariant").hidden = !s || !!s.claim;
  $("#query-row").hidden = !s || !!s.claim;            // ad-hoc query shares the verify row's gate
  if ($("#query-row").hidden) { $("#query-stack").hidden = true; $("#query-suggest").hidden = true; }  // hide the chips too
  if (!s) { el.hidden = true; return; }
  el.hidden = false;
  const [icon, name, note] = VERDICTS[s.verdict] || ["·", s.verdict, ""];
  // title= tooltips teach the verification concepts in place — the words appear in the panel,
  // not the editor, so the editor glossary can't reach them (Sam #163).
  const vhelp = "the model's GLOBAL behaviour, solved from the transition relation over the whole "
    + "reachable set — not one simulated run.";
  let html = `<span class="verdict v-${s.verdict}" title="${vhelp}">${icon} ${name}</span>`
    + (note ? `<span class="dim">${note}</span>` : "");
  if (s.fixed_points && s.fixed_points.length) {
    const fp = s.fixed_points.slice(0, 3).map(
      (f) => "(" + Object.entries(f).map(([k, v]) => `${k}=${v}`).join(", ") + ")").join("  ");
    const more = s.fixed_points.length > 3 ? ` +${s.fixed_points.length - 3}` : "";
    const label = s.verdict === "nondeterministic" ? "rest states" : "fixed point";
    const fhelp = "a REACHABLE state the system maps to itself — once here it stays. Found by "
      + "solving T(s,s), then intersected with the reachable set so it's never a phantom.";
    html += `<span class="struct-fp" title="${fhelp}">● ${label}: ${fp}${more}</span>`;
  }
  const b = s.bounds || {}, keys = Object.keys(b);
  if (keys.length) {
    const bstr = keys.map((k) => `${k} ∈ [${b[k][0]}, ${b[k][1]}]`).join("   ");
    const bhelp = "the exact range each variable spans over the solution space — z3-proven "
      + "(Optimize over the unrolled transition), not the min/max of one run.";
    html += `<span class="struct-bounds" title="${bhelp}">⊏ boundary${s.capped ? " (≥, capped)" : ""}: ${bstr}</span>`;
  }
  el.innerHTML = html;
}

// Interactive diagram overlay (#184): drop transparent hover targets over the rendered
// solution_space scatter — hover → tooltip of that point's full state; click → pin it until
// the next hover. fx/fy are figure fractions from the top-left, so they map directly to a
// wrapper sized exactly to the image. No-op (and clears) when there are no points.
function fmtState(st) {
  // Round float values to ~4 significant figures — a raw f64 like -0.012319949034860988 is noise
  // in a hover tooltip (Marek #228). Integers/bools/enums pass through unchanged.
  const fmtVal = (v) => (typeof v === "number" && !Number.isInteger(v)) ? Number(v.toPrecision(4)) : v;
  return Object.entries(st || {})
    .map(([k, v]) => `${k}=${fmtVal(v)}`).join("  ");
}
function overlayPoints(wrap, points) {
  if (!wrap) { lastOverlay = null; return; }
  // Record the live overlay so the trace scrubber can ring the current step's state on it.
  // Only a wrap WITH plottable points is a useful target; otherwise clear (no ring possible).
  lastOverlay = (points && points.length) ? { wrap, points } : null;
  let pinned = false;
  const show = (txt, x, y) => {
    gloss.textContent = txt; gloss.hidden = false;
    gloss.style.left = Math.min(x + 12, window.innerWidth - 380) + "px";
    gloss.style.top = (y + 18) + "px";
  };
  const hide = () => { if (!pinned) gloss.hidden = true; };
  if (!points || !points.length) return;
  points.forEach((p) => {
    if (typeof p.fx !== "number" || typeof p.fy !== "number") return;
    const t = document.createElement("div");
    t.className = "pt-target";
    t.style.left = (p.fx * 100) + "%";
    t.style.top = (p.fy * 100) + "%";
    const txt = fmtState(p.state);
    t.title = txt + " — click to explore from here";   // native tooltip + explore hint (#242)
    t.addEventListener("mouseenter", (e) => { pinned = false; show(txt, e.clientX, e.clientY); });
    t.addEventListener("mousemove", (e) => { if (!pinned) show(txt, e.clientX, e.clientY); });
    t.addEventListener("mouseleave", hide);
    // Click → pin the tooltip AND explore from this state (#242): "assume the machine is HERE",
    // what's reachable forward + what run leads here. explorePoint lives in app-verify.js.
    t.addEventListener("click", (e) => {
      e.stopPropagation(); pinned = true; show(txt, e.clientX, e.clientY);
      if (typeof explorePoint === "function" && p.state) explorePoint(p.state);
    });
    wrap.appendChild(t);
  });
}

function paint(data, ms) {
  clearInterval(_elapsedTimer); solving.hidden = true;   // stop the elapsed ticker — result is in
  $("#latency").textContent = ms != null ? `${ms} ms` : "";
  gloss.hidden = true;                                  // clear any pinned overlay tooltip from the
                                                       // previous program — a ghost pin must not
                                                       // float over the new diagram (Marek #172).
  $("#banner").classList.remove("recomputing");        // analysis returned — undim
  $("#structure").classList.remove("recomputing");
  $("#view").classList.remove("recomputing");
  // Tint each dropped-constraint line in the editor, on every result (ok / error / claim
  // alike) — the silent bug surfaces AT the line, not just in the console banner. Cleared
  // here too: an empty/absent dropped_locs wipes the previous run's amber markers.
  markDroppedLines(data.dropped_locs, data.warnings);
  const view = $("#view"), warn = $("#warnings");
  $("#view-caption").textContent = "";                   // clear the per-view caption on any result;
                                                         // the OK path below re-sets it for the new view.
  if (!data.ok) {
    exitCompareModes();                                // never a two-up / past view over an error or claim
    $("#structure").hidden = true;
    $("#invariant").hidden = true;                     // no reachable set → no verify row
    $("#query-row").hidden = true; $("#query-stack").hidden = true; $("#query-suggest").hidden = true;
    $("#inv-result").textContent = "";
    $("#tabs").innerHTML = "";                          // no current valid view — don't leave the
                                                       // 16-tab strip inviting clicks over a stale
                                                       // / empty diagram (Marek #147/#148).
    // a pure claim (no FSM) isn't an error — it's a solve target, not a thing to visualize
    if (/no fsm schemas? found/i.test(data.error || "")) {
      setStatus("claim — use Solve", "ok");
      $("#errors").hidden = true; warn.hidden = true;
      view.classList.remove("stale");
      view.innerHTML = '<div class="ph">No state machine to visualize.<br>'
        + 'Press <b>⊨ Solve</b> (top bar) to run this claim → a witness, or UNSAT.</div>';
      $("#banner").className = "live";
      $("#banner").textContent = "◆ a claim (a relation) — solve it for a witness assignment";
      $("#honesty").innerHTML = '<span class="dim">⊨ Solve runs the constraints → SAT (with a witness) or UNSAT</span>';
      clearErrorLine();
      return;
    }
    setStatus("error", "err");
    $("#errors").hidden = false;
    $("#errors").textContent = humanizeError(data.error || "analysis failed");
    markErrorLine(data.error, data.error_loc);     // highlight the offending line in the gutter
    // the diagram on screen is from a PREVIOUS good run — mark it stale; never show
    // green reachable-state stats next to a red parse error.
    view.classList.add("stale");
    $("#banner").className = "stale";
    $("#banner").textContent = "▷ source has an error — fix it to refresh the analysis";
    $("#honesty").innerHTML = data.dropped
      ? `<span class="dropped">⚠ ${data.dropped} dropped constraint(s)</span><span class="dim">diagram stale — fix the error</span>`
      : `<span class="dim">diagram stale — fix the error above</span>`;
    warn.hidden = !(data.dropped && data.warnings);
    if (!warn.hidden) warn.textContent = data.warnings;
    return;
  }
  view.classList.remove("stale");
  $("#errors").hidden = true;
  clearErrorLine();
  setStatus("ok", "ok");
  $("#banner").className = "live";
  $("#banner").innerHTML = "◆ " + annotateConcepts(data.banner);
  renderStructure(data.structure);
  renderQuerySuggestions(data);                            // example-query chips (Sam #248)
  activeView = data.view;

  renderViewTabs(data, activeView, run);             // the view tab strip (app-history.js)

  // We're back to a live result — leave any read-only "past run" mode.
  pastView = null;
  // the rendered view: single live picture, or — when something is pinned — two-up (#207).
  renderLiveView(view, data);

  // the one-line "what am I looking at?" caption under the diagram — set on every render, cleared
  // when the view has no caption (so a stale caption never lingers under a different picture).
  $("#view-caption").textContent = (data.png && VIEW_CAPTIONS[data.view]) ? VIEW_CAPTIONS[data.view] : "";

  // run-history (#209): snapshot only SUCCESSFUL, drawable results — never errors / claims /
  // backend-down, and never a result with no png (nothing to thumbnail).
  if (data.png) {
    pushHistory(history, {
      ts: Date.now(), fname: $("#fname").textContent, banner: data.banner || data.view || "",
      view: data.view, png: data.png, points: data.points || [], ms,
      source: lastSource,    // the source that produced this picture — the model-diff (📌 ⇄ diff) pins A's source
    }, HISTORY_CAP);
  }
  renderHistory();

  // the honesty line (branching ×N surfaces nondeterminism right next to the stats)
  const dropCls = data.dropped ? "dropped" : "clean";
  const dropTxt = data.dropped ? `⚠ ${data.dropped} dropped constraint(s)` : "✓ 0 dropped constraints";
  const branch = data.branching >= 2 ? ` · branching ×${data.branching}` : "";
  // Scope certification (Ana #243): is "no more reachable" a PROOF or a CAP? When the BFS reached a
  // fixpoint it's the COMPLETE set (sound to reason over); when it hit the limit it's a bounded
  // sample. Say which, so "9 reachable states" can't be mistaken for a proof when it's a sample.
  const scopeCert = scopeCertHtml(data);   // honesty-line scope certification (app-symbols.js, Marek #274)
  $("#honesty").innerHTML =
    `<span class="${dropCls}">${dropTxt}</span>` +
    `<span class="dim">${scopeCert} · ${data.edges} transitions${branch}</span>` +
    `<span class="dim">vars: ${(data.vars || []).join(", ")}</span>`;

  // which constraint(s) vanished — the actual dropped text, not just a count, with a
  // did-you-mean for the capital-True/False footgun.
  warn.hidden = !(data.dropped && data.warnings);
  if (!warn.hidden) {
    const tf = /\bTrue\b|\bFalse\b/.test(data.warnings)
      ? "→ Booleans are lowercase in Evident: use true / false, not True / False.\n\n" : "";
    warn.textContent = tf + data.warnings;
  }
}


// The backend (solver) is unreachable OR returned an error status — it crashed or was stopped.
// NEVER leave the prior picture/verdict looking live (Ana #202, Marek #206): mark everything stale,
// hide the verdict, and say so loudly so a stale diagram is never mistaken for the current program's.
function backendDown(detail) {
  clearTimeout(_dimTimer); clearInterval(_elapsedTimer); solving.hidden = true;
  exitCompareModes();                                    // don't show a stale two-up / past view over a dead backend
  setStatus("backend down", "err");
  $("#banner").className = "stale";
  $("#banner").textContent = "⚠ backend unavailable — the solver isn't responding";
  $("#structure").hidden = true; $("#invariant").hidden = true; $("#query-row").hidden = true; $("#query-stack").hidden = true;
  $("#tabs").innerHTML = "";
  $("#view-caption").textContent = "";                   // no live diagram → no caption
  // BLANK the diagram entirely — a greyed-but-plausible picture (with its old title) can still read
  // as a believable lie when the backend is dead (Marek #177). Replace it with a clear placeholder.
  $("#view").classList.remove("recomputing", "stale");
  $("#view").innerHTML = '<div class="ph">⚠ backend unreachable — no live diagram.<br>Restart the server, then edit to refresh.</div>';
  $("#errors").hidden = false;
  $("#errors").textContent = "Could not reach the backend (it may have crashed or been stopped). "
    + "The picture above is stale. Restart it:\n\n    ./ide/web/run.sh   (or  python3 ide/web/server.py)\n\n(" + detail + ")";
}

async function run(view) {
  const source = editor.getValue();
  lastSource = source;
  // A saved-slot name (set on Save / on opening a slot) wins over the derived declaration
  // name — the user named this buffer, so honor it. Cleared when a sample/slot loads fresh.
  if (currentSlotName) {
    $("#fname").textContent = currentSlotName.replace(/\.ev$/, "") + ".ev";
  } else {
    const nm = source.match(/^\s*(?:fsm|claim|type|schema)\s+([A-Za-z_]\w*)/m);
    $("#fname").textContent = (nm ? nm[1] : "untitled") + ".ev";
  }
  setStatus("computing…", "busy");
  // Immediately mark the derived panels recomputing — the PREVIOUS program's Structure verdict,
  // verify result and solve witness must NEVER read as current while a new analysis runs, on a
  // switch / edit / error alike (Marek #64/#91/#93). paint() repaints or hides them on result.
  $("#banner").classList.add("recomputing");
  $("#structure").classList.add("recomputing");
  $("#view").classList.add("recomputing");                 // dim the OLD picture, not just the banner
  $("#inv-result").textContent = "";                       // last verify result is stale on any edit
  $("#query-result").textContent = "";                     // …and the ad-hoc query verdict (Marek #230)
  clearTrace();                                            // …and so is the counterexample scrubber
  if (!$("#solve").hidden) {                                // stale witness/UNSAT under a changed source
    $("#solve-head").innerHTML = '<span class="dim">source changed — press re-solve</span>';
    $("#solve-body").classList.add("stale");               // grey the board too, like #view (Sam #211)
  }
  const t0 = performance.now();
  // A live elapsed counter so a multi-second solve (real-valued / high-fanout FSMs run 1–8s) reads
  // as WORKING, not frozen (Ana/Marek #202). Only kicks in after 400ms so fast analyses don't flicker.
  clearInterval(_elapsedTimer);
  _elapsedTimer = setInterval(() => {
    const s = (performance.now() - t0) / 1000;
    if (s > 0.4) {
      setStatus(`solving… ${s.toFixed(1)}s`, "busy");
      solving.hidden = false; solving.textContent = `⟳ solving… ${s.toFixed(1)}s`;
    }
  }, 100);
  try {
    const res = await fetch("/api/analyze", {
      method: "POST", headers: { "content-type": "application/json" },
      // A source edit (run() with no view) sends null so the server RE-RECOMMENDS the
      // lead view for what was just written — otherwise a tab click pins the view and a
      // later edit that turns the machine nondeterministic keeps showing a flat line.
      // A tab click (run("phase_portrait")) passes its view explicitly and is honored.
      body: JSON.stringify({ source, view: view || null }),
    });
    // A 500 RESOLVES the fetch (only a network drop rejects it), so without this check an HTTP
    // error would fall through and silently leave the prior picture looking live (Marek #206).
    if (!res.ok) { backendDown(`the solver returned HTTP ${res.status} — it likely crashed on that input`); return; }
    const data = await res.json();
    paint(data, Math.round(performance.now() - t0));
  } catch (e) {
    backendDown(String(e));
  }
}

// Persist + debounced analyze, driven from the single session 'change' handler above.
function scheduleAnalyze() {
  try { localStorage.setItem("evident-buffer", editor.getValue()); } catch (e) {}
  clearTimeout(timer); timer = setTimeout(() => run(), 350);
}

// --- solve/query: run a claim → SAT witness or UNSAT; pin vars for solve-for-X --------
// The witness/UNSAT rendering + domain-picture renderers live in app-verify.js; this is the
// fetch orchestration that drives them.
async function solve(enumerate) {
  const source = editor.getValue();
  const given = parsePins($("#solve-given").value);
  // Name the claim explicitly so the solver doesn't choke on "ambiguous" when the file also
  // declares a type/enum (e.g. toposort's `type Edge` + `claim toposort`).
  const cm = source.match(/^\s*claim\s+([A-Za-z_]\w*)/m);
  const claim = cm ? cm[1] : null;
  $("#solve").hidden = false;
  $("#solve-head").innerHTML = `<span class="dim">${enumerate ? "enumerating…" : "solving…"}</span>`;
  $("#solve-body").innerHTML = "";
  try {
    const res = await fetch("/api/solve", {
      method: "POST", headers: { "content-type": "application/json" },
      body: JSON.stringify({ source, claim, given, enumerate: !!enumerate, limit: 20 }),
    });
    renderSolve(await res.json(), given);
  } catch (e) {
    $("#solve-head").innerHTML = `<span class="bad">solve failed: ${e}</span>`;
  }
}

$("#solve-btn").onclick = () => solve(false);
$("#solve-resolve").onclick = () => solve(false);

// Export the SMT-LIB encoding (Ana #200): copy to clipboard so you can re-run it in z3 / paste it
// into notes; fall back to a .smt2 download where the clipboard is blocked.
$("#smtlib-btn").onclick = async () => {
  setStatus("exporting SMT-LIB…", "busy");
  try {
    const res = await fetch("/api/smtlib", {
      method: "POST", headers: { "content-type": "application/json" },
      body: JSON.stringify({ source: editor.getValue() }),
    });
    if (!res.ok) { backendDown(`the solver returned HTTP ${res.status}`); return; }
    const d = await res.json();
    if (!d.ok) { setStatus("✕ " + (d.error || "export failed"), "err"); return; }
    try {
      await navigator.clipboard.writeText(d.smtlib);
      setStatus("SMT-LIB copied to clipboard ✓", "ok");
    } catch (_) {                                       // clipboard blocked → download instead
      const a = document.createElement("a");
      a.href = URL.createObjectURL(new Blob([d.smtlib], { type: "text/plain" }));
      a.download = ($("#fname").textContent || "model").replace(/\.ev$/, "") + ".smt2";
      a.click(); URL.revokeObjectURL(a.href);
      setStatus("SMT-LIB downloaded ✓", "ok");
    }
  } catch (e) { setStatus("✕ " + e, "err"); }
};
$("#solve-all").onclick = () => solve(true);
if ($("#pin-btn")) $("#pin-btn").onclick = () => togglePin();
if ($("#diff-btn")) $("#diff-btn").onclick = () => runDiff();

// kick off
// The concern files (app-*.js) define their handlers/helpers but defer their DOM wiring to an
// initX() called here — they load BEFORE this file, so `editor` and the core globals (which this
// file owns and creates above) don't exist at their top level. initEditorInput runs first so the
// change/analyze handler is attached (after the initial setValue, matching the original order).
initEditorInput();   // auto-indent + token-input + hover tooltips (app-editor.js)
initBuffer();    // save/export/share buttons + #samples menu (app-buffer.js)
initVerify();    // verify-console listeners (app-verify.js)
initPalette();
setupPanZoom();  // wheel-zoom / drag-pan / dbl-click-reset on #view — listeners attached ONCE (#233)
run();
maybeAutoTour();
// If we loaded a program from a shared link, say so — but only after run()'s "computing…"
// settles, so the message isn't immediately clobbered. Subtle, dismissed by the next action.
if (SHARED != null) {
  setTimeout(() => { setStatus("loaded from a shared link ✓", "ok"); }, 900);
}
