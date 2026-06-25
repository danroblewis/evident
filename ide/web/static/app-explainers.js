"use strict";

// ==============================================================================
// app-explainers.js — the per-sample "how this works" explainer copy (Task #102, #250).
//
// Split out of app-data.js to keep it under the CLAUDE.md ≤500-line convention. This is the
// onboarding teaching layer (a plain-English narrative + a "why this code does that" + a
// "try this" per sample), distinct from the sample SOURCE tables (which stay in app-data.js).
// explainerFor() reverse-looks-up a buffer against SAMPLES / DEFAULT_PROGRAM (app-data.js,
// resolved at call time); renderExplainer (app.js) renders the result. Behaviour-preserving.
// ==============================================================================

// --- per-sample "how this works" explainers (Task #102, concern #250) --------------
// The GLOSSARY (app-symbols.js) teaches what a single GLYPH means; these teach what a
// whole MODEL means — the gap a newcomer hits after reading "it ramps to 5" but still
// can't say WHY. Each entry: a plain-English narrative of the concept the sample
// embodies, then a concrete "why this particular code produces that behavior", then one
// "try this" nudge. Rendered as a collapsible note under the banner (wired in app.js).
//
// Keyed by the SAMPLES key. A sample with no entry simply shows no note — these are a
// teaching layer over the samples that most need explaining (the FSMs / the Δ idea), not
// a mandate to caption all 19. The CONCEPTS map below is the shared vocabulary they lean on.
const EXPLAIN_FSM_PREAMBLE =
  "An <b>fsm</b> is a state machine written as a <i>difference equation</i>: instead of "
  + "looping in your head, you state how each variable RELATES from one tick to the next, and "
  + "the solver replays it. For any carried variable <code>x</code>: <code>x = …</code> writes "
  + "<i>this</i> tick's value, and <code>_x</code> reads the value on the <i>previous</i> tick. "
  + "You SEED the start value with <code>:=</code> on the declaration "
  + "(<code>x ∈ Int := 0</code> — sugar for \"on tick 0, x = 0\") since there's no previous tick. "
  + "<code>Δx</code> is shorthand for <code>x − _x</code> — the <i>change</i> each tick — so "
  + "<code>Δx = 1</code> literally says \"x rises by one every tick\".";

const EXPLAINERS = {
  "counter · a terminating clock (FSM)": {
    what: "A counter that climbs to 5 and then stops — the simplest difference equation.",
    why: EXPLAIN_FSM_PREAMBLE
      + "<br><br><b>Why it ramps then halts:</b> tick 0 seeds <code>count = 0</code>. "
      + "Every later tick says <code>Δcount = (_count &lt; 5 ? 1 : 0)</code> — \"rise by 1 while "
      + "below 5, otherwise rise by 0\". So count goes 0,1,2,3,4,5 and then sits at 5 forever: a "
      + "<i>fixed point</i>. The diagram's structure line calls this <b>Terminates</b> because the "
      + "machine reaches a state it can never leave.",
    tryit: "Change the <code>5</code> to <code>8</code> and watch the ramp grow. Or change "
      + "<code>count ∈ Int := 0</code> to <code>:= 3</code> — the seed shifts, so the ramp starts at 3.",
  },
  "accumulate · a driven pipeline (FSM)": {
    what: "Two coupled variables: a driver (i) counts up, and a follower (sum) accumulates it — "
      + "a running total.",
    why: EXPLAIN_FSM_PREAMBLE
      + "<br><br><b>Why this is a 'pipeline':</b> <code>i</code> advances on its own "
      + "(<code>Δi = 1</code> while below 5), and <code>sum</code> is <i>driven by</i> i — each tick "
      + "it adds the current i (<code>Δsum = _i</code>). One variable leads, the other follows. "
      + "Both <code>i</code> and <code>sum</code> seed to 0 with <code>:=</code> on their declarations, "
      + "then advance by their own <code>Δ</code> each tick.",
    tryit: "Add a third line <code>Δsum = _i + 1</code>? No — that would be a SECOND constraint on the "
      + "same change and over-constrain it. Instead try changing <code>_i &lt; 5</code> to "
      + "<code>_i &lt; 8</code> in both deltas and watch the total grow.",
  },
  "vending · stock, coins & a vault (FSM)": {
    what: "A real vending machine: coins accumulate, products sell, the customer can cancel, the "
      + "operator services it. The free <code>act</code> each tick makes it nondeterministic.",
    why: EXPLAIN_FSM_PREAMBLE
      + "<br><br><b>Why it's nondeterministic:</b> <code>act</code> is declared but never pinned, "
      + "so the solver is free to pick ANY action each tick — insert a coin, purchase, cancel, "
      + "service. From one state there are several legal next states, so the future fans out. The "
      + "<code>act = … ⇒ (…)</code> lines are a <i>dispatch table</i>: each names what changes when "
      + "that action fires. Open <code>state_graph</code> to see every reachable configuration and "
      + "how the actions connect them.",
    tryit: "Pin the action by adding <code>act = InsertCoin</code> as a top-level line — now the "
      + "machine is deterministic (only coins go in) and the reachable graph collapses to a line.",
  },
  "traffic light · a cyclic state machine (FSM)": {
    what: "A light cycling Red → Green → Yellow → Red forever, holding each color for 2 ticks.",
    why: EXPLAIN_FSM_PREAMBLE
      + "<br><br><b>Why it cycles instead of halting:</b> there's no fixed point — every state "
      + "leads to a different one, so the machine loops endlessly. The <code>_timer ≥ 2</code> guard "
      + "is the dwell logic: while the timer is below 2 the color holds and <code>Δtimer = 1</code>; "
      + "once it hits 2 the timer resets and the color advances via the inner dispatch table "
      + "(<code>_light = Red ⇒ light = Green</code>, …). The structure line reads <b>Cyclic</b>.",
    tryit: "Change <code>_timer ≥ 2</code> to <code>_timer ≥ 4</code> — each color now holds twice as "
      + "long. The cycle is the same shape, just slower.",
  },
  "oscillator · a damped spring (FSM, phase spiral)": {
    what: "Two real variables — position and velocity — that push on each other, like a mass on a "
      + "spring losing energy to friction.",
    why: EXPLAIN_FSM_PREAMBLE
      + "<br><br><b>Why it spirals:</b> velocity changes the position (<code>Δpos = _vel/6</code>) "
      + "and position-plus-damping changes the velocity (<code>Δvel = (−_pos − _vel/2)/6</code>). "
      + "Two coupled difference equations like this trace a curve in (pos, vel) space — open the "
      + "<code>phase_portrait</code> view to see the orbit spiral inward toward the equilibrium at "
      + "the origin. The solver finds that fixed point and marks it <b>Unstable</b> (the orbit "
      + "moves away from it before damping pulls it back).",
    tryit: "Soften the damping: change <code>_vel / 2.0</code> to <code>_vel / 8.0</code>. The spiral "
      + "tightens more slowly — less friction, more oscillation before it settles.",
  },
  "collatz · the 3n+1 orbit (FSM)": {
    what: "The famous Collatz map: halve n if it's even, else compute 3n+1. A wild integer orbit "
      + "that (conjecturally) always falls to 1.",
    why: EXPLAIN_FSM_PREAMBLE
      + "<br><br><b>Why one equation, no Δ:</b> here next-n isn't a small change from _n — it "
      + "either halves or roughly triples — so it's written as a plain function of the previous "
      + "value, <code>n = (… ? _n/2 : 3*_n+1)</code>, not a Δ. (Δ is for steady increments; a value "
      + "that's a fresh function each tick stays a plain equation.) The <code>2*(_n/2) = _n</code> "
      + "test is how you check evenness without a modulo operator.",
    tryit: "Change the seed <code>n = 27</code> to <code>n = 97</code> and watch a different, longer "
      + "orbit — every starting value falls to 1, but the path length varies wildly.",
  },
  "random walk · nondeterministic drift (FSM)": {
    what: "A walker that steps freely in x and y each tick — a 2-D random walk.",
    why: EXPLAIN_FSM_PREAMBLE
      + "<br><br><b>Why it's nondeterministic:</b> <code>dx</code> and <code>dy</code> are bounded "
      + "to <code>{−1, 0, 1}</code> but never pinned, so the solver may choose any legal step. The "
      + "walker's NEXT position isn't determined by its current one — the future fans out. "
      + "<code>occupancy_heatmap</code> shows where it tends to dwell across many possible walks; "
      + "<code>reachability_tree</code> shows the branching of all the places it could go.",
    tryit: "Widen the step: change both bounds to <code>−2 ≤ dx ≤ 2</code>. The walker now covers "
      + "ground faster and the reachable region grows.",
  },
  "pick · a nondeterministic choice (FSM)": {
    what: "A counter that climbs by a free amount (1, 2, or 3) each tick — the smallest "
      + "nondeterministic machine.",
    why: EXPLAIN_FSM_PREAMBLE
      + "<br><br><b>Why it's the canonical Δ example:</b> <code>step</code> is declared with a range "
      + "(<code>1 ≤ step ∈ Int ≤ 3</code>) but left free, so the solver picks a value each tick and "
      + "<code>Δcount = step</code> applies it. This is the difference-equation idea at its purest: "
      + "you state the <i>rule for the change</i>, not the sequence of values. Different runs ramp at "
      + "different rates — that's the nondeterminism.",
    tryit: "Widen the choice to <code>1 ≤ step ∈ Int ≤ 5</code>, or pin it with <code>step = 2</code> "
      + "to make the machine deterministic (count always rises by exactly 2).",
  },
};

// explainerFor: reverse-lookup which sample a buffer matches, returning its explainer.
// Driven by CONTENT (not the menu) so it works however the program arrived — sample menu,
// command palette, share link, or the tour. Returns null for user-written / unmatched buffers.
function explainerFor(source) {
  const src = (source || "").trim();
  for (const name of Object.keys(EXPLAINERS)) {
    const sample = SAMPLES[name];
    if (sample && sample.trim() === src) return { name, ...EXPLAINERS[name] };
  }
  // DEFAULT_PROGRAM is the accumulate sample by reference; match it too.
  if (src === DEFAULT_PROGRAM.trim() && EXPLAINERS["accumulate · a driven pipeline (FSM)"]) {
    return { name: "accumulate · a driven pipeline (FSM)",
             ...EXPLAINERS["accumulate · a driven pipeline (FSM)"] };
  }
  return null;
}
