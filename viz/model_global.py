"""model_global.py — the ALL-INITIAL-CONDITIONS transition graph (a mixin).

`reachable()` (in evident_viz.py) walks the successor relation FORWARD from the
single seeded init (`is_first_tick = true`). For a deterministic FSM that is one
trajectory — it shows only the basin the seed falls into, never the global
dynamics. This mixin adds `full_state_graph()`: the transition graph over EVERY
valid carried-state assignment, ignoring the seed.

It does NOT reimplement the transition. It enumerates the product of the BOUNDED
DISCRETE carried vars (bool → {T,F}, enum → its variants, int → its proven
reachable range from z3 Optimize over the transition), then applies the EXISTING
`successors()` (the ¬is_first_tick dynamics `reachable()` steps with) and the
EXISTING `_key` dedup to each enumerated state. Reals / strings / seqs / unbounded
ints are not finitely enumerable, so the method returns `discrete=False` and the
caller (render_state_graph) falls back to the from-init `reachable()` path.

Reads only attributes Model.__init__ / CodecMixin set: `carried`, `consts`,
`enum_variants`, `assertions`, `first_tick`, plus `successors` / `_key`.
"""
import itertools

import z3

from model_const import SOLVE_TIMEOUT_MS

# A single carried var whose discrete product would exceed this many values is
# treated as unbounded — we don't enumerate a 10⁶-wide int "range". The whole
# product is separately capped by the caller's `limit`.
_MAX_VAR_DOMAIN = 4096


def _finite_numeric(bound):
    """The Python int/float value of a z3 numeral objective bound, or None if it is the
    ±∞ sentinel (`oo`) z3 Optimize returns for an UNBOUNDED objective. Handles both the
    int path (`is_int_value`) and the rational path a Real objective produces."""
    if z3.is_int_value(bound):
        return bound.as_long()
    if z3.is_rational_value(bound):
        return float(bound.as_fraction())
    return None                                    # unbounded (±∞) → no finite bound


class GlobalGraphMixin:
    def proven_range(self, var):
        """The PROVEN reachable [lo, hi] of a numeric (int OR real) carried var, via z3
        Optimize over the transition (max/min its current const with is_first_tick=false).
        Returns (lo, hi) — Python ints for an int var, floats for a real var — or None if
        either bound is ±∞ / missing (unbounded → no finite range to enumerate or grid).

        This is the SINGLE bounds source for both the discrete-enumeration path (`_int_range`
        wraps it for ints) and the continuous-ensemble grid (render_time_series seeds reals
        within this proven box). Same machinery solved_bounds uses, but per-var and keyed by
        the var's own const (no short-name collision) since we need the exact const."""
        c = self.consts.get(var["name"])
        if c is None:
            return None
        lo = hi = None
        for sense in ("max", "min"):
            opt = z3.Optimize()
            opt.set("timeout", SOLVE_TIMEOUT_MS)
            opt.add(self.assertions)
            if self.first_tick is not None:
                opt.add(self.first_tick == False)  # noqa: E712
            handle = opt.maximize(c) if sense == "max" else opt.minimize(c)
            if opt.check() != z3.sat:
                return None
            # Read the PROVEN optimum from the objective handle, not model.eval(c): a model is
            # just one satisfying witness, but the handle is ±∞ (`oo`) for an unbounded objective.
            # An unbounded var has no finite domain to enumerate / grid → not bounded.
            val = _finite_numeric(handle.upper() if sense == "max" else handle.lower())
            if val is None:
                return None                        # unbounded objective (±∞) → not bounded
            if sense == "max":
                hi = val
            else:
                lo = val
        return (lo, hi) if lo is not None and hi is not None and lo <= hi else None

    def _int_range(self, var):
        """The proven reachable [lo, hi] of an INT carried var as Python ints, or None if
        unbounded / non-integral. Thin int-typed wrapper over `proven_range`."""
        rng = self.proven_range(var)
        if rng is None:
            return None
        lo, hi = rng
        if lo != int(lo) or hi != int(hi):
            return None
        return (int(lo), int(hi))

    def _var_domain(self, var):
        """The finite list of values a single carried var can take, or None if it is not
        finitely enumerable (real / string / seq, or an int with no proven finite range /
        a range wider than _MAX_VAR_DOMAIN)."""
        kind = var["kind"]
        if kind == "bool":
            return [True, False]
        if kind == "enum":
            return list(self.enum_variants.get(var["name"], []))
        if kind == "int":
            rng = self._int_range(var)
            if rng is None:
                return None
            lo, hi = rng
            if hi - lo + 1 > _MAX_VAR_DOMAIN:
                return None
            return list(range(lo, hi + 1))
        return None                                # real / string / seq → not enumerable

    def _enumerable_domains(self):
        """({name: [values]}, ok). `ok` is False as soon as ANY carried var is not finitely
        enumerable — the global graph can't be built soundly over a continuous/unbounded axis,
        so the caller falls back. Order follows self.carried for a stable product."""
        domains = {}
        for v in self.carried:
            dom = self._var_domain(v)
            if dom is None:
                return None, False
            domains[v["name"]] = dom
        return domains, True

    def full_state_graph(self, limit=5000):
        """The transition graph from ALL initial conditions: enumerate EVERY valid carried
        assignment (the product of the bounded discrete carried vars, IGNORING is_first_tick)
        and apply the EXISTING successor relation to each. Returns
        (states, edges, info) where info = {"discrete": bool, "capped": bool}.

          - discrete=False → some carried var is real/string/seq/unbounded-int; states/edges
            are empty and the caller must fall back to the from-init reachable() path.
          - capped=True    → the discrete product (or the resulting graph) exceeded `limit`;
            states/edges hold what fit, honestly flagged.

        States are deduped by the EXISTING `_key`; edges are (from_index, to_index) over the
        successor image — the same `successors()` reachable() steps with, so the dynamics are
        identical, only the ROOT SET differs (all states vs the single seed)."""
        if self.has_two_tick:
            return [], [], {"discrete": False, "capped": False}  # pair-state product: separate task
        domains, ok = self._enumerable_domains()
        if not ok:
            return [], [], {"discrete": False, "capped": False}
        names = [v["name"] for v in self.carried]
        sizes = [len(domains[n]) for n in names]
        product = 1
        for s in sizes:
            product *= s
        capped = product > limit
        states, index, edges = [], {}, []

        def intern(state):
            k = self._key(state)
            i = index.get(k)
            if i is None:
                i = index[k] = len(states)
                states.append(state)
            return i

        for combo in itertools.islice(
                itertools.product(*[domains[n] for n in names]), limit):
            cur = dict(zip(names, combo))
            i = intern(cur)
            for nxt in self.successors(cur):
                edges.append((i, intern(nxt)))
        if len(states) >= limit or product > limit:
            capped = True
        return states, edges, {"discrete": True, "capped": capped}
