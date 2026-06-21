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

// Worked examples chosen to demonstrate DISTINCT model shapes and language features — not
// seven counters. The FSMs exercise different dynamics (a terminating ramp, a real cyclic
// machine, a 2-D phase spiral, a wild integer orbit, nondeterministic drift); the claims
// show algorithms expressed as constraints (solve them with ⊨ Solve).
const SAMPLES = {
  "counter · a terminating clock (FSM)":
`fsm counter
    count ∈ Int
    is_first_tick ⇒ count = 0
    ¬is_first_tick ⇒ Δcount = (_count < 5 ? 1 : 0)
    done ∈ Bool = (count ≥ 5)`,
  "accumulate · a driven pipeline (FSM)": DEFAULT_PROGRAM,
  "vending · stock, coins & a vault (FSM)":
`-- A real vending machine: coins accumulate (up to a capacity), products sell from stock
-- into the operator's vault, the customer can cancel for a refund, and the operator
-- services it. The free \`act\` each tick makes the machine nondeterministic.
enum Mode = Idle | Coining | Dispensing | Refunding | Servicing
enum Act  = InsertCoin | Purchase | Cancel | Service

fsm vending
    mode    ∈ Mode
    0 ≤ balance ∈ Int ≤ 5      -- coins in the receptacle (capacity 5)
    0 ≤ stock   ∈ Int ≤ 3      -- units of product remaining
    0 ≤ vault   ∈ Int ≤ 12     -- money the operator has collected
    act     ∈ Act              -- free customer/operator choice each tick

    is_first_tick ⇒ (mode = Idle ∧ balance = 0 ∧ stock = 3 ∧ vault = 0)

    (¬is_first_tick ∧ act = InsertCoin ∧ _balance < 5) ⇒ (mode = Coining ∧ balance = _balance + 1 ∧ stock = _stock ∧ vault = _vault)
    (¬is_first_tick ∧ act = InsertCoin ∧ _balance ≥ 5) ⇒ (mode = Coining ∧ balance = _balance ∧ stock = _stock ∧ vault = _vault)
    (¬is_first_tick ∧ act = Purchase ∧ _balance ≥ 3 ∧ _stock > 0) ⇒ (mode = Dispensing ∧ balance = _balance - 3 ∧ stock = _stock - 1 ∧ vault = _vault + 3)
    (¬is_first_tick ∧ act = Purchase ∧ (_balance < 3 ∨ _stock = 0)) ⇒ (mode = Idle ∧ balance = _balance ∧ stock = _stock ∧ vault = _vault)
    (¬is_first_tick ∧ act = Cancel)  ⇒ (mode = Refunding ∧ balance = 0 ∧ stock = _stock ∧ vault = _vault)
    (¬is_first_tick ∧ act = Service) ⇒ (mode = Servicing ∧ balance = _balance ∧ stock = 3 ∧ vault = 0)`,
  "traffic light · a cyclic state machine (FSM)":
`enum Light = Red | Green | Yellow

fsm traffic
    light ∈ Light
    timer ∈ Int
    is_first_tick ⇒ (light = Red ∧ timer = 0)
    (¬is_first_tick ∧ _timer ≥ 2) ⇒ timer = 0
    (¬is_first_tick ∧ _timer < 2) ⇒ Δtimer = 1
    (¬is_first_tick ∧ _timer < 2) ⇒ light = _light
    (¬is_first_tick ∧ _timer ≥ 2 ∧ _light = Red)    ⇒ light = Green
    (¬is_first_tick ∧ _timer ≥ 2 ∧ _light = Green)  ⇒ light = Yellow
    (¬is_first_tick ∧ _timer ≥ 2 ∧ _light = Yellow) ⇒ light = Red`,
  "oscillator · a damped spring (FSM, phase spiral)":
`-- Two interacting real variables — position and velocity. Open the phase_portrait view:
-- the trajectory spirals in (pos, vel) space. The solver finds the equilibrium at the
-- origin, and the structure line reports it as an UNSTABLE one (the orbit diverges from it).
fsm oscillator
    pos ∈ Real
    vel ∈ Real
    is_first_tick ⇒ (pos = 60.0 ∧ vel = 0.0)
    ¬is_first_tick ⇒ Δpos = _vel / 6.0
    ¬is_first_tick ⇒ Δvel = (0.0 - _pos - _vel / 2.0) / 6.0`,
  "collatz · the 3n+1 orbit (FSM)":
`-- The Collatz map: halve n if even, else 3n+1. A wild integer orbit that always falls to 1.
-- (No modulo operator yet, so even-ness is 2·(n/2) = n via integer division.)
fsm collatz
    n ∈ Int
    is_first_tick ⇒ n = 27
    ¬is_first_tick ⇒ n = (_n ≤ 1 ? 1 : (2 * (_n / 2) = _n ? _n / 2 : 3 * _n + 1))`,
  "random walk · nondeterministic drift (FSM)":
`-- Each tick the walker steps freely in x and y. The free dx/dy make it nondeterministic;
-- the occupancy_heatmap shows where it dwells, the reachability_tree shows the fan.
fsm random_walk
    x ∈ Int
    y ∈ Int
    dx ∈ Int
    dy ∈ Int
    -1 ≤ dx ≤ 1
    -1 ≤ dy ≤ 1
    is_first_tick ⇒ (x = 0 ∧ y = 0)
    ¬is_first_tick ⇒ Δx = dx
    ¬is_first_tick ⇒ Δy = dy`,
  "pick · a nondeterministic choice (FSM)":
`fsm pick
    count ∈ Int
    1 ≤ step ∈ Int ≤ 3
    is_first_tick ⇒ count = 0
    ¬is_first_tick ⇒ Δcount = step`,
  "N-queens · an algorithm as constraints (⊨ Solve)":
`-- No search algorithm: just state what a valid board IS, and the solver finds one.
-- Indented lines after a ⇒ (or a ∀ :) are a conjunction — all must hold.
claim queens
    col ∈ Seq(Int)
    #col = 4

    ∀ i ∈ {0..3} :
        0 ≤ col[i]
        col[i] ≤ 3

    ∀ i ∈ {0..3} :
        ∀ j ∈ {0..3} :
            i < j ⇒
                col[i] ≠ col[j]
                col[i] - col[j] ≠ i - j
                col[i] - col[j] ≠ j - i`,
  "graph coloring · 3-color a map (⊨ Solve)":
`-- Color six regions so no two neighbors share a color — the classic CSP, as constraints.
enum Hue = Red | Green | Blue

claim graph_coloring
    wa  ∈ Hue
    nt  ∈ Hue
    sa  ∈ Hue
    q   ∈ Hue
    nsw ∈ Hue
    v   ∈ Hue
    wa ≠ nt
    wa ≠ sa
    nt ≠ sa
    nt ≠ q
    sa ≠ q
    sa ≠ nsw
    sa ≠ v
    q  ≠ nsw
    nsw ≠ v`,
  "sum-pair · solve-for-X (⊨ Solve, pin x=3)":
`claim sum_pair
    x ∈ Int
    y ∈ Int
    0 ≤ x ≤ 10
    0 ≤ y ≤ 10
    x + y = 10`,
};

const $ = (s) => document.querySelector(s);

// --- Evident syntax-highlighting mode ---------------------------------------------
// A code editor with no language mode shows undifferentiated grey text. This tokenizes
// Evident: keywords, the Unicode/ASCII operators, comments, strings, numbers, _prev
// reads, Type/Variant capitals, and booleans — each mapped to a dracula-themed class.
CodeMirror.defineMode("evident", function () {
  const KEYWORDS = new Set([
    "claim", "type", "enum", "fsm", "schema", "import", "assert", "match",
    "subclaim", "in", "is_first_tick", "coindexed", "edges",
  ]);
  const ATOMS = new Set(["true", "false"]);
  const OPS = "∈∉∀∃⇒⟸↦→⟨⟩≤≥≠Δ¬∧∨∪∩×·⊆∅=<>+-*/?:.,";
  return {
    startState() { return {}; },
    token(stream) {
      if (stream.eatSpace()) return null;
      if (stream.match("--")) { stream.skipToEnd(); return "comment"; }
      const ch = stream.peek();
      if (ch === '"') {
        stream.next();
        while (!stream.eol()) { const c = stream.next(); if (c === '"') break; if (c === "\\") stream.next(); }
        return "string";
      }
      if (/[0-9]/.test(ch)) { stream.eatWhile(/[0-9.]/); return "number"; }
      if (/[A-Za-z_]/.test(ch)) {
        stream.eatWhile(/[A-Za-z0-9_]/);
        const w = stream.current();
        if (KEYWORDS.has(w)) return "keyword";
        if (ATOMS.has(w)) return "atom";
        if (w[0] === "_") return "variable-2";     // previous-tick read (_state)
        if (/^[A-Z]/.test(w)) return "def";        // Type name / enum Variant
        return "variable";
      }
      if (OPS.indexOf(ch) >= 0) { stream.next(); return "operator"; }
      stream.next();
      return null;
    },
  };
});

const cm = CodeMirror.fromTextArea($("#code"), {
  mode: "evident",
  theme: "dracula", lineNumbers: true, lineWrapping: true,
  viewportMargin: Infinity, value: DEFAULT_PROGRAM,
  smartIndent: false, electricChars: false, indentWithTabs: false, indentUnit: 4,
  extraKeys: {
    // Evident is indentation-sensitive. Auto-indent on Enter: copy the current line's
    // leading whitespace, and add one level after a block opener (an fsm/claim/type/enum
    // header, or a line ending in ⇒) — so a hand-typed body lands correctly indented
    // instead of at column 0 (which the parser rejects). Tab inserts 4 spaces.
    "Enter": (e) => {
      const cur = e.getCursor();
      const line = e.getLine(cur.line);
      const indent = (line.match(/^[ \t]*/) || [""])[0];
      const opener = /^\s*(fsm|claim|type|enum|schema)\b/.test(line) || /⇒\s*$/.test(line);
      e.replaceSelection("\n" + indent + (opener ? "    " : ""));
    },
    "Tab": (e) => e.replaceSelection("    "),
    "Shift-Tab": (e) => e.execCommand("indentLess"),
  },
});
// Persist the buffer across reloads — losing your work on an accidental refresh is the
// fastest way to lose a user's trust.
const SAVED = (() => { try { return localStorage.getItem("evident-buffer"); } catch (e) { return null; } })();
cm.setValue(SAVED != null ? SAVED : DEFAULT_PROGRAM);

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

// --- hover-to-learn glossary ------------------------------------------------------
// The notation is unfamiliar and several constructs are sugar whose expansion IS the
// semantics. Hover a keyword/operator/_prev/Bool to learn it — without leaving for docs.
const GLOSSARY = {
  fsm: "fsm — a state machine: a claim that carries state across ticks. _x reads the previous tick; x = … writes this tick (a difference equation).",
  claim: "claim — a predicate / relation. Also how you write tests and reusable constraint modules. Run ⊨ Solve to get a witness.",
  type: "type — a record/struct with local invariants. A noun you instantiate, e.g. type IVec2(x, y ∈ Int).",
  enum: "enum — a tagged union; variants may carry payloads and recurse, e.g. enum Result = Ok(Int) | Err(String).",
  schema: "schema — a named set defined by membership conditions (synonym for type).",
  is_first_tick: "is_first_tick — Bool, true only on the FSM's first tick. Used to seed the initial state.",
  match: "match — pattern-match an enum value across its variants.",
  subclaim: "subclaim — a named nested claim, scoped to its parent's variables.",
  "∈": "∈  membership / typing — 'x ∈ Int' declares x has type Int.   type \\in",
  "⇒": "⇒  implies — 'A ⇒ B' means: if A then B.   type \\imp",
  "⟸": "⟸  reverse-implies (dispatch) — 'A ⟸ B' means A applies when B.   type \\when",
  "∀": "∀  for-all — '∀ x ∈ s : P' means P holds for every x in s.   type \\all",
  "∃": "∃  there-exists.   type \\exists",
  "Δ": "Δ  forward difference — 'Δx' = x − _x (this tick minus last). 'Δx = 1' means x rises by 1 each tick.   type \\Delta",
  "↦": "↦  maps-to / rename, e.g. (slot ↦ value).   type \\mapsto",
  "≤": "≤  less-than-or-equal.   type \\le", "≥": "≥  greater-than-or-equal.   type \\ge",
  "≠": "≠  not-equal.   type \\ne", "¬": "¬  logical not.   type \\neg",
  "∧": "∧  logical and.   type \\and", "∨": "∨  logical or.   type \\or",
  "⟨": "⟨ ⟩  sequence literal ⟨a, b, c⟩.   type \\langle \\rangle",
  "⟩": "⟨ ⟩  sequence literal ⟨a, b, c⟩.   type \\langle \\rangle",
};
const gloss = document.createElement("div");
gloss.id = "gloss"; gloss.hidden = true; document.body.appendChild(gloss);
function glossFor(t) {
  if (GLOSSARY[t]) return GLOSSARY[t];
  if (t && t[0] === "_") return `${t} — previous-tick read: the value of ${t.slice(1)} on the prior tick.`;
  if (t === "true" || t === "false") return `${t} — Boolean literal (lowercase). Capital True/False is an unbound name — a silent bug.`;
  return null;
}
cm.getWrapperElement().addEventListener("mousemove", (e) => {
  const cls = (e.target && e.target.className) || "";
  if (typeof cls === "string" && /\bcm-(keyword|operator|variable-2|atom)\b/.test(cls)) {
    const g = glossFor((e.target.textContent || "").trim());
    if (g) {
      gloss.textContent = g; gloss.hidden = false;
      gloss.style.left = Math.min(e.clientX + 12, window.innerWidth - 380) + "px";
      gloss.style.top = (e.clientY + 18) + "px";
      return;
    }
  }
  gloss.hidden = true;
});
cm.getWrapperElement().addEventListener("mouseleave", () => { gloss.hidden = true; });

// --- inline error line marker -----------------------------------------------------
let _errLine = null;
function clearErrorLine() {
  if (_errLine != null) { cm.removeLineClass(_errLine, "background", "cm-error-line"); _errLine = null; }
}
function markErrorLine(err) {
  clearErrorLine();
  const m = (err || "").match(/line (\d+)/i);
  if (m) {
    const ln = parseInt(m[1], 10) - 1;
    if (ln >= 0 && ln < cm.lineCount()) { cm.addLineClass(ln, "background", "cm-error-line"); _errLine = ln; }
  }
}

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

// The SOLVER-computed structure of the whole model (not a single run): a verdict, the
// rigorous fixed points / equilibria (states the solver proves map to themselves), and the
// exact boundary of the solution space (min..max each variable spans over the reachable set).
const VERDICTS = {
  terminates:       ["✓", "Terminates", "the orbit converges to a fixed point"],
  cyclic:           ["↻", "Cyclic", "revisits states forever — no fixed point"],
  nondeterministic: ["⑂", "Nondeterministic", "a free choice fans the future out"],
  unstable:         ["⚠", "Unstable equilibrium", "a fixed point exists, but the orbit diverges from it"],
  unbounded:        ["→", "Unbounded", "grows without settling"],
  settles:          ["·", "Settles", ""],
};
function renderStructure(s) {
  const el = $("#structure");
  if (!s) { el.hidden = true; return; }
  el.hidden = false;
  const [icon, name, note] = VERDICTS[s.verdict] || ["·", s.verdict, ""];
  let html = `<span class="verdict v-${s.verdict}">${icon} ${name}</span>`
    + (note ? `<span class="dim">${note}</span>` : "");
  if (s.fixed_points && s.fixed_points.length) {
    const fp = s.fixed_points.slice(0, 3).map(
      (f) => "(" + Object.entries(f).map(([k, v]) => `${k}=${v}`).join(", ") + ")").join("  ");
    const more = s.fixed_points.length > 3 ? ` +${s.fixed_points.length - 3}` : "";
    const label = s.verdict === "nondeterministic" ? "rest states" : "fixed point";
    html += `<span class="struct-fp">● ${label}: ${fp}${more}</span>`;
  }
  const b = s.bounds || {}, keys = Object.keys(b);
  if (keys.length) {
    const bstr = keys.map((k) => `${k} ∈ [${b[k][0]}, ${b[k][1]}]`).join("   ");
    html += `<span class="struct-bounds">⊏ boundary${s.capped ? " (≥, capped)" : ""}: ${bstr}</span>`;
  }
  el.innerHTML = html;
}

function paint(data, ms) {
  $("#latency").textContent = ms != null ? `${ms} ms` : "";
  const view = $("#view"), warn = $("#warnings");
  if (!data.ok) {
    $("#structure").hidden = true;
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
    markErrorLine(data.error);                     // highlight the offending line in the gutter
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
  clearErrorLine();
  setStatus("ok", "ok");
  $("#banner").className = "live";
  $("#banner").textContent = "◆ " + data.banner;
  renderStructure(data.structure);
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

  // which constraint(s) vanished — the actual dropped text, not just a count, with a
  // did-you-mean for the capital-True/False footgun.
  warn.hidden = !(data.dropped && data.warnings);
  if (!warn.hidden) {
    const tf = /\bTrue\b|\bFalse\b/.test(data.warnings)
      ? "→ Booleans are lowercase in Evident: use true / false, not True / False.\n\n" : "";
    warn.textContent = tf + data.warnings;
  }
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

cm.on("change", () => {
  try { localStorage.setItem("evident-buffer", cm.getValue()); } catch (e) {}
  clearTimeout(timer); timer = setTimeout(() => run(), 350);
});

// --- solve/query: run a claim → SAT witness or UNSAT; pin vars for solve-for-X --------
function parsePins(s) {
  const given = {};
  (s || "").split(",").forEach((pair) => {
    const eq = pair.indexOf("=");
    if (eq > 0) { const k = pair.slice(0, eq).trim(); if (k) given[k] = pair.slice(eq + 1).trim(); }
  });
  return given;
}

function escapeHtml(s) { return (s || "").replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;"); }

function renderSolve(d, given) {
  const head = $("#solve-head"), body = $("#solve-body");
  const pinned = Object.keys(given || {});
  if (!d.ok) { head.innerHTML = `<span class="bad">✕ ${escapeHtml(d.error || "query failed")}</span>`; body.innerHTML = ""; return; }

  // enumeration — a list of distinct witnesses, with exhaustive/▸limit honesty
  if (d.solutions) {
    const n = d.count != null ? d.count : d.solutions.length;
    if (!n) { head.innerHTML = `<span class="unsat">⊭ UNSAT</span> — <b>${d.claim || "claim"}</b> has no solutions`; body.innerHTML = ""; return; }
    head.innerHTML = `<span class="sat">⊨ ${d.complete ? "all " + n : "≥ " + n}</span> distinct witness${n === 1 ? "" : "es"} of <b>${d.claim || "claim"}</b>`
      + (d.complete ? ` <span class="dim">(complete — the solver exhausted the space)</span>`
                    : ` <span class="dim">(showing ${n}; stopped at the limit, more may exist)</span>`);
    body.innerHTML = d.solutions.map((s, i) =>
      `<div class="sol"><span class="dim">#${i + 1}</span> `
      + Object.keys(s).sort().map((k) => `${k}=${escapeHtml(JSON.stringify(s[k]))}`).join("&nbsp;&nbsp;") + `</div>`).join("");
    return;
  }

  // UNSAT — with a delta-debugged core (which constraints conflict)
  if (d.satisfied === false) {
    head.innerHTML = `<span class="unsat">⊭ UNSAT</span> — <b>${d.claim || "claim"}</b> has no solution`
      + (pinned.length ? ` <span class="dim">with ${pinned.join(", ")} pinned</span>` : "");
    body.innerHTML = (d.core && d.core.length)
      ? `<div class="dim">conflicting core — removing any one of these makes it solvable:</div>`
        + `<table>${d.core.map((c) => `<tr><td class="k">line ${c.line}</td><td class="v">${escapeHtml(c.text)}</td></tr>`).join("")}</table>`
      : `<span class="dim">no assignment satisfies the constraints${pinned.length ? " under those pins — try different ones." : "."}</span>`;
    return;
  }

  // single SAT witness
  head.innerHTML = `<span class="sat">⊨ SAT</span> — <b>${d.claim || "claim"}</b> has a witness`
    + (pinned.length ? ` <span class="dim">(pinned: ${pinned.join(", ")})</span>` : "");
  const keys = Object.keys(d.bindings || {}).sort();
  body.innerHTML = keys.length
    ? `<table>${keys.map((k) => `<tr><td class="k">${k}${pinned.includes(k) ? " 📌" : ""}</td>`
        + `<td class="v">${escapeHtml(JSON.stringify(d.bindings[k]))}</td></tr>`).join("")}</table>`
    : '<span class="dim">satisfiable (no free variables to report)</span>';
}

async function solve(enumerate) {
  const source = cm.getValue();
  const given = parsePins($("#solve-given").value);
  $("#solve").hidden = false;
  $("#solve-head").innerHTML = `<span class="dim">${enumerate ? "enumerating…" : "solving…"}</span>`;
  $("#solve-body").innerHTML = "";
  try {
    const res = await fetch("/api/solve", {
      method: "POST", headers: { "content-type": "application/json" },
      body: JSON.stringify({ source, given, enumerate: !!enumerate, limit: 20 }),
    });
    renderSolve(await res.json(), given);
  } catch (e) {
    $("#solve-head").innerHTML = `<span class="bad">solve failed: ${e}</span>`;
  }
}

$("#solve-btn").onclick = () => solve(false);
$("#solve-resolve").onclick = () => solve(false);
$("#solve-all").onclick = () => solve(true);
$("#solve-close").onclick = () => { $("#solve").hidden = true; };
$("#solve-given").addEventListener("keydown", (e) => { if (e.key === "Enter") solve(false); });

// --- samples menu: open a worked example -----------------------------------------
const sel = $("#samples");
sel.innerHTML = '<option value="">open sample…</option>' +
  Object.keys(SAMPLES).map((k) => `<option value="${k}">${k}</option>`).join("");
sel.onchange = () => {
  if (SAMPLES[sel.value]) {
    cm.setValue(SAMPLES[sel.value]);
    $("#solve-given").value = "";   // a fresh sample must not inherit the last pin…
    $("#solve").hidden = true;       // …nor leave a stale UNSAT/witness over the new program
    run();
  }
  sel.value = "";          // reset the label so the same sample can be re-opened
};

// kick off
run();
