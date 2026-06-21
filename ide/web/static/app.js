"use strict";

// LaTeX-style Unicode input: type \word + a non-letter, get the operator.
const UNI = {
  in: "∈", notin: "∉", forall: "∀", all: "∀", exists: "∃", any: "∃",
  implies: "⇒", imp: "⇒", then: "⇒", Rightarrow: "⇒", impliedby: "⟸", when: "⟸",
  mapsto: "↦", to: "→", langle: "⟨", rangle: "⟩", leq: "≤", le: "≤", geq: "≥",
  ge: "≥", neq: "≠", ne: "≠", Delta: "Δ", delta: "Δ", neg: "¬", not: "¬",
  land: "∧", and: "∧", lor: "∨", or: "∨",
  cup: "∪", cap: "∩", times: "×", cdot: "·", subseteq: "⊆", emptyset: "∅",
};

const DEFAULT_PROGRAM =
`fsm accumulate
    i   ∈ Int
    sum ∈ Int
    is_first_tick ⇒
        i = 0
        sum = 0
    ¬is_first_tick ⇒
        Δi   = (_i < 5 ? 1 : 0)
        Δsum = (_i < 5 ? _i : 0)`;

// A small menu of worked examples — so a newcomer can learn by opening, not just by the
// one preloaded program. Each is a distinct model SHAPE the diagram characterizes.
const SAMPLES = {
  "accumulate · driven pipeline": DEFAULT_PROGRAM,
  "counter · terminating clock":
`fsm counter
    count ∈ Int
    is_first_tick ⇒ count = 0
    ¬is_first_tick ⇒ Δcount = (_count < 5 ? 1 : 0)
    done ∈ Bool = (count ≥ 5)`,
  "runaway · the bug hunt":
`fsm runaway
    count ∈ Int
    is_first_tick ⇒ count = 0
    ¬is_first_tick ⇒ Δcount = 1`,
  "pick · nondeterministic":
`fsm pick
    count ∈ Int
    1 ≤ step ∈ Int ≤ 3
    is_first_tick ⇒ count = 0
    ¬is_first_tick ⇒ Δcount = step`,
  "vending · cyclic machine":
`enum Mode = Idle | Coining | Vending

fsm vending
    mode ∈ Mode
    is_first_tick ⇒ mode = Idle
    (¬is_first_tick ∧ _mode = Idle)    ⇒ mode = Coining
    (¬is_first_tick ∧ _mode = Coining) ⇒ mode = Vending
    (¬is_first_tick ∧ _mode = Vending) ⇒ mode = Idle`,
  "queens · solve a puzzle (⊨ Solve)":
`claim queens
    col ∈ Seq(Int)
    #col = 4
    ∀ i ∈ {0..3} : 0 ≤ col[i] ∧ col[i] ≤ 3
    ∀ i ∈ {0..3} : ∀ j ∈ {0..3} :
        i < j ⇒ (col[i] ≠ col[j] ∧ col[i] - col[j] ≠ i - j ∧ col[i] - col[j] ≠ j - i)`,
  "sum-pair · solve-for-X (⊨ Solve, pin x=3)":
`claim sum_pair
    x ∈ Int
    y ∈ Int
    0 ≤ x ≤ 10
    0 ≤ y ≤ 10
    x + y = 10`,
};

const $ = (s) => document.querySelector(s);

const cm = CodeMirror.fromTextArea($("#code"), {
  theme: "dracula", lineNumbers: true, lineWrapping: false,
  viewportMargin: Infinity, value: DEFAULT_PROGRAM,
  smartIndent: false, electricChars: false, indentWithTabs: false, indentUnit: 4,
  extraKeys: {
    // Evident is indentation-sensitive (like Python). CodeMirror's default Enter
    // (newlineAndIndent) COPIES the previous line's indent, which then stacks on top
    // of the leading whitespace already present in typed/pasted text — so every line
    // drifts further right until the parser rejects the program. Insert a bare newline;
    // the text carries its own indentation. Tab inserts 4 spaces so nesting stays easy.
    "Enter": (e) => e.replaceSelection("\n"),
    "Tab": (e) => e.replaceSelection("    "),
    "Shift-Tab": (e) => e.execCommand("indentLess"),
  },
});
cm.setValue(DEFAULT_PROGRAM);

// --- Unicode input method ---------------------------------------------------------
cm.on("inputRead", (cm_, change) => {
  const typed = change.text.join("");
  if (!typed || /[a-zA-Z\\]/.test(typed[typed.length - 1])) return; // commit on a non-letter
  const cur = cm_.getCursor();
  const before = cm_.getLine(cur.line).slice(0, cur.ch);
  const mt = before.match(/\\([a-zA-Z]+)(.)$/);
  if (mt && UNI[mt[1]]) {
    const start = { line: cur.line, ch: cur.ch - mt[0].length };
    cm_.replaceRange(UNI[mt[1]] + mt[2], start, cur);
  }
});

// --- the live loop ----------------------------------------------------------------
let timer = null, activeView = null, lastSource = "";

function setStatus(text, cls) { const s = $("#status"); s.textContent = text; s.className = cls || "dim"; }

// Translate parser jargon into something a newcomer can act on. The raw error stays
// (it's precise); we append a plain-language hint for the common footguns.
function humanizeError(err) {
  let hint = "";
  if (/got Ident\(/.test(err) || /expected schema\/claim\/type\/import\/enum/i.test(err)) {
    hint = "\n\n→ This usually means a body line isn't indented. Indent declarations and "
         + "constraints 4 spaces under their fsm/claim/type.";
  } else if (/couldn't translate to Bool/i.test(err)) {
    hint = "\n\n→ A constraint was dropped — often a typo'd or undeclared name, or a capital "
         + "True/False (Evident uses lowercase true/false).";
  } else if (/lex error|unexpected character/i.test(err)) {
    hint = "\n\n→ An unrecognized character. For operators, type a backslash word "
         + "(e.g. \\in → ∈, \\implies → ⇒, \\Delta → Δ).";
  }
  return err + hint;
}

function paint(data, ms) {
  $("#latency").textContent = ms != null ? `${ms} ms` : "";
  const view = $("#view"), warn = $("#warnings");
  if (!data.ok) {
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
      return;
    }
    setStatus("error", "err");
    $("#errors").hidden = false;
    $("#errors").textContent = humanizeError(data.error || "analysis failed");
    // the diagram on screen is from a PREVIOUS good run — mark it stale; never show
    // green reachable-state stats next to a red parse error.
    view.classList.add("stale");
    $("#banner").className = "stale";
    $("#honesty").innerHTML = data.dropped
      ? `<span class="dropped">⚠ ${data.dropped} dropped constraint(s)</span><span class="dim">diagram stale — fix the error</span>`
      : `<span class="dim">diagram stale — fix the error above</span>`;
    warn.hidden = !(data.dropped && data.warnings);
    if (!warn.hidden) warn.textContent = data.warnings;
    return;
  }
  view.classList.remove("stale");
  $("#errors").hidden = true;
  setStatus("ok", "ok");
  $("#banner").className = "live";
  $("#banner").textContent = "◆ " + data.banner;
  activeView = data.view;

  // tabs
  const tabs = $("#tabs");
  tabs.innerHTML = "";
  (data.views || []).forEach((v) => {
    const el = document.createElement("div");
    el.className = "tab" + (v === activeView ? " on" : "");
    el.textContent = v.replace(/_/g, " ");
    el.onclick = () => run(v);
    tabs.appendChild(el);
  });

  // the rendered view
  view.innerHTML = data.png
    ? `<img alt="${data.view}" src="data:image/png;base64,${data.png}">`
    : `<div class="ph">no view for this program</div>`;

  // the honesty line (branching ×N surfaces nondeterminism right next to the stats)
  const dropCls = data.dropped ? "dropped" : "clean";
  const dropTxt = data.dropped ? `⚠ ${data.dropped} dropped constraint(s)` : "✓ 0 dropped constraints";
  const branch = data.branching >= 2 ? ` · branching ×${data.branching}` : "";
  const nStates = data.capped ? `≥${data.states} (capped sample)` : `${data.states}`;
  $("#honesty").innerHTML =
    `<span class="${dropCls}">${dropTxt}</span>` +
    `<span class="dim">${nStates} reachable states · ${data.edges} transitions${branch}</span>` +
    `<span class="dim">vars: ${(data.vars || []).join(", ")}</span>`;

  // which constraint(s) vanished — the actual dropped text, not just a count
  warn.hidden = !(data.dropped && data.warnings);
  if (!warn.hidden) warn.textContent = data.warnings;
}

async function run(view) {
  const source = cm.getValue();
  lastSource = source;
  const nm = source.match(/^\s*(?:fsm|claim|type|schema)\s+([A-Za-z_]\w*)/m);
  $("#fname").textContent = (nm ? nm[1] : "untitled") + ".ev";
  setStatus("computing…", "busy");
  const t0 = performance.now();
  try {
    const res = await fetch("/api/analyze", {
      method: "POST", headers: { "content-type": "application/json" },
      // A source edit (run() with no view) sends null so the server RE-RECOMMENDS the
      // lead view for what was just written — otherwise a tab click pins the view and a
      // later edit that turns the machine nondeterministic keeps showing a flat line.
      // A tab click (run("phase_portrait")) passes its view explicitly and is honored.
      body: JSON.stringify({ source, view: view || null }),
    });
    const data = await res.json();
    paint(data, Math.round(performance.now() - t0));
  } catch (e) {
    setStatus("server error", "err");
    $("#errors").hidden = false;
    $("#errors").textContent = "could not reach the backend: " + e;
  }
}

cm.on("change", () => { clearTimeout(timer); timer = setTimeout(() => run(), 350); });

// --- solve/query: run a claim → SAT witness or UNSAT; pin vars for solve-for-X --------
function parsePins(s) {
  const given = {};
  (s || "").split(",").forEach((pair) => {
    const eq = pair.indexOf("=");
    if (eq > 0) { const k = pair.slice(0, eq).trim(); if (k) given[k] = pair.slice(eq + 1).trim(); }
  });
  return given;
}

function renderSolve(d, given) {
  const head = $("#solve-head"), body = $("#solve-body");
  const pinned = Object.keys(given || {});
  if (!d.ok) { head.innerHTML = `<span class="bad">✕ ${d.error || "query failed"}</span>`; body.innerHTML = ""; return; }
  if (d.satisfied) {
    head.innerHTML = `<span class="sat">⊨ SAT</span> — <b>${d.claim || "claim"}</b> has a witness`
      + (pinned.length ? ` <span class="dim">(pinned: ${pinned.join(", ")})</span>` : "");
    const keys = Object.keys(d.bindings || {}).sort();
    body.innerHTML = keys.length
      ? `<table>${keys.map((k) => `<tr><td class="k">${k}${pinned.includes(k) ? " 📌" : ""}</td>`
          + `<td class="v">${JSON.stringify(d.bindings[k])}</td></tr>`).join("")}</table>`
      : '<span class="dim">satisfiable (no free variables to report)</span>';
  } else {
    head.innerHTML = `<span class="unsat">⊭ UNSAT</span> — <b>${d.claim || "claim"}</b> has no solution`
      + (pinned.length ? ` <span class="dim">with ${pinned.join(", ")} pinned</span>` : "");
    body.innerHTML = `<span class="dim">no assignment satisfies the constraints${pinned.length ? " under those pins — try different ones." : "."}</span>`;
  }
}

async function solve() {
  const source = cm.getValue();
  const given = parsePins($("#solve-given").value);
  $("#solve").hidden = false;
  $("#solve-head").innerHTML = '<span class="dim">solving…</span>';
  $("#solve-body").innerHTML = "";
  try {
    const res = await fetch("/api/solve", {
      method: "POST", headers: { "content-type": "application/json" },
      body: JSON.stringify({ source, given }),
    });
    renderSolve(await res.json(), given);
  } catch (e) {
    $("#solve-head").innerHTML = `<span class="bad">solve failed: ${e}</span>`;
  }
}

$("#solve-btn").onclick = () => solve();
$("#solve-resolve").onclick = () => solve();
$("#solve-close").onclick = () => { $("#solve").hidden = true; };
$("#solve-given").addEventListener("keydown", (e) => { if (e.key === "Enter") solve(); });

// --- samples menu: open a worked example -----------------------------------------
const sel = $("#samples");
sel.innerHTML = '<option value="">open sample…</option>' +
  Object.keys(SAMPLES).map((k) => `<option value="${k}">${k}</option>`).join("");
sel.onchange = () => {
  if (SAMPLES[sel.value]) { cm.setValue(SAMPLES[sel.value]); run(); }
  sel.value = "";          // reset the label so the same sample can be re-opened
};

// kick off
run();
