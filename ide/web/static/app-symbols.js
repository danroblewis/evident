"use strict";

// ==============================================================================
// app-symbols.js — notation help: glossary / concept / per-view caption data and the
// pure lookup+format helpers over them, plus typable-token expansion for input fields
// and parser-jargon humanization. Loaded after app-data.js, before app.js.
//
// Pure data + hoisted functions only (no top-level DOM/editor side effects); the gloss
// tooltip element and the editor/banner/tab hover wiring stay in app.js, where the
// `gloss` element and `editor` live. Behaviour-preserving move out of app.js.
// ==============================================================================

// --- hover-to-learn glossary (keyword/operator/_prev/Bool → one-line explanation) ---
const GLOSSARY = {
  fsm: "fsm — a state machine: a claim that carries state across ticks. _x reads the previous tick; x = … writes this tick (a difference equation).",
  claim: "claim — a predicate / relation. Also how you write tests and reusable constraint modules. Run ⊨ Solve to get a witness.",
  type: "type — a record/struct with local invariants. A noun you instantiate, e.g. type IVec2(x, y ∈ Int).",
  enum: "enum — a tagged union; variants may carry payloads and recurse, e.g. enum Result = Ok(Int) | Err(String).",
  schema: "schema — a named set defined by membership conditions (synonym for type).",
  is_first_tick: "is_first_tick — Bool, true only on the FSM's first tick. Used to seed the initial state.",
  is_second_tick: "is_second_tick — Bool, true only on the SECOND tick. Sets the 2nd initial condition for a ΔΔ (second-order) model.",
  "ΔΔ": "ΔΔ  second difference — 'ΔΔx' = x − 2·_x + __x (needs two ticks of history, __x). Lets a 2nd-order system, e.g. an oscillator, be written in ONE variable — the runtime carries velocity as history.   type \\Delta\\Delta",
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

function glossFor(t) {
  if (GLOSSARY[t]) return GLOSSARY[t];
  if (t && t.startsWith("__")) return `${t} — two-ticks-ago read: the value of ${t.slice(2)} two ticks back (the history a ΔΔ second-order model carries).`;
  if (t && t[0] === "_") return `${t} — previous-tick read: the value of ${t.slice(1)} on the prior tick.`;
  if (t === "true" || t === "false") return `${t} — Boolean literal (lowercase). Capital True/False is an unbound name — a silent bug.`;
  return null;
}

// Flat {term, def} list over the keyword/operator glossary AND the dynamics concepts, for the
// searchable ⌘K glossary (Sam #246) — so a newcomer can look up "claim" / "Δ" / "cyclic" and read
// what it means without leaving the editor. GLOSSARY values already lead with "term — …"; CONCEPTS
// values are bare definitions, so prefix the key. `term` drives the fuzzy match, `def` is the full text.
function glossaryItems() {
  const items = [];
  for (const [k, v] of Object.entries(GLOSSARY)) items.push({ term: k, def: v });
  for (const [k, v] of Object.entries(CONCEPTS)) items.push({ term: k, def: `${k} — ${v}` });
  return items;
}

// --- banner concept glosses --------------------------------------------------------
const CONCEPTS = {
  "inductive invariant": "a bound z3 PROVED is closed under the transition: true now ⇒ true next tick ⇒ true forever. A proof, not a sample.",
  "Driven pipeline": "a deterministic recurrence: one independent variable (the clock/driver) advances on its own; the rest are computed from it.",
  "Driven": "a deterministic recurrence: one variable advances on its own clock; the others follow from it.",
  "fixed point": "a state the transition maps to itself — reach it and the machine stays. The equilibrium of the dynamics.",
  "Nondeterministic": "from some state there are ≥2 legal next states — a free choice fans the future out.",
  "Cyclic": "the machine revisits states forever in a loop — eventually periodic, with no fixed point.",
  "Unbounded": "a variable grows without limit — the reachable set never closes.",
  "Unstable": "a fixed point the dynamics move AWAY from — a tiny nudge and the orbit diverges.",
  "reachable": "the states the machine can actually enter from its start, found by SOLVING the transition — not guessed.",
  "recurrence": "each tick's value is defined from the previous tick(s): x = f(_x).",
};
const _CONCEPT_KEYS = Object.keys(CONCEPTS).sort((a, b) => b.length - a.length);  // longest first

// annotateConcepts: wrap each known concept in a hoverable span (see app.js for wiring).
function annotateConcepts(text) {
  // Collect non-overlapping concept matches on the ORIGINAL text, then wrap in ONE left-to-right
  // pass. A previous version did iterated `.replace` over the accumulating HTML, so a later key
  // ("recurrence") matched the same word INSIDE an earlier span's data-gloss attribute and nested a
  // broken span — leaking a stray `">` into the banner (Sam #185). Building once from the source
  // text makes inserted markup unreachable to the matcher.
  const esc = escapeHtml(text);
  const spans = [];                                 // {s, e, k} on `esc`, longest key wins
  for (const k of _CONCEPT_KEYS) {                  // _CONCEPT_KEYS is longest-first
    const re = new RegExp("(?<![\\w-])(" + k.replace(/[.*+?^${}()|[\]\\]/g, "\\$&") + ")(?![\\w-])", "g");
    let m;
    while ((m = re.exec(esc)) !== null) {
      const s = m.index, e = s + m[0].length;
      if (!spans.some((sp) => s < sp.e && e > sp.s)) spans.push({ s, e, k });   // skip overlaps
    }
  }
  spans.sort((a, b) => a.s - b.s);
  let out = "", pos = 0;
  for (const sp of spans) {
    out += esc.slice(pos, sp.s)
         + `<span class="concept" data-gloss="${escapeHtml(CONCEPTS[sp.k])}">${esc.slice(sp.s, sp.e)}</span>`;
    pos = sp.e;
  }
  return out + esc.slice(pos);
}

// --- per-view captions: "what am I looking at?" -----------------------------------
const VIEW_CAPTIONS = {
  solution_space:
    "shows the SOLVED boundary of the program, not one run · read it as each variable's full range (left) + the feasible region of the two principal vars (right) · tells you what states are possible at all, with fixed points marked.",
  time_series:
    "shows one trajectory (~60 ticks) from the initial state · read it as every state variable plotted against tick number on stacked tracks (numeric=line, bool/enum=step) · tells you how each value evolves over time.",
  state_graph:
    "shows the reachable state-transition graph · read it as nodes=states, arrows=transitions of state=f(_state), terminal/absorbing states ringed · tells you every state the machine can enter and how they connect.",
  phase_portrait:
    "shows the difference-equation vector field · read it as each point's displacement successor(p)−p as an arrow, colored by step magnitude, faceted per categorical value · tells you which way the dynamics flow across value-space.",
  reachability_tree:
    "shows the breadth-first reachability tree from the initial state · read it as each node at its depth = shortest-path length from the start, keeping only first-discovery edges · tells you how many steps it takes to reach each state.",
  morse_graph:
    "shows the recurrence skeleton — the condensation DAG of the reachable graph · read it as one node per strongly-connected component (cycle), classified attractor/repeller/transient · tells you where the dynamics get trapped vs pass through.",
  occupancy_heatmap:
    "shows where the system spends its time · read it as a 2-D histogram of many-seed/many-step visited points over two axes, brightness = visit density (log) · tells you the occupied region / attractor of state-space.",
  timing_diagram:
    "shows one ~40-tick run as EE-style waveforms · read it as one stacked track per variable (bool/enum=digital edges, numeric=analog line) ordered most-informative on top · tells you when each signal changes relative to the others.",
  transition_matrix:
    "shows the transition relation as an adjacency-matrix heatmap · read it as cell (i,j) lit iff state i → state j, states ordered so the top categorical forms blocks · tells you whether transitions stay within a mode (block-diagonal) or switch.",
  basin_map:
    "shows the basins of attraction · read it as a 2-axis projection of start states colored by WHICH terminal (fixed point / cycle / terminal SCC) each eventually settles into · tells you where each starting state ends up.",
  orbit_scatter:
    "shows one trajectory as discrete unconnected dots in two state axes · read it as each dot = one tick's state, gaps = the jump the equation makes; categorical on color, time gradient when none · tells you the orbit's shape (loop=cycle, pile-up=fixed point).",
  scatter_matrix:
    "shows pairwise projections of all state variables · read it as an N×N grid of scatter panels (one per variable pair), hued by the top categorical var · tells you which variables correlate or separate across the reachable set.",
  parallel_coords:
    "shows the reachable state set as polylines (Inselberg) · read it as each state a line crossing every variable's axis at its value, hued by the top categorical · tells you which value-combinations cluster per class.",
  chord_diagram:
    "shows transition flow on a circular categorical axis · read it as nodes = values of the top categorical (room→room, mode→mode), arc width/opacity = transition count, arc hue = a second categorical · tells you how much flow goes between which categories.",
  nullcline_field:
    "shows the qualitative phase-plane sign field over two numeric axes · read it as the plane shaded by the sign of each component's change, with nullclines (zero-change curves) overlaid; their crossings are fixed points · tells you which way each variable is pushed everywhere.",
  fixedpoint_map:
    "shows where the system comes to rest · read it as a 2-axis projection with fixed points as large markers, short cycles as arrowed loops, other sampled states as faint dots · tells you the attractors standing out against the basin.",
  cobweb:
    "shows a 1-D map x_n → x_{n+1} as a cobweb plot · read it as both axes the same scalar, staircasing between the map curve and the diagonal; faceted per categorical mode · tells you whether iterating the scalar converges, cycles, or diverges.",
  function_graph:
    "shows the COMPILED data-flow graph — how the solver reduced the constraints to per-variable functions · read it as an edge W→V when V's next value is computed from W's previous; a feedback cycle (pos↔vel) is coupled dynamics, a pure DAG is a driven pipeline · tells you the program's coupling structure, not its runs.",
  function_residual:
    "shows what the solver COMPILED vs what stayed a CONSTRAINT — the functions (the JIT's update law) beside the residual invariants (e.g. 0≤timer≤2) that never reduced to a function · tells you how much of your relational program is computation and where it's still truly relational.",
  function_guards:
    "shows the GUARD DECISION TREES of the piecewise functions — each guarded variable's branch conditions tried into the nested decision the solver found (is_first_tick? → _timer<2? → _light==?) · tells you the branching control-flow each variable's next value is computed by.",
  function_behavior:
    "shows the BEHAVIOUR of each extracted function — its next value sampled over the variables it reads (their previous values) · for an enum output it's the guard PARTITION (which branch wins where), for numeric the transfer surface · tells you what each compiled function actually computes, not just its shape.",
};

// --- parser-jargon humanization ----------------------------------------------------
// Rust lexer token names → the literal the user actually typed (Sam #195).
const _TOKEN_NAMES = {
  Eq: "'='", Lt: "'<'", Gt: "'>'", Le: "'≤'", Ge: "'≥'", Ne: "'≠'", Plus: "'+'", Minus: "'-'",
  Star: "'*'", Slash: "'/'", LParen: "'('", RParen: "')'", LBrace: "'{'", RBrace: "'}'",
  Colon: "':'", Comma: "','", Dot: "'.'", Implies: "'⇒'", In: "'∈'", Forall: "'∀'",
  Newline: "a line break", Indent: "indentation", Dedent: "a dedent", Eof: "end of input",
};
function humanizeError(err) {
  let hint = "";                                     // pick the hint from the RAW message first
  if (/got Ident\(/.test(err) || /expected schema\/claim\/type\/import\/enum/i.test(err)) {
    hint = "\n\n→ This usually means a body line isn't indented. Indent declarations and "
         + "constraints 4 spaces under their fsm/claim/type.";
  } else if (/\bgot Eq\b/.test(err)) {
    hint = "\n\n→ Unexpected '=' — Evident uses a single '=' for equality (there is no '=='); "
         + "check for a doubled '= =', or an '=' where an expression was expected.";
  } else if (/couldn't translate to Bool/i.test(err)) {
    hint = "\n\n→ A constraint was dropped — often a typo'd or undeclared name, or a capital "
         + "True/False (Evident uses lowercase true/false).";
  } else if (/lex error|unexpected character/i.test(err)) {
    hint = "\n\n→ An unrecognized character. For operators, type a backslash word "
         + "(e.g. \\in → ∈, \\implies → ⇒, \\Delta → Δ).";
  }
  // then humanize the token names for display
  err = err.replace(/got Ident\("([^"]*)"\)/g, "got '$1'")
           .replace(/\bgot ([A-Z]\w+)\b/g, (m, t) => _TOKEN_NAMES[t] ? "got " + _TOKEN_NAMES[t] : m);
  return err + hint;
}

// --- structure verdicts (icon, name, note) -----------------------------------------
const VERDICTS = {
  terminates:       ["✓", "Terminates", "the orbit converges to a fixed point"],
  cyclic:           ["↻", "Cyclic", "revisits states forever — no fixed point"],
  nondeterministic: ["⑂", "Nondeterministic", "a free choice fans the future out"],
  unstable:         ["⚠", "Unstable equilibrium", "a fixed point exists, but the orbit diverges from it"],
  unbounded:        ["→", "Unbounded", "grows without settling"],
  settles:          ["·", "Settles", ""],
};

// --- typable shortcuts in the ⊢ verify / solve input fields (Sam #212/#160) --------
// The same \\word / >= expansion as the editor, for plain <input>s. Wired in app.js.
function expandFieldSymbols(el) {
  const v = el.value, pos = el.selectionStart;
  const before = v.slice(0, pos);
  let start = -1, rep = "";
  const bs = before.match(/\\([a-zA-Z]+)([^a-zA-Z])$/);     // \word + a just-typed non-letter
  if (bs && UNI[bs[1]]) { start = pos - bs[0].length; rep = UNI[bs[1]] + bs[2]; }
  else if (OP_PAIRS[before.slice(-2)]) { start = pos - 2; rep = OP_PAIRS[before.slice(-2)]; }
  if (start >= 0) {
    el.value = v.slice(0, start) + rep + v.slice(pos);
    el.setSelectionRange(start + rep.length, start + rep.length);
  }
}
