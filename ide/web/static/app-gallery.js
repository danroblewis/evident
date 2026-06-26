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

// --- witness → Evident pin text / JSON (#365) -------------------------------------
// Render ONE witness value as the Evident SOURCE expression for it — what you'd paste to pin it:
//   Int / Bool        → 3        / true            (lowercase bool, Evident's literal)
//   enum (string)     → Green                      (a bare variant name)
//   Seq of primitives → ⟨1, 3, 0, 2⟩               (Evident's sequence literal)
//   Seq of records    → ⟨ (from ↦ 0, to ↦ 1), … ⟩  (named-field record literals)
//   record (object)   → (field ↦ v, …)
// Pure — no DOM. The result round-trips into a program body; the scalar subset also parses in the
// solve-for-X box (parsePins splits `name = value` on commas). String values that AREN'T bare
// identifiers (rare) are quoted so they stay valid source.
function _pinValue(v) {
  if (v === null || v === undefined) return "?";
  if (typeof v === "boolean") return v ? "true" : "false";
  if (typeof v === "number") return String(v);
  if (typeof v === "string") return /^[A-Za-z_]\w*$/.test(v) ? v : JSON.stringify(v);  // bare enum vs quoted string
  if (Array.isArray(v)) return "⟨" + v.map(_pinValue).join(", ") + "⟩";
  if (typeof v === "object") return "(" + Object.keys(v).map((k) => `${k} ↦ ${_pinValue(v[k])}`).join(", ") + ")";
  return String(v);
}
// A whole witness as pin CONSTRAINT lines — one `name = <expr>` per variable, in sorted order.
// This is the exact text to paste into the program (or, for scalars, the solve-for-X box) to pin
// the assignment and explore around it (#365). The single source of truth for the pin format.
function witnessToPins(w) {
  return Object.keys(w || {}).sort().map((k) => `${k} = ${_pinValue(w[k])}`).join("\n");
}
// A whole witness as a var→value JSON map (pretty-printed) — the data form for #365's "save json".
function witnessToJson(w) { return JSON.stringify(w || {}, null, 2); }

// --- gallery state ----------------------------------------------------------------
// One persistent collection across solves. `claim` + `source` tag the batch so a fresh
// solve of a DIFFERENT program resets it (stale witnesses must never sit beside live ones).
const _gallery = { witnesses: [], shown: 0, bookmarks: new Set(), selected: [], claim: "", source: "" };

// --- bookmark PERSISTENCE (#393) --------------------------------------------------
// Bookmarked witnesses survive a re-solve, a sample reload, AND a page refresh — Alloy-style "keep the
// good instance for the session". We persist ONLY the explicitly-bookmarked witnesses (the var→value
// maps), keyed by the MODEL (the claim/fsm name) so counter's bookmarks never bleed onto a different
// program, capped per model so the store stays bounded. The live `_gallery.bookmarks` index Set is
// rebuilt from these on every load — the persisted maps are the source of truth.
const _BM_KEY = "evident-witness-bookmarks";   // localStorage: { modelName: [witness, …] }
const _BM_CAP = 20;                            // per-model bookmark cap (bounded)
function _bmStore() {
  try { const o = JSON.parse(localStorage.getItem(_BM_KEY) || "{}"); return (o && typeof o === "object" && !Array.isArray(o)) ? o : {}; }
  catch (e) { return {}; }
}
function _bmWrite(store) { try { localStorage.setItem(_BM_KEY, JSON.stringify(store)); } catch (e) {} }
function _bmFor(model) { const s = _bmStore()[model]; return Array.isArray(s) ? s : []; }
// Add/remove witness `w` to/from `model`'s persisted bookmark list (de-dup by JSON identity, capped).
function _bmToggle(model, w) {
  if (!model) return;
  const store = _bmStore(), list = Array.isArray(store[model]) ? store[model] : [];
  const k = JSON.stringify(w), at = list.findIndex((x) => JSON.stringify(x) === k);
  if (at >= 0) list.splice(at, 1);
  else { list.push(w); if (list.length > _BM_CAP) list.shift(); }   // bounded: drop the oldest past the cap
  if (list.length) store[model] = list; else delete store[model];
  _bmWrite(store);
}

// Load an enumerated batch (or a single witness) into the gallery. A new claim/source
// REPLACES the collection; the same target APPENDS de-duplicated (so a single Solve after
// an enumeration keeps witness #1 beside the new one without dupes). #393: the model's PERSISTED
// bookmarks are merged in (so they survive a re-solve / reload / refresh) and the bookmark index Set
// is rebuilt from them — a restored bookmark sits in the gallery, marked ★, fully usable (compare/pins).
function loadGallery(witnesses, claim, source, complete) {
  const same = _gallery.claim === claim && _gallery.source === source;
  if (!same) { _gallery.witnesses = []; _gallery.bookmarks = new Set(); _gallery.selected = []; }
  _gallery.claim = claim; _gallery.source = source; _gallery.complete = complete;
  const seen = new Set(_gallery.witnesses.map((w) => JSON.stringify(w)));
  const addWitness = (w) => { const k = JSON.stringify(w); if (!seen.has(k)) { seen.add(k); _gallery.witnesses.push(w); } };
  // #393: bring this model's persisted bookmarks in FIRST (so they lead + survive a fresh solve)…
  _bmFor(claim).forEach(addWitness);
  // …then the freshly-solved witnesses (de-duped against the bookmarks + each other).
  witnesses.forEach(addWitness);
  // rebuild the live bookmark index Set from the persisted maps (the source of truth).
  const persisted = new Set(_bmFor(claim).map((w) => JSON.stringify(w)));
  _gallery.bookmarks = new Set();
  _gallery.witnesses.forEach((w, i) => { if (persisted.has(JSON.stringify(w))) _gallery.bookmarks.add(i); });
  _gallery.shown = same ? _gallery.shown : 0;
  if (_gallery.shown >= _gallery.witnesses.length) _gallery.shown = _gallery.witnesses.length - 1;
  if (_gallery.shown < 0) _gallery.shown = 0;
  renderGallery();
}

function _galleryGoto(i) {
  const n = _gallery.witnesses.length;
  _gallery.shown = i < 0 ? 0 : (i > n - 1 ? n - 1 : i);
  renderGallery();
}
// #393: toggling a bookmark flips the live index Set AND persists/unpersists the witness MAP (keyed by
// the model) so it survives a re-solve / reload / refresh.
function _toggleBookmark(i) {
  if (_gallery.bookmarks.has(i)) _gallery.bookmarks.delete(i); else _gallery.bookmarks.add(i);
  _bmToggle(_gallery.claim, _gallery.witnesses[i]);
  renderGallery();
}
// #362: A and B are EXPLICIT ordered slots the user sets DIRECTLY (never "current minus picked").
// `_gallery.selected` is the ordered pick list [A] or [A, B]. Clicking a witness's ◆ pick-target (or
// shift-clicking its chip) fills the first FREE slot — A then B — so two clicks anywhere in the strip
// set up a compare. Re-picking an already-picked witness clears ITS slot (so you can swap one side
// without losing the other). Picking a third when both are full rotates B out (A stays, newest → B).
function _toggleSelect(i) {
  const s = _gallery.selected, at = s.indexOf(i);
  if (at >= 0) { s.splice(at, 1); renderGallery(); return; }   // already picked → release that slot
  if (s.length < 2) s.push(i);                                  // fill A, then B
  else s[1] = i;                                                // both full → replace B (A is sticky)
  renderGallery();
}
// #362: clear ONE compare slot directly (the ✕ on the A/B indicator chips), leaving the other set.
function _clearSlot(slot) {
  if (slot < _gallery.selected.length) _gallery.selected.splice(slot, 1);
  renderGallery();
}

// --- gallery rendering ------------------------------------------------------------
// The thumbnail strip: one chip per witness (★ if bookmarked), the current one highlighted.
// #362: the chip BODY navigates (click flips to it); a per-chip ◆ pick-target (the direct compare
// affordance) sets this witness into the A/B compare slot WITHOUT navigating first — so two ◆ clicks
// anywhere set up a side-by-side. Shift-clicking the chip body picks it too (Ana's alternative). The
// picked side shows its A/B letter and a filled ◆.
function _galleryStrip() {
  return _gallery.witnesses.map((w, i) => {
    const cur = i === _gallery.shown, bm = _gallery.bookmarks.has(i), sel = _gallery.selected.indexOf(i);
    const cls = "g-chip" + (cur ? " cur" : "") + (sel >= 0 ? " sel" : "");
    const tag = sel === 0 ? "A" : (sel === 1 ? "B" : "");
    const pickGlyph = sel >= 0 ? "◆" : "◇";
    const pickTitle = sel >= 0 ? `picked as ${tag} — click to release` : "pick for compare (sets A, then B)";
    return `<span class="${cls}" data-pickchip="${i}" title="witness #${i + 1} — click to view · shift-click to pick for compare">`
      + `${bm ? "★" : ""}#${i + 1}${tag ? `<span class="g-ab">${tag}</span>` : ""}`
      + `<span class="g-pick" data-pick="${i}" title="${pickTitle}">${pickGlyph}</span></span>`;
  }).join("");
}

// #362: the persistent "comparing A ↔ B" indicator — shows BOTH slots at once the moment one is set
// (so the user always sees the pick state mid-selection, not only after both land). Each filled slot
// chip has a ✕ to clear just that side; an empty slot reads "—". Hidden entirely when nothing's picked.
function _compareBar() {
  if (!_gallery.selected.length) return "";
  const slot = (n, lbl) => {
    const i = _gallery.selected[n];
    if (i === undefined) return `<span class="g-slot empty">${lbl} —</span>`;
    return `<span class="g-slot set" data-slotgoto="${i}" title="view witness #${i + 1}">${lbl} #${i + 1}`
      + `<span class="g-slotx" data-clearslot="${n}" title="clear ${lbl}">✕</span></span>`;
  };
  const ready = _gallery.selected.length === 2;
  return `<div class="g-cmpbar"><span class="g-cmplabel">comparing</span>`
    + slot(0, "◆ A") + `<span class="g-cmparrow">↔</span>` + slot(1, "◆ B")
    + (ready ? "" : ` <span class="dim">— pick a second witness's ◆ to see the diff</span>`)
    + `</div>`;
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
  // #362: the per-witness pick button names the slot it will fill (Set A / Set B / picked), so the
  // button + the strip ◆ + shift-click are three routes to the SAME explicit-slot pick.
  const selAt = _gallery.selected.indexOf(i);
  const pickLbl = selAt === 0 ? "◆ picked A" : selAt === 1 ? "◆ picked B"
    : _gallery.selected.length === 0 ? "⇄ set A" : _gallery.selected.length === 1 ? "⇄ set B" : "⇄ replace B";
  body.innerHTML =
    `<div class="g-bar">`
    + `<button class="g-nav" data-goto="${i - 1}" ${i === 0 ? "disabled" : ""}>◀</button>`
    + `<span class="g-strip">${_galleryStrip()}</span>`
    + `<button class="g-nav" data-goto="${i + 1}" ${i === n - 1 ? "disabled" : ""}>▶</button>`
    + `<button class="g-act" data-bm="${i}" title="bookmark this witness">${_gallery.bookmarks.has(i) ? "★ unbookmark" : "☆ bookmark"}</button>`
    + `<button class="g-act" data-sel="${i}" title="pick this witness for side-by-side compare — fills slot A then B">${pickLbl}</button>`
    // #365: KEEP this witness — copy it as Evident pin constraints (paste back to pin + explore around it),
    // or save the var→value map as .json. Operate on the currently-shown witness #i.
    + `<button class="g-act" data-pins="${i}" title="copy this witness as Evident pin constraints — paste into your program (or the solve-for-X box, for scalars) to pin this assignment and explore around it">⧉ pins</button>`
    + `<button class="g-act" data-json="${i}" title="download this witness as a .json var→value map">↧ json</button>`
    + `</div>`
    + _compareBar()
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
    // #365: copy either side's witness as Evident pins straight from the compare header.
    + ` <button class="g-act g-mini" data-pins="${ia}" title="copy witness A (#${ia + 1}) as Evident pin constraints">⧉ pin A</button>`
    + `<button class="g-act g-mini" data-pins="${ib}" title="copy witness B (#${ib + 1}) as Evident pin constraints">⧉ pin B</button>`
    + ` <span class="g-clearcmp" data-clearcmp="1">✕ clear compare</span></div>`
    + tbl + `<div class="g-cmprow">${pane(a, "A", ia)}${pane(b, "B", ib)}</div>`;
}

// #365: copy witness #i as Evident pin constraints to the clipboard (fall back to the solve-for-X
// box where the clipboard is blocked, so the text is still reachable). setStatus lives in app.js.
async function _copyWitnessPins(i) {
  const w = _gallery.witnesses[i];
  if (!w) return;
  const text = witnessToPins(w);
  try {
    await navigator.clipboard.writeText(text);
    if (typeof setStatus === "function") setStatus(`witness #${i + 1} copied as Evident pins ✓ — paste to pin it`, "ok");
  } catch (_) {                                   // clipboard blocked → drop the SCALAR pins into solve-for-X
    const scalars = Object.keys(w).sort().filter((k) => typeof w[k] !== "object")
      .map((k) => `${k}=${_pinValue(w[k])}`).join(", ");
    const box = $("#solve-given"); if (box) box.value = scalars;
    if (typeof setStatus === "function") setStatus("clipboard blocked — scalar pins put in the solve-for-X box", "ok");
  }
}
// #365: download witness #i as a .json var→value map.
function _saveWitnessJson(i) {
  const w = _gallery.witnesses[i];
  if (!w) return;
  const a = document.createElement("a");
  a.href = URL.createObjectURL(new Blob([witnessToJson(w)], { type: "application/json" }));
  a.download = `${(_gallery.claim || "witness").replace(/[^\w.-]+/g, "_")}-witness-${i + 1}.json`;
  document.body.appendChild(a); a.click(); a.remove(); URL.revokeObjectURL(a.href);
  if (typeof setStatus === "function") setStatus(`witness #${i + 1} saved as .json ✓`, "ok");
}

// Delegate clicks for paging/bookmark/select/clear — re-attached on every render (the strip
// is rebuilt each time). One listener per control via data-* attrs keeps the markup declarative.
function _wireGallery() {
  const root = $("#solve-body");
  // chip body: plain click navigates; SHIFT-click picks into the next free A/B slot (#362).
  root.querySelectorAll("[data-pickchip]").forEach((el) =>
    el.onclick = (e) => {
      const i = parseInt(el.getAttribute("data-pickchip"), 10);
      if (e.shiftKey) { e.preventDefault(); _toggleSelect(i); } else _galleryGoto(i);
    });
  // the per-chip ◆ pick-target: pick directly into A/B without navigating (stop the chip's nav click).
  root.querySelectorAll("[data-pick]").forEach((el) =>
    el.onclick = (e) => { e.stopPropagation(); _toggleSelect(parseInt(el.getAttribute("data-pick"), 10)); });
  // the persistent A↔B indicator: a slot chip navigates to its witness; its ✕ clears just that slot (#362).
  root.querySelectorAll("[data-slotgoto]").forEach((el) =>
    el.onclick = () => _galleryGoto(parseInt(el.getAttribute("data-slotgoto"), 10)));
  root.querySelectorAll("[data-clearslot]").forEach((el) =>
    el.onclick = (e) => { e.stopPropagation(); _clearSlot(parseInt(el.getAttribute("data-clearslot"), 10)); });
  root.querySelectorAll("[data-bm]").forEach((el) =>
    el.onclick = () => _toggleBookmark(parseInt(el.getAttribute("data-bm"), 10)));
  root.querySelectorAll("[data-sel]").forEach((el) =>
    el.onclick = () => _toggleSelect(parseInt(el.getAttribute("data-sel"), 10)));
  root.querySelectorAll("[data-clearcmp]").forEach((el) =>
    el.onclick = () => { _gallery.selected = []; renderGallery(); });
  // #365: copy-as-pins / save-json — operate on the witness index in the attribute.
  root.querySelectorAll("[data-pins]").forEach((el) =>
    el.onclick = (e) => { e.stopPropagation(); _copyWitnessPins(parseInt(el.getAttribute("data-pins"), 10)); });
  root.querySelectorAll("[data-json]").forEach((el) =>
    el.onclick = (e) => { e.stopPropagation(); _saveWitnessJson(parseInt(el.getAttribute("data-json"), 10)); });
}

// node test hook (no-op in the browser, where `module` is undefined).
if (typeof module !== "undefined" && module.exports) {
  module.exports = { witnessLeaves, diffWitnesses, seqDiffIndices, compareRows, witnessToPins, witnessToJson };
}
