"use strict";

// ==============================================================================
// app-solve.js — solve/witness RESULT RENDERING: the SAT/UNSAT/enumeration view, the
// z3-Optimize (max/min) result, and the domain-picture renderers for Seq witnesses
// (board / grid / record table / cell strip, Task #68/#196).
//
// Split out of app-verify.js to keep both files under the CLAUDE.md ≤500-line convention —
// a single concern: turning a /api/solve | /api/optimize response into a panel. The
// /api/solve fetch ORCHESTRATION stays in app.js (solve()); this is the rendering half.
// Behaviour-preserving move.
//
// Hoisted functions only — they reference escapeHtml (app-data.js) / editor (app.js) /
// loadGallery (app-gallery.js) / $ at CALL time, so loading this before those is safe.
// parsePins is used by solve() in app.js; renderSolve/renderOptimize by the fetch handlers.
// ==============================================================================

// --- solve/query: pin parsing + result rendering ----------------------------------
function parsePins(s) {
  const given = {};
  (s || "").split(",").forEach((pair) => {
    const eq = pair.indexOf("=");
    if (eq > 0) { const k = pair.slice(0, eq).trim(); if (k) given[k] = pair.slice(eq + 1).trim(); }
  });
  return given;
}

// escapeHtml lives in app-data.js (first-loaded), shared by every concern file.

// One conflicting core as a line-number/text table.
function coreTable(core) {
  return `<table>${core.map((c) =>
    `<tr><td class="k">line ${c.line}</td><td class="v">${escapeHtml(c.text)}</td></tr>`).join("")}</table>`;
}

// The UNSAT body: every minimal core. `d.cores` (a list of cores) is preferred; we fall back to
// the single `d.core` for older responses. One core → "remove any one"; N independent cores →
// "fix one constraint from EACH", each core its own visually-separated group.
function renderCores(d, pinned) {
  const cores = (d.cores && d.cores.length) ? d.cores
              : (d.core && d.core.length ? [d.core] : []);
  if (!cores.length) {
    return `<span class="dim">no assignment satisfies the constraints${
      pinned.length ? " under those pins — try different ones." : "."}</span>`;
  }
  if (cores.length === 1) {
    return `<div class="dim">conflicting core — removing any one of these makes it solvable:</div>`
      + coreTable(cores[0]);
  }
  const more = d.cores_complete === false ? " (at least)" : "";
  return `<div class="dim">${cores.length}${more} independent contradictions — `
    + `fix one constraint from EACH to make it solvable:</div>`
    + cores.map((core, i) =>
        `<div class="core-group"><div class="dim">contradiction #${i + 1}</div>${coreTable(core)}</div>`
      ).join("");
}

// The one-line honesty banner for the symmetry fold (Ana #271). Names the interchangeable value-set
// it broke and the orbit/raw counts; when NO set is provably interchangeable, says so (no-op toggle).
function foldNote(d) {
  const sets = d.folded_sets || {};
  const names = Object.keys(sets);
  if (!names.length) {
    return `<div class="dim sym-note">no provable value symmetry to fold — every witness shown as-is `
      + `<span class="dim">(a value is foldable only when it is never named in a constraint and never ordered)</span></div>`;
  }
  const desc = names.map((e) => `{${(sets[e] || []).join(", ")}}`).join(" and ");
  const orbits = d.folded_count != null ? d.folded_count : (d.folded || []).length;
  const raw = d.raw_count != null ? d.raw_count : (d.solutions || []).length;
  return `<div class="dim sym-note">folded the interchangeable ${escapeHtml(desc)} — `
    + `${orbits} orbit${orbits === 1 ? "" : "s"} of ${raw} raw solution${raw === 1 ? "" : "s"} `
    + `<span class="dim">(each rep is one genuine witness; "(×k symmetric)" counts its relabelings)</span></div>`;
}

function renderSolve(d, given) {
  const head = $("#solve-head"), body = $("#solve-body");
  body.classList.remove("stale");                          // fresh result — undim (Sam #211)
  const pinned = Object.keys(given || {});
  if (!d.ok) { head.innerHTML = `<span class="bad">✕ ${escapeHtml(d.error || "query failed")}</span>`; body.innerHTML = ""; return; }

  // enumeration — a list of distinct witnesses. Routed into the saved-witness GALLERY
  // (app-gallery.js): each is KEPT so you can page between them, bookmark, and DIFF two
  // side-by-side (Tasks #235/#170), instead of the prior flat list that you couldn't browse.
  if (d.solutions) {
    const n = d.count != null ? d.count : d.solutions.length;
    if (!n) { head.innerHTML = `<span class="unsat">⊭ UNSAT</span> — <b>${d.claim || "claim"}</b> has no solutions`; body.innerHTML = ""; return; }
    // Route the enumeration into the saved-witness GALLERY (#170/#235 — keep #1 beside #3, compare two),
    // feeding it the SYMMETRY-FOLDED canonical reps when the backend folded value-symmetric orbits (Ana
    // #271): the fold's collapsing is preserved (fewer, canonical witnesses), the gallery owns the display.
    const folded = d.folded && Object.keys(d.folded_sets || {}).length ? d.folded : null;
    const items = folded ? folded.map((o) => o.bindings) : d.solutions;
    const src = (typeof editor !== "undefined") ? editor.getValue() : "";
    loadGallery(items, d.claim || "claim", src, !!d.complete);
    return;
  }

  // UNSAT — with the minimal conflicting cores (which constraints conflict). One core renders
  // as before ("remove any one"); multiple INDEPENDENT cores each render as their own group, so
  // a user fixing an over-constrained model sees every contradiction at once (Ana #205).
  if (d.satisfied === false) {
    head.innerHTML = `<span class="unsat">⊭ UNSAT</span> — <b>${d.claim || "claim"}</b> has no solution`
      + (pinned.length ? ` <span class="dim">with ${pinned.join(", ")} pinned</span>` : "");
    body.innerHTML = renderCores(d, pinned);
    return;
  }

  // single SAT witness
  head.innerHTML = `<span class="sat">⊨ SAT</span> — <b>${d.claim || "claim"}</b> has a witness`
    + (pinned.length ? ` <span class="dim">(pinned: ${pinned.join(", ")})</span>` : "");
  const keys = Object.keys(d.bindings || {}).sort();
  if (!keys.length) { body.innerHTML = '<span class="dim">satisfiable (no free variables to report)</span>'; return; }
  // Domain picture(s): any Seq binding draws as a board / cell strip ABOVE the raw table
  // (Task #68) — a beginner can't read positional arrays as a solution.
  const src = (typeof editor !== "undefined") ? editor.getValue() : "";
  // A var that draws as a domain picture (board / grid / record table) is shown ONLY as that
  // picture — the raw JSON row underneath read like a debug dump (Marek #204). Scalars (no
  // picture) keep their row; the picture IS the source of truth for the rest.
  const vizByKey = {};
  keys.forEach((k) => { const v = seqViz(k, d.bindings[k], src); if (v) vizByKey[k] = v; });
  const viz = keys.map((k) => vizByKey[k]).filter(Boolean).join("");
  const rawKeys = keys.filter((k) => !vizByKey[k]);
  // A single witness still gets a "keep in gallery" action — so witness #1 from a plain Solve
  // can be kept beside #3 from an enumeration and the two diffed (Task #235/#170).
  const keep = `<div class="g-bar"><button class="g-act" id="solve-keep" `
    + `title="add this witness to the gallery so you can compare it with another">＋ keep in gallery</button></div>`;
  body.innerHTML = (viz ? `<div class="viz-wrap">${viz}</div>` : "")
    + (rawKeys.length ? `<table>${rawKeys.map((k) => `<tr><td class="k">${k}${pinned.includes(k) ? " 📌" : ""}</td>`
        + `<td class="v">${escapeHtml(JSON.stringify(d.bindings[k]))}</td></tr>`).join("")}</table>` : "")
    + keep;
  const kb = $("#solve-keep");
  if (kb) kb.onclick = () => loadGallery([d.bindings], d.claim || "claim", src, false);
  // The board IS the domain answer — lead with it. When the witness draws as a board/grid, scroll the
  // solve panel into view so the filled answer isn't missed below the abstract feasibility heatmap (Sam #247).
  if (viz) requestAnimationFrame(() => { const p = $("#solve"); if (p) p.scrollIntoView({ behavior: "smooth", block: "nearest" }); });
}

// --- optimize: the QUANTITATIVE move (z3 Optimize) — maximize/minimize a numeric var ----------
// The solve surface answers feasibility (SAT/UNSAT); this answers "what's the extremal value of
// var subject to the claim, and which assignment achieves it" — the optimization query Ana drives
// daily. Reuses the same claim-name resolution as solve(); renders into the solve panel.
function _resolveClaim() {
  const source = (typeof editor !== "undefined") ? editor.getValue() : "";
  const sel = $("#claim-select");
  const cm = source.match(/^\s*claim\s+([A-Za-z_]\w*)/m);
  return (sel && !sel.hidden && sel.value) ? sel.value : (cm ? cm[1] : null);
}

async function runOptimize(direction) {
  const source = (typeof editor !== "undefined") ? editor.getValue() : "";
  const v = $("#opt-var").value.trim();
  if (!v) { $("#opt-var").focus(); return; }
  $("#solve").hidden = false;
  $("#solve-head").innerHTML = `<span class="dim">${direction === "min" ? "minimizing" : "maximizing"} ${escapeHtml(v)}…</span>`;
  $("#solve-body").innerHTML = "";
  try {
    const res = await fetch("/api/optimize", {
      method: "POST", headers: { "content-type": "application/json" },
      body: JSON.stringify({ source, claim: _resolveClaim(), var: v, direction }),
    });
    renderOptimize(await res.json());
  } catch (e) {
    $("#solve-head").innerHTML = `<span class="bad">optimize failed: ${escapeHtml(String(e))}</span>`;
  }
}

function renderOptimize(d) {
  const head = $("#solve-head"), body = $("#solve-body");
  body.classList.remove("stale");
  if (!d.ok) { head.innerHTML = `<span class="bad">✕ ${escapeHtml(d.error || "optimize failed")}</span>`; body.innerHTML = ""; return; }
  const arrow = d.direction === "min" ? "⤓ min" : "⤒ max";
  if (d.satisfied === false) {
    head.innerHTML = `<span class="unsat">∄ extremum</span> — <b>${escapeHtml(d.var)}</b> is unbounded (or the claim is UNSAT)`;
    body.innerHTML = `<span class="dim">no finite ${arrow} of ${escapeHtml(d.var)} over <b>${escapeHtml(d.claim || "claim")}</b></span>`;
    return;
  }
  head.innerHTML = `<span class="sat">${arrow}</span> <b>${escapeHtml(d.var)}</b> = `
    + `<span class="extremal">${escapeHtml(String(d.extremal))}</span> over <b>${escapeHtml(d.claim || "claim")}</b>`;
  const b = d.bindings || {};
  const keys = Object.keys(b).sort();
  body.innerHTML = `<div class="opt-result"><span class="dim">optimizing assignment</span>`
    + `<table>${keys.map((k) => `<tr><td class="k">${escapeHtml(k)}${k === d.var ? " ★" : ""}</td>`
        + `<td class="v">${escapeHtml(JSON.stringify(b[k]))}</td></tr>`).join("")}</table></div>`;
}

// --- domain-picture rendering for Seq witnesses (Task #68 / #196) -----------------
// A Seq witness is hard to read as a flat array. Draw it as the domain shape it is:
//   • record-Seq (array of objects)            → a small TABLE, one row per element
//   • sudoku-shaped Int-Seq (length K², 1..K)  → a K×K filled grid
//   • N-queens column-Seq (permutation + name) → a chessboard with pieces
//   • anything else                            → the honest index→value cell strip
// Shapes are detected from the witness itself plus the source (`#name = N`), generically —
// no sample names are hardcoded.
function seqViz(name, val, source) {
  if (!Array.isArray(val) || !val.length) return null;
  const n = val.length;

  // record-Seq: every element is a plain object (a record). Render columns = field names.
  if (val.every((v) => v && typeof v === "object" && !Array.isArray(v))) {
    return recordTable(name, val);
  }

  // only primitive-Int seqs get a numeric picture; mixed/non-int seqs fall through.
  if (!val.every((v) => typeof v === "number" && Number.isInteger(v))) return null;

  // A queens board needs TWO honest signals, not just "values in 0..N-1" — that also matches a
  // topological order (pos=[0,1,2,3,4]) and a sudoku row, which would draw a wrong chessboard
  // (Marek #68/#92). Require: a queens-like variable NAME *and* a true permutation of 0..N-1
  // (one queen per row AND column).
  const queensName = /^(col|cols|queen|queens|row|rows|board)$/.test(name.toLowerCase());
  const isPermutation = n >= 4 && new Set(val).size === n && val.every((v) => v >= 0 && v < n);
  if (queensName && isPermutation) return queensBoard(name, val);

  // sudoku-shaped: a flat Int-Seq whose length is a perfect square K² (4, 9, 16, 25),
  // with every value a single symbol in 1..K (or 0..K-1). Reshape it into the K×K grid the
  // values already imply — Sam shouldn't reshape 16 index=value lines in his head.
  const k = Math.round(Math.sqrt(n));
  if (k >= 2 && k * k === n) {
    const min = Math.min(...val), max = Math.max(...val);
    const oneBased = min >= 1 && max <= k;        // 1..K (the canonical sudoku numbering)
    const zeroBased = min >= 0 && max <= k - 1;   // 0..K-1
    if (oneBased || zeroBased) return sudokuGrid(name, val, k);
  }

  return cellStrip(name, val);
}

// One row per element, one column per record field. Replaces a raw-JSON array of objects with a
// scannable table (subset-sum's {weight, take} items, toposort's {from, to} edges, sudoku boxes).
function recordTable(name, rows) {
  // union of field names across rows, in first-seen order (rows are homogeneous in practice).
  const cols = [];
  rows.forEach((r) => Object.keys(r).forEach((c) => { if (!cols.includes(c)) cols.push(c); }));
  const fmt = (v) =>
    typeof v === "boolean" ? (v ? "✓" : "·")
      : (v && typeof v === "object") ? escapeHtml(JSON.stringify(v))
      : escapeHtml(String(v));
  const head = `<tr><th>#</th>${cols.map((c) => `<th>${escapeHtml(c)}</th>`).join("")}</tr>`;
  const trs = rows.map((r, i) =>
    `<tr><td class="rt-i">${i}</td>`
    + cols.map((c) => `<td>${c in r ? fmt(r[c]) : ""}</td>`).join("") + `</tr>`).join("");
  return `<div class="viz"><div class="viz-label">${escapeHtml(name)} `
    + `<span class="dim">(${rows.length} × {${cols.map(escapeHtml).join(", ")}})</span></div>`
    + `<table class="rec-table">${head}${trs}</table></div>`;
}

// A flat Int-Seq reshaped into the K×K grid its values imply (sudoku / latin-square style).
function sudokuGrid(name, vals, k) {
  let cells = "";
  for (let r = 0; r < k; r++) {
    for (let c = 0; c < k; c++) {
      // subgrid shading when K is itself a perfect square (4→2×2 boxes, 9→3×3) — purely visual.
      const sub = Math.round(Math.sqrt(k));
      const boxed = sub * sub === k && (Math.floor(r / sub) + Math.floor(c / sub)) % 2 === 1;
      cells += `<div class="scell${boxed ? " box" : ""}">${escapeHtml(String(vals[r * k + c]))}</div>`;
    }
  }
  return `<div class="viz"><div class="viz-label">${escapeHtml(name)} — ${k}×${k} grid`
    + ` <span class="dim">(${escapeHtml(name)}[r·${k}+c] → cell at row r, col c)</span></div>`
    + `<div class="sgrid" style="grid-template-columns:repeat(${k},1fr)">${cells}</div></div>`;
}

// `#name = N` in the source → N (the pinned Seq length), else null.
function pinnedLen(source, name) {
  const m = (source || "").match(new RegExp("#\\s*" + name.replace(/[.*+?^${}()|[\\]\\\\]/g, "\\$&") + "\\s*=\\s*(\\d+)"));
  return m ? parseInt(m[1], 10) : null;
}

function queensBoard(name, cols) {
  const n = cols.length;
  let cells = "";
  for (let r = 0; r < n; r++) {
    for (let c = 0; c < n; c++) {
      const dark = (r + c) % 2 === 1;
      const q = cols[r] === c;
      cells += `<div class="qsq${dark ? " dark" : ""}${q ? " q" : ""}">${q ? "♛" : ""}</div>`;
    }
  }
  return `<div class="viz"><div class="viz-label">${escapeHtml(name)} — ${n}×${n} board`
    + ` <span class="dim">(row i → queen at column ${escapeHtml(name)}[i])</span></div>`
    + `<div class="qboard" style="grid-template-columns:repeat(${n},1fr)">${cells}</div></div>`;
}

function cellStrip(name, arr) {
  const cells = arr.map((v, i) =>
    `<div class="cell"><div class="cell-idx">${i}</div><div class="cell-val">${escapeHtml(String(v))}</div></div>`).join("");
  return `<div class="viz"><div class="viz-label">${escapeHtml(name)} `
    + `<span class="dim">(index → value)</span></div><div class="strip">${cells}</div></div>`;
}
