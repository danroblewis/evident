"use strict";

// app-structure.js — the #structure panel + the interactive diagram overlay.
//
// Two related "read the rendered result" concerns, split out of app.js (loaded before it):
//   • renderStructure(s)  — the SOLVER-computed structure of the whole model (verdict, fixed
//     points / rest states, solution-space bounds).
//   • fmtState / overlayPoints — drop transparent hover targets over the rendered scatter so
//     each point reveals its full state (hover) and can be explored from (click).
//
// These reference globals that live elsewhere by call time: $, gloss (app.js), VERDICTS
// (app-symbols.js), explorePoint (app-verify.js). They are referenced only inside function
// bodies, which run after every script has loaded — so load-order among the scripts is safe.

// The most recent diagram overlay (the live `.view-wrap` + its identifiable points), so the
// trace scrubber can locate the current step's state ON the diagram and ring it (#231/#206 —
// "the trace lights up the explorer"). Set when overlayPoints draws the live overlay; cleared
// by a render with no points so a stale ring never floats over a different picture.
// Read by app-verify.js's trace scrubber (loaded after this file).
let lastOverlay = null;

// The SOLVER-computed structure of the whole model (not a single run): a verdict, the
// rigorous fixed points / equilibria (states the solver proves map to themselves), and the
// exact boundary of the solution space (min..max each variable spans over the reachable set).
// #432: the verdict moved to the HEADER as compact chips. renderStructure keeps the data carrier
// (#structure stays populated for any legacy reader + the recompute toggles), runs the interrogate
// gates, stashes `s` for the dossier, and paints the header chips. The full evidence is in the
// dossier modal (buildDossier), opened by clicking any chip — preserving every #341/#345/#348 proof.
let lastStructure = null;
// #447: a verdict card (dossier / header chips) must never outlive the model it describes. Every analyze
// or load bumps modelRev; the open dossier is stamped with the rev it was built from and LIVE-REFRESHED
// to the current model on each renderStructure (or closed when the new model has no verdict).
let modelRev = 0;
let _dossierRev = -1;
function renderStructure(s) {
  const el = $("#structure");
  // the invariant checker only makes sense for an FSM with a reachable set — not a raw claim
  $("#invariant").hidden = !s || !!s.claim;
  $("#query-row").hidden = !s || !!s.claim;            // ad-hoc query shares the verify row's gate
  if ($("#query-row").hidden) { $("#query-stack").hidden = true; $("#query-suggest").hidden = true; }  // hide the chips too
  lastStructure = s || null;
  modelRev++;                                          // #447: a new model state — invalidate any open verdict card
  renderVerdictHeader(s);
  // #447: an OPEN dossier must track the live model, never show a previous program's verdict. Re-bind it
  // to the current structure (or close it when the new model has no FSM verdict — e.g. a claim / error).
  if (!$("#ck-modal").hidden && $("#ck-modal-title").textContent.startsWith("Verdict")) {
    if (s && !s.claim) openDossier(); else closeModal();
  }
  if (!s) { el.hidden = true; return; }
  el.hidden = true;   // #432: the data carrier is never shown; the header chips + dossier present it
}

// The compact header treatment (web-ide-shell.md §R.1): a colored headline chip (the verdict class)
// + up to two evidence chips (boundary, a relations marker) + the ⛨ soundness glyph + the ? help.
// Each chip opens the dossier. Capped so the header never wraps.
// #363: signature of the verdict chips currently painted (verdict + the boundary/relations they show).
// When unchanged across a recompute the chip nodes keep their identity (no flicker / no focus loss); only
// the live soundness badge (a child span) is left to refresh independently. A changed verdict → rebuild.
let _vchipsSig = null;
function renderVerdictHeader(s) {
  const chips = $("#verdict-chips"), helpBtn = $("#verdict-help-btn");
  if (!s) { chips.hidden = true; chips.innerHTML = ""; _vchipsSig = null; if (helpBtn) helpBtn.hidden = true; return; }
  const [icon, name] = VERDICTS[s.verdict] || ["·", s.verdict, ""];
  let html = `<span class="vchip vchip-headline v-${s.verdict}" data-doss="1" title="click for the full verdict — what's true, what's forced, witnesses to replay">${icon} ${name}<span class="snd-badge"></span></span>`;
  // evidence chip 1 — the boundary (compact: just the first var, "+N" for the rest)
  const b = s.bounds || {}, bkeys = Object.keys(b);
  if (bkeys.length) {
    const k0 = bkeys[0];
    const more = bkeys.length > 1 ? ` +${bkeys.length - 1}` : "";
    html += `<span class="vchip vchip-evidence" data-doss="1" title="the proven boundary of each variable — click for all">⊏ ${k0}∈[${b[k0][0]},${b[k0][1]}]${more}</span>`;
  }
  // evidence chip 2 — a single relations marker (implied / forced-equal), if any
  const nrel = (s.relations || []).length + (s.forced_certs || []).length;
  const neq = (s.equalities || []).length;
  if (nrel) html += `<span class="vchip vchip-evidence" data-doss="1" title="relations forced in every solution — click for the proofs">⊢ ${nrel} implied</span>`;
  else if (neq) html += `<span class="vchip vchip-evidence" data-doss="1" title="variables forced equal — click for detail">= ${s.equalities.map(([a,bb])=>`${String(a).split(".").pop()}=${String(bb).split(".").pop()}`).join(", ")}</span>`;
  // ⛨ soundness glyph — the fabrication self-check; paints the headline chip's corner badge
  html += `<button class="vchip vchip-snd" id="vchip-snd" title="double-check this verdict — re-check the abstract answer against a brute-force enumeration of this model">⛨</button>`;
  chips.hidden = false;
  if (helpBtn) helpBtn.hidden = false;
  // #363: the chips' content is determined by `html`; if it's identical to what's already on screen and the
  // nodes are still there, DON'T tear them down — keep their identity so the header doesn't flicker on every
  // recompute. The snd-badge is a child span that runSoundness refreshes on its own, untouched by this skip.
  if (html === _vchipsSig && chips.firstChild) return;
  chips.innerHTML = html;
  _vchipsSig = html;
  chips.querySelectorAll("[data-doss]").forEach((c) => { c.onclick = () => openDossier(); });
  const sndBtn = $("#vchip-snd");
  if (sndBtn) sndBtn.onclick = (e) => { e.stopPropagation(); runSoundness(); };
}

// #332: the ⛨ soundness self-check — re-checks the abstract verdict against brute-force enumeration,
// then paints the headline chip's corner badge (✓/✗) so "has this been cross-checked?" is glanceable.
async function runSoundness() {
  const badge = $("#verdict-chips .snd-badge");
  if (badge) badge.innerHTML = ' <span class="dim">…</span>';
  try {
    const body = JSON.stringify({ source: editor.getValue(), verify_soundness: true });
    const d = await (await fetch("/api/analyze", { method: "POST", headers: { "content-type": "application/json" }, body })).json();
    const snd = d.soundness, txt = _fmtSoundness(snd);
    const ok = snd && snd.verdict === "sound";
    const bad = snd && snd.verdict === "mismatch";
    if (badge) badge.innerHTML = ok ? '<span class="snd-ok">✓</span>' : bad ? '<span class="snd-bad">✗</span>' : "";
    // surface the full sentence in the dossier's soundness line if it's open
    const line = $("#doss-snd-line"); if (line) line.textContent = txt;
  } catch (e) { const line = $("#doss-snd-line"); if (line) line.textContent = "✕ " + e; }
}

// Build the verdict dossier modal body (web-ide-shell.md §R.1) from the stashed structure: three
// labeled groups — what's true / what's forced / witnesses you can replay. The relation/cert proofs
// (#341/#345/#348) are preserved verbatim, just given room. The ▶ replay buttons drive the same
// showTrace scrubber on the live view.
function buildDossier(s) {
  const [icon, name, note] = VERDICTS[s.verdict] || ["·", s.verdict, ""];
  const _sn = (n) => String(n).split(".").pop();
  // title= tooltips teach the verification concepts in place — the words appear in the panel,
  const body = $("#ck-modal-body");
  // GROUP 1 — what's true: the verdict note, fixed points, the boundary
  let g1 = "";
  if (note) g1 += `<div class="doss-row"><span class="doss-val dim">${note}</span></div>`;
  if (s.fixed_points && s.fixed_points.length) {
    const fp = s.fixed_points.slice(0, 6).map(
      (f) => "(" + Object.entries(f).map(([k, v]) => `${k}=${v}`).join(", ") + ")").join("  ");
    const more = s.fixed_points.length > 6 ? ` +${s.fixed_points.length - 6}` : "";
    const label = s.verdict === "nondeterministic" ? "rest states" : "fixed point";
    g1 += `<div class="doss-row"><span class="doss-key doss-fp">● ${label}</span><span class="doss-val">${fp}${more}</span></div>`;
  }
  const b = s.bounds || {}, keys = Object.keys(b);
  if (keys.length) {
    const bstr = keys.map((k) => `${k} ∈ [${b[k][0]}, ${b[k][1]}]`).join("\n");
    g1 += `<div class="doss-row"><span class="doss-key doss-bounds">⊏ boundary${s.capped ? " (≥, capped)" : ""}</span><span class="doss-val">${bstr}</span></div>`;
  }
  // GROUP 2 — what's forced: forced-equal / forced-different / implied (click-for-proof) / Farkas certs
  let g2 = "";
  if (s.equalities && s.equalities.length)
    g2 += `<div class="doss-row"><span class="doss-key doss-rel">= forced equal</span><span class="doss-val">${s.equalities.map(([a, bb]) => `${_sn(a)}=${_sn(bb)}`).join(", ")}</span></div>`;
  if (s.inequalities && s.inequalities.length)
    g2 += `<div class="doss-row"><span class="doss-key doss-rel">≠ forced different</span><span class="doss-val">${s.inequalities.map(([a, bb]) => `${_sn(a)}≠${_sn(bb)}`).join(", ")}</span></div>`;
  if (s.relations && s.relations.length) {
    const rels = s.relations.map((r, i) =>
      `<span class="struct-rel" data-i="${i}" title="click for the proof — which constraints force this">${r.eq}</span>`).join("  ");
    g2 += `<div class="doss-row"><span class="doss-key doss-rel">⊢ implied</span><span class="doss-val">${rels}  <span class="dim" style="font-size:11px">› click for proof</span></span></div><div id="rel-proof" class="dim" style="padding:2px 0 6px"></div>`;
  }
  if (s.forced_certs && s.forced_certs.length) {
    const fcs = s.forced_certs.map((c, i) =>
      `<span class="struct-rel" data-fc="${i}" title="click for the Farkas/Motzkin certificate — which inequalities pin this">${c.what}</span>`).join("  ");
    g2 += `<div class="doss-row"><span class="doss-key doss-rel">⊢ pinned by ≤/≥</span><span class="doss-val">${fcs}</span></div><div id="fc-proof" class="dim" style="padding:2px 0 6px"></div>`;
  }
  // GROUP 3 — witnesses you can replay (drive the trace scrubber on the live view) + the soundness audit line
  const lasso = s.rest_cycle && s.rest_cycle.length >= 2 ? s.rest_cycle : null;
  const reach = s.reach_path && s.reach_path.length >= 2 ? s.reach_path : null;
  let g3 = "";
  if (reach) g3 += `<div class="doss-row"><button id="replay-reach" class="struct-replay" title="step through the path from the initial state to where this run comes to rest">▶ replay a run that finishes</button></div>`;
  if (lasso) g3 += `<div class="doss-row"><button id="replay-lasso" class="struct-replay" title="step through a run that loops forever among non-rest states, never reaching an end state">▶ replay a run that never finishes</button></div>`;
  g3 += `<div class="doss-row"><button id="verify-snd" class="struct-replay" title="re-check the abstract verdict against a brute-force enumeration of THIS model — a fabrication self-check">⛨ double-check this verdict</button><span id="doss-snd-line" class="dim"></span></div>`;

  body.innerHTML =
    `<div class="doss-group"><div class="doss-glabel">what's true</div>${g1 || '<div class="doss-row"><span class="doss-val dim">—</span></div>'}</div>`
    + (g2 ? `<div class="doss-group"><div class="doss-glabel">what's forced</div>${g2}</div>` : "")
    + `<div class="doss-group"><div class="doss-glabel">witnesses you can replay</div>${g3}</div>`;

  // wire the witnesses — they close the modal and drive the live-view trace scrubber
  const rb = body.querySelector("#replay-lasso");
  if (rb) rb.onclick = () => { closeModal(); showTrace(lasso, "a run looping forever — never resting (#333/#334)", "violation", 0); };
  const pb = body.querySelector("#replay-reach");
  if (pb) pb.onclick = () => { closeModal(); showTrace(reach, "the path from init to where the run comes to rest (#326)", "info", -1); };
  const vb = body.querySelector("#verify-snd");
  if (vb) vb.onclick = runSoundness;
  // #341/#345/#348: click a relation → its proof (preserved verbatim from the old strip).
  body.querySelectorAll(".struct-rel").forEach((sp) => {
    sp.onclick = () => {
      if (sp.dataset.fc !== undefined) {
        const c = s.forced_certs[+sp.dataset.fc];
        $("#fc-proof").textContent = ` ⊢ ${c.what} — Farkas/Motzkin certificate (λ≥0 over the inequalities):  ${c.cert} `;
        return;
      }
      const r = s.relations[+sp.dataset.i];
      const why = r.combo
        ? `derived as  ${r.combo}`
        : (r.motzkin
          ? `Farkas/Motzkin certificate (λ≥0 over the inequalities):  ${r.motzkin}`
          : `forced by: ${(r.core || []).join("  ∧  ") || "the claim"}`);
      const out = $("#rel-proof");
      out.textContent = ` ⊢ ${r.eq} — ${why}  (Z3-proven: claim ∧ ¬(${r.eq}) is UNSAT) `;
      if (r.smtlib) {
        const cb = document.createElement("button");
        cb.className = "struct-replay"; cb.textContent = "⧉ copy proof";
        cb.title = "copy the derivation + the SMT-LIB proof obligation (paste into z3: UNSAT proves it, get-unsat-core names the forcing constraints)";
        cb.onclick = (e) => {
          e.stopPropagation();
          navigator.clipboard.writeText(`${r.eq}\n${why}\n\n; SMT-LIB proof obligation — paste into z3:\n${r.smtlib}`);
          cb.textContent = "✓ copied";
        };
        out.appendChild(cb);
      }
    };
  });
  return { icon, name };
}

// Open the verdict dossier modal from the stashed structure. Stamped with the model rev it was built
// from (#447) so a later analyze/load can detect + refresh it rather than leave a stale verdict on screen.
function openDossier() {
  if (!lastStructure) return;
  const { icon, name } = buildDossier(lastStructure);
  $("#ck-modal-title").textContent = `Verdict — ${icon} ${name}`;
  const help = $("#ck-modal-help"); if (help) { help.hidden = false; help.onclick = openHelp; }
  _dossierRev = modelRev;
  $("#ck-modal").hidden = false;
}

function closeModal() { $("#ck-modal").hidden = true; }


function _fmtSoundness(snd) {
  if (!snd) return "no result";
  if (snd.verdict === "n/a" || !snd.applicable) return "soundness: n/a — " + (snd.detail || "not exactly enumerable here");
  if (snd.verdict === "mismatch") return "⚠ SOUNDNESS MISMATCH — " + (snd.detail || "abstract ≠ brute-force; please report");
  if (snd.verdict === "sound") return "✓ cross-checked — the abstract verdict matches a brute-force enumeration of this model";
  return "inconclusive — neither check could run (Z3 undecided + box unbounded); no claim made (#335)";
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
