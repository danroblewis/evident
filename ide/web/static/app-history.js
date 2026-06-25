"use strict";

// ==============================================================================
// app-history.js — run-history ring-buffer + relative-age helpers (tasks #209/#207).
// Pure, headless-testable; the history array, thumbnail strip, and pin/compare DOM
// wiring stay in app.js. Loaded before app.js. Behaviour-preserving move.
// ==============================================================================

// Push a snapshot onto a newest-first ring buffer, capping length. Pure (returns the
// array) so it's unit-testable headless; mutates in place for the module array.
function pushHistory(arr, snap, cap) {
  arr.unshift(snap);
  if (arr.length > cap) arr.length = cap;
  return arr;
}

// Human "relative age" of a past timestamp vs now. Pure — unit-tested headless.
function relativeAge(deltaMs) {
  const s = Math.max(0, Math.floor(deltaMs / 1000));
  if (s < 5) return "just now";
  if (s < 60) return s + "s ago";
  const m = Math.floor(s / 60);
  if (m < 60) return m + "m ago";
  const h = Math.floor(m / 60);
  return h + "h ago";
}

// ==============================================================================
// Pin / compare / history rendering (tasks #207/#209/#184). These render the live
// view (single or two-up), the thumbnail history strip, and the read-only past-run
// view. They read/write the `history` / `pinnedA` / `pastView` state declared in
// app.js (shared script-global scope) and are called from paint()/run() there at
// call time. Moved verbatim out of app.js — behaviour-preserving.
// ==============================================================================

// One picture as a `.view-wrap` (image + optional hover overlay), or a placeholder when the
// program has no view. Shared by the single-view and two-up (#207) paths.
function viewPane(data, withOverlay) {
  if (!data.png) return `<div class="ph">no view for this program</div>`;
  const pane = document.createElement("div");
  pane.className = "view-wrap";
  // Descriptive alt-text for screen readers — the per-view caption PLUS the live verdict, reachable
  // count, and proven bounds, so a screen-reader user gets what the picture actually shows for THIS
  // program, not a generic description (Ana #216/#49/#238).
  const live = [];
  if (data.banner) live.push(String(data.banner));
  if (data.states != null) live.push(`${data.states}${data.capped ? "+" : ""} reachable state${data.states === 1 ? "" : "s"}`);
  const b = data.structure && data.structure.bounds;
  if (b) {
    const ranges = Object.entries(b).map(([k, v]) => `${k} in [${v[0]}, ${v[1]}]`).join(", ");
    if (ranges) live.push(ranges);
  }
  const alt = (`${(data.view || "diagram").replace(/_/g, " ")} — ${VIEW_CAPTIONS[data.view] || ""}`
    + (live.length ? " · " + live.join(" · ") : "")).replace(/"/g, "&quot;");
  pane.innerHTML = `<img alt="${alt}" src="data:image/png;base64,${data.png}">`;
  // #285: the HONESTY marker — is this picture PROVEN (abstract Z3 over all conditions), EXHAUSTIVE (the
  // full state graph), or SAMPLED (trajectories / a capped fallback)? Never let a sampled cloud read as a proof.
  const R = { proven: ["✓ proven", "abstract Z3 over ALL conditions — a proof, not a sample"],
    exhaustive: ["✓ all states", "the COMPLETE bounded-discrete state graph — every reachable state, not a sample"],
    sampled: ["~ sampled", "sampled trajectories / a capped or continuous run — NOT exhaustive; don't read it as a proof"] };
  if (data.rigor && R[data.rigor]) {
    const badge = document.createElement("div");
    badge.className = "rigor-badge rigor-" + data.rigor;
    badge.textContent = R[data.rigor][0]; badge.title = R[data.rigor][1];
    pane.appendChild(badge);
  }
  const _im = pane.querySelector("img");                   // the base64 img has no size until decoded —
  if (_im) _im.addEventListener("load", () => resetPanZoom(pane));   // re-fit once its size is known (#176)
  kKnob(pane, data);                                       // #327: the k-induction depth knob (reachable_region only)
  if (withOverlay) overlayPoints(pane, data.points || []);
  return pane;
}


function kKnob(pane, data) {
  // #327: raise k to deepen the unrolling and watch the reachable box CLOSE (proven inductive) or keep
  // growing (unbounded). The rigor badge above flips proven/sampled with closure — knob + badge, one story.
  if (data.view !== "reachable_region") return;
  const cur = data.k || 1;
  const knob = document.createElement("div");
  knob.className = "k-knob";
  const lbl = document.createElement("span");
  lbl.textContent = "k-induction";
  lbl.title = "k-induction depth — deepen the unrolling to try to PROVE the reachable box closed";
  knob.appendChild(lbl);
  const mk = (label, nk, title, off) => {
    const b = document.createElement("button");
    b.textContent = label; b.title = title; b.disabled = !!off;
    b.onclick = () => { kDepth = nk; run("reachable_region"); };
    return b;
  };
  knob.appendChild(mk("−", Math.max(1, cur - 1), "lower the induction depth", cur <= 1));
  const val = document.createElement("span"); val.className = "k-val"; val.textContent = "k=" + cur;
  knob.appendChild(val);
  knob.appendChild(mk("+", Math.min(64, cur + 1),
    "raise the depth — deeper unrolling, the box tightens toward closing", cur >= 64));
  pane.appendChild(knob);
}

// Render the live result into #view. Single picture normally; two-up (pinned A · live B) once
// the 📌 button has captured a snapshot (#207). Only the live B pane carries the #184 overlay.
function renderLiveView(view, data) {
  view.innerHTML = "";
  if (!pinnedA) {
    const pane = viewPane(data, true);
    if (typeof pane === "string") view.innerHTML = pane;
    else { view.appendChild(pane); resetPanZoom(pane); }   // identity zoom for the new picture (#233)
    return;
  }
  const row = document.createElement("div");
  row.className = "compare-row";
  row.appendChild(comparePane("A · pinned", pinnedA.banner, viewPane(pinnedA, false), true));
  row.appendChild(comparePane("B · live", data.banner || data.view || "", viewPane(data, true), false));
  view.appendChild(row);
  resetPanZoom(row.querySelector(".view-wrap"));            // reset zoom on a fresh render (#233)
}

// One labelled column of the two-up compare. `ghost` dims the pinned A so the live B reads as
// the current picture. The A column carries an ✕ to unpin.
function comparePane(label, caption, body, ghost) {
  const col = document.createElement("div");
  col.className = "compare-pane" + (ghost ? " ghost" : "");
  const head = document.createElement("div");
  head.className = "compare-label";
  head.textContent = label;
  if (ghost) {
    const x = document.createElement("span");
    x.className = "compare-unpin"; x.textContent = "✕"; x.title = "unpin A — back to single live view";
    x.onclick = () => setPinned(null);
    head.appendChild(x);
  }
  col.appendChild(head);
  if (typeof body === "string") { const ph = document.createElement("div"); ph.innerHTML = body; col.appendChild(ph); }
  else col.appendChild(body);
  const cap = document.createElement("div");
  cap.className = "compare-cap dim"; cap.textContent = caption;
  col.appendChild(cap);
  return col;
}

// The history strip (#209): up to HISTORY_CAP thumbnails, newest first. Click → read-only past
// view. Empty strip when there's no history. The current past-view thumb (if any) is outlined.
function renderHistory() {
  const strip = $("#history");
  if (!strip) return;
  strip.innerHTML = "";
  if (!history.length) return;
  const now = Date.now();
  history.forEach((snap, i) => {
    if (!snap.png) return;            // skip a snapshot with no picture (degrade gracefully)
    const age = relativeAge(now - snap.ts);
    const thumb = document.createElement("img");
    thumb.className = "hist-thumb" + (pastView === snap ? " on" : "");
    thumb.src = `data:image/png;base64,${snap.png}`;
    thumb.alt = snap.view;
    thumb.title = `${snap.banner}  ·  ${age}`;
    thumb.onclick = () => viewPastRun(snap);
    strip.appendChild(thumb);
  });
}

// Open a past snapshot read-only in #view (#209). A note says how long ago + how to return; the
// next edit / analyze (paint clears pastView) bounces back to live.
function viewPastRun(snap) {
  if (!snap || !snap.png) return;
  pastView = snap;
  const view = $("#view");
  view.classList.remove("stale", "recomputing");
  const age = relativeAge(Date.now() - snap.ts);
  view.innerHTML = `<div class="past-wrap"><div class="past-note">⟲ past run (${age}) — edit to return to live</div>`
    + `<div class="view-wrap"><img alt="${(snap.banner || snap.view || "").replace(/"/g, "&quot;")}" src="data:image/png;base64,${snap.png}"></div></div>`;
  $("#view-caption").textContent = snap.banner || "";
  renderHistory();   // re-outline the active thumbnail
}

// 📌 toggle (#207): capture the most-recent live result as A, or unpin if already pinned. We pin
// the newest history snapshot (it mirrors the current live result), so A is a real drawable run.
function togglePin() {
  if (pinnedA) { setPinned(null); return; }
  const snap = history.find((s) => s.png);
  if (!snap) {                       // nothing drawable to pin yet — say so, don't silently no-op (Ana #267)
    setStatus("nothing to pin yet — wait for the picture to render, then 📌", "dim");
    return;
  }
  setPinned(snap);
}

function setPinned(snap) {
  pinnedA = snap;
  const btn = $("#pin-btn");
  if (btn) { btn.classList.toggle("on", !!snap); btn.textContent = snap ? "📌 unpin" : "📌 pin"; }
  syncDiffBtn();
  // re-render the live view in the new layout, using the freshest history snapshot as B.
  if (!pastView && history.length) renderLiveView($("#view"), history[0]);
}

// ⇄ diff is meaningful only with a pinned A that carries its source (the model-diff needs both
// programs). Hidden otherwise, so it degrades gracefully to "pin something first".
function syncDiffBtn() {
  const btn = $("#diff-btn");
  if (!btn) return;
  const ready = !!(pinnedA && pinnedA.source);
  btn.hidden = !ready;
  if (!ready) { const panel = $("#diff-panel"); if (panel) panel.hidden = true; }
}

// ==============================================================================
// Model-diff (#223): POST {source_a: pinnedA, source_b: live editor} → /api/diff and render the
// relational delta — which reachable states APPEARED in B, which VANISHED from A, how many stayed.
// The relational analog of a text diff; aligns on the reachable-graph state identity, server-side.
// ==============================================================================
async function runDiff() {
  const panel = $("#diff-panel");
  if (!panel) return;
  if (!pinnedA || !pinnedA.source) {           // no pin → nothing to diff against (degrade gracefully)
    panel.hidden = true;
    return;
  }
  panel.hidden = false;
  panel.className = "diff-busy";
  panel.innerHTML = `<div class="diff-head dim">⇄ diffing pinned A vs live B…</div>`;
  try {
    const res = await fetch("/api/diff", {
      method: "POST", headers: { "content-type": "application/json" },
      body: JSON.stringify({ source_a: pinnedA.source, source_b: editor.getValue() }),
    });
    const data = res.ok ? await res.json() : { ok: false, error: `the solver returned HTTP ${res.status}` };
    renderDiff(panel, data);
  } catch (e) {
    renderDiff(panel, { ok: false, error: String(e) });
  }
}

// One state row: `var=val · var=val` over the diff's shared var set, in the var order the server
// returned (so A's and B's rows read in the same column order).
function diffStateRow(state, vars) {
  return vars.map((v) => `${v}=${state[v]}`).join(" · ");
}

function renderDiff(panel, data) {
  panel.className = "";
  if (!data.ok) {
    panel.classList.add("diff-err");
    panel.innerHTML = `<div class="diff-head">⇄ diff</div><div class="diff-msg">${(data.error || "diff failed").replace(/</g, "&lt;")}</div>`;
    return;
  }
  const { vars, appeared, vanished, common, a_total, b_total,
          appeared_edges = [], vanished_edges = [], common_edges = 0, a_edges = 0, b_edges = 0 } = data;
  const list = (rows, cls, sym, truncated) => {
    if (!rows.length) return `<div class="diff-none dim">${sym} none</div>`;
    const items = rows.map((s) => `<li>${diffStateRow(s, vars).replace(/</g, "&lt;")}</li>`).join("");
    const more = truncated ? `<li class="dim">… (capped)</li>` : "";
    return `<ul class="diff-list ${cls}">${items}${more}</ul>`;
  };
  // Transition delta — only shown when edges actually changed; catches a rewired guard whose
  // reachable STATE set is identical but whose RELATION differs (Marek #232).
  const edgeRow = (e) => `${diffStateRow(e.src, vars)} → ${diffStateRow(e.dst, vars)}`.replace(/</g, "&lt;");
  const edgeList = (rows, cls, sym, truncated) => rows.length
    ? `<ul class="diff-list ${cls}">${rows.map((e) => `<li>${edgeRow(e)}</li>`).join("")}${truncated ? '<li class="dim">… (capped)</li>' : ""}</ul>`
    : `<div class="diff-none dim">${sym} none</div>`;
  const edgeSection = (appeared_edges.length || vanished_edges.length)
    ? `<div class="diff-head diff-edges">⇄ ${appeared_edges.length} transition${appeared_edges.length === 1 ? "" : "s"} appeared · ${vanished_edges.length} vanished · = ${common_edges} unchanged`
      + `<span class="dim"> &nbsp;(A ${a_edges} → B ${b_edges} transitions)</span></div>`
      + `<div class="diff-cols"><div class="diff-col"><div class="diff-col-h appeared">▲ new transitions</div>${edgeList(appeared_edges, "appeared", "▲", data.edges_appeared_truncated)}</div>`
      + `<div class="diff-col"><div class="diff-col-h vanished">▼ removed transitions</div>${edgeList(vanished_edges, "vanished", "▼", data.edges_vanished_truncated)}</div></div>`
    : "";
  panel.innerHTML =
    `<div class="diff-head">▲ ${appeared.length} appeared · ▼ ${vanished.length} vanished · = ${common} unchanged`
    + `<span class="dim"> &nbsp;(A ${a_total} → B ${b_total} reachable states)</span></div>`
    + `<div class="diff-cols">`
    + `<div class="diff-col"><div class="diff-col-h appeared">▲ appeared in B</div>${list(appeared, "appeared", "▲", data.appeared_truncated)}</div>`
    + `<div class="diff-col"><div class="diff-col-h vanished">▼ vanished from A</div>${list(vanished, "vanished", "▼", data.vanished_truncated)}</div>`
    + `</div>`
    + edgeSection
    + functionDiffSection(data.function_diff);
}

// The COMPILED-STRUCTURE delta (#318): which per-variable functions appeared/vanished/changed when the
// source was edited — the layer beneath the reachable-state diff. A rewired guard that leaves the
// reachable states identical still shows here as a changed function.
function functionDiffSection(fd) {
  if (!fd) return "";
  const esc = (s) => String(s == null ? "" : s).replace(/</g, "&lt;");
  const row = (r) => {
    const sym = { appeared: "▲", vanished: "▼", changed: "~" }[r.status] || "=";
    const cls = r.status === "vanished" ? "vanished" : "appeared";
    const detail = r.status === "changed" ? `${esc(r.before)} → ${esc(r.after)}`
      : r.status === "appeared" ? `= ${esc(r.after)}` : `was ${esc(r.before)}`;
    return `<li class="${cls}">${sym} <b>${esc(r.var)}</b><span class="dim"> &nbsp;${detail}</span></li>`;
  };
  const couplingShift = fd.coupling_before !== fd.coupling_after
    ? `${fd.coupling_before} → ${fd.coupling_after}` : fd.coupling_after;
  return `<div class="diff-head diff-fns">⚙ compiled structure — ${fd.changed.length} function${fd.changed.length === 1 ? "" : "s"} changed`
    + `<span class="dim"> &nbsp;(${fd.pct_before}% → ${fd.pct_after}% computed · ${couplingShift})</span></div>`
    + (fd.changed.length
      ? `<ul class="diff-list">${fd.changed.map(row).join("")}</ul>`
      : `<div class="diff-none dim">= no function changed (only state values differ)</div>`);
}

// On error / claim / backend-down we must not leave a two-up or a past view over a dead/changed
// backend (degrade gracefully). Drop back to single-view mode; history itself is preserved.
function exitCompareModes() {
  pastView = null;
  if (pinnedA) setPinned(null);
  const panel = $("#diff-panel");
  if (panel) panel.hidden = true;             // a stale diff must not linger over a changed/dead backend
}

// The diagram taxonomy (web-ide-shell.md §3): every view slotted into one of four analysis-type
// FAMILIES — what question the view answers — in A→B→C→D order. The strip groups by family so the
// ~25-view set reads as a sorted gallery instead of a registry-order wall. Source of truth for the
// names is render.py's ALL_VIEWS + CLAIM_VIEWS; claim_space (CLAIM_VIEWS-only) leads family A as the
// claim analog of solution_space. Any view NOT listed here still renders — it falls into a trailing
// "other" group (never vanishes).
const VIEW_FAMILIES = [
  ["solution space", ["claim_space", "solution_space", "solution_structure"]],
  ["terminal · end-state", ["terminal_map", "reachable_region", "fixedpoint_map", "basin_map", "morse_graph"]],
  ["dynamics over time", ["state_graph", "reachability_tree", "time_series", "timing_diagram", "space_time",
    "transition_matrix", "phase_portrait", "nullcline_field", "cobweb", "orbit_scatter", "occupancy_heatmap"]],
  ["structure · law", ["scatter_matrix", "parallel_coords", "chord_diagram", "function_graph",
    "function_residual", "function_guards", "function_behavior", "function_complexity"]],
];
const VIEW_FAMILY = (() => { const m = {}; VIEW_FAMILIES.forEach(([fam, vs]) => vs.forEach(v => { m[v] = fam; })); return m; })();

// Each view's BEST-CASE rigor class — mirrors render.py's partition (_ALWAYS_PROVEN / _BOUND_VIEWS /
// _ENUMERATE_VIEWS). This is the KIND of view (a per-chip hint); the ACTIVE render's true, capping-aware
// rigor still comes from the backend as data.rigor and is shown on the figure (paint()). The chip dot
// never over-claims a specific render — it classifies the view, the figure badge classifies the result.
const _BOUND_VIEWS = new Set(["solution_space", "terminal_map", "reachable_region"]);
const _ENUMERATE_VIEWS = new Set(["state_graph", "basin_map", "fixedpoint_map", "transition_matrix",
  "timing_diagram", "time_series", "reachability_tree", "orbit_scatter"]);
function viewBaseRigor(v) {
  if (v === "claim_space" || v === "solution_structure" || v.startsWith("function_")) return "proven";
  if (_BOUND_VIEWS.has(v)) return "proven";
  if (_ENUMERATE_VIEWS.has(v)) return "exhaustive";
  return "sampled";
}
const _RIGOR_DOT = { proven: ["⊨", "best case: PROVEN — abstract Z3 over all conditions"],
  exhaustive: ["▦", "best case: EXHAUSTIVE — the complete bounded-discrete state graph"],
  sampled: ["~", "SAMPLED — trajectories / a capped or continuous run, never a proof"] };

// The currently BROWSED family (UI-only state): which family's chips row 2 shows. Distinct from the
// rendered view (browsing ≠ selecting). Re-synced to the active view's family on every (re)analysis so
// the active chip is always visible; family-tab clicks change it without re-rendering the figure.
let browsedFamily = null;

// The view tab strip — COMPACT FAMILY TABS with drill-in (web-ide-shell.md §0 figure-dominant layout):
//   row 1 = the available families (only those with ≥1 view for this model); the browsed one highlighted.
//   row 2 = only the BROWSED family's view chips, each with its best-case rigor dot.
// Clicking a family tab switches the browsed family (row 2 re-renders; figure unchanged). Clicking a chip
// renders that view (onRun) — which re-enters here and re-syncs the browsed family to it. Two short rows
// instead of the whole 25-chip wall, so the figure (6c) gets the bulk of the region.
function renderViewTabs(data, activeView, onRun) {
  const tabs = $("#tabs");
  const avail = data.views || [];
  // available families in A→D order; any unmapped views go in a trailing "other" so nothing vanishes
  const fams = VIEW_FAMILIES.map(([fam, vs]) => [fam, vs.filter(v => avail.includes(v))]).filter(([, vs]) => vs.length);
  const other = avail.filter(v => !VIEW_FAMILY[v]);
  if (other.length) fams.push(["other", other]);
  const famNames = fams.map(([fam]) => fam);
  // sync: default the browsed family to the active view's family (so its chip is on-screen); fall back to
  // the first available family if the active view is unmapped or its family has no views this model.
  const activeFam = VIEW_FAMILY[activeView] || (other.includes(activeView) ? "other" : null);
  browsedFamily = (activeFam && famNames.includes(activeFam)) ? activeFam
    : (famNames.includes(browsedFamily) ? browsedFamily : famNames[0]);

  function paint() {
    tabs.innerHTML = "";
    tabs.setAttribute("role", "tablist");
    // ROW 1 — the family tabs
    const row1 = document.createElement("div");
    row1.className = "tab-fam-row";
    fams.forEach(([fam, vs]) => {
      const ft = document.createElement("div");
      ft.className = "fam-tab" + (fam === browsedFamily ? " on" : "");
      ft.textContent = fam;
      // #427: clicking a family SELECTS + RENDERS its first view (not just browse). If that view is
      // already active, just browse (no pointless re-run). The render re-enters renderViewTabs and the
      // sync makes this family browsed with its first chip highlighted.
      ft.onclick = () => {
        const first = vs[0];
        if (first && first !== activeView) onRun(first);
        else { browsedFamily = fam; paint(); }
      };
      row1.appendChild(ft);
    });
    tabs.appendChild(row1);
    // ROW 2 — only the browsed family's chips
    const row2 = document.createElement("div");
    row2.className = "tab-chip-row";
    const chips = (fams.find(([fam]) => fam === browsedFamily) || [, []])[1];
    chips.forEach((v, i) => {
      const el = document.createElement("div");
      el.className = "tab" + (v === activeView ? " on" : "");
      el.textContent = v.replace(/_/g, " ");
      const rig = viewBaseRigor(v);
      const dot = document.createElement("span");
      dot.className = "tab-rigor rigor-" + rig;
      dot.textContent = _RIGOR_DOT[rig][0]; dot.title = _RIGOR_DOT[rig][1];
      el.appendChild(dot);
      el.setAttribute("role", "tab");
      el.setAttribute("aria-selected", v === activeView ? "true" : "false");
      el.tabIndex = v === activeView ? 0 : -1;
      if (VIEW_CAPTIONS[v]) el.dataset.gloss = VIEW_CAPTIONS[v];   // hover a tab → its caption
      el.onclick = () => onRun(v);
      el.onkeydown = (e) => {                     // roving ←/→ within the browsed family's chips
        if (e.key === "Enter" || e.key === " ") { e.preventDefault(); onRun(v); }
        else if (e.key === "ArrowRight" || e.key === "ArrowLeft") {
          e.preventDefault();
          const els = [...row2.querySelectorAll(".tab")];
          els[(i + (e.key === "ArrowRight" ? 1 : els.length - 1)) % els.length].focus();
        }
      };
      row2.appendChild(el);
    });
    tabs.appendChild(row2);
  }
  paint();
}
