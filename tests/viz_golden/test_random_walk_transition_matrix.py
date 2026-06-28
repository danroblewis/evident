"""Golden standard: (random_walk, transition_matrix).

The model (PATTERN.md):  fsm random_walk; x,y ∈ Int := 0; -1 ≤ Δx ≤ 1; -1 ≤ Δy ≤ 1.

EXPERT MATHEMATICS (verified against the transition with successors.one_step_successors):
  * The transition matrix is the state×state adjacency of the transition relation: cell (i,j) lit
    ⇔ state_i → state_j (docs/design/state-space-diagrams.md §2; the adjacency-matrix view of a
    transition system, Baier & Katoen ch. 2). Row i's out-degree = how many successors state_i has.
  * For this KING-MOVE walk every state has EXACTLY 9 successors, so the expert expects each
    (interior) ROW to have up to 9 lit cells — max row out-degree 9, NOT 1. The relation is
    translation-invariant, so that 9-stencil repeats down the diagonal band.
  * The walk is infinite-state (is_discrete()=False, full_state_graph empty), so the renderer
    samples a representative state grid and bins successors. A faithful sampling must still record
    that each sampled state maps to its ~9 neighbours; a matrix with EVERY row out-degree 1 has
    binned a SINGLE successor per state — collapsing the 9-way nondeterminism.

CURRENT STATUS (exposes the reported random_walk regression):
  transition_matrix uses the "sampled state grid" mode and produces a matrix whose every row has
  out-degree exactly 1 (it bins one successor per sampled state). So `_drawn_branching_reflects_nine`
  FAILS: the model branches 9 ways (the independent probe proves it) but the matrix captured 1.
  That FAIL is the signal — the matrix flattened the king-move fan to a single-successor map.
"""
from golden import Check, run_case
from successors import one_step_successors

SOURCE = "fsm random_walk\n    x, y ∈ Int := 0\n    -1 ≤ Δx ≤ 1\n    -1 ≤ Δy ≤ 1"


def _has_states(model, data):
    n = data["n_states"]
    return n >= 2, f"n_states={n}, mode={data['mode']!r} (a transition matrix needs ≥2 states)"


def _model_branches_nine(model, data):
    """GROUND TRUTH independent of the renderer: 9 one-step successors from the origin."""
    s = len(one_step_successors(model, {"x": 0, "y": 0}))
    return s == 9, f"the transition has {s} one-step successors from the origin (expected 9)"


def _drawn_branching_reflects_nine(model, data):
    """Some matrix ROW must have out-degree up to 9 — the matrix should capture the king-move fan,
    not a single successor per state."""
    mx = data["max_out_degree"]
    ok = mx >= 9
    why = "captures the 9-way king-move branching"
    if mx == 1:
        why = "REGRESSED: every row has out-degree 1 — the matrix binned ONE successor per state, " \
              "collapsing the 9-way nondeterminism"
    elif 1 < mx < 9:
        why = f"only {mx}-way branching captured — short of the 9-way king-move stencil"
    return ok, f"max row out-degree = {mx}; {why}. out_degree_hist={data['out_degree_hist']}"


CHECKS = [
    Check("matrix has ≥2 states", _has_states),
    Check("the transition branches 9 ways (king-move ground truth)", _model_branches_nine),
    Check("the MATRIX reflects 9-way branching (some row out-degree ≥9)", _drawn_branching_reflects_nine),
]


def case():
    # transition_matrix's render() takes a MODEL (render(m, out_path)); the IDE adapter passes one.
    # run_case drives it through the IDE contract — _render_via_model handles the model-taking shape.
    return run_case("random_walk", SOURCE, "transition_matrix", CHECKS)
