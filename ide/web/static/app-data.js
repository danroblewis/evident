"use strict";

// ==============================================================================
// app-data.js — sample programs (DEFAULT_PROGRAM / SAMPLES) + their headline-view picker,
// plus the shared escapeHtml helper, for the Evident IDE. Pure constants, no DOM / editor
// dependency. Loaded BEFORE app.js (and the other app-*.js concern files) so they all share
// these globals. Two sibling concerns split out to stay under the CLAUDE.md ≤500-line
// convention: the typable-token maps (UNI / WORD_MNEMONICS / OP_PAIRS) → app-symbols-data.js,
// and the per-sample "how this works" copy → app-explainers.js (both loaded before this file).
// ==============================================================================

const DEFAULT_PROGRAM =
`fsm accumulate
    i   ∈ Int := 0
    sum ∈ Int := 0
    Δi   = (_i < 5 ? 1 : 0)
    Δsum = (_i < 5 ? _i : 0)`;

// Worked examples chosen to demonstrate DISTINCT model shapes and language features — not
// seven counters. The FSMs exercise different dynamics (a terminating ramp, a real cyclic
// machine, a 2-D phase spiral, a wild integer orbit, nondeterministic drift); the claims
// show algorithms expressed as constraints (solve them with ⊨ Solve).
// The headline diagram for a sample — the view it's NAMED for ("Rule 90 · … timing_diagram") or
// that its comment says to open ("Open the phase_portrait view"). Loading a sample jumps straight
// to that view instead of the generic recommendation (#87/#128/#168). Special/headline views are
// listed BEFORE the generic ones so an incidental "state_graph" mention never beats the real one;
// the server validates the token and falls back to the recommended view if it's unknown.
const VIEW_TOKENS = ["space_time", "phase_portrait", "cobweb", "nullcline_field", "timing_diagram",
  "function_graph", "function_behavior", "function_guards", "function_complexity", "function_residual",
  "morse_graph", "basin_map", "fixedpoint_map", "chord_diagram", "transition_matrix",
  "occupancy_heatmap", "orbit_scatter", "parallel_coords", "reachability_tree", "scatter_matrix",
  "state_graph", "time_series", "solution_space"];
function headlineView(name, source) {
  const t = (name || "") + " " + (source || "");
  const m = t.match(/open the ([a-z_]+)\s*(?:--\s*)?(?:view|tab)/i);   // explicit instruction (tolerates a wrapped "-- tab")
  if (m && VIEW_TOKENS.includes(m[1].toLowerCase())) return m[1].toLowerCase();
  return VIEW_TOKENS.find((v) => t.includes(v)) || null;              // else the first headline token mentioned
}

const SAMPLES = {
  "counter · a terminating clock (FSM)":
`fsm counter
    count ∈ Int := 0
    Δcount = (_count < 5 ? 1 : 0)
    done ∈ Bool = (count ≥ 5)`,
  "accumulate · a driven pipeline (FSM)": DEFAULT_PROGRAM,
  "predator-prey · Lotka-Volterra (coupled functions)":
`-- Lotka-Volterra: prey grow, predators eat prey and starve. The functionizer compiles two COUPLED
-- functions (the prey↔predator feedback, with _prey·_pred product terms) — open the function_graph
-- tab to see the coupling cycle, or function_behavior for the transfer surfaces.
fsm predator_prey
    prey ∈ Real := 40.0
    pred ∈ Real := 9.0
    Δprey = _prey * 0.1 - _prey * _pred * 0.01
    Δpred = _prey * _pred * 0.005 - _pred * 0.1`,
  "logistic map · chaos (nonlinear function)":
`-- The logistic map x' = r·x·(1-x), r = 3.7 — the canonical route to chaos. A single nonlinear
-- function; open function_behavior for its parabolic transfer map, or cobweb for the dynamics.
fsm logistic
    x ∈ Real := 0.3
    x = 3.7 * _x * (1.0 - _x)`,
  "bouncing ball · hybrid (guarded functions)":
`-- A ball under gravity that bounces off the floor (pos ≤ 0 flips & damps the velocity). The
-- functionizer compiles 3-branch GUARDED functions (init / free-fall / bounce) — open function_guards.
fsm ball
    pos ∈ Real := 50.0
    vel ∈ Real := 0.0
    (_pos > 0.0) ⇒ (Δpos = _vel ∧ Δvel = 0.0 - 9.8)
    (_pos ≤ 0.0) ⇒ (pos = 0.0 ∧ vel = 0.0 - _vel * 0.7)`,
  "spring chain · 6 coupled masses (dense data-flow)":
`-- Three masses connected by springs (Hooke's law, nearest-neighbour coupling). The functionizer
-- compiles SIX coupled functions; the middle mass reads both its neighbours — open function_graph
-- for the dense coupling DAG (the most cross-edges of any sample).
fsm springs
    x1 ∈ Real := 10.0
    x2, x3 ∈ Real := 0.0
    v1, v2, v3 ∈ Real := 0.0
    Δx1 = _v1 * 0.1
    Δx2 = _v2 * 0.1
    Δx3 = _v3 * 0.1
    Δv1 = (0.0 - _x1 * 2.0 + _x2) * 0.1
    Δv2 = (_x1 - _x2 * 2.0 + _x3) * 0.1
    Δv3 = (_x2 - _x3 * 2.0) * 0.1`,
  "thermostat · hysteresis (mode-switching guards)":
`-- A heater with hysteresis: temp rises while Heating, falls while Idle; the mode switches when temp
-- crosses 22 (→ Idle) or 18 (→ Heating). The functionizer compiles a 4-branch GUARDED mode function
-- and a per-mode temp function — open function_guards (and the ✓ total & unambiguous verdict).
enum Mode = Heating | Idle
fsm thermostat
    temp ∈ Real := 15.0
    mode ∈ Mode := Heating
    (_mode = Heating) ⇒ Δtemp = 1.0
    (_mode = Idle) ⇒ Δtemp = 0.0 - 0.5
    (_temp ≥ 22.0) ⇒ mode = Idle
    (_temp ≤ 18.0) ⇒ mode = Heating
    (18.0 < _temp ∧ _temp < 22.0) ⇒ mode = _mode`,
  "DVD bounce · 2 vars (velocity in the history)":
`-- The bouncing-logo screensaver in 2 variables, not 4. The velocity isn't a separate field —
-- it lives in the position HISTORY: Δx = x − _x. Seed the rate directly with  Δx := 3  (initial +3),
-- carry it with  Δx = Δ_x , and flip it at each wall. The second-order shift register
-- (__x = _x one tick ago) makes Δ_x read the right rate from tick 1, so the bounce is exact.
fsm dvd
    x ∈ Real := 50.0
    y ∈ Real := 30.0
    Δx := 3.0
    Δy := 2.0
    (0.0 < _x < 100.0) ⇒ Δx = Δ_x
    ¬(0.0 < _x < 100.0) ⇒ Δx = 0.0 - Δ_x
    (0.0 < _y < 60.0) ⇒ Δy = Δ_y
    ¬(0.0 < _y < 60.0) ⇒ Δy = 0.0 - Δ_y`,
  "SIR epidemic · 3 coupled compartments":
`-- The SIR model: susceptibles get infected (the S·I product), infected recover. THREE coupled
-- functions — a driven cascade S→I→R with one product coupling. Open function_graph.
fsm sir
    s ∈ Real := 99.0
    i ∈ Real := 1.0
    r ∈ Real := 0.0
    Δs = 0.0 - _s * _i * 0.001
    Δi = _s * _i * 0.001 - _i * 0.05
    Δr = _i * 0.05`,
  "cruise control · PID loop (coupled feedback)":
`-- A speed controller: error = target − speed, an integral accumulates error, and speed responds.
-- A feedback LOOP (speed↔error↔integral) — open function_graph for the controller cycle.
fsm cruise
    speed ∈ Real := 0.0
    error ∈ Real := 0.0
    integ ∈ Real := 0.0
    error = 60.0 - _speed
    Δinteg = _error
    Δspeed = _error * 0.3 + _integ * 0.05`,
  "elevator · bouncing controller (deep dispatch)":
`-- An elevator that rides to the top then back to the bottom. The functionizer compiles 5-branch
-- GUARDED functions on (dir, floor) — open function_guards for the deep decision tree.
enum Dir = Up | Down
fsm elevator
    floor ∈ Int := 0
    0 ≤ floor ≤ 3
    dir ∈ Dir := Up
    (_dir = Up ∧ _floor < 3) ⇒ (floor = _floor + 1 ∧ dir = Up)
    (_dir = Up ∧ _floor = 3) ⇒ (floor = _floor ∧ dir = Down)
    (_dir = Down ∧ _floor > 0) ⇒ (floor = _floor - 1 ∧ dir = Down)
    (_dir = Down ∧ _floor = 0) ⇒ (floor = _floor ∧ dir = Up)`,
  "Collatz · 3n+1 (guarded integer map)":
`-- The Collatz map: n even → n/2, n odd → 3n+1 (even tested as n = 2·(n/2)). A clean 2-way guarded
-- integer function — open function_guards for the even/odd decision.
fsm collatz
    n ∈ Int := 27
    1 ≤ n ≤ 100000
    (_n = 2 * (_n / 2)) ⇒ n = _n / 2
    (_n ≠ 2 * (_n / 2)) ⇒ n = 3 * _n + 1`,
  "double pendulum · full coupling (densest functions)":
`-- Two coupled pendula (linearized): each angular velocity is driven by BOTH angles and the other
-- velocity. The functionizer compiles 4 functions where each velocity reads all four variables —
-- a near-complete coupling graph (6 feedback cycles). Open function_graph.
fsm pendulum
    a1 ∈ Real := 1.0
    a2 ∈ Real := 0.5
    w1, w2 ∈ Real := 0.0
    Δa1 = _w1 * 0.1
    Δa2 = _w2 * 0.1
    Δw1 = (0.0 - _a1 * 3.0 + _a2 * 2.0 - _w2 * 0.5) * 0.1
    Δw2 = (_a1 * 2.0 - _a2 * 3.0 + _w1 * 0.5) * 0.1`,
  "vending · stock, coins & a vault (FSM)":
`-- A real vending machine: coins accumulate (up to a capacity), products sell from stock
-- into the operator's vault, the customer can cancel for a refund, and the operator
-- services it. The free \`act\` each tick makes the machine nondeterministic.
enum Mode = Idle | Coining | Dispensing | Refunding | Servicing
enum Act  = InsertCoin | Purchase | Cancel | Service

fsm vending
    mode    ∈ Mode := Idle
    balance ∈ Int := 0
    0 ≤ balance ≤ 5            -- coins in the receptacle (capacity 5)
    stock   ∈ Int := 3
    0 ≤ stock ≤ 3             -- units of product remaining
    vault   ∈ Int := 0
    0 ≤ vault ≤ 12            -- money the operator has collected
    act     ∈ Act              -- free customer/operator choice each tick

    act = InsertCoin ⇒
        _balance < 5 ⇒
            mode = Coining
            balance = _balance + 1
            stock = _stock
            vault = _vault
        _balance ≥ 5 ⇒
            mode = Coining
            balance = _balance
            stock = _stock
            vault = _vault
    (act = Purchase ∧ _balance ≥ 3 ∧ _stock > 0) ⇒
        mode = Dispensing
        balance = _balance - 3
        stock = _stock - 1
        vault = _vault + 3
    (act = Purchase ∧ (_balance < 3 ∨ _stock = 0)) ⇒
        mode = Idle
        balance = _balance
        stock = _stock
        vault = _vault
    act = Cancel ⇒
        mode = Refunding
        balance = 0
        stock = _stock
        vault = _vault
    act = Service ⇒
        mode = Servicing
        balance = _balance
        stock = 3
        vault = 0`,
  "traffic light · a cyclic state machine (FSM)":
`enum Light = Red | Green | Yellow

fsm traffic
    light ∈ Light := Red
    timer ∈ Int := 0
    0 ≤ timer ≤ 2
    _timer ≥ 2 ⇒
        timer = 0
        _light = Red    ⇒ light = Green
        _light = Green  ⇒ light = Yellow
        _light = Yellow ⇒ light = Red
    _timer < 2 ⇒
        Δtimer = 1
        light = _light`,
  "oscillator · a damped spring (FSM, phase spiral)":
`-- Two interacting real variables — position and velocity. Open the phase_portrait view:
-- the trajectory spirals in (pos, vel) space. The solver finds the equilibrium at the
-- origin, and the structure line reports it as an UNSTABLE one (the orbit diverges from it).
fsm oscillator
    pos ∈ Real := 60.0
    vel ∈ Real := 0.0
    Δpos = _vel / 6.0
    Δvel = (0.0 - _pos - _vel / 2.0) / 6.0`,
  "collatz · the 3n+1 orbit (FSM)":
`-- The Collatz map: halve n if even, else 3n+1. A wild integer orbit that always falls to 1.
-- (No modulo operator yet, so even-ness is 2·(n/2) = n via integer division.)
fsm collatz
    n ∈ Int := 27
    n = (_n ≤ 1 ? 1 : (2 * (_n / 2) = _n ? _n / 2 : 3 * _n + 1))`,
  "random walk · nondeterministic drift (FSM)":
`-- Each tick the walker steps freely in x and y: the free per-tick change Δx, Δy ∈ {-1, 0, 1} makes
-- it nondeterministic. The occupancy_heatmap shows where it dwells, the reachability_tree the fan.
fsm random_walk
    x, y ∈ Int := 0
    -1 ≤ Δx ≤ 1
    -1 ≤ Δy ≤ 1`,
  "pick · a nondeterministic choice (FSM)":
`fsm pick
    count ∈ Int := 0
    1 ≤ step ∈ Int ≤ 3
    Δcount = step`,
};

// The per-sample "how this works" explainer copy (EXPLAIN_FSM_PREAMBLE / EXPLAINERS / explainerFor)
// moved to app-explainers.js (loaded before this file); renderExplainer (app.js) calls explainerFor.

// --- shared pure helper ------------------------------------------------------------
// escapeHtml lives here (the first-loaded file) so every later concern file can use it.
function escapeHtml(s) { return (s || "").replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;"); }
