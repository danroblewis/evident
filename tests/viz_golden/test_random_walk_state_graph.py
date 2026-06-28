"""Golden standard: (random_walk, state_graph).

The model (PATTERN.md):  fsm random_walk; x,y ∈ Int := 0; -1 ≤ Δx ≤ 1; -1 ≤ Δy ≤ 1.

EXPERT MATHEMATICS (verified against the transition with successors.one_step_successors):
  * The state_graph draws the transition relation made literal — nodes = state vectors, edges =
    transitions (docs/design/state-space-diagrams.md §2, "the transition relation made literal";
    Baier & Katoen, Principles of Model Checking, ch. 2 — the reachability/transition graph).
  * For this KING-MOVE walk every state has EXACTLY 9 successors (Δx,Δy each ∈ {-1,0,+1}, incl.
    stay-put), and the relation is TRANSLATION-INVARIANT, so an interior node's out-degree is 9
    everywhere. The expert therefore expects a BRANCHING FAN: many nodes, a max out-degree of 9
    (NOT 1 and NOT 4 — a 4-cap would mean the king-move walk was drawn as a 4-neighbour walk).
  * The walk is infinite-state, so the renderer must draw a BOUNDED reachable window (a BFS unroll
    from the origin caps at a node limit). That window is still a genuine fan — its frontier nodes
    branch 9 ways. A graph of ≤1 node is a DEGENERATE collapse, not an honest finite window.

CURRENT STATUS (the user reported random_walk regressed — these checks EXPOSE it):
  state_graph falls into "deterministic run (trajectory)" mode → ONE node with a self-loop, because
  build_reachable_graph() returns None for the unbounded walk and the trajectory of a
  nondeterministic walk collapses to a single state. So the branching-fan checks FAIL: the model
  CAN branch 9 ways (the independent probe proves it), but the renderer drew a 1-node dot. THAT
  failure is the signal — the diagram regressed from a fan to a point.
"""
from golden import Check, run_case
from successors import one_step_successors

SOURCE = "fsm random_walk\n    x, y ∈ Int := 0\n    -1 ≤ Δx ≤ 1\n    -1 ≤ Δy ≤ 1"


def _is_a_graph(model, data):
    n = data["n_nodes"]
    ok = n > 1 and not data["degenerate"]
    return ok, f"n_nodes={n}, degenerate={data['degenerate']} (a state graph must have >1 node — " \
               f"mode drawn: {data['mode']!r})"


def _model_branches_nine(model, data):
    """GROUND TRUTH (independent of the renderer): the transition itself yields 9 successors from
    the origin — the king-move stencil. This is what the graph SHOULD reflect."""
    s = len(one_step_successors(model, {"x": 0, "y": 0}))
    return s == 9, f"the transition has {s} one-step successors from the origin (expected 9 — " \
                   f"Δx,Δy ∈ {{-1,0,+1}} independent)"


def _drawn_branching_is_nine(model, data):
    """The DRAWN graph must reflect that 9-way branching: some node has out-degree 9 (NOT 4 — a
    4-cap is the king-move→4-neighbour regression; NOT 1 — a 1-cap is the trajectory collapse)."""
    mx = data["max_out_degree"]
    ok = mx == 9
    why = "matches the 9-way king-move stencil"
    if mx == 1:
        why = "REGRESSED: drawn as a single trajectory/self-loop (the fan collapsed to a point)"
    elif mx == 4:
        why = "REGRESSED: drawn as a 4-neighbour walk (lost the diagonal king moves)"
    elif mx == 0:
        why = "no edges drawn"
    return ok, f"max_out_degree drawn = {mx}; {why}. out_degree_hist={data['out_degree_hist']}"


def _translation_invariant(model, data):
    """A second state branches 9 ways too — the stencil is the same everywhere (translation-
    invariant), so the fan isn't a one-off origin artifact."""
    s = len(one_step_successors(model, {"x": 5, "y": -3}))
    return s == 9, f"the transition has {s} successors from (5,-3) as well (translation-invariant)"


CHECKS = [
    Check("state_graph is an actual graph (>1 node, not degenerate)", _is_a_graph),
    Check("the transition branches 9 ways (king-move ground truth)", _model_branches_nine),
    Check("the DRAWN graph reflects 9-way branching (not 1, not 4)", _drawn_branching_is_nine),
    Check("branching is translation-invariant (9 from another state too)", _translation_invariant),
]


def case():
    # state_graph's render() signature is render(smt2, schema, out, all_conditions=None) — it does
    # NOT take x_var/y_var, so run_case falls back to the 3-arg IDE call automatically.
    return run_case("random_walk", SOURCE, "state_graph", CHECKS)
