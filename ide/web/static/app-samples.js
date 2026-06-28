"use strict";

// ==============================================================================
// app-samples.js — the SOLVE / claim worked-example programs + the larger dynamics
// demos (and the maze, #475), Object.assign'd onto the SAMPLES manifest defined in
// app-data.js. Split out to keep both files under the CLAUDE.md ≤500-line convention
// (the sample table outgrew one file once the 148-line maze was embedded). Loaded
// AFTER app-data.js (SAMPLES must exist) and before app.js. Each entry: a dropdown
// KEY → the .ev SOURCE (verbatim from examples/ for the worked examples).
// ==============================================================================

Object.assign(SAMPLES, {
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
    cells ∈ Seq(Int) := ⟨0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0⟩
    #cells = 11
    ∀ i ∈ {1..9} : cells[i] = ((_cells[i-1] + _cells[i+1]) = 1 ? 1 : 0)
    (cells[0] = 0 ∧ cells[10] = 0)`,
  "bistable · two basins of attraction (FSM, basin_map)":
`-- A random walk between two absorbing walls at 0 and 6 (gambler's ruin).
-- Each tick a free step ±1, unless already at a wall, where it sticks. From the
-- middle the walk can end at EITHER wall, so the reachable graph has two terminal
-- states. Open basin_map: it colors each reachable state by the wall it falls to.
fsm bistable
    x ∈ Int := 3
    step ∈ Int
    -1 ≤ step ≤ 1
    0 ≤ x
    x ≤ 6
    Δx = (_x = 0 ? 0 : (_x = 6 ? 0 : step))`,
  "fixed point · a 1-D map's staircase (FSM, cobweb)":
`-- A 1-D contraction map: each tick x moves a quarter of the way to 40.
-- It converges monotonically to the fixed point. Open the cobweb view: the
-- red staircase climbs from the seed to where the map line meets y = x.
fsm fixedpoint
    x ∈ Int := 4
    x = _x + (40 - _x) / 4`,
  "four signals · a 4-variable system (FSM, scatter_matrix)":
`-- Four genuinely-carried sawtooths on coprime periods (11, 5, 7, 3). Each pair
-- sweeps a different lattice. Open scatter_matrix: every pairwise plane at once,
-- with each variable's distribution on the diagonal. (parallel_coords also fits.)
fsm fourvar
    a, b, c, d ∈ Int := 0
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
    clk, clk2 ∈ Bool := false
    count ∈ Int := 0
    pulse ∈ Bool := false
    clk = ¬_clk
    clk2 = (¬_clk ? ¬_clk2 : _clk2)
    count = (_count ≥ 3 ? 0 : _count + 1)
    pulse = (¬_pulse ∧ _count ≥ 3)`,
  "maze · constraint-solved pathfinding (no algorithm — the solver finds the path)":
`-- 24_maze — a constraint-solved MAZE. The walls are FIXED data; the
-- player's path is what the SOLVER finds.
--
-- This is the constraint-programming pitch in one file: you state the
-- maze (a grid + a Seq of wall cells) and the rules of a legal walk
-- (stay in bounds, never step on a wall, only move to a grid-adjacent
-- cell, make progress toward the goal), and Z3 produces a satisfying
-- path. We never write the path; we constrain it and read it back.
--
-- Two halves, the usual demo shape:
--
--   * fsm \`solve\` — the runnable trajectory. It carries \`pos ∈ IVec2\`
--     tick-to-tick; each tick the solver picks a legal next cell that
--     is strictly closer to the goal. Because BOTH the walls (carried
--     as a Seq in the model) and the moving \`pos\` are state the run
--     exposes, the solution_space / occupancy_heatmap diagrams show
--     the maze AND the player threading through it.
--
--   * static \`sat_*\` / \`unsat_*\` claims — the pure constraint problem,
--     no ticks: a fixed-length \`path ∈ Seq(IVec2)\` whose interior cells
--     the solver fills in. sat: a path exists for the open maze. unsat:
--     a wall dropped into the only corridor makes the goal unreachable.
--
-- Idioms (CLAUDE.md): cells are a record-as-vector \`IVec2\`, NOT parallel
-- x/y Seqs; walls are a single \`Seq(IVec2)\` (one cell per element, no
-- pairing to drift); every rule is element-iteration (\`∀ c ∈ path\`,
-- \`∀ w ∈ walls\`, \`edges(path)\`), never a \`{0..n-1}\` index loop;
-- adjacency is manhattan distance 1 via \`abs\`.

import "stdlib/runtime.ev"

type IVec2(x, y ∈ Int)

-- ── The maze ─────────────────────────────────────────────────
-- A 3×3 grid. Walls block the middle column's lower two cells,
-- (1,0) and (1,1), so the only way from (0,0) to (2,2) is up the
-- left edge, across the top, and down — an L around the barrier.
--
--   y=2  . . G        '.' open   'G' goal
--   y=1  . # .        '#' wall   'S' start
--   y=0  S # .
--        x: 0 1 2

-- A legal walk over \`cells\` against \`walls\`, on a \`size\`×\`size\` grid:
-- in bounds, off every wall, each consecutive pair grid-adjacent.
claim LegalWalk(cells ∈ Seq, walls ∈ Seq, size ∈ Int)
    ∀ c ∈ cells : (0 ≤ c.x ∧ c.x ≤ size - 1 ∧ 0 ≤ c.y ∧ c.y ≤ size - 1)
    ∀ c ∈ cells : (∀ w ∈ walls : c ≠ w)
    ∀ (a, b) ∈ edges(cells) : abs(a.x - b.x) + abs(a.y - b.y) = 1

-- ── The runnable solver: walk the maze, one solved step per tick ──
fsm solve
    goal ∈ IVec2 = IVec2(2, 2)

    walls ∈ Seq(IVec2)
    #walls = 2
    walls[0] = IVec2(1, 0)
    walls[1] = IVec2(1, 1)

    pos ∈ IVec2
    is_first_tick ⇒ pos = IVec2(0, 0)

    -- in bounds, never on a wall (holds every tick, including the seed)
    0 ≤ pos.x ∧ pos.x ≤ 2 ∧ 0 ≤ pos.y ∧ pos.y ≤ 2
    ∀ w ∈ walls : pos ≠ w

    at_goal ∈ Bool = (pos.x = goal.x ∧ pos.y = goal.y)

    -- manhattan distance to the goal, this tick and last tick
    dist      ∈ Int = abs(pos.x  - goal.x) + abs(pos.y  - goal.y)
    prev_dist ∈ Int = abs(_pos.x - goal.x) + abs(_pos.y - goal.y)
    was_at_goal ∈ Bool = (_pos.x = goal.x ∧ _pos.y = goal.y)

    -- Off the goal: step to a grid-adjacent cell strictly closer to it.
    -- The solver picks WHICH adjacent cell — that's the pathfinding.
    (¬is_first_tick ∧ ¬was_at_goal) ⇒
        (abs(pos.x - _pos.x) + abs(pos.y - _pos.y) = 1 ∧ dist = prev_dist - 1)
    -- At the goal: stay put (so the run reaches a fixpoint and halts).
    (¬is_first_tick ∧ was_at_goal) ⇒ (pos.x = _pos.x ∧ pos.y = _pos.y)

    sx ∈ String = to_str(pos.x)
    sy ∈ String = to_str(pos.y)
    here ∈ String = "(" ++ sx ++ "," ++ sy ++ ")"
    effects = (at_goal
        ? ⟨Println("reached goal at " ++ here), Exit(0)⟩
        : ⟨Println("at " ++ here)⟩)

-- ── Static tests: the maze as a pure constraint problem ──────────

-- SAT: the open maze admits an L-shaped 5-cell path around the wall
-- column. The solver fills in path[1..3]; we pin only start and goal.
claim sat_open_maze_has_path
    path ∈ Seq(IVec2)
    walls ∈ Seq(IVec2)
    #path = 5
    #walls = 2
    path[0] = IVec2(0, 0)
    path[4] = IVec2(2, 2)
    walls[0] = IVec2(1, 0)
    walls[1] = IVec2(1, 1)
    LegalWalk(path, walls, 3)

-- UNSAT: a 1×5 corridor (cells (0,0)..(0,4)) with a wall dropped at
-- (0,2) — the single chokepoint. No legal walk from (0,0) to (0,4)
-- can avoid it, so the solver reports no path exists.
claim unsat_wall_blocks_corridor
    path ∈ Seq(IVec2)
    walls ∈ Seq(IVec2)
    #path = 5
    #walls = 1
    path[0] = IVec2(0, 0)
    path[4] = IVec2(0, 4)
    walls[0] = IVec2(0, 2)
    -- 1-wide corridor: x pinned to 0, y free in 0..4
    ∀ c ∈ path : (c.x = 0 ∧ 0 ≤ c.y ∧ c.y ≤ 4)
    ∀ c ∈ path : (∀ w ∈ walls : c ≠ w)
    ∀ (a, b) ∈ edges(path) : abs(a.x - b.x) + abs(a.y - b.y) = 1

-- SAT: the same corridor with NO wall — the straight walk exists.
-- (Pins the contrast: it's the wall, not the geometry, that blocks.)
claim sat_open_corridor
    path ∈ Seq(IVec2)
    walls ∈ Seq(IVec2)
    #path = 5
    #walls = 0
    path[0] = IVec2(0, 0)
    path[4] = IVec2(0, 4)
    LegalWalk(path, walls, 5)

-- SAT: an open cell is off every wall. (0,1) is open in the 3×3 maze,
-- so it satisfies the off-every-wall rule — a direct check that the
-- wall set actually constrains.
claim sat_open_cell_off_walls
    walls ∈ Seq(IVec2)
    #walls = 2
    walls[0] = IVec2(1, 0)
    walls[1] = IVec2(1, 1)
    c ∈ IVec2 = IVec2(0, 1)
    ∀ w ∈ walls : c ≠ w

-- UNSAT: the mirror — a cell pinned ON a wall cannot be off every wall.
claim unsat_cell_on_wall
    walls ∈ Seq(IVec2)
    #walls = 2
    walls[0] = IVec2(1, 0)
    walls[1] = IVec2(1, 1)
    c ∈ IVec2 = IVec2(1, 1)
    ∀ w ∈ walls : c ≠ w
`,
});
