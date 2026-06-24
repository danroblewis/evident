"use strict";

// ==============================================================================
// app-gallery.js — the saved-witness GALLERY + side-by-side COMPARE for the Solve
// panel (Tasks #235 + #170). Enumeration ("all ⊨*") used to REPLACE each witness with
// the next; here every enumerated/solved witness is KEPT client-side so you can:
//   • page/flip between them (an Alloy-style instance browser),
//   • bookmark the interesting ones,
//   • pick TWO and DIFF them — which variable/cell changed between two solutions,
//     rendered side-by-side with the differing leaves highlighted.
//
// Split out of app-verify.js along the seam: app-verify owns single-result rendering +
// the domain pictures (seqViz), this owns the multi-witness collection + compare. Hoisted
// functions only; escapeHtml / seqViz / $ are referenced at CALL time (load-order safe).
// The PURE half (diffWitnesses / leafDiff / compareRows) is node-testable without a DOM.
// ==============================================================================

// --- pure diff/compare core (node-testable, no DOM) -------------------------------

// Flatten a witness object into dotted leaf paths → primitive values, so two witnesses
// can be diffed leaf-by-leaf even when a var is a record-Seq (`items[2].weight`) or a
// nested record. Arrays index by `[i]`; plain objects descend by `.field`.
function witnessLeaves(w, prefix, out) {
  out = out || {};
  if (Array.isArray(w)) {
    w.forEach((v, i) => witnessLeaves(v, `${prefix}[${i}]`, out));
  } else if (w && typeof w === "object") {
    Object.keys(w).forEach((k) => witnessLeaves(w[k], prefix ? `${prefix}.${k}` : k, out));
  } else {
    out[prefix] = w;
  }
  return out;
}

// The set of leaf paths whose value DIFFERS between two witnesses (union of both leaf sets,
// so an added/removed leaf counts as a difference). Returned sorted for stable rendering.
function diffWitnesses(a, b) {
  const la = witnessLeaves(a, "", {}), lb = witnessLeaves(b, "", {});
  const keys = new Set([...Object.keys(la), ...Object.keys(lb)]);
  const diff = [];
  keys.forEach((k) => { if (JSON.stringify(la[k]) !== JSON.stringify(lb[k])) diff.push(k); });
  return diff.sort();
}

// For a single top-level var, the indices (0-based) of differing elements between two Seq
// witnesses — drives per-cell highlighting in the compare view. Non-array → empty.
function seqDiffIndices(arrA, arrB) {
  if (!Array.isArray(arrA) || !Array.isArray(arrB)) return [];
  const n = Math.max(arrA.length, arrB.length), out = [];
  for (let i = 0; i < n; i++) {
    if (JSON.stringify(arrA[i]) !== JSON.stringify(arrB[i])) out.push(i);
  }
  return out;
}

// The per-variable comparison rows for two witnesses: every top-level var (union of keys),
// with a `changed` flag. Pure — the renderer turns these into a side-by-side table.
function compareRows(a, b) {
  const keys = [...new Set([...Object.keys(a || {}), ...Object.keys(b || {})])].sort();
  return keys.map((k) => ({
    key: k,
    a: (a || {})[k],
    b: (b || {})[k],
    changed: JSON.stringify((a || {})[k]) !== JSON.stringify((b || {})[k]),
  }));
}

// --- gallery state ----------------------------------------------------------------
// One persistent collection across solves. `claim` + `source` tag the batch so a fresh
// solve of a DIFFERENT program resets it (stale witnesses must never sit beside live ones).
const _gallery = { witnesses: [], shown: 0, bookmarks: new Set(), selected: [], claim: "", source: "" };

// Load an enumerated batch (or a single witness) into the gallery. A new claim/source
// REPLACES the collection; the same target APPENDS de-duplicated (so a single Solve after
// an enumeration keeps witness #1 beside the new one without dupes).
function loadGallery(witnesses, claim, source, complete) {
  const same = _gallery.claim === claim && _gallery.source === source;
  if (!same) { _gallery.witnesses = []; _gallery.bookmarks = new Set(); _gallery.selected = []; }
  _gallery.claim = claim; _gallery.source = source; _gallery.complete = complete;
  const seen = new Set(_gallery.witnesses.map((w) => JSON.stringify(w)));
  witnesses.forEach((w) => { const k = JSON.stringify(w); if (!seen.has(k)) { seen.add(k); _gallery.witnesses.push(w); } });
  _gallery.shown = same ? _gallery.shown : 0;
  if (_gallery.shown >= _gallery.witnesses.length) _gallery.shown = _gallery.witnesses.length - 1;
  renderGallery();
}

function _galleryGoto(i) {
  const n = _gallery.witnesses.length;
  _gallery.shown = i < 0 ? 0 : (i > n - 1 ? n - 1 : i);
  renderGallery();
}
function _toggleBookmark(i) {
  if (_gallery.bookmarks.has(i)) _gallery.bookmarks.delete(i); else _gallery.bookmarks.add(i);
  renderGallery();
}
// Pick a witness into the compare slot (max two). Re-picking a selected one deselects it.
function _toggleSelect(i) {
  const s = _gallery.selected, at = s.indexOf(i);
  if (at >= 0) s.splice(at, 1);
  else { s.push(i); if (s.length > 2) s.shift(); }
  renderGallery();
}

// --- gallery rendering ------------------------------------------------------------
// The thumbnail strip: one chip per witness (★ if bookmarked, ◆ if in the compare pick),
// the current one highlighted. Click flips to it; the ★/◆ glyphs are click targets too.
function _galleryStrip() {
  return _gallery.witnesses.map((w, i) => {
    const cur = i === _gallery.shown, bm = _gallery.bookmarks.has(i), sel = _gallery.selected.indexOf(i);
    const cls = "g-chip" + (cur ? " cur" : "") + (sel >= 0 ? " sel" : "");
    const tag = sel === 0 ? "A" : (sel === 1 ? "B" : "");
    return `<span class="${cls}" data-goto="${i}" title="witness #${i + 1}">`
      + `${bm ? "★" : ""}#${i + 1}${tag ? `<span class="g-ab">${tag}</span>` : ""}</span>`;
  }).join("");
}

// One witness as its domain picture(s) + raw rows — the same split app-verify uses for a
// single SAT witness, reused here so a gallery page reads identically to Solve's witness view.
function _witnessBody(w, source, highlightKeys) {
  const keys = Object.keys(w || {}).sort();
  if (!keys.length) return '<span class="dim">witness with no free variables</span>';
  const vizByKey = {};
  keys.forEach((k) => { const v = seqViz(k, w[k], source); if (v) vizByKey[k] = v; });
  const viz = keys.map((k) => vizByKey[k]).filter(Boolean).join("");
  const rawKeys = keys.filter((k) => !vizByKey[k]);
  const hl = (k) => (highlightKeys && highlightKeys.has(k)) ? " class=\"g-changed\"" : "";
  return (viz ? `<div class="viz-wrap">${viz}</div>` : "")
    + (rawKeys.length ? `<table>${rawKeys.map((k) =>
        `<tr${hl(k)}><td class="k">${escapeHtml(k)}</td><td class="v">${escapeHtml(JSON.stringify(w[k]))}</td></tr>`).join("")}</table>` : "");
}

// Draw the gallery: head (count + paging), the thumbnail strip, the current witness body,
// and — when two are picked — the side-by-side compare/diff. All wiring is delegated below.
function renderGallery() {
  const head = $("#solve-head"), body = $("#solve-body");
  body.classList.remove("stale");
  const n = _gallery.witnesses.length;
  if (!n) { head.innerHTML = `<span class="unsat">⊭ UNSAT</span> — <b>${escapeHtml(_gallery.claim || "claim")}</b> has no solutions`; body.innerHTML = ""; return; }
  const i = _gallery.shown, more = _gallery.complete ? "" : " (≥; stopped at the limit)";
  head.innerHTML = `<span class="sat">⊨ ${n}${more}</span> witness${n === 1 ? "" : "es"} of <b>${escapeHtml(_gallery.claim || "claim")}</b>`
    + ` <span class="dim">— browsing #${i + 1} of ${n}</span>`;
  // Two picked → the side-by-side compare/diff replaces the single body; else page one witness.
  const cmp = (_gallery.selected.length === 2) ? _renderCompare() : "";
  body.innerHTML =
    `<div class="g-bar">`
    + `<button class="g-nav" data-goto="${i - 1}" ${i === 0 ? "disabled" : ""}>◀</button>`
    + `<span class="g-strip">${_galleryStrip()}</span>`
    + `<button class="g-nav" data-goto="${i + 1}" ${i === n - 1 ? "disabled" : ""}>▶</button>`
    + `<button class="g-act" data-bm="${i}" title="bookmark this witness">${_gallery.bookmarks.has(i) ? "★ unbookmark" : "☆ bookmark"}</button>`
    + `<button class="g-act" data-sel="${i}" title="pick this witness for side-by-side compare (pick two)">⇄ ${_gallery.selected.indexOf(i) >= 0 ? "picked" : "compare"}</button>`
    + `</div>`
    + (cmp || `<div class="g-one">${_witnessBody(_gallery.witnesses[i], _gallery.source, null)}</div>`);
  _wireGallery();
}

// The two-up compare: a per-variable table (changed rows flagged) ABOVE the two witnesses
// drawn as their domain pictures, with differing top-level vars highlighted in each pane.
function _renderCompare() {
  const [ia, ib] = _gallery.selected, a = _gallery.witnesses[ia], b = _gallery.witnesses[ib];
  const rows = compareRows(a, b), changed = new Set(rows.filter((r) => r.changed).map((r) => r.key));
  const nDiff = changed.size;
  // For a changed Seq var, pinpoint WHICH elements differ (cell-level locality) — e.g. two
  // subset-sum solutions that differ only at items[2] read as "differs at index 2", not a wall.
  const where = (r) => {
    if (!r.changed || !Array.isArray(r.a) || !Array.isArray(r.b)) return "";
    const ix = seqDiffIndices(r.a, r.b);
    return ix.length ? ` <span class="dim">(at ${ix.length > 6 ? ix.length + " indices" : "index " + ix.join(", ")})</span>` : "";
  };
  const tbl = `<table class="g-difftable"><tr><th>var</th><th>A · #${ia + 1}</th><th>B · #${ib + 1}</th></tr>`
    + rows.map((r) =>
        `<tr${r.changed ? ' class="g-changed"' : ""}><td class="k">${escapeHtml(r.key)}${r.changed ? " <span class=\"g-delta\">Δ</span>" + where(r) : ""}</td>`
        + `<td class="v">${escapeHtml(JSON.stringify(r.a))}</td><td class="v">${escapeHtml(JSON.stringify(r.b))}</td></tr>`).join("")
    + `</table>`;
  const pane = (w, lbl, idx) => `<div class="g-pane"><div class="compare-label">${lbl} · #${idx + 1}</div>`
    + _witnessBody(w, _gallery.source, changed) + `</div>`;
  return `<div class="g-diffhead dim">comparing #${ia + 1} vs #${ib + 1} — `
    + `${nDiff ? nDiff + " variable" + (nDiff === 1 ? "" : "s") + " differ (Δ)" : "identical witnesses"}`
    + ` <span class="g-clearcmp" data-clearcmp="1">✕ clear compare</span></div>`
    + tbl + `<div class="g-cmprow">${pane(a, "A", ia)}${pane(b, "B", ib)}</div>`;
}

// Delegate clicks for paging/bookmark/select/clear — re-attached on every render (the strip
// is rebuilt each time). One listener per control via data-* attrs keeps the markup declarative.
function _wireGallery() {
  $("#solve-body").querySelectorAll("[data-goto]").forEach((el) =>
    el.onclick = () => _galleryGoto(parseInt(el.getAttribute("data-goto"), 10)));
  $("#solve-body").querySelectorAll("[data-bm]").forEach((el) =>
    el.onclick = () => _toggleBookmark(parseInt(el.getAttribute("data-bm"), 10)));
  $("#solve-body").querySelectorAll("[data-sel]").forEach((el) =>
    el.onclick = () => _toggleSelect(parseInt(el.getAttribute("data-sel"), 10)));
  $("#solve-body").querySelectorAll("[data-clearcmp]").forEach((el) =>
    el.onclick = () => { _gallery.selected = []; renderGallery(); });
}

// node test hook (no-op in the browser, where `module` is undefined).
if (typeof module !== "undefined" && module.exports) {
  module.exports = { witnessLeaves, diffWitnesses, seqDiffIndices, compareRows };
}
