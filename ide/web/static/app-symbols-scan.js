"use strict";

// ==============================================================================
// app-symbols-scan.js — the buffer SYMBOL SCAN + symbol resolution: the one source of
// truth for "what does the buffer declare, where, and of what type" (#387/#388/#389).
//
// Split out of app-editor.js to keep it under the CLAUDE.md ≤500-line convention. Three
// editor features read this scan: the autocomplete completer (parseScopeIdents /
// parseBufferSymbols), hover-for-type (declHoverHtml), and go-to-definition (declAtToken
// + _gotoDecl + gotoDefinitionAtCursor). bufferDecls(text) is the unified Map<name,
// {name,kind,type,line}> they share. Pure scan functions are headless-testable; the
// resolution helpers reference `editor` / `gloss` / `setStatus` at CALL time (load-order
// safe — this loads before app-editor.js + app.js). Behaviour-preserving move.
// ==============================================================================

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

// #387/#388: the UNIFIED declaration map — one scan, two consumers (hover-for-type + go-to-def). Every
// declared symbol → { name, kind, type, line }: vars (kind "var", their type), claim/fsm names (kind
// "claim"), type/enum/schema NAMES (kind "type"/"enum", enums also carry their variant list), and enum
// VARIANTS (kind "variant", whose `enum` is its parent's name). `line` is 1-based — the decl line both
// features jump to / report. Pure (text → Map) so it's unit-testable headless. First declaration wins.
function bufferDecls(text) {
  const src = text || "";
  const lineOf = (idx) => src.slice(0, idx).split("\n").length;
  const decls = new Map();
  const put = (name, rec) => { if (name && !decls.has(name)) decls.set(name, { name, ...rec }); };
  // vars (with their type + decl line) — reuse the membership/assignment scan.
  for (const [name, d] of parseScopeDecls(src)) put(name, { kind: "var", type: d.type, line: d.line });
  // claim / fsm / subclaim names
  let m; const claimRe = /^\s*(?:claim|fsm|subclaim)\s+([A-Za-z_]\w*)/gm;
  while ((m = claimRe.exec(src)) !== null) put(m[1], { kind: "claim", type: null, line: lineOf(m.index) });
  // type / schema NAMES
  const typeRe = /^\s*(?:type|schema)\s+([A-Za-z_]\w*)/gm;
  while ((m = typeRe.exec(src)) !== null) put(m[1], { kind: "type", type: null, line: lineOf(m.index) });
  // enum NAMES (+ their variant list) AND each variant as its own decl pointing at the enum's line.
  const enumRe = /^\s*enum\s+([A-Za-z_]\w*)\s*=\s*([^\n]+)/gm;
  while ((m = enumRe.exec(src)) !== null) {
    const ln = lineOf(m.index), ename = m[1];
    const variants = m[2].split("|").map((p) => (p.match(/\s*([A-Za-z_]\w*)/) || [])[1]).filter(Boolean);
    put(ename, { kind: "enum", type: null, line: ln, variants });
    variants.forEach((v) => put(v, { kind: "variant", type: ename, line: ln }));
  }
  return decls;
}

// #389: the completer's view — names grouped by kind, derived from the unified bufferDecls map.
function parseBufferSymbols(text) {
  const claims = [], types = [], variants = [];
  for (const d of bufferDecls(text).values()) {
    if (d.kind === "claim") claims.push(d.name);
    else if (d.kind === "type" || d.kind === "enum") types.push(d.name);
    else if (d.kind === "variant") variants.push(d.name);
  }
  return { claims, types, variants };
}

// Cached decls for the hover / go-to-def handlers — the hover fires on every mousemove, so we
// re-parse only when the buffer text actually changed. #387/#388: the UNIFIED bufferDecls map (vars +
// claims + types + enums + variants), so both features resolve every declared symbol, not just vars.
let _declsCache = { src: null, decls: null };
function scopeDecls() {
  const src = editor.session.getValue();
  if (src !== _declsCache.src) _declsCache = { src, decls: bufferDecls(src) };
  return _declsCache.decls;
}

// #387/#388: resolve a raw token to its declaration, following the _/Δ prefix to the BASE var. Returns
// { decl, base, prefix } or null. `prefix` is the carried-form prefix ("_", "Δ", …) the token carried,
// so the hover can say "prev-tick value of count" and go-to-def still lands on count's decl.
function declAtToken(raw) {
  const t = (raw || "").trim();
  if (!/^[A-Za-zΔ¬_][A-Za-z0-9_]*$/.test(t)) return null;
  const base = stripIdentPrefix(t);
  const prefix = t.slice(0, t.length - base.length);   // "" | "_" | "__" | "Δ" | "ΔΔ" | "¬"
  const decl = scopeDecls().get(base);
  return decl ? { decl, base, prefix } : null;
}

// #388: the hover-for-type card for a declared symbol. Kind-aware:
//   var      → "count : Int — declared line 2"  (a _/Δ form notes the carried meaning)
//   claim    → "helper — claim, line 7"
//   type     → "IVec2 — type, line 3"
//   enum     → "Light — enum {Red, Green, Yellow}, line 1"
//   variant  → "Red — variant of Light, line 1"
function declHoverHtml(raw) {
  const r = declAtToken(raw);
  if (!r) return null;
  const { decl, base, prefix } = r;
  const ln = `<span style="opacity:.6">  ·  line ${decl.line}  ·  ⌘-click to jump</span>`;
  const carried = prefix === "_" ? ` <span style="opacity:.6">— prev-tick value of ${base}</span>`
    : prefix === "__" ? ` <span style="opacity:.6">— two-ticks-ago value of ${base}</span>`
    : prefix && prefix[0] === "Δ" ? ` <span style="opacity:.6">— per-tick change in ${base}</span>` : "";
  if (decl.kind === "var") {
    const ty = decl.type ? "  :  " + decl.type : "  <span style='opacity:.6'>(type inferred)</span>";
    return `<b>${prefix}${base}</b>${ty}${carried}${ln}`;
  }
  if (decl.kind === "enum") {
    const vs = (decl.variants || []).join(", ");
    return `<b>${base}</b>  <span style="opacity:.8">enum</span> {${vs}}${ln}`;
  }
  if (decl.kind === "variant") return `<b>${base}</b>  <span style="opacity:.8">variant of ${decl.type}</span>${ln}`;
  return `<b>${base}</b>  <span style="opacity:.8">${decl.kind}</span>${ln}`;   // claim / type
}

// #387: jump the cursor to a declaration's line + select the DECLARED NAME on it (shared by Ctrl-click /
// F12 / the palette). gotoLine centers + flashes; we then select the symbol's own occurrence on that line
// (not selectLine, which would bump the cursor onto the NEXT row) so the cursor lands ON the declaration.
function _gotoDecl(decl) {
  const Range = ace.require("ace/range").Range;
  const row = decl.line - 1;                       // 0-based
  const lineText = editor.session.getLine(row) || "";
  const at = lineText.indexOf(decl.name);
  editor.gotoLine(decl.line, at >= 0 ? at : 0, true);   // 1-based line; column on the name; centers + flashes
  if (at >= 0) editor.selection.setRange(new Range(row, at, row, at + decl.name.length));   // select the name itself
  editor.focus();
  if (typeof gloss !== "undefined") gloss.hidden = true;
}
// #387: go-to-definition from the cursor, for the ⌘K "Go to definition" command (no token under cursor → no-op msg).
function gotoDefinitionAtCursor() {
  const pos = editor.getCursorPosition();
  const tok = editor.session.getTokenAt(pos.row, pos.column + 1);
  const r = tok ? declAtToken((tok.value || "").trim()) : null;
  if (r) _gotoDecl(r.decl);
  else setStatus("put the cursor on a declared symbol, then Go to definition", "dim");
}
