#!/usr/bin/env python3
"""render_cobweb.py — classic 1-D map cobweb plot for any Evident IR.

Usage:
    python3 viz/render_cobweb.py <smt2> <schema> <out_path>

Picks the primary scalar (int) state var, samples the transition's successor
over a grid of pinned values of that var (holding the other vars at the initial
state, or a sensible default), plots x_n vs x_{n+1}, draws the y=x diagonal, and
staircases an orbit from a seed.

For non-numeric programs (no int var) we project the primary enum to its ordinal
and cobweb that; if nothing projects we emit a titled N/A placeholder. The
dynamics ALWAYS come from querying m.successor(), never hardcoded.
"""
import sys

import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt

sys.path.insert(0, "viz")
from evident_viz import load


def _pick_primary(m):
    """Return (var, mode) where mode in {'int','enum-ordinal'} or (None, None)."""
    for v in m.state_vars:
        if v["kind"] == "int":
            return v, "int"
    for v in m.state_vars:
        if v["kind"] == "enum":
            return v, "enum-ordinal"
    return None, None


def _base_state(m):
    """A starting state dict to hold the non-primary vars fixed."""
    init = m.initial_state()
    if init is not None:
        return init
    # Fabricate a neutral state from each var's first/default value.
    state = {}
    for v in m.state_vars:
        if v["kind"] == "int":
            state[v["name"]] = 0
        elif v["kind"] == "bool":
            state[v["name"]] = False
        elif v["kind"] == "enum":
            state[v["name"]] = m.enum_variants[v["name"]][0]
        elif v["kind"] == "real":
            state[v["name"]] = 0.0
        else:
            state[v["name"]] = ""
    return state


def _to_ord(m, var, value):
    if var["kind"] == "enum":
        return m.enum_variants[var["name"]].index(value)
    return value


def _from_ord(m, var, o):
    if var["kind"] == "enum":
        variants = m.enum_variants[var["name"]]
        o = max(0, min(len(variants) - 1, int(round(o))))
        return variants[o]
    return int(round(o))


def _sample_map(m, var, mode, base):
    """Sample x_{n+1} = f(x_n) for x_n over the var's range. Returns (xs, ys)
    as parallel lists in ordinal space (== value for int)."""
    name = var["name"]
    if mode == "enum-ordinal":
        grid = list(range(len(m.enum_variants[name])))
    else:
        # Span the seed range generously for numeric fixed-point systems.
        lo, hi, n = -3200, 3200, 121
        grid = [lo + (hi - lo) * i // (n - 1) for i in range(n)]

    xs, ys = [], []
    for x in grid:
        state = dict(base)
        state[name] = _from_ord(m, var, x)
        nxt = m.successor(state)
        if nxt is None:
            continue
        xs.append(x)
        ys.append(_to_ord(m, var, nxt[name]))
    return xs, ys


def _staircase(m, var, mode, base, seed, steps=40):
    """Build a cobweb staircase orbit: (x0,0)->(x0,f(x0))->(f(x0),f(x0))->..."""
    name = var["name"]
    px, py = [], []
    x = seed
    cur = _to_ord(m, var, m.successor({**base, name: _from_ord(m, var, x)})[name]) \
        if m.successor({**base, name: _from_ord(m, var, x)}) is not None else None
    # Reset: walk forward step by step, drawing vertical then horizontal segs.
    x = seed
    px.append(x); py.append(x)  # start on the diagonal
    seen = set()
    for _ in range(steps):
        state = {**base, name: _from_ord(m, var, x)}
        nxt = m.successor(state)
        if nxt is None:
            break
        y = _to_ord(m, var, nxt[name])
        px.append(x); py.append(y)   # vertical to the map
        px.append(y); py.append(y)   # horizontal to the diagonal
        key = round(y, 6)
        if key in seen:
            break
        seen.add(key)
        x = y
    return px, py


def render(smt2, schema, out_path):
    m = load(smt2, schema)
    var, mode = _pick_primary(m)

    fig, ax = plt.subplots(figsize=(7.5, 7.5))
    title = f"{m.fsm}  —  cobweb"

    if var is None:
        ax.text(0.5, 0.5,
                f"N/A for {('/'.join(sorted({v['kind'] for v in m.state_vars})))} state:\n"
                "no scalar (int) or enum var to cobweb",
                ha="center", va="center", fontsize=13, wrap=True)
        ax.set_axis_off()
        ax.set_title(title)
        fig.savefig(out_path, dpi=120, bbox_inches="tight")
        plt.close(fig)
        return

    base = _base_state(m)
    xs, ys = _sample_map(m, var, mode, base)

    if not xs:
        ax.text(0.5, 0.5,
                f"N/A: transition unsat across the sampled range of {var['name']}",
                ha="center", va="center", fontsize=13)
        ax.set_axis_off()
        ax.set_title(title)
        fig.savefig(out_path, dpi=120, bbox_inches="tight")
        plt.close(fig)
        return

    lo = min(min(xs), min(ys))
    hi = max(max(xs), max(ys))
    pad = (hi - lo) * 0.04 + 0.5
    lo -= pad; hi += pad

    # The map x_{n+1} = f(x_n).
    ax.plot(xs, ys, "o", color="#1f77b4", ms=4, label=r"$x_{n+1}=f(x_n)$")
    if mode == "int":
        ax.plot(xs, ys, "-", color="#1f77b4", lw=1, alpha=0.5)

    # y = x diagonal.
    ax.plot([lo, hi], [lo, hi], "--", color="#888", lw=1, label="y = x")

    # Staircase orbit from a seed.
    if mode == "int":
        seed = 2000  # near the limit cycle for fixed-point numeric systems
        if not (lo <= seed <= hi):
            seed = int((lo + hi) / 2)
    else:
        seed = _to_ord(m, var, base[var["name"]])
    px, py = _staircase(m, var, mode, base, seed)
    if len(px) > 1:
        ax.plot(px, py, "-", color="#d62728", lw=1.2, alpha=0.8,
                label=f"orbit (seed={var['name']}={_from_ord(m, var, seed)})")
        ax.plot(px[0], py[0], "o", color="#d62728", ms=6)

    ax.set_xlim(lo, hi)
    ax.set_ylim(lo, hi)
    ax.set_aspect("equal", adjustable="box")

    held = [v["name"] for v in m.state_vars if v["name"] != var["name"]]
    xlabel = var["name"] + ("  (n)" if mode == "int" else "  ordinal (n)")
    ax.set_xlabel(xlabel)
    ax.set_ylabel(var["name"] + ("  (n+1)" if mode == "int" else "  ordinal (n+1)"))

    if mode == "enum-ordinal":
        variants = m.enum_variants[var["name"]]
        ax.set_xticks(range(len(variants)))
        ax.set_xticklabels(variants, rotation=45, ha="right", fontsize=8)
        ax.set_yticks(range(len(variants)))
        ax.set_yticklabels(variants, fontsize=8)

    sub = ""
    if held:
        sub = "  (others held: " + ", ".join(held) + ")"
    if mode == "enum-ordinal":
        sub = "  enum projected to ordinal" + sub
    ax.set_title(title + "\n" + var["name"] + sub, fontsize=11)
    ax.legend(loc="upper left", fontsize=9)
    ax.grid(True, alpha=0.2)

    fig.savefig(out_path, dpi=120, bbox_inches="tight")
    plt.close(fig)


if __name__ == "__main__":
    if len(sys.argv) != 4:
        print("usage: render_cobweb.py <smt2> <schema> <out_path>", file=sys.stderr)
        sys.exit(2)
    render(sys.argv[1], sys.argv[2], sys.argv[3])
