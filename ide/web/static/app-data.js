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
  "predator-prey · Lotka-Volterra (coupled functions)":
`-- Lotka-Volterra: prey grow, predators eat prey and starve. The functionizer compiles two COUPLED
-- functions (the prey↔predator feedback, with _prey·_pred product terms) — open the function_graph
-- tab to see the coupling cycle, or function_behavior for the transfer surfaces.
fsm predator_prey
    prey ∈ Real
    pred ∈ Real
    is_first_tick ⇒ (prey = 40.0 ∧ pred = 9.0)
    ¬is_first_tick ⇒ Δprey = _prey * 0.1 - _prey * _pred * 0.01
    ¬is_first_tick ⇒ Δpred = _prey * _pred * 0.005 - _pred * 0.1`,
  "logistic map · chaos (nonlinear function)":
`-- The logistic map x' = r·x·(1-x), r = 3.7 — the canonical route to chaos. A single nonlinear
-- function; open function_behavior for its parabolic transfer map, or cobweb for the dynamics.
fsm logistic
    x ∈ Real
    is_first_tick ⇒ x = 0.3
    ¬is_first_tick ⇒ x = 3.7 * _x * (1.0 - _x)`,
  "bouncing ball · hybrid (guarded functions)":
`-- A ball under gravity that bounces off the floor (pos ≤ 0 flips & damps the velocity). The
-- functionizer compiles 3-branch GUARDED functions (init / free-fall / bounce) — open function_guards.
fsm ball
    pos ∈ Real
    vel ∈ Real
    is_first_tick ⇒ (pos = 50.0 ∧ vel = 0.0)
    (¬is_first_tick ∧ _pos > 0.0) ⇒ (Δpos = _vel ∧ Δvel = 0.0 - 9.8)
    (¬is_first_tick ∧ _pos ≤ 0.0) ⇒ (pos = 0.0 ∧ vel = 0.0 - _vel * 0.7)`,
  "spring chain · 6 coupled masses (dense data-flow)":
`-- Three masses connected by springs (Hooke's law, nearest-neighbour coupling). The functionizer
-- compiles SIX coupled functions; the middle mass reads both its neighbours — open function_graph
-- for the dense coupling DAG (the most cross-edges of any sample).
fsm springs
    x1, x2, x3 ∈ Real
    v1, v2, v3 ∈ Real
    is_first_tick ⇒ (x1 = 10.0 ∧ x2 = 0.0 ∧ x3 = 0.0 ∧ v1 = 0.0 ∧ v2 = 0.0 ∧ v3 = 0.0)
    ¬is_first_tick ⇒ Δx1 = _v1 * 0.1
    ¬is_first_tick ⇒ Δx2 = _v2 * 0.1
    ¬is_first_tick ⇒ Δx3 = _v3 * 0.1
    ¬is_first_tick ⇒ Δv1 = (0.0 - _x1 * 2.0 + _x2) * 0.1
    ¬is_first_tick ⇒ Δv2 = (_x1 - _x2 * 2.0 + _x3) * 0.1
    ¬is_first_tick ⇒ Δv3 = (_x2 - _x3 * 2.0) * 0.1`,
  "thermostat · hysteresis (mode-switching guards)":
`-- A heater with hysteresis: temp rises while Heating, falls while Idle; the mode switches when temp
-- crosses 22 (→ Idle) or 18 (→ Heating). The functionizer compiles a 4-branch GUARDED mode function
-- and a per-mode temp function — open function_guards (and the ✓ total & unambiguous verdict).
enum Mode = Heating | Idle
fsm thermostat
    temp ∈ Real
    mode ∈ Mode
    is_first_tick ⇒ (temp = 15.0 ∧ mode = Heating)
    (¬is_first_tick ∧ _mode = Heating) ⇒ Δtemp = 1.0
    (¬is_first_tick ∧ _mode = Idle) ⇒ Δtemp = 0.0 - 0.5
    (¬is_first_tick ∧ _temp ≥ 22.0) ⇒ mode = Idle
    (¬is_first_tick ∧ _temp ≤ 18.0) ⇒ mode = Heating
    (¬is_first_tick ∧ 18.0 < _temp ∧ _temp < 22.0) ⇒ mode = _mode`,
  "DVD bounce · 4-wall (guard partition)":
`-- The bouncing-logo screensaver: position drifts, each velocity flips at its two walls. The
-- functionizer compiles 3-branch GUARDED velocity functions (in-bounds vs the two wall conditions)
-- — open function_behavior for the wall-flip partition map.
fsm dvd
    px, py, vx, vy ∈ Real
    is_first_tick ⇒ (px = 50.0 ∧ py = 30.0 ∧ vx = 3.0 ∧ vy = 2.0)
    ¬is_first_tick ⇒ Δpx = _vx
    ¬is_first_tick ⇒ Δpy = _vy
    (¬is_first_tick ∧ 0.0 < _px ∧ _px < 100.0) ⇒ vx = _vx
    (¬is_first_tick ∧ (_px ≤ 0.0 ∨ _px ≥ 100.0)) ⇒ vx = 0.0 - _vx
    (¬is_first_tick ∧ 0.0 < _py ∧ _py < 60.0) ⇒ vy = _vy
    (¬is_first_tick ∧ (_py ≤ 0.0 ∨ _py ≥ 60.0)) ⇒ vy = 0.0 - _vy`,
  "SIR epidemic · 3 coupled compartments":
`-- The SIR model: susceptibles get infected (the S·I product), infected recover. THREE coupled
-- functions — a driven cascade S→I→R with one product coupling. Open function_graph.
fsm sir
    s ∈ Real
    i ∈ Real
    r ∈ Real
    is_first_tick ⇒ (s = 99.0 ∧ i = 1.0 ∧ r = 0.0)
    ¬is_first_tick ⇒ Δs = 0.0 - _s * _i * 0.001
    ¬is_first_tick ⇒ Δi = _s * _i * 0.001 - _i * 0.05
    ¬is_first_tick ⇒ Δr = _i * 0.05`,
  "cruise control · PID loop (coupled feedback)":
`-- A speed controller: error = target − speed, an integral accumulates error, and speed responds.
-- A feedback LOOP (speed↔error↔integral) — open function_graph for the controller cycle.
fsm cruise
    speed ∈ Real
    error ∈ Real
    integ ∈ Real
    is_first_tick ⇒ (speed = 0.0 ∧ error = 0.0 ∧ integ = 0.0)
    ¬is_first_tick ⇒ error = 60.0 - _speed
    ¬is_first_tick ⇒ Δinteg = _error
    ¬is_first_tick ⇒ Δspeed = _error * 0.3 + _integ * 0.05`,
  "elevator · bouncing controller (deep dispatch)":
`-- An elevator that rides to the top then back to the bottom. The functionizer compiles 5-branch
-- GUARDED functions on (dir, floor) — open function_guards for the deep decision tree.
enum Dir = Up | Down
fsm elevator
    0 ≤ floor ∈ Int ≤ 3
    dir ∈ Dir
    is_first_tick ⇒ (floor = 0 ∧ dir = Up)
    (¬is_first_tick ∧ _dir = Up ∧ _floor < 3) ⇒ (floor = _floor + 1 ∧ dir = Up)
    (¬is_first_tick ∧ _dir = Up ∧ _floor = 3) ⇒ (floor = _floor ∧ dir = Down)
    (¬is_first_tick ∧ _dir = Down ∧ _floor > 0) ⇒ (floor = _floor - 1 ∧ dir = Down)
    (¬is_first_tick ∧ _dir = Down ∧ _floor = 0) ⇒ (floor = _floor ∧ dir = Up)`,
  "Collatz · 3n+1 (guarded integer map)":
`-- The Collatz map: n even → n/2, n odd → 3n+1 (even tested as n = 2·(n/2)). A clean 2-way guarded
-- integer function — open function_guards for the even/odd decision.
fsm collatz
    1 ≤ n ∈ Int ≤ 100000
    is_first_tick ⇒ n = 27
    (¬is_first_tick ∧ _n = 2 * (_n / 2)) ⇒ n = _n / 2
    (¬is_first_tick ∧ _n ≠ 2 * (_n / 2)) ⇒ n = 3 * _n + 1`,
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

    ¬is_first_tick ⇒
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
    timer ∈ Int
    is_first_tick ⇒ (light = Red ∧ timer = 0)
    ¬is_first_tick ⇒
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
    ¬is_first_tick ⇒
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
    ¬is_first_tick ⇒ x = _x + (40 - _x) / 4`,
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
    ¬is_first_tick ⇒
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
    ¬is_first_tick ⇒
        clk = ¬_clk
        clk2 = (¬_clk ? ¬_clk2 : _clk2)
        count = (_count ≥ 3 ? 0 : _count + 1)
        pulse = (¬_pulse ∧ _count ≥ 3)`,
};

// --- shared pure helper ------------------------------------------------------------
// escapeHtml lives here (the first-loaded file) so every later concern file can use it.
function escapeHtml(s) { return (s || "").replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;"); }
