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
  match: "match — pattern-match an enum value across its variants: indented 'Ctor(b) ⇒ body' arms, lowered to nested if-then-else.",
  matches: "matches — variant recognizer: 'e matches Ctor(_)' is a Bool, true when e is that variant (payload ignored). Use match to extract a payload.",
  subclaim: "subclaim — a named nested claim, scoped to its parent's variables.",
  "∈": "∈  membership / typing — 'x ∈ Int' declares x has type Int.   type \\in",
  "⇒": "⇒  implies — 'A ⇒ B' means: if A then B.   type \\imp",
  "⟸": "⟸  reverse-implies (dispatch) — 'A ⟸ B' means A applies when B.   type \\when",
  "∀": "∀  for-all — '∀ x ∈ s : P' means P holds for every x in s.   type \\all",
  "∃": "∃  there-exists.   type \\exists",
  "Δ": "Δ  forward difference — 'Δx' = x − _x (this tick minus last). 'Δx = 1' means x rises by 1 each tick.   type \\Delta",
  ":=": ":=  seed — 'x ∈ Int := 0' sets x's value on the FIRST tick (there's no previous tick to read). Sugar for 'is_first_tick ⇒ x = 0'. Use it to give a carried variable its starting value.",
  "++": "++  sequence concatenation — 'a ++ b ++ ⟨c⟩' flattens named chunks into one Seq at load time. (A single '+' is addition.)",
  "⤳": "⤳  leads-to (liveness) — 'P ⤳ Q' means: every state where P holds is eventually followed by Q. Checked by ⊢ verify.",
  "↦": "↦  maps-to / rename, e.g. (slot ↦ value).   type \\mapsto",
  "≤": "≤  less-than-or-equal.   type \\le", "≥": "≥  greater-than-or-equal.   type \\ge",
  "≠": "≠  not-equal.   type \\ne", "¬": "¬  logical not.   type \\neg",
  "∧": "∧  logical and.   type \\and", "∨": "∨  logical or.   type \\or",
  "+": "+  addition. '++' (two pluses) concatenates sequences: 'a ++ b ++ ⟨c⟩' flattens named chunks into one Seq at load time.",
  ".": ".  field access (win.renderer). Two dots '..TypeName' is passthrough / trait-mixin — it brings another type's fields and constraints into this scope WITHOUT a dotted prefix.",
  "⟨": "⟨ ⟩  sequence literal ⟨a, b, c⟩.   type \\langle \\rangle",
  "⟩": "⟨ ⟩  sequence literal ⟨a, b, c⟩.   type \\langle \\rangle",
  // built-in TYPE names — the first thing a newcomer hovers after 'x ∈ ' (Sam #266)
  Int: "Int — the integers (…, -1, 0, 1, …). 'x ∈ Int' declares x an integer; '0 ≤ x ∈ Int ≤ 9' declares and bounds it in one line.",
  Real: "Real — the real numbers (the solver keeps them exact). Use for a continuous quantity — a position, a rate.",
  Nat: "Nat — the natural numbers 0, 1, 2, … (non-negative integers).",
  Bool: "Bool — a truth value: true or false (lowercase). Comparisons (<, =) and logic (∧, ∨, ¬) produce Bool.",
  String: "String — text. Join with ++; build from an Int with to_str.",
  Seq: "Seq(T) — an ordered sequence of T. Build with ⟨a, b, c⟩, join with ++, iterate ∀ x ∈ s; #s is its length.",
  Set: "Set T — an unordered collection of T (no order, no duplicates). Membership is x ∈ s.",
  coindexed: "coindexed(a, b, …) — zip parallel sequences by position: ∀ (x, y) ∈ coindexed(a, b) pairs a[i] with b[i].",
  edges: "edges(s) — iterate a sequence's adjacent pairs: ∀ (a, b) ∈ edges(s) gives (s[i], s[i+1]) — e.g. for monotonicity.",
  Distinct: "Distinct(s, n) — a stdlib claim: the first n elements of s are pairwise different (e.g. N-queens columns).",
  assert: "assert — pin a ground fact (a value or a complete lookup table), e.g. 'assert exits = { … }'.",
};

function glossFor(t) {
  if (GLOSSARY[t]) return GLOSSARY[t];
  if (t && t.startsWith("__")) return `${t} — two-ticks-ago read: the value of ${t.slice(2)} two ticks back (the history a ΔΔ second-order model carries).`;
  if (t && t[0] === "_") return `${t} — previous-tick read: the value of ${t.slice(1)} on the prior tick.`;
  if (t === "true" || t === "false") return `${t} — Boolean literal (lowercase). Capital True/False is an unbound name — a silent bug.`;
  return null;
}

// #366: multi-CHARACTER operators (`:=`, `++`, `□◇`) tokenize as separate single-char tokens in the Ace
// mode (the operator rule matches one glyph at a time), so a single-token hover never sees them. Given the
// hovered LINE + column, return the gloss for a multi-char operator that SPANS the cursor — checked before
// the single-token path so `:=` teaches "seed", not `:`/`=` separately. Longest operators first.
const _MULTI_OPS = ["□◇", ":=", "++", "<=", ">=", "!=", "=>", "⤳"];
function glossAtCursor(line, col) {
  for (const op of _MULTI_OPS) {
    // does `op` occupy [start, start+len) covering `col`? scan the few placements around the cursor.
    for (let s = Math.max(0, col - op.length); s <= col; s++) {
      if (line.slice(s, s + op.length) === op && col >= s && col <= s + op.length) {
        const g = GLOSSARY[op]; if (g) return g;
      }
    }
  }
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

// The honesty line's reachable-scope certification. "✓ complete" is only honest for a DISCRETE machine
// whose BFS reached a fixpoint WITH states found — a 0 count (claim / continuous / gave-up) or a
// real-valued domain must never read as proven-complete (Marek #274).
function scopeCertHtml(data) {
  if (!data.states)
    return `<span class="scope-cap" title="No reachable-state enumeration: a claim (a relation, not a machine), a continuous/non-enumerable domain, or the solver gave up. NOT a proof that the machine reaches nothing.">no enumerable reachable set</span>`;
  if (data.capped)
    return `<span class="scope-cap" title="The reachability search stopped at the scope bound (${data.scope}) — the true reachable set may be LARGER. A bounded SAMPLE, not a complete enumeration; raise the scope knob to explore further (or push toward ✓ complete). Treat &quot;not found&quot; as inconclusive.">≥${data.states} reachable (capped at scope ${data.scope} — raise it)</span>`;
  if (data.continuous)
    return `<span class="scope-cap" title="Real-valued (continuous) domain: the reachable set is SAMPLED along trajectories, not exhaustively enumerated — the true set is uncountable. &quot;not found&quot; is inconclusive.">${data.states} reachable (sampled — continuous)</span>`;
  return `<span class="scope-exh" title="The reachability search reached a FIXPOINT: every state the machine can enter from its start has been found. The COMPLETE reachable set — a proof, not a sample, so &quot;no state satisfies P&quot; is conclusive.">${data.states} reachable (✓ complete)</span>`;
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
  // functionizer-view vocabulary — so the function_* banners get glossed like the dynamics ones (Sam #271)
  "functionized": "reduced by the solver to a COMPUTATION — a per-variable function (the JIT's update law), not a residual constraint.",
  "residual": "a constraint the solver did NOT reduce to a function — the genuinely-relational part it keeps as a standing invariant (e.g. a type bound 0 ≤ timer ≤ 2).",
  "autonomous": "a closed self-map x' = f(_x): the dynamics depend only on their own past, with no external driver.",
  "self-map": "a closed recurrence x' = f(_x) with no external driver — see autonomous.",
  "piecewise": "a function defined by cases — guarded branches, each a different body under a different condition.",
  "coupled": "variables feed back into each other (a cycle in the data-flow graph), e.g. an oscillator's position ↔ velocity.",
  "driver": "an independent variable that advances on its own and feeds the computed ones — the clock of a driven pipeline.",
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
    "shows the ENSEMBLE of trajectories over all initial conditions · read it as every state variable on stacked tracks (numeric=line, bool/enum=step), the shaded band = the reachable value envelope at each tick (real/unbounded models fall back to one seeded run) · tells you how each value CAN evolve from any start, not just one run.",
  value_heatmap:
    "shows every carried variable's value over time as a dense raster (the transpose of time_series) · read it as one ROW per carried variable, one COLUMN per tick, cell colour = that variable's value on its own per-row scale (binary=black/white, enum=palette, numeric=viridis); life's 16 grid cells c00..c33 each become a row, so the cellular automaton's evolution falls straight out · tells you the whole trajectory of state at a glance — which variables drift, latch, oscillate, or hold.",
  state_graph:
    "shows the reachable state-transition graph · read it as nodes=states, arrows=transitions of state=f(_state), terminal/absorbing states ringed · tells you every state the machine can enter and how they connect.",
  phase_portrait:
    "shows the difference-equation vector field · read it as each point's displacement successor(p)−p as an arrow, colored by step magnitude, faceted per categorical value · tells you which way the dynamics flow across value-space.",
  reachability_tree:
    "shows the reachability forest from ALL initial conditions · read it as a synthetic ∅ root over every start state, each node at its BFS depth = shortest distance from the nearest init; finite discrete systems CLOSE at the true saturation depth (not a fixed cap) · tells you how many steps reach each state, over all starts.",
  morse_graph:
    "shows the recurrence skeleton — the condensation DAG of the reachable graph · read it as one node per strongly-connected component (cycle), classified attractor/repeller/transient · tells you where the dynamics get trapped vs pass through.",
  occupancy_heatmap:
    "shows where the system spends its time · read it as a 2-D histogram of many-seed/many-step visited points over two axes, brightness = visit density (log) · tells you the occupied region / attractor of state-space.",
  timing_diagram:
    "shows an ENSEMBLE over all initial conditions as EE-style waveforms · read it as one stacked track per variable (bool/enum=digital edges, numeric=analog line), the shaded band = the reachable value envelope at each tick (real/unbounded models fall back to one seeded run) · tells you what every signal can do from any start, not just one trajectory.",
  transition_matrix:
    "shows the transition relation as an adjacency-matrix heatmap · read it as cell (i,j) lit iff state i → state j, states ordered so the top categorical forms blocks · tells you whether transitions stay within a mode (block-diagonal) or switch.",
  basin_map:
    "shows the basins of attraction · read it as a 2-axis projection of start states colored by WHICH terminal (fixed point / cycle / terminal SCC) each eventually settles into · tells you where each starting state ends up.",
  orbit_scatter:
    "shows the orbit scatter over MANY initial conditions (single-variable systems use a delay embedding, x_t vs x_{t+1}) · read it as each dot = one sampled state, categorical on color, multi-attractor basins distinct · tells you the orbit's shape and where trajectories settle (loop=cycle, pile-up=fixed point).",
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
  function_complexity:
    "shows the COMPILATION COST of each function — its branching plus arithmetic, ranked · read it as the per-tick work the JIT emits for each variable (a constant is cheap, a deep guarded function with big expressions is expensive) · tells you where the program's compute actually goes, invisible in the dynamics views.",
  space_time:
    "shows a Seq-carried state's evolution as a space-time raster · read it as rows=ticks, columns=Seq positions, cell colour=value (binary for 0/1, a colormap for wider ranges); for Rule 90 it draws the Sierpiński triangle · tells you the spatial pattern a 1-D cellular-automaton-style FSM produces over time.",
  terminal_map:
    "shows the ABSTRACT terminal set — where the FSM can come to rest · read it as the absorbing states (whose only successor is themselves), solved by Z3 over the one-step relation with no enumeration; ∅ means it never stops (a daemon) · tells you whether the machine can complete, and at which end states.",
  reachable_region:
    "shows the ABSTRACT reachable region — where the FSM can ever be · read it as a bounding box PROVEN to contain the reachable set by k-induction (sound, no enumeration): bounded / provably-unbounded / indeterminate · tells you whether the state stays in a finite region, even on infinite state spaces.",
  solution_structure:
    "shows a claim's SOLUTION-SPACE structure — what it determines vs leaves free · read it as the backbone (variables forced to one value in every solution, green) plus the free variables over their proven ranges (blue), with implied equalities called out · tells you what the claim actually pins down, beyond the bare ranges.",
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

// --- help/about content for the cockpit (Nadia's #440) -----------------------------
// Plain-language def of every VERDICT + INTERROGATE term, each with a "when you'd use it" line.
// Checked against viz/model_query.py / model_temporal.py / model_analysis.py / terminal_states.py /
// soundness_check.py. Rendered by helpOverlayHtml() into the ? overlay; also fed to ⌘K so the same
// definitions are searchable. Each entry: [term, definition, when, [sub-bullets]].
const HELP_SECTIONS = [
  ["Verdict — what the tool concluded about your whole model (solved, not simulated)", [
    ["Terminates", "every run eventually reaches a state it can never leave (a fixed point) and stops there.", "you WANT your machine to finish (a counter, an installer, a protocol that completes)."],
    ["Cyclic", "the machine loops through a set of states forever; it never settles.", "you EXPECT a loop (a traffic light, a clock) — or you're surprised it never stops."],
    ["Nondeterministic", "from some state there's more than one legal next state, so the future fans out into many runs.", "your model has a free choice (an input, a coin flip) — or you UNDER-constrained it by accident and it drifted."],
    ["Unstable", "an equilibrium exists but runs move away from it; the orbit diverges.", "checking whether a controller/feedback loop actually settles."],
    ["Unbounded", "a variable grows without limit; the reachable set never closes.", "you forgot a bound and a value runs away."],
    ["fixed point", "a state that maps to itself: reach it and the machine stays. The equilibrium it rests at.", ""],
    ["boundary", "the exact lowest..highest value each numeric variable can ever take, proven by the solver (not the min/max of one run).", "'can this counter ever exceed 5?' — read it off the boundary."],
    ["▶ replay a run that finishes", "step through one concrete trajectory from the start state to where it stops, ringing each state on the diagram.", "'HOW does it reach the end, not just that it does.'"],
    ["▶ replay a run that never finishes", "step through a witnessing run that loops forever among non-final states and never reaches an end state. The PROOF that 'Terminates' isn't guaranteed for EVERY run — some run dodges the finish line.", "the verdict says it can rest, but you need to see the run that DOESN'T."],
    ["⛨ double-check this verdict", "re-checks the tool's own abstract answer against a brute-force enumeration of your model, and tells you if they disagree. It verifies the IDE, not your program.", "you're about to trust a verdict for something that matters and want a second, independent computation to agree."],
  ]],
  ["Interrogate — ask your own questions about the model", [
    ["verify (⊢)", "PROVE a property holds on EVERY reachable state (safety, e.g. count ≤ 5) or on EVERY run (liveness). If it fails you get a counterexample TRACE you can step through.", "'is balance ALWAYS ≥ 0?' 'does it ALWAYS eventually reach Idle?'", [
      ["safety (□ / 'always')", "the property is true in every reachable state. Example: count ≤ 5."],
      ["eventually (◇)", "every run reaches a state where the property holds, at least once. Example: ◇ done = true."],
      ["infinitely often (□◇)", "every run reaches it again and again, forever (it never gets permanently stuck without it). Example: □◇ light = Green."],
      ["leads-to (P ⤳ Q)", "whenever P happens, Q eventually follows. Example: mode = Coining ⤳ mode = Idle."],
      ["WF (weak fairness)", "ignore unrealistic runs that forever refuse an always-available step; check liveness only over fair runs."],
    ]],
    ["query (⊨?)", "FIND one reachable state matching a condition (or prove none exists). The mirror image of verify: verify asks 'is it ALWAYS true?', query asks 'is it EVER true?'.", "'CAN the vault ever hold 3 coins while the mode is Vending?' — type mode = Vending ∧ vault = 3."],
    ["assumptions (assert ⊢+)", "stack conditions and re-ask query under all of them at once, like adding hypotheses.", "narrowing down 'under these conditions, what's still possible?'"],
  ]],
];

// Render the #440 help content into the cockpit ? overlay body.
function helpOverlayHtml() {
  return HELP_SECTIONS.map(([title, terms]) =>
    `<div class="help-section"><h3>${title}</h3>` +
    terms.map(([term, def, when, subs]) =>
      `<div class="help-term"><b>${term}</b> — ${def}` +
      (when ? `<span class="help-when">When: ${when}</span>` : "") +
      (subs ? subs.map(([st, sd]) => `<span class="help-sub"><b>${st}</b> — ${sd}</span>`).join("") : "") +
      `</div>`).join("") +
    `</div>`).join("");
}

// --- typable shortcuts in the ⊢ verify / solve input fields (Sam #212/#160) --------
// The same \\word / >= expansion as the editor, for plain <input>s. Wired in app.js.
// Live symbol expansion for the verify/query inputs — the editor's typable-token input (#190), in
// three flavors: \word LaTeX (UNI), ASCII op pairs (<= → ≤), and bare word mnemonics (and → ∧). The
// bare case excludes a \-prefixed word (the LaTeX case owns those). A lowercase var that would
// collide can't exist — the editor converts the same words when the model is written.
function expandFieldSymbols(el) {
  const v = el.value, pos = el.selectionStart;
  const before = v.slice(0, pos);
  let start = -1, rep = "";
  const bs = before.match(/\\([a-zA-Z]+)([^a-zA-Z])$/);     // \word + a just-typed non-letter
  const wm = before.match(/(?:^|[^A-Za-z0-9_\\])([A-Za-z]+)([^A-Za-z0-9_])$/);   // a bare mnemonic word, not \-prefixed
  if (bs && UNI[bs[1]]) { start = pos - bs[0].length; rep = UNI[bs[1]] + bs[2]; }
  else if (OP_PAIRS[before.slice(-2)]) { start = pos - 2; rep = OP_PAIRS[before.slice(-2)]; }
  else if (wm && WORD_MNEMONICS[wm[1]]) { start = pos - wm[2].length - wm[1].length; rep = WORD_MNEMONICS[wm[1]] + wm[2]; }
  if (start >= 0) {
    el.value = v.slice(0, start) + rep + v.slice(pos);
    el.setSelectionRange(start + rep.length, start + rep.length);
  }
}
