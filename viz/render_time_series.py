#!/usr/bin/env python3
"""render_time_series — ENSEMBLE-over-initial-conditions renderer for ANY Evident IR.

A single from-init chain shows only the basin the seed falls into — a bistable seeded at
x=1 looks like it ALWAYS decays to 0, hiding the second attractor at 6. So this renderer
forward-simulates an ENSEMBLE of initial conditions (`time_series_ensemble.ensemble_inits`)
and, per state variable, plots:

  * every trajectory as a faint line (per-var track layout, tick on the shared x-axis),
  * a BOLD reachable ENVELOPE — the min–max band (+ median) per tick across the ensemble.
    This is the headline all-conditions signal: the SPREAD of values reachable at each step,
    not one line.

The inits come from the EXISTING all-conditions machinery, and each trajectory is stepped
with the EXISTING successor relation (`step_trajectory` reuses `_advance`) — the transition
is never reimplemented:

  * DISCRETE / bounded → inits = full_state_graph()'s enumerated state set (every valid
    carried assignment), sampled if huge.
  * CONTINUOUS / Real → inits = a proven-bounds product grid over the carried vars.
  * UNBOUNDED carried var → no honest ensemble box; falls back to a single from-init run with
    an explicit "single run (unbounded init)" note.

Divergence is clamped: a chaotic / continuous model can overflow to 1e300; `step_trajectory`
truncates such a chain so the envelope never crashes on an OverflowError.

  * numeric vars (int/real)  -> ensemble lines + min–max band + median
  * bool/enum/string vars    -> ensemble step lines + reached-value band (ordinal extent)

Usage:
    python3 viz/render_time_series.py <smt2> <schema> <out_path.png>
"""
import sys
import os

sys.path.insert(0, os.path.join(os.path.dirname(__file__)))
sys.path.insert(0, "viz")

import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt
import numpy as np

from evident_viz import load
from time_series_ensemble import ensemble_inits, step_trajectory
from time_series_walk import pick_seed, excited_seed, walk, to_ordinal, _flatten_seqs

STEPS = 60


def _categorical_yticks(ax, m, var):
    """Label a categorical var's row with its FULL ladder (every enum variant / false|true),
    not just the visited values, so the row reads as the variable's whole range. Shared by the
    ensemble and single-run tracks."""
    kind = var["kind"]
    if kind == "enum":
        labels = {i: vlbl for i, vlbl in enumerate(m.enum_variants.get(var["name"], []))}
    elif kind == "bool":
        labels = {0: "false", 1: "true"}
    else:
        return
    if labels:
        ks = sorted(labels)
        ax.set_yticks(ks)
        ax.set_yticklabels([labels[k] for k in ks], fontsize=8)
        ax.set_ylim(min(ks) - 0.4, max(ks) + 0.4)


def _na_card(m, out_path, msg):
    fig, ax = plt.subplots(figsize=(10, 4))
    ax.axis("off")
    ax.text(0.5, 0.5, msg, ha="center", va="center", fontsize=14)
    fig.suptitle(f"{m.fsm} — time_series", fontsize=14, fontweight="bold")
    fig.savefig(out_path, dpi=120, bbox_inches="tight")
    plt.close(fig)


def _ordered_vars(m, sample_state, flat_vars):
    """The plot ROW order: numeric, then categorical, then derived interface vars (each a
    type-split projection of the importance-ranked state_vars), falling back to a seq-only
    fsm's per-element tracks. `sample_state` gates the derived vars (only those actually
    present in the trajectory states)."""
    derived = [v for v in m.derived if v["name"] in sample_state]
    ordered = m.numeric_vars + m.categorical_vars + derived
    return ordered if ordered else list(flat_vars)


def _track_matrix(m, var, trajs, nticks):
    """A (n_trajectories × nticks) float matrix of `var`'s ordinal value per trajectory per
    tick, padded with NaN past each trajectory's end (so a short chain doesn't fabricate a
    flat tail). Numeric vars keep their value; categorical vars map to their ordinal."""
    name = var["kind"]
    rows = []
    for tr in trajs:
        row = [np.nan] * nticks
        for t, s in enumerate(tr):
            if name in ("int", "real"):
                v = s.get(var["name"])
                row[t] = float(v) if isinstance(v, (int, float)) else np.nan
            else:
                row[t] = to_ordinal(m, var, s[var["name"]])[0]
        rows.append(row)
    return np.array(rows, float)


def _draw_track(ax, m, var, mat, ticks, rank):
    """One row: every trajectory faint, plus the bold reachable envelope (min–max band +
    median) across the ensemble at each tick. `mat` is _track_matrix's NaN-padded matrix."""
    kind = var["kind"]
    badge = "derived" if var.get("role") == "derived" else m.var_class(var)
    ax.set_title(f"#{rank + 1}  {badge}", loc="left", fontsize=8, color="#888", pad=2)
    numeric = kind in ("int", "real")
    line_color = "#1f77b4" if numeric else "#d62728"

    # faint individual trajectories
    for row in mat:
        if numeric:
            ax.plot(ticks, row, color=line_color, lw=0.6, alpha=0.18, zorder=1)
        else:
            ax.step(ticks, row, where="post", color=line_color, lw=0.6,
                    alpha=0.18, zorder=1)

    # reachable ENVELOPE: min–max band + median, computed per tick over the ensemble
    # (ignoring NaN-padded tails — nanmin/nanmax of an all-NaN column is itself NaN,
    # which matplotlib simply skips, so a tick no trajectory reaches leaves a gap).
    with np.errstate(all="ignore"):
        lo = np.nanmin(mat, axis=0)
        hi = np.nanmax(mat, axis=0)
        med = np.nanmedian(mat, axis=0)
    valid = ~np.isnan(lo)
    tk = np.array(ticks, float)
    ax.fill_between(tk[valid], lo[valid], hi[valid], color=line_color, alpha=0.22,
                    zorder=2, label="reachable envelope (min–max)")
    if numeric:
        ax.plot(tk[valid], med[valid], color=line_color, lw=2.0, zorder=3,
                label="ensemble median")
    else:
        ax.step(tk[valid], med[valid], where="post", color=line_color, lw=2.0,
                zorder=3, label="ensemble median")

    ax.set_ylabel(var["name"], rotation=0, ha="right", va="center", fontsize=9)
    if numeric:
        ax.grid(True, alpha=0.3)
    else:
        _categorical_yticks(ax, m, var)
        ax.grid(True, axis="x", alpha=0.3)


def _render_ensemble(m, out_path, inits, kind, note):
    """Forward-simulate every init, expand seqs, drop constant rows, and draw the per-var
    ensemble + envelope. Returns a one-line render note (string)."""
    prefer_change = m.is_discrete()
    raw = [step_trajectory(m, init, STEPS, prefer_change) for init in inits]
    trajs = [tr for tr in raw if tr]
    if not trajs:
        _na_card(m, out_path, f"N/A for {m.fsm}: no trajectories from any initial condition")
        return "ensemble: empty"

    # Expand seqs per trajectory (each gets the same pseudo-var set; state_vars is shared).
    flat_vars = m.state_vars
    flat_trajs = []
    for tr in trajs:
        fv, ftr = _flatten_seqs(m.state_vars, tr)
        flat_vars = fv
        flat_trajs.append(ftr)
    trajs = flat_trajs
    nticks = max(len(tr) for tr in trajs)
    ticks = list(range(nticks))

    ordered = _ordered_vars(m, trajs[0][0], flat_vars)

    # A var is CONSTANT only if it never moves across the WHOLE ensemble (not just one run) —
    # otherwise the ensemble's whole point (the fan) would be suppressed. Report the held set.
    def held(var):
        seen = set()
        for tr in trajs:
            for s in tr:
                if var["name"] in s:
                    seen.add(s[var["name"]])
        return next(iter(seen)) if len(seen) == 1 else None

    constants = [(v, held(v)) for v in ordered if held(v) is not None]
    varying = [v for v in ordered if held(v) is None]
    if not varying:
        fig, ax = plt.subplots(figsize=(10, max(3.0, 0.4 * len(constants) + 2)))
        ax.axis("off")
        lines = "\n".join(f"  {v['name']} = {hv}" for v, hv in constants)
        ax.text(0.5, 0.5,
                f"N/A — every state variable is constant across the ensemble\n"
                f"({len(trajs)} initial conditions; no dynamics to plot)\n\n{lines}",
                ha="center", va="center", fontsize=12, family="monospace")
        fig.suptitle(f"{m.fsm} — time_series", fontsize=14, fontweight="bold")
        fig.savefig(out_path, dpi=120, bbox_inches="tight")
        plt.close(fig)
        return f"ensemble: all {len(ordered)} vars constant"

    nvars = len(varying)
    fig, axes = plt.subplots(nvars, 1, sharex=True,
                             figsize=(11, max(2.2 * nvars, 3.0)))
    if nvars == 1:
        axes = [axes]
    for rank, (ax, var) in enumerate(zip(axes, varying)):
        mat = _track_matrix(m, var, trajs, nticks)
        _draw_track(ax, m, var, mat, ticks, rank)
    axes[0].legend(loc="upper right", fontsize=7, framealpha=0.85)
    axes[-1].set_xlabel("tick")
    fig.suptitle(
        f"{m.fsm} — time_series  ({note}; {len(trajs)} trajectories, ≤{nticks} ticks; "
        f"{nvars} varying vars)",
        fontsize=13, fontweight="bold")
    if constants:
        held_s = ",  ".join(f"{v['name']}={hv}" for v, hv in constants)
        fig.text(0.5, 0.005, f"held constant across the ensemble: {held_s}",
                 ha="center", va="bottom", fontsize=8, color="#666")
    fig.tight_layout(rect=[0, 0.02 if constants else 0, 1, 0.97])
    fig.savefig(out_path, dpi=120, bbox_inches="tight")
    plt.close(fig)
    return (f"{kind} ensemble: {len(trajs)} trajectories, {nvars} varying vars")


def _render_single_run(m, out_path, reason):
    """The honest single-trajectory fallback for an UNBOUNDED model (no finite ensemble box):
    follow one chain from initial_state() and plot each var as one line, captioned with WHY
    the ensemble couldn't be built."""
    seed = pick_seed(m)
    if seed is None:
        _na_card(m, out_path,
                 f"N/A for {m.fsm}: no initial state\n(transition has no first-tick model)")
        return "single-run: no init"
    # An unbounded OSCILLATOR (pendulum/vanderpol) whose init is the origin fixed point would plot a
    # flat 'no dynamics' line; on this unbounded path there's no proven bound to violate, so excite
    # off the fixed point to trace the real limit cycle (the same off-origin start phase_portrait
    # probes). Returns None unless it's genuinely a fixed-point oscillator, so bounded/honest-flat
    # models are untouched (Marek #183).
    excited = excited_seed(m)
    if excited is not None:
        seed, reason = excited, reason + "; excited off the fixed-point origin to trace the orbit"
    traj = walk(m, seed, STEPS)
    flat_vars, traj = _flatten_seqs(m.state_vars, traj)
    ticks = list(range(len(traj)))
    ordered = _ordered_vars(m, traj[0], flat_vars)

    def held_value(var):
        vals = [s[var["name"]] for s in traj]
        return vals[0] if all(v == vals[0] for v in vals) else None

    constants = [(v, held_value(v)) for v in ordered if held_value(v) is not None]
    varying = [v for v in ordered if held_value(v) is None]
    if not varying:
        _na_card(m, out_path,
                 f"N/A — every state variable is constant over the trajectory\n"
                 f"({len(traj)} ticks from seed {m.label(seed)}; no dynamics to plot)")
        return "single-run: all constant"

    nvars = len(varying)
    fig, axes = plt.subplots(nvars, 1, sharex=True,
                             figsize=(11, max(2.2 * nvars, 3.0)))
    if nvars == 1:
        axes = [axes]
    for rank, (ax, var) in enumerate(zip(axes, varying)):
        name, kind = var["name"], var["kind"]
        badge = "derived" if var.get("role") == "derived" else m.var_class(var)
        ax.set_title(f"#{rank + 1}  {badge}", loc="left", fontsize=8, color="#888", pad=2)
        if kind in ("int", "real"):
            ax.plot(ticks, [s[name] for s in traj], marker="o", markersize=3,
                    linewidth=1.4, color="#1f77b4")
            ax.grid(True, alpha=0.3)
        else:
            ys = [to_ordinal(m, var, s[name])[0] for s in traj]
            ax.step(ticks, ys, where="post", linewidth=1.6, color="#d62728",
                    marker="o", markersize=3)
            _categorical_yticks(ax, m, var)
            ax.grid(True, axis="x", alpha=0.3)
        ax.set_ylabel(name, rotation=0, ha="right", va="center", fontsize=9)
    axes[-1].set_xlabel("tick")
    fig.suptitle(
        f"{m.fsm} — time_series  (single run (unbounded init): {reason}; "
        f"seed {m.label(seed)}, {len(traj)} ticks)",
        fontsize=12, fontweight="bold")
    if constants:
        held = ",  ".join(f"{v['name']}={hv}" for v, hv in constants)
        fig.text(0.5, 0.005, f"held constant (suppressed): {held}",
                 ha="center", va="bottom", fontsize=8, color="#666")
    fig.tight_layout(rect=[0, 0.02 if constants else 0, 1, 0.97])
    fig.savefig(out_path, dpi=120, bbox_inches="tight")
    plt.close(fig)
    return f"single-run (unbounded): {nvars} varying vars"


def render(smt2, schema, out_path):
    m = load(smt2, schema)
    inits, kind, note = ensemble_inits(m)
    if inits is None:
        # No finite ensemble box (some carried var unbounded) — honest single-run fallback.
        return _render_single_run(m, out_path, note)
    return _render_ensemble(m, out_path, inits, kind, note)


if __name__ == "__main__":
    if len(sys.argv) != 4:
        print("usage: render_time_series.py <smt2> <schema> <out.png>", file=sys.stderr)
        sys.exit(2)
    render(sys.argv[1], sys.argv[2], sys.argv[3])
    print(f"wrote {sys.argv[3]}")
