"use strict";

// ==============================================================================
// app-data.js — sample programs + typable-token maps for the Evident IDE.
// Pure constants, no DOM / editor dependency. Loaded BEFORE app.js (and the other
// app-*.js concern files) so they all share UNI / WORD_MNEMONICS / OP_PAIRS /
// DEFAULT_PROGRAM / SAMPLES. Moved verbatim out of app.js; behaviour-preserving.
// ==============================================================================

// --- typable-token input -----------------------------------------------------------
// Two ways to type the Unicode operators Evident's lexer expects:
//  (1) LaTeX-style backslash input: \word + a non-letter  →  the operator.
//  (2) bare mnemonic auto-replacement (Task #34): a standalone word/op-pair converts
//      as you type, WORD-BOUNDARY SAFE — `in`→∈ but `Int`/`min`/`Coining` stay put.
const UNI = {
  in: "∈", notin: "∉", forall: "∀", all: "∀", exists: "∃", any: "∃",
  implies: "⇒", imp: "⇒", then: "⇒", Rightarrow: "⇒", impliedby: "⟸", when: "⟸",
  mapsto: "↦", to: "→", langle: "⟨", rangle: "⟩", leq: "≤", le: "≤", geq: "≥",
  ge: "≥", neq: "≠", ne: "≠", Delta: "Δ", delta: "Δ", neg: "¬", not: "¬",
  land: "∧", and: "∧", lor: "∨", or: "∨",
  cup: "∪", cap: "∩", times: "×", cdot: "·", subseteq: "⊆", emptyset: "∅",
  // Liveness operators for the ⊢ verify field: ◇ (eventually / AF) and □◇ (infinitely often).
  diamond: "◇", eventually: "◇", box: "□", always: "□", infinitely: "□◇",
};

// Bare mnemonics that convert when the COMPLETE preceding word is one of these and a
// non-word char is then typed. The lexer accepts only `in`/`mapsto` as words and the
// four ASCII op-pairs natively; everything else here MUST be converted to the real glyph
// so the program lexes. (Task #34.)
const WORD_MNEMONICS = {
  in: "∈", notin: "∉", implies: "⇒", impliedby: "⟸", when: "⟸",
  forall: "∀", all: "∀", exists: "∃", any: "∃", delta: "Δ",
  and: "∧", or: "∨", not: "¬", mapsto: "↦", to: "→",
  langle: "⟨", rangle: "⟩", leq: "≤", geq: "≥", neq: "≠",
  times: "×", cdot: "·", cup: "∪", cap: "∩", subseteq: "⊆", emptyset: "∅",
};
// Two-char ASCII operator pairs: convert the instant the 2nd char is typed.
const OP_PAIRS = { "<=": "≤", ">=": "≥", "!=": "≠", "=>": "⇒" };

const DEFAULT_PROGRAM =
`fsm accumulate
    i   ∈ Int
    sum ∈ Int
    is_first_tick ⇒
        i = 0
        sum = 0
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
  "DVD bounce · 4-wall (guard partition)":
`-- The bouncing-logo screensaver: position drifts, each velocity flips at its two walls. The
-- functionizer compiles 3-branch GUARDED velocity functions (in-bounds vs the two wall conditions)
-- — open function_behavior for the wall-flip partition map.
fsm dvd
    px ∈ Real := 50.0
    py ∈ Real := 30.0
    vx ∈ Real := 3.0
    vy ∈ Real := 2.0
    Δpx = _vx
    Δpy = _vy
    (0.0 < _px ∧ _px < 100.0) ⇒ vx = _vx
    ((_px ≤ 0.0 ∨ _px ≥ 100.0)) ⇒ vx = 0.0 - _vx
    (0.0 < _py ∧ _py < 60.0) ⇒ vy = _vy
    ((_py ≤ 0.0 ∨ _py ≥ 60.0)) ⇒ vy = 0.0 - _vy`,
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
    0 ≤ floor ∈ Int ≤ 3
    dir ∈ Dir
    is_first_tick ⇒ (floor = 0 ∧ dir = Up)
    (_dir = Up ∧ _floor < 3) ⇒ (floor = _floor + 1 ∧ dir = Up)
    (_dir = Up ∧ _floor = 3) ⇒ (floor = _floor ∧ dir = Down)
    (_dir = Down ∧ _floor > 0) ⇒ (floor = _floor - 1 ∧ dir = Down)
    (_dir = Down ∧ _floor = 0) ⇒ (floor = _floor ∧ dir = Up)`,
  "Collatz · 3n+1 (guarded integer map)":
`-- The Collatz map: n even → n/2, n odd → 3n+1 (even tested as n = 2·(n/2)). A clean 2-way guarded
-- integer function — open function_guards for the even/odd decision.
fsm collatz
    1 ≤ n ∈ Int ≤ 100000
    is_first_tick ⇒ n = 27
    (_n = 2 * (_n / 2)) ⇒ n = _n / 2
    (_n ≠ 2 * (_n / 2)) ⇒ n = 3 * _n + 1`,
  "double pendulum · full coupling (densest functions)":
`-- Two coupled pendula (linearized): each angular velocity is driven by BOTH angles and the other
-- velocity. The functionizer compiles 4 functions where each velocity reads all four variables —
-- a near-complete coupling graph (6 feedback cycles). Open function_graph.
fsm pendulum
    a1, a2 ∈ Real
    w1, w2 ∈ Real
    is_first_tick ⇒ (a1 = 1.0 ∧ a2 = 0.5 ∧ w1 = 0.0 ∧ w2 = 0.0)
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
    mode    ∈ Mode
    0 ≤ balance ∈ Int ≤ 5      -- coins in the receptacle (capacity 5)
    0 ≤ stock   ∈ Int ≤ 3      -- units of product remaining
    0 ≤ vault   ∈ Int ≤ 12     -- money the operator has collected
    act     ∈ Act              -- free customer/operator choice each tick

    is_first_tick ⇒
        mode = Idle
        balance = 0
        stock = 3
        vault = 0

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
    light ∈ Light
    0 ≤ timer ∈ Int ≤ 2
    is_first_tick ⇒ (light = Red ∧ timer = 0)
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
    pos ∈ Real
    vel ∈ Real
    is_first_tick ⇒ (pos = 60.0 ∧ vel = 0.0)
    Δpos = _vel / 6.0
    Δvel = (0.0 - _pos - _vel / 2.0) / 6.0`,
  "collatz · the 3n+1 orbit (FSM)":
`-- The Collatz map: halve n if even, else 3n+1. A wild integer orbit that always falls to 1.
-- (No modulo operator yet, so even-ness is 2·(n/2) = n via integer division.)
fsm collatz
    n ∈ Int
    is_first_tick ⇒ n = 27
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
    count ∈ Int
    1 ≤ step ∈ Int ≤ 3
    is_first_tick ⇒ count = 0
    Δcount = step`,
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

  // --- algorithms as constraints (run with ⊨ Solve — the solver replaces the algorithm) ---
  "topo sort · a DAG's linear order (⊨ Solve)":
`-- A DAG's edges as constraints; the solver finds a linear order respecting them.
-- No traversal, no visited-set — just "every edge points forward in the order".
type Edge(from, to ∈ Int)

claim toposort
    edges ∈ Seq(Edge)
    pos   ∈ Seq(Int)
    #edges = 5
    #pos   = 5

    edges[0] = Edge(0, 1)
    edges[1] = Edge(0, 2)
    edges[2] = Edge(1, 3)
    edges[3] = Edge(2, 3)
    edges[4] = Edge(3, 4)

    ∀ i ∈ {0..4} :
        0 ≤ pos[i]
        pos[i] ≤ 4
    ∀ i ∈ {0..4} :
        ∀ j ∈ {0..4} :
            i < j ⇒ pos[i] ≠ pos[j]
    ∀ e ∈ edges :
        pos[e.from] < pos[e.to]`,
  "4×4 sudoku · fill the grid (⊨ Solve)":
`-- 4×4 Sudoku: state the rules (each row, column, and 2×2 box holds 1..4 once)
-- and pin a few givens. The solver fills the rest — no backtracking written.
type Box(a, b, c, d ∈ Int)

claim sudoku
    cell  ∈ Seq(Int)
    boxes ∈ Seq(Box)
    #cell  = 16
    #boxes = 4

    ∀ i ∈ {0..15} :
        1 ≤ cell[i]
        cell[i] ≤ 4

    -- givens
    cell[0]  = 1
    cell[2]  = 3
    cell[8]  = 2
    cell[15] = 1

    -- rows distinct
    ∀ r ∈ {0..3} :
        ∀ a ∈ {0..3} :
            ∀ b ∈ {0..3} :
                a < b ⇒ cell[r * 4 + a] ≠ cell[r * 4 + b]
    -- columns distinct
    ∀ c ∈ {0..3} :
        ∀ a ∈ {0..3} :
            ∀ b ∈ {0..3} :
                a < b ⇒ cell[a * 4 + c] ≠ cell[b * 4 + c]
    -- the four 2×2 boxes, named by their member cells
    boxes[0] = Box(cell[0],  cell[1],  cell[4],  cell[5])
    boxes[1] = Box(cell[2],  cell[3],  cell[6],  cell[7])
    boxes[2] = Box(cell[8],  cell[9],  cell[12], cell[13])
    boxes[3] = Box(cell[10], cell[11], cell[14], cell[15])
    ∀ x ∈ boxes :
        x.a ≠ x.b
        x.a ≠ x.c
        x.a ≠ x.d
        x.b ≠ x.c
        x.b ≠ x.d
        x.c ≠ x.d`,
  "subset-sum · pick items hitting a target (⊨ Solve)":
`-- Subset-sum: choose a subset of these weights that totals exactly the target.
-- 'take' is a yes/no per item; the solver finds which items to take.
type Item(weight ∈ Int, take ∈ Bool)

claim subset_sum
    items ∈ Seq(Item)
    #items = 6
    target ∈ Int = 15

    items[0].weight = 3
    items[1].weight = 7
    items[2].weight = 1
    items[3].weight = 8
    items[4].weight = 4
    items[5].weight = 11

    -- the taken weights must total exactly the target
    chosen ∈ Int = (items[0].take ? 3 : 0) + (items[1].take ? 7 : 0) + (items[2].take ? 1 : 0) + (items[3].take ? 8 : 0) + (items[4].take ? 4 : 0) + (items[5].take ? 11 : 0)
    chosen = target`,
  "sort · output a sorted permutation (⊨ Solve)":
`-- Sorting as constraints: 'out' is the SAME multiset as 'input', but ascending.
-- No compare-and-swap; just "ordered" + "a permutation of the input".
claim sort_constraints
    input ∈ Seq(Int)
    out   ∈ Seq(Int)
    #input = 5
    #out   = 5

    input[0] = 30
    input[1] = 10
    input[2] = 50
    input[3] = 20
    input[4] = 40

    -- out is ascending
    ∀ (a, b) ∈ edges(out) :
        a ≤ b

    -- out is a permutation of input: each is a rearrangement of the other.
    -- (inputs are distinct, so multiset-equality = mutual element membership)
    ∀ i ∈ {0..4} :
        ∃ j ∈ {0..4} : out[j] = input[i]
    ∀ j ∈ {0..4} :
        ∃ i ∈ {0..4} : input[i] = out[j]`,

  // --- diagram-value demos (each FSM picked to make one underused view shine) ---
  "Rule 90 · a cellular-automaton fractal (Seq-state FSM, space_time)":
`-- Rule 90 elementary cellular automaton: each cell becomes the XOR of its two
-- neighbours (1 iff exactly one neighbour is 1). The WHOLE carried state is a
-- Seq(Int) of 0/1 — a sequence-valued FSM, where the next sequence is a function
-- of the previous one. A single seed grows the Sierpinski-triangle fractal. Open
-- the space_time view — it stacks every tick into one raster (rows = ticks, columns
-- = cells), and the Sierpiński triangle falls straight out. (timing_diagram puts
-- each cell on its own lane; time_series tracks them too.)
fsm rule90
    cells ∈ Seq(Int)
    #cells = 11
    is_first_tick ⇒ cells = ⟨0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0⟩
    ∀ i ∈ {1..9} : cells[i] = ((_cells[i-1] + _cells[i+1]) = 1 ? 1 : 0)
    (cells[0] = 0 ∧ cells[10] = 0)`,
  "bistable · two basins of attraction (FSM, basin_map)":
`-- A random walk between two absorbing walls at 0 and 6 (gambler's ruin).
-- Each tick a free step ±1, unless already at a wall, where it sticks. From the
-- middle the walk can end at EITHER wall, so the reachable graph has two terminal
-- states. Open basin_map: it colors each reachable state by the wall it falls to.
fsm bistable
    x ∈ Int
    step ∈ Int
    -1 ≤ step ≤ 1
    is_first_tick ⇒ x = 3
    0 ≤ x
    x ≤ 6
    Δx = (_x = 0 ? 0 : (_x = 6 ? 0 : step))`,
  "fixed point · a 1-D map's staircase (FSM, cobweb)":
`-- A 1-D contraction map: each tick x moves a quarter of the way to 40.
-- It converges monotonically to the fixed point. Open the cobweb view: the
-- red staircase climbs from the seed to where the map line meets y = x.
fsm fixedpoint
    x ∈ Int
    is_first_tick ⇒ x = 4
    x = _x + (40 - _x) / 4`,
  "four signals · a 4-variable system (FSM, scatter_matrix)":
`-- Four genuinely-carried sawtooths on coprime periods (11, 5, 7, 3). Each pair
-- sweeps a different lattice. Open scatter_matrix: every pairwise plane at once,
-- with each variable's distribution on the diagonal. (parallel_coords also fits.)
fsm fourvar
    a ∈ Int
    b ∈ Int
    c ∈ Int
    d ∈ Int
    is_first_tick ⇒ (a = 0 ∧ b = 0 ∧ c = 0 ∧ d = 0)
    a = (_a ≥ 10 ? 0 : _a + 1)
    b = (_b ≥ 4  ? 0 : _b + 1)
    c = (_c ≥ 6  ? 0 : _c + 1)
    d = (_d ≥ 2  ? 0 : _d + 1)`,
  "digital block · clock + flags (FSM, timing_diagram)":
`-- A small synchronous digital block, all four signals genuinely carried tick-to-tick:
--   clk   — toggles every tick (the master clock)
--   clk2  — a divide-by-2 clock: toggles only on clk's rising edge
--   count — a 2-bit counter advancing each tick, wrapping at 3
--   pulse — a one-tick strobe, high only on the tick the counter wraps
-- Open timing_diagram: all four stack as waveforms on one time axis (a logic analyzer).
fsm timing
    clk   ∈ Bool
    clk2  ∈ Bool
    count ∈ Int
    pulse ∈ Bool
    is_first_tick ⇒ (clk = false ∧ clk2 = false ∧ count = 0 ∧ pulse = false)
    clk = ¬_clk
    clk2 = (¬_clk ? ¬_clk2 : _clk2)
    count = (_count ≥ 3 ? 0 : _count + 1)
    pulse = (¬_pulse ∧ _count ≥ 3)`,
};

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
  + "the solver replays it. <code>_count</code> reads the value on the <i>previous</i> tick; "
  + "<code>count = …</code> writes <i>this</i> tick, <code>_count</code> reads the PREVIOUS tick. "
  + "You SEED the start value with <code>:=</code> on the declaration "
  + "(<code>count ∈ Int := 0</code> — sugar for \"on tick 0, count = 0\") since there's no previous tick. "
  + "<code>Δcount</code> is shorthand for <code>count − _count</code> — the <i>change</i> each "
  + "tick — so <code>Δcount = 1</code> literally says \"count rises by one every tick\".";

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
      + "Note both deltas live under ONE <code>¬is_first_tick ⇒</code> guard as an indented block — "
      + "that's the idiom for grouping several changes under the same condition.",
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

// --- shared pure helper ------------------------------------------------------------
// escapeHtml lives here (the first-loaded file) so every later concern file can use it.
function escapeHtml(s) { return (s || "").replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;"); }
