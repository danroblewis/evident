"""Analyze-support helpers: parse-error location, the dropped-constraint
source locator, and the model-shape banner + lead-view recommendation.

Pure-ish: no subprocess, no FastAPI. The `_banner`/`_recommend` pair reads the
loaded model's functional-dependency analysis and reachable-graph facts; the
`_dropped_*`/`_idents`/`_error_loc` cluster turns runtime warning text back into
1-based source line numbers so the editor can tint where the silent bug was
written.
"""
import re
from collections import Counter

import networkx as nx  # SCC detection for the cyclic-vs-terminating banner

_LOC_RE = re.compile(r"\bline (\d+), col (\d+)\b")


def _reachable_stats(m, limit):
    """Explore the model's reachable graph (bounded by `limit`) and summarize it for the
    analyze response: returns (states, edges, n_states, n_edges, max_branch, capped, recurrent).
    `max_branch` is the largest out-degree; `recurrent` is the largest SCC size — ≥2
    distinguishes eventually-periodic (vending) from a terminating-driven chain (counter),
    which the banner must not flatten; `capped` flags that the reachable set didn't fit `limit`."""
    states, edges = m.reachable(limit=limit)
    n_states, n_edges = len(states), len(edges)
    out_deg = Counter(src for src, _ in edges)
    max_branch = max(out_deg.values()) if out_deg else 1
    capped = n_states >= limit
    recurrent = 1
    if edges:
        g = nx.DiGraph(); g.add_edges_from(edges)
        recurrent = max((len(c) for c in nx.strongly_connected_components(g)), default=1)
    return states, edges, n_states, n_edges, max_branch, capped, recurrent


def _model_diff(model_a, model_b, limit, cap=40):
    """The relational analog of a text diff: align the reachable sets of two models that
    share a carried-var set, and report which states APPEARED (in B not A), VANISHED (in A
    not B), and the COMMON count. States are aligned by `state_key` — the same identity the
    reachable graph dedups on — so the delta is on the *relation*, not on source text.

    Returns a dict ready to serialize: {ok, vars, appeared, vanished, common, a_total,
    b_total, appeared_truncated, vanished_truncated}. When A and B carry different var sets,
    returns {ok:False, error:…} — aligning by key across mismatched vars is meaningless."""
    a_names, b_names = model_a.carried_names(), model_b.carried_names()
    if a_names != b_names:
        fmt = lambda s: "{" + ", ".join(sorted(n.split(".")[-1] for n in s)) + "}"
        return {"ok": False,
                "error": f"diff needs the same variables — A has {fmt(a_names)}, "
                         f"B has {fmt(b_names)}"}
    states_a, edges_a = model_a.reachable(limit=limit)
    states_b, edges_b = model_b.reachable(limit=limit)
    by_key_a = {model_a.state_key(s): s for s in states_a}
    by_key_b = {model_b.state_key(s): s for s in states_b}
    keys_a, keys_b = set(by_key_a), set(by_key_b)
    appeared = [by_key_b[k] for k in keys_b - keys_a]
    vanished = [by_key_a[k] for k in keys_a - keys_b]
    # Edge delta: an edge is keyed (src_state_key, dst_state_key). This catches a CHANGED RELATION
    # even when the state set is identical — rewire a guard so Green→Red becomes Green→Yellow and the
    # states are unchanged but the transitions differ (Marek #232).
    def _edge_map(model, states, edges):
        sk = [model.state_key(s) for s in states]
        return {(sk[i], sk[j]): (states[i], states[j]) for i, j in edges}
    em_a, em_b = _edge_map(model_a, states_a, edges_a), _edge_map(model_b, states_b, edges_b)
    ek_a, ek_b = set(em_a), set(em_b)
    appeared_edges = [{"src": em_b[k][0], "dst": em_b[k][1]} for k in ek_b - ek_a]
    vanished_edges = [{"src": em_a[k][0], "dst": em_a[k][1]} for k in ek_a - ek_b]
    short = sorted(n.split(".")[-1] for n in a_names)
    return {
        "ok": True,
        "vars": short,
        "appeared": appeared[:cap],
        "vanished": vanished[:cap],
        "common": len(keys_a & keys_b),
        "a_total": len(keys_a),
        "b_total": len(keys_b),
        "appeared_truncated": len(appeared) > cap,
        "vanished_truncated": len(vanished) > cap,
        "appeared_edges": appeared_edges[:cap],
        "vanished_edges": vanished_edges[:cap],
        "common_edges": len(ek_a & ek_b),
        "a_edges": len(ek_a),
        "b_edges": len(ek_b),
        "edges_appeared_truncated": len(appeared_edges) > cap,
        "edges_vanished_truncated": len(vanished_edges) > cap,
    }


def _error_loc(msg: str):
    """Pull a 1-based (line, col) out of a parse/lex error message — the runtime
    formats them as 'parse error at line N, col N: …'. Returns None when absent."""
    m = _LOC_RE.search(msg or "")
    return {"line": int(m.group(1)), "col": int(m.group(2))} if m else None


# A dropped-constraint warning carries the DESUGARED expr, not a source line — the
# runtime can't cheaply thread a line through BodyItem::Constraint (huge match-site
# blast radius). So locate it in the source by token overlap: tokenize the dropped
# pretty-text and each source line into identifiers, and pick the line that shares the
# most DISTINCTIVE identifiers (rarer in the source ⇒ heavier — the typo'd/undeclared
# name like `stp` is what pins the right line). Robust for the common case.
_DROP_RE = re.compile(r"dropped constraint \(couldn't translate to Bool\):\s*(.+)$")
_IDENT_RE = re.compile(r"[A-Za-z_]\w*")
# desugaring noise that doesn't map back to a distinctive source token
_IDENT_STOP = {"is_first_tick"}


def _idents(text: str):
    """Identifiers in `text`, lowered to their source form: a desugared `_count`
    prev-read traces back to the carried name `count`, so strip a single leading `_`."""
    out = set()
    for tok in _IDENT_RE.findall(text or ""):
        bare = tok[1:] if tok.startswith("_") and len(tok) > 1 else tok
        if bare and bare not in _IDENT_STOP:
            out.add(bare)
    return out


def _dropped_pretties(msg: str):
    """The desugared expr text of each dropped-constraint warning line."""
    return [m.group(1).strip()
            for ln in (msg or "").splitlines()
            if (m := _DROP_RE.search(ln))]


def _dropped_locs(source: str, msg: str):
    """1-based source line numbers for each dropped constraint, by distinctive-token
    overlap. Frequency-weight each shared identifier by 1/(source occurrences) so the
    rarest name (the typo) dominates; ties keep the first (earliest) line."""
    pretties = _dropped_pretties(msg)
    if not pretties:
        return []
    lines = (source or "").splitlines()
    line_idents = [_idents(ln) for ln in lines]
    freq = Counter(name for s in line_idents for name in s)
    locs = []
    for pretty in pretties:
        want = _idents(pretty)
        best_n, best_score = None, 0.0
        for i, have in enumerate(line_idents):
            shared = want & have
            if not shared:
                continue
            score = sum(1.0 / freq[name] for name in shared)
            if score > best_score + 1e-9:
                best_n, best_score = i + 1, score
        if best_n is not None:
            locs.append(best_n)
    return locs


def _banner(m, max_branch=1, recurrent=1, states=None):
    """The model-shape line, from the functional-dependency analysis. Two reachable-graph
    facts override the dependency verdict: BRANCHING (a state with ≥2 successors is
    nondeterministic no matter what), and a RECURRENT cycle (a ≥2-state SCC is
    eventually-periodic, not a terminating chain — so the banner must say 'cyclic')."""
    try:
        ind = m.independence(states=states)      # reuse the analyze's reachable sample (#217)
    except Exception:
        return "model shape: (unavailable)"
    short = lambda n: n.split(".")[-1]
    if max_branch >= 2:
        drv = ind.get("driver")
        hint = f"; candidate driver of the deterministic part: {short(drv)}" if drv else ""
        return (f"Nondeterministic — up to {max_branch} successors from some state "
                f"(a free choice fans out){hint}")
    if ind["verdict"] == "driven" and ind.get("driver"):
        drv = short(ind["driver"])
        deps = [short(d) for d in ind.get("dependents", [])[:4]]
        if deps:
            return (f"Driven pipeline — independent variable: {drv}"
                    f" — computed from it: {', '.join(deps)}")
        if recurrent >= 2:
            return (f"Cyclic — {drv} cycles through a recurrent loop of {recurrent} states "
                    f"(eventually periodic, no fixpoint)")
        return f"Driven — {drv} advances on its own clock (a deterministic recurrence)"
    if ind["verdict"] == "nondeterministic":
        return "Nondeterministic — the free choice is the input, not a state variable"
    # A relational (no single driver) machine whose reachable graph has a real recurrent
    # SCC is a CYCLE, not just a tangle: the variables co-determine in a loop and the orbit
    # eventually repeats. Say 'cyclic' (traffic: light+timer recur every N ticks) rather than
    # the static 'genuinely relational' phrasing, which read as terminating.
    if recurrent >= 2:
        return (f"Cyclic — {recurrent} states recur; the variables co-determine in a loop "
                f"(eventually periodic, no fixpoint)")
    return "Genuinely relational — no independent variable (a cycle; every variable co-determines)"


def _recommend(m, n_states, max_branch, discrete, views):
    """Pick the lead view from the model's shape:
      - a SMALL DISCRETE machine → state_graph: it draws the whole structure at once —
        branch out-edges AND back-edges/cycles — which a tree would hide and a noodle
        would bury. (A 3-state vending loop reads as a loop here, not a fanned tree.)
      - otherwise, any BRANCHING (some state has ≥2 successors) → reachability_tree, so the
        fan is visible where the full graph would be an unreadable noodle. Keyed on the
        branching factor (not edge count) so it still fires when a large reachable set hits
        the exploration cap (n_edges ≈ n_states).
      - otherwise, a DETERMINISTIC system with ≥2 interacting NUMERIC variables →
        phase_portrait: the compelling view is the orbit in (var₁, var₂) space, not a pair
        of separate time-series lines. The oscillator spirals in (pos, vel); a time series
        would split that single trajectory across two flat plots and hide the spiral. Gated
        on ¬discrete (a tiny discrete machine reads as state_graph above) and on the
        deterministic path (max_branch < 2) so the genuinely-branching numeric systems —
        vending, pick — still go to reachability_tree above, not here.
      - otherwise the time series: a deterministic numeric ramp/trajectory reads as a clean
        line, faithful and fast for almost everything.

      BUT lead with solution_space whenever there's a numeric variable: the DEFAULT picture
      should be the BOUNDARY of what the variables can be (the solved range of each var + the
      feasible set + fixed points), not one trajectory through it. The dynamics views are one
      tab click away. (Purely categorical machines have no numeric boundary, so they fall
      through to state_graph below.)"""
    # `not discrete` ⟺ ≥1 numeric var, and `n_numeric` counts them — both from interface-var KINDS
    # (cheap), NOT the ranked `numeric_vars`/`state_vars` property, which RE-SAMPLES to order vars by
    # variation and cost ~830ms on a real-valued model. The lead-view pick needs the kinds, not the
    # ranking, so this alone roughly halves real-valued analyze latency (Ana #217). The ranking still
    # happens lazily for renderers that actually need axis order.
    n_numeric = sum(1 for v in m.interface_vars if v["kind"] not in ("bool", "enum", "string"))
    if "solution_space" in views and n_numeric:
        return "solution_space"
    if "state_graph" in views and discrete and n_states <= 30:
        return "state_graph"
    if "reachability_tree" in views and max_branch >= 2:
        return "reachability_tree"
    if "phase_portrait" in views and not discrete and n_numeric >= 2:
        return "phase_portrait"
    return "time_series" if "time_series" in views else (views[0] if views else None)
