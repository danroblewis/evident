"""Golden standard: (random_walk, reachability_tree).

The model (the 2-D nondeterministic KING-MOVE walk):

    fsm random_walk
        x, y ∈ Int := 0
        -1 ≤ Δx ≤ 1
        -1 ≤ Δy ≤ 1

EXPERT MATHEMATICS (derived, then VERIFIED against the transition with an independent one-step probe
— never read off current renderer output):

  * A reachability tree is a BFS unrolling from the initial condition; each state edges to its
    newly-discovered successors, so a node's BRANCHING FACTOR is the count of its distinct successors
    (docs/design/state-space-diagrams.md: "BFS unrolling from init ... a branching fan").
  * Δx and Δy are chosen INDEPENDENTLY and SIMULTANEOUSLY each tick, each in {-1, 0, +1}. So EVERY
    state has exactly 9 one-step successors — the full Moore / KING-MOVE neighbourhood (8 neighbours
    + the stay-put (0,0) step). The independent probe `_one_step_successors` confirms the origin has
    these 9: {(-1,-1),(-1,0),(-1,1),(0,-1),(0,0),(0,1),(1,-1),(1,0),(1,1)}.
  * The renderer DROPS the stay-put self-loop (it isn't a tree edge), so in the tree the king-move fan
    shows as BRANCHING 8 — NOT 4. A max branching of ≤4 would mean the king-move walk had collapsed to
    a 4-neighbour (von Neumann) walk: a real REGRESSION this golden test must expose.
  * The tree is ROOTED at the model's initial condition — the ORIGIN (x=y=0, from `:= 0`). The fan
    grows symmetrically outward from there; depth = shortest distance from the root.

These expectations come from the math, not the output. Where current output VIOLATES them, the check
FAILS — and that failure is the signal (the user reports random_walk's diagrams regressed).
"""
import z3

from golden import Check, run_case

SOURCE = "fsm random_walk\n    x, y ∈ Int := 0\n    -1 ≤ Δx ≤ 1\n    -1 ≤ Δy ≤ 1"


# ---- independent transition oracle: the one-step successor set of a given (x,y) ----

def _one_step_successors(model, X, Y):
    """The EXACT set of distinct (x', y') reachable in ONE tick from (x=X, y=Y), via block-and-resolve
    over the ¬is_first_tick transition (the renderer-independent ground truth). Pins the previous
    tick to (X, Y) and enumerates all next states."""
    body = z3.And(*model.assertions) if len(model.assertions) != 1 else model.assertions[0]
    s = z3.Solver()
    s.add(body)
    if model.first_tick is not None:
        s.add(model.first_tick == False)                       # noqa: E712 — the step relation
    px, py = model.consts["_x"], model.consts["_y"]
    s.add(px == X, py == Y)
    nx_, ny_ = model.consts["x"], model.consts["y"]
    out = set()
    while s.check() == z3.sat:
        mdl = s.model()
        vx, vy = mdl.eval(nx_).as_long(), mdl.eval(ny_).as_long()
        out.add((vx, vy))
        s.add(z3.Or(nx_ != vx, ny_ != vy))
    return out


# ---- expert expectations over the renderer's .data.json (+ the transition oracle) ----

def _is_a_branching_tree(model, data):
    ok = data["n_nodes"] > 1 and data["max_depth"] >= 1
    return ok, f"n_nodes={data['n_nodes']}, max_depth={data['max_depth']} (a tree must branch out from the root)"


def _origin_has_nine_successors(model, data):
    """THE LOAD-BEARING math check, probed directly on the transition (not on .data.json): the origin
    has exactly the 9 king-move successors. Asserting 5 (von-Neumann + stay) or 9 distinguishes this
    model's true neighbourhood; it is the ground truth the tree's fan must match."""
    succ = _one_step_successors(model, 0, 0)
    king = {(dx, dy) for dx in (-1, 0, 1) for dy in (-1, 0, 1)}
    ok = succ == king
    return ok, f"origin one-step successors = {sorted(succ)} (expected the 9 king-move steps)"


def _fan_is_eight_not_four(model, data):
    """The tree's max branching factor must be 8 (the 9 king-move successors minus the dropped
    stay-put self-loop) — NOT 4. A max ≤4 means the king-move fan collapsed to a 4-neighbour walk."""
    mb = data["max_branching"]
    ok = mb == 8
    return ok, (f"max_branching={mb}, distinct_out_degrees={data['distinct_out_degrees']} "
                f"(expected 8 = 9 king-move successors − the dropped stay-put self-loop; ≤4 = collapsed)")


def _rooted_at_origin(model, data):
    """The tree must be rooted at the model's initial condition — the ORIGIN (x=y=0). A root anywhere
    else means the seed-picker abandoned the program's init (e.g. mis-classifying the stay-put
    successor as a fixed point and falling back to a hardcoded grid seed)."""
    root = data.get("root_state")
    ok = root is not None and root.get("x") == 0 and root.get("y") == 0
    return ok, f"root_state={root} (expected the origin {{x:0, y:0}} — the model's `:= 0` init)"


def _root_matches_model_initial_state(model, data):
    """Cross-check the data's root against the model's OWN initial_state() — they must agree (the tree
    roots where the program says it starts)."""
    init = model.initial_state()
    root = data.get("root_state")
    ok = root is not None and init is not None and \
        root.get("x") == init.get("x") and root.get("y") == init.get("y")
    return ok, f"root_state={root} vs model.initial_state()={init} (must match)"


CHECKS = [
    Check("is a branching tree (>1 node, depth ≥1)", _is_a_branching_tree),
    Check("origin has the 9 king-move successors (transition oracle)", _origin_has_nine_successors),
    Check("tree fan is 8-wide, not collapsed to 4", _fan_is_eight_not_four),
    Check("tree is rooted at the origin (the `:= 0` init)", _rooted_at_origin),
    Check("root matches model.initial_state()", _root_matches_model_initial_state),
]


def case():
    """The golden case record. reachability_tree takes NO axes — run_case falls back to the 3-arg
    render() call automatically when the x_var/y_var kwargs raise TypeError."""
    return run_case("random_walk", SOURCE, "reachability_tree", CHECKS)
