"use strict";

const $ = (s) => document.querySelector(s);

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

// --- "how this works" explainer (Task #102 / concern #250) ------------------------
// Fill the #explainer note from the buffer's matched sample, or hide it for a buffer
// that isn't one of the explained samples. Driven by content via explainerFor(), so it
// follows the program however it loaded (menu, ⌘K, share link, tour). Collapsed state is
// remembered per session so an expert who closes it once isn't re-nagged on the next sample.
let _explainerOpen = false;
function renderExplainer(source) {
  const el = $("#explainer");
  if (!el) return;
  const ex = explainerFor(source);
  if (!ex) { el.hidden = true; el.open = false; el.querySelector(".ex-body").innerHTML = ""; return; }  // #361: clear, no stale text
  el.querySelector(".ex-body").innerHTML =
    `<div class="ex-what">${ex.what}</div>`
    + `<div class="ex-why">${ex.why}</div>`
    + `<div class="ex-try"><b>Try this:</b> ${ex.tryit}</div>`;
  el.hidden = false;
  el.open = _explainerOpen;
  el.ontoggle = () => { _explainerOpen = el.open; };
}

// --- the live loop ----------------------------------------------------------------
let timer = null, activeView = null, lastSource = "", _dimTimer = null, _elapsedTimer = null, _analyzeCtrl = null;
// STICKY VIEW: the user's last EXPLICITLY-selected view (a chip click, or a sample's headline view).
// On a source edit (run() with no view) we send this so the rendered view PERSISTS across edits instead
// of resetting to the recommended lead view. The backend honors it iff it's available for the new model
// (render.py: `req.view if req.view in VIEWS else _recommend(...)`) and reports the actual view it rendered,
// so an unavailable preferred view falls back gracefully and the family-sync follows data.view. Null until
// the first explicit selection ⇒ the first render still uses the recommended view.
let preferredView = null;
// #421 axisOverride + #436 activeInteractive/lastViews live in app-axes.js / app-interactive.js (cross-script globals).
window.addEventListener("keydown", (e) => { if (e.key === "Escape" && _analyzeCtrl) _analyzeCtrl.abort(); });  // Esc cancels an in-flight analyze (#149); guard avoids stealing Esc from modals

// The #structure panel + the interactive diagram overlay (renderStructure / fmtState /
// overlayPoints) and the `lastOverlay` they maintain live in app-structure.js (loaded before
// this file). paint() below calls renderStructure; app-verify.js reads lastOverlay.

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
let scopeBound = null;        // the scope knob's value (#21/#84); null ⇒ server default (REACH_LIMIT)
let kDepth = null;            // #327: reachable_region k-induction depth; null ⇒ k=1 (the one-step box)
let allConditions = false;    // state_graph: GLOBAL dynamics (every initial condition) vs from-init (diagram #1)
let _loadV = 0;               // #359: load-version token — a debounced analyze checks it so a stale buffer can't clobber a newer load

// Push a snapshot onto a newest-first ring buffer, capping length. Pure (returns the
// array) so it's unit-testable headless; mutates in place for the module array.

function setStatus(text, cls) { const s = $("#status"); s.textContent = text; s.className = cls || "dim"; }

// Translate parser jargon into something a newcomer can act on. The raw error stays
// (it's precise); we append a plain-language hint for the common footguns.
// Rust lexer token names → the literal the user actually typed (Sam #195: "got Eq" is jargon).

// renderStructure (#structure panel) + fmtState / overlayPoints (the interactive diagram
// overlay) moved to app-structure.js. paint() below calls renderStructure.

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
    renderVerdictHeader(null);                          // #432: a stale source has no current verdict — clear the header chips
    $("#invariant").hidden = true;                     // no reachable set → no verify row
    $("#query-row").hidden = true; $("#query-stack").hidden = true; $("#query-suggest").hidden = true;
    $("#inv-result").textContent = "";
    $("#tabs").innerHTML = "";                          // no current valid view — don't leave the
                                                       // 16-tab strip inviting clicks over a stale
                                                       // / empty diagram (Marek #147/#148).
    $("#axes-ctl").hidden = true;                       // #421: no live figure → no axis selector
    $("#allcond-ctl").hidden = true;
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
  updateVerifyPlaceholder(data);                           // concrete ⊢verify hint from the model's own vars (#155)
  activeView = data.view;
  lastViews = (data.views || []).slice(); lastClaim = !!data.claim;   // #436: for re-rendering the strip when an interactive view is active

  renderViewTabs(data, activeView, run);             // the view tab strip (app-history.js)

  // We're back to a live result — leave any read-only "past run" mode.
  pastView = null;
  // #436: an interactive view (query/verify) owns #view + has borrowed controls re-parented in. A stale
  // in-flight analyze must not clobber that cell (innerHTML= would destroy the borrowed #inv-prop etc.,
  // crashing a later loadProgram). Skip the figure render; banner/verdict above already ran.
  if (activeInteractive) return;
  // the rendered view: single live picture, or — when something is pinned — two-up (#207).
  renderLiveView(view, data);

  // the one-line "what am I looking at?" caption under the diagram — set on every render, cleared
  // when the view has no caption (so a stale caption never lingers under a different picture).
  $("#view-caption").textContent = (data.png && VIEW_CAPTIONS[data.view]) ? VIEW_CAPTIONS[data.view] : "";

  // ALL-INITIAL-CONDITIONS toggle (diagram #1): only meaningful for state_graph. Show it under
  // that view, hide it elsewhere, and append an honest phrase to the caption telling the reader
  // WHICH dynamics they're seeing — global (every init) vs reachable from the seeded init.
  const sg = data.png && data.view === "state_graph";
  $("#allcond-ctl").hidden = !sg;
  if (sg) {
    $("#allcond-in").checked = allConditions;
    const phrase = data.all_conditions
      ? " — global dynamics: every initial condition"
      : " — reachable from the seeded init";
    $("#view-caption").textContent += phrase;
  }

  // #421 AXIS SELECTOR: the backend echoes data.axes = {x, y, requested, fell_back} for the axis-taking
  // views, else null. Show the "axes ▾" control only when axes != null; reflect the active axes; let the
  // user override which var rides each axis. cobweb is 1-D (y echoes x) → show only the x select.
  renderAxesCtl(data);

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
  $("#structure").classList.remove("recomputing");      // drop the mid-recompute dim too — mirror #view below
  $("#structure").hidden = true; $("#invariant").hidden = true; $("#query-row").hidden = true; $("#query-stack").hidden = true;
  renderVerdictHeader(null);                              // #432: drop the stale verdict chips from the header too
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
  // An explicit view (a sample's headline #87/#128/#168, or a tab click) must not be clobbered by
  // a debounced re-analyze that setValue/edits just scheduled — that run() carries no view and would
  // re-recommend over it. Cancel the pending timer so the explicit view is the one that lands.
  if (view !== undefined) clearTimeout(timer);
  // #436: an explicit normal-view selection leaves any interactive view (restore its borrowed controls
  // to the interrogate panel); a no-view edit while an interactive view is open re-renders IT (re-runs
  // against the freshly-edited model) instead of fetching a PNG — so the query view stays open + live.
  if (view !== undefined && activeInteractive) { activeInteractive = null; _restoreInterrogate(); }
  else if (view === undefined && activeInteractive) { openInteractiveView(activeInteractive); return; }
  // STICKY VIEW: an explicit selection becomes the preferred view; a no-view edit re-uses it so the
  // rendered view survives the edit. Pass it on the request — the backend renders it if available for
  // the new model, else falls back to the recommended view (and reports which via data.view).
  if (view !== undefined) preferredView = view;
  const requestView = view !== undefined ? view : preferredView;
  // #421: the axis override for the view we're about to render (if the user picked one); null ⇒ auto-pick.
  const ax = axisParamsFor(requestView);   // { x_var, y_var } — app-axes.js
  const source = editor.getValue();
  lastSource = source;
  updateClaimPicker(source);   // show the entry-claim dropdown for multi-claim files (#86)
  renderExplainer(source);     // keep the "how this works" note in sync with the buffer (#102)
  // A saved-slot name (set on Save / on opening a slot) wins over the derived declaration
  // name — the user named this buffer, so honor it. Cleared when a sample/slot loads fresh.
  if (currentSlotName) {
    $("#fname").textContent = currentSlotName.replace(/\.ev$/, "") + ".ev";
  } else {
    // Pick the ENTRY decl, not the first one: an `fsm` is the entry; else the headline `claim` (the
    // LAST non-test claim — helper types/claims come first, e.g. toposort); else any type/enum. The old
    // "first fsm|claim|type|schema" showed a helper type's name for multi-decl samples (Marek #95/#108).
    const fsm = source.match(/^\s*fsm\s+([A-Za-z_]\w*)/m);
    let nm = fsm && fsm[1];
    if (!nm) {
      const claims = [...source.matchAll(/^\s*claim\s+([A-Za-z_]\w*)/gm)]
        .map((c) => c[1]).filter((n) => !/^(?:sat|unsat)_/.test(n));
      nm = claims.length ? claims[claims.length - 1] : null;
    }
    if (!nm) { const h = source.match(/^\s*(?:type|schema|enum)\s+([A-Za-z_]\w*)/m); nm = h ? h[1] : "untitled"; }
    $("#fname").textContent = nm + ".ev";
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
    // A witness BOARD: grey it, keep it visible (Sam #211). A text UNSAT core / scalar witness:
    // CLEAR it — a core naming variables the new source no longer has is misleading (Ana #266).
    if ($("#solve-body").querySelector(".viz-wrap")) $("#solve-body").classList.add("stale");
    else $("#solve-body").innerHTML = "";
  }
  const t0 = performance.now();
  // A live elapsed counter so a multi-second solve (real-valued / high-fanout FSMs run 1–8s) reads
  // as WORKING, not frozen (Ana/Marek #202). Only kicks in after 400ms so fast analyses don't flicker.
  _analyzeCtrl = new AbortController();   // #149: a handle so Esc can abort this analyze mid-solve
  clearInterval(_elapsedTimer);
  _elapsedTimer = setInterval(() => {
    const s = (performance.now() - t0) / 1000;
    if (s > 0.4) {
      setStatus(`solving… ${s.toFixed(1)}s · Esc to cancel`, "busy");
      solving.hidden = false; solving.textContent = `⟳ solving… ${s.toFixed(1)}s · Esc to cancel`;
    }
  }, 100);
  try {
    const res = await fetch("/api/analyze", {
      method: "POST", headers: { "content-type": "application/json" },
      signal: _analyzeCtrl.signal,
      // STICKY VIEW: a tab click passes its view explicitly; a source edit re-uses the last explicit
      // selection (preferredView) so the rendered view persists across edits. The server renders the
      // requested view if it's available for the new model, else re-recommends the lead view — so a
      // view that no longer applies falls back gracefully, and data.view reports what actually rendered.
      // entry: which top-level fsm/claim to render — the picker, else the runtime's last-defined default (#290).
      // x_var/y_var (#421): explicit projection axes for the axis-taking views; null ⇒ the backend auto-picks.
      body: JSON.stringify({ source, view: requestView || null, scope: scopeBound, k: kDepth, all_conditions: allConditions, entry: pickedEntry(), x_var: ax.x_var, y_var: ax.y_var }),
    });
    // A 500 RESOLVES the fetch (only a network drop rejects it), so without this check an HTTP
    // error would fall through and silently leave the prior picture looking live (Marek #206).
    if (!res.ok) { backendDown(`the solver returned HTTP ${res.status} — it likely crashed on that input`); return; }
    const data = await res.json();
    paint(data, Math.round(performance.now() - t0));
  } catch (e) {
    if (e.name === "AbortError") {            // #149: user pressed Esc — not a backend failure
      clearInterval(_elapsedTimer); solving.hidden = true;
      for (const id of ["banner", "structure", "view"]) $("#" + id).classList.remove("recomputing");
      setStatus("cancelled — edit to re-run", "dim"); return;
    }
    backendDown(String(e));
  }
}

// Persist + debounced analyze, driven from the single session 'change' handler above.
function scheduleAnalyze() {
  try { localStorage.setItem("evident-buffer", editor.getValue()); } catch (e) {}
  renderExplainer(editor.getValue());   // #361: hide/update the explainer INSTANTLY on edit, not 350ms later
  const v = _loadV;                      // #359: skip this run if a newer program loaded before the debounce fired
  clearTimeout(timer); timer = setTimeout(() => { if (v === _loadV) run(); }, 350);
}

// --- solve/query: run a claim → SAT witness or UNSAT; pin vars for solve-for-X --------
// The witness/UNSAT rendering + domain-picture renderers live in app-verify.js; this is the
// fetch orchestration that drives them.
// The entry the user picked, or null to let the runtime default to the LAST-DEFINED fsm/claim (#290).
// The visible picker wins; otherwise null (no override) so the binary's source-order default applies.
function pickedEntry() {
  const sel = $("#claim-select");
  return (sel && !sel.hidden && sel.value) ? sel.value : null;
}

async function solve(enumerate) {
  const source = editor.getValue();
  const given = parsePins($("#solve-given").value);
  // Name the entry explicitly so the solver doesn't choke when the file declares several entries
  // or a helper type/enum (e.g. toposort's `type Edge` + `claim toposort`). The picker wins; else
  // fall back to the LAST-DEFINED entry — the runtime's default (#86/#290).
  const entries = topLevelEntries(source);
  const claim = pickedEntry() || (entries.length ? entries[entries.length - 1] : null);
  $("#solve").hidden = false;
  $("#solve-head").innerHTML = `<span class="dim">${enumerate ? "enumerating…" : "solving…"}</span>`;
  $("#solve-body").innerHTML = "";
  try {
    const fold = enumerate && $("#fold-sym") && $("#fold-sym").checked;   // fold value-symmetric witnesses (Ana #271)
    const res = await fetch("/api/solve", {
      method: "POST", headers: { "content-type": "application/json" },
      body: JSON.stringify({ source, claim, given, enumerate: !!enumerate, limit: 20, fold_symmetry: !!fold }),
    });
    renderSolve(await res.json(), given);
  } catch (e) {
    $("#solve-head").innerHTML = `<span class="bad">solve failed: ${e}</span>`;
  }
}

$("#solve-btn").onclick = () => solve(false);
$("#solve-resolve").onclick = () => solve(false);
// Toggling fold re-runs the enumeration so the canonical/raw view switches live (Ana #271).
if ($("#fold-sym")) $("#fold-sym").onchange = () => solve(true);

// Export the SMT-LIB encoding (Ana #200): copy to clipboard so you can re-run it in z3 / paste it
// into notes; fall back to a .smt2 download where the clipboard is blocked.
$("#smtlib-btn").onclick = async () => {
  const ku = parseInt($("#unroll-in") && $("#unroll-in").value, 10);
  const unroll = (ku && ku > 0) ? ku : null;
  setStatus(unroll ? `unrolling ${unroll} steps…` : "exporting SMT-LIB…", "busy");
  try {
    const res = await fetch("/api/smtlib", {
      method: "POST", headers: { "content-type": "application/json" },
      body: JSON.stringify({ source: editor.getValue(), unroll }),
    });
    if (!res.ok) { backendDown(`the solver returned HTTP ${res.status}`); return; }
    const d = await res.json();
    if (!d.ok) { setStatus("✕ " + (d.error || "export failed"), "err"); return; }
    const okMsg = unroll ? `unrolled SMT-LIB (${unroll} steps) copied ✓` : "SMT-LIB copied to clipboard ✓";
    try {
      await navigator.clipboard.writeText(d.smtlib);
      setStatus(okMsg, "ok");
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
initAutocomplete();  // keyword/type/in-scope-var completer (app-editor.js, Marek #276/#279)
initBuffer();    // save/export/share buttons + #samples menu (app-buffer.js)
initVerify();    // verify-console listeners (app-verify.js)
initPalette();
initAxes();      // #421 axis-selector toggle + x/y select listeners (app-axes.js)
setupPanZoom();  // wheel-zoom / drag-pan / dbl-click-reset on #view — listeners attached ONCE (#233)

// #433/#440: the help/about overlay (helpOverlayHtml, app-symbols.js) in the shared #ck-modal chrome.
function openHelp() {
  $("#ck-modal-body").innerHTML = helpOverlayHtml();
  $("#ck-modal-title").textContent = "Help — what do these mean?";
  $("#ck-modal-help").hidden = true;     // already in help; no nested ?
  $("#ck-modal").hidden = false;
}
// #432/#433/#426: wire the cockpit modals (verdict dossier + help + history). closeModal() lives in
// app-structure.js (the dossier owner); here we attach the close affordances + the launchers.
if ($("#verdict-help-btn")) $("#verdict-help-btn").onclick = openHelp;
if ($("#ck-modal-close")) $("#ck-modal-close").onclick = () => closeModal();
if ($("#ck-modal")) $("#ck-modal").addEventListener("click", (e) => { if (e.target.id === "ck-modal") closeModal(); });
if ($("#history-btn")) $("#history-btn").onclick = () => { $("#hist-modal").hidden = false; };
if ($("#hist-modal-close")) $("#hist-modal-close").onclick = () => { $("#hist-modal").hidden = true; };
if ($("#hist-modal")) $("#hist-modal").addEventListener("click", (e) => { if (e.target.id === "hist-modal") $("#hist-modal").hidden = true; });
// Esc closes whichever cockpit modal is open (the in-flight-analyze Esc above is guarded by _analyzeCtrl).
window.addEventListener("keydown", (e) => {
  if (e.key !== "Escape") return;
  if (!$("#ck-modal").hidden) { closeModal(); e.stopPropagation(); }
  else if (!$("#hist-modal").hidden) { $("#hist-modal").hidden = true; e.stopPropagation(); }
});
// Scope knob (#21/#84): set the exploration/verification bound, then re-analyze. Empty ⇒ server default.
$("#scope-in").addEventListener("change", () => {
  const v = parseInt($("#scope-in").value, 10);
  scopeBound = (v && v > 0) ? v : null;
  run();
});
// ALL-INITIAL-CONDITIONS toggle (diagram #1): flip global vs from-init dynamics, then re-analyze
// pinned to state_graph (an explicit view, so the re-run isn't re-recommended off the graph).
$("#allcond-in").addEventListener("change", () => {
  allConditions = $("#allcond-in").checked;
  run("state_graph");
});
// Entry picker (#290): choosing a different top-level fsm/claim re-renders THAT entry. Re-analyze
// with no explicit view so the server re-recommends the lead view for the newly-selected entry.
$("#claim-select").addEventListener("change", () => run());
run();
maybeAutoTour();
// If we loaded a program from a shared link, say so — but only after run()'s "computing…"
// settles, so the message isn't immediately clobbered. Subtle, dismissed by the next action.
if (SHARED != null) {
  setTimeout(() => { setStatus("loaded from a shared link ✓", "ok"); }, 900);
}
