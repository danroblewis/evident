"""model_ranking — variable RANKING / selection layer over a loaded `Model` (a RankingMixin).

Split out of `model_analysis.py` along the seam between CHOOSING variables and
SOLVING for bounds/structure. This module answers "which variables, in what order,
onto which channels?":

  - `state_vars` / `ranked_vars` / `_rank_and_dedup` — informational dedup + axis-pair
    ranking (the recommended axis ORDER)
  - `_sample_states` — the cached reachable sample every ranking query shares
  - `axis_bounds` — the reachable numeric domain a renderer scales within
  - `independence` — functional-dependency / driver analysis over the sample

Provided as a MIXIN class `Model` inherits; bodies moved VERBATIM (still `self`-based),
a behavior-preserving relocation. Cross-mixin helpers (`_pick_rep`, `var_class`,
faceting) live in AnalysisMixin and resolve through `self` at call time.
"""
import os


class RankingMixin:
    # ---- variable ranking: dedup redundant ('same-graph') vars, rank the rest ----
    @property
    def state_vars(self):
        """Interface variables, deduplicated (informationally-equivalent vars merged)
        and ranked by how much they vary — the recommended axis ORDER. Renderers take
        as many as they need (top 2 for a phase portrait, all for a scatter matrix).
        Falls back to raw interface order if sampling is degenerate. Cached.

        Override (for the selector-evaluation sweep): if the env var
        EVIDENT_VIZ_VARS is set to a comma-separated list of leaf names, return
        exactly those (in that order) instead of the ranked selection — this lets a
        driver force any variable combination onto a renderer to compare against the
        selector's own pick."""
        ov = os.environ.get("EVIDENT_VIZ_VARS")
        if ov:
            by_name = {v["name"]: v for v in self.carried}
            chosen = [by_name[n] for n in ov.split(",") if n in by_name]
            if chosen:
                return chosen
        if self._ranked is None:
            try:
                self._ranked = self._rank_and_dedup()
            except Exception:
                self._ranked = list(self.interface_vars)
        return self._ranked

    def ranked_vars(self):
        return self.state_vars

    def _sample_states(self, limit=1500):
        # Cached: independence/axis_bounds/change_rates/ranked_vars/facet all sample the same
        # reachable set every analyze — re-running reachable(1500) per caller was most of the
        # real-FSM latency (#171). Compute once per Model, reuse. (Keyed on the default limit; a
        # non-default limit bypasses the cache.)
        if limit != 1500:
            states, _ = self.reachable(limit=limit)
            return states if len(states) >= 2 else self.trajectory(steps=400)
        if getattr(self, "_sample_cache", None) is None:
            states, _ = self.reachable(limit=limit)
            self._sample_cache = states if len(states) >= 2 else self.trajectory(steps=400)
        return self._sample_cache

    @staticmethod
    def _strip_isolated_sentinels(vals):
        """Drop seed/sentinel artifacts (±1e6 fold initializer, lone solver spike) from a
        SORTED value list by ISOLATION — peel an endpoint only when it sits across a gap to
        the rest that is huge BOTH relative to the bulk's own spread AND in absolute terms.
        A dense transient (every adjacent gap small relative to the range) is never peeled,
        so a spiral's 1→0 decay keeps its full extent. Returns the kept values (never empty)."""
        if len(vals) < 4:
            return vals
        kept = list(vals)
        # The bulk spread: the inner 50% range (IQR), a scale immune to the very endpoints
        # we're testing. A sentinel gap dwarfs this; a transient's gaps don't.
        n = len(kept)
        bulk = kept[(3 * n) // 4] - kept[n // 4]
        # Peel up to a couple of artifacts from EACH end (a fold seeds one low + one high).
        for _ in range(4):
            if len(kept) < 4:
                break
            span = kept[-1] - kept[0]
            if span <= 0:
                break
            gap_hi = kept[-1] - kept[-2]
            gap_lo = kept[1] - kept[0]
            # An ISOLATED extreme: the gap to its neighbour vastly exceeds the data's own
            # bulk spread (so a DENSE transient — every adjacent gap small vs the bulk — is
            # never peeled), and is a real fraction of the span (so float noise near a flat
            # core isn't peeled). bulk==0 → a flat core with a lone spike: the gap IS the span.
            def isolated(gap):
                return (gap > 1e-9 and gap >= 0.25 * span
                        and (bulk == 0 or gap >= 50 * bulk))
            if gap_hi >= gap_lo and isolated(gap_hi):
                kept = kept[:-1]
            elif isolated(gap_lo):
                kept = kept[1:]
            else:
                break
        return kept or list(vals)

    def axis_bounds(self, name, pad=0.08):
        """(lo, hi) of a NUMERIC variable over the REACHABLE sample — the real domain a
        renderer should grid / scale / seed within, INSTEAD of a hardcoded ±3000 box
        (the 'fabrication' bug: gridding a guessed range invents cycles/basins the
        program never enters). Returns None for a non-numeric var or an empty sample;
        callers fall back to a default box only for genuinely unbounded continuous
        dynamics. bool is excluded (it's categorical, encode ordinally elsewhere).

        ROBUST to sentinel values: programs mark 'empty / uninitialized' with extreme
        seeds — -1 for 'none' (an empty cache slot, no node popped yet) and ±1e6 for a
        fold initializer (max seeded low, min seeded high so the first real value wins).
        A single such point would otherwise blow the axis out to ±1e6 and crush the real
        data. We strip a sentinel by ISOLATION, not by quantile: an extreme value that
        sits across a GAP orders of magnitude larger than the data's own bulk spread is a
        seed artifact, not data. A smooth transient (a spiral sink decaying 1→0, every
        value real and densely packed) has no such gap, so its FULL extent is kept — the
        earlier 3×IQR quantile fence wrongly trimmed it, because a decay spends most ticks
        near the sink so the IQR collapses to ~0 and the legitimate early extent reads as
        an outlier (#465 follow-up). We also (2) floor at 0 when the only remaining
        sub-zero value is a unit -1 'none' marker — while PRESERVING genuinely-negative
        data (a balance that really overdrafts, whose bulk sits below 0, keeps its range)."""
        states = self._sample_states()
        vals = sorted(s[name] for s in states if type(s.get(name)) in (int, float))
        if not vals:
            return None
        vals = self._strip_isolated_sentinels(vals)
        lo, hi = float(min(vals)), float(max(vals))
        if lo == hi:
            return (lo - 1.0, hi + 1.0)
        m = (hi - lo) * pad
        lo_out, hi_out = lo - m, hi + m
        if lo >= 0:                                  # non-negative-domain var: never pad below 0
            lo_out = max(0.0, lo_out)
        elif -1.0 <= lo < 0 < hi:                    # only sub-zero is a '-1 = none' sentinel: drop it
            lo_out = 0.0
        return (lo_out, hi_out)

    def _rank_and_dedup(self):
        import math
        vs = list(self.interface_vars)
        if len(vs) <= 1:
            self.variable_groups = [{"rep": v["name"], "members": [v["name"]],
                                     "entropy": 0.0} for v in vs]
            return vs
        states = self._sample_states()
        if len({self._key(s) for s in states}) < 2:   # degenerate sample (e.g. a
            return vs                                   # fixed-point seed) — don't dedup
        series = {v["name"]: [s[v["name"]] for s in states] for v in vs}

        def partition(name):
            g = {}
            for i, val in enumerate(series[name]):
                g.setdefault(val, []).append(i)
            # An INJECTIVE var (every sampled value distinct — the all-singletons
            # partition) carries NO equivalence signal: it matches every other
            # injective var, so two independent continuous axes (x, y of a phase
            # plane, sampled along ONE orbit where each x maps to a unique y) would
            # collapse into one — and the renderer then sees a 1-var state and bails
            # "needs 2 axes" on a textbook 2-D system (#465/#468). Equivalence-by-
            # partition is only meaningful for vars that genuinely co-quantize (a
            # bool mirroring another, a mode that tracks a phase); give an injective
            # var a NAME-UNIQUE signature so it never merges with another.
            sig = frozenset(frozenset(idxs) for idxs in g.values())
            if len(g) == len(series[name]):              # all-distinct → injective
                return (name, sig)
            return sig

        def entropy(name):
            n = len(series[name])
            c = {}
            for val in series[name]:
                c[val] = c.get(val, 0) + 1
            return -sum((k / n) * math.log2(k / n) for k in c.values())

        # Group by identical partition signature = informationally equivalent
        # ('same graph'); keep one representative per group, ranked by entropy.
        groups = {}
        for v in vs:
            groups.setdefault(partition(v["name"]), []).append(v)
        reps = {}              # name -> (var, entropy, members)
        for members in groups.values():
            rep = self._pick_rep(members)
            reps[rep["name"]] = (rep, entropy(rep["name"]), [m["name"] for m in members])

        H = {nm: reps[nm][1] for nm in reps}                       # marginal entropy
        order = self._select_axis_order(reps, series, H)

        self.variable_groups = [{"rep": nm, "members": reps[nm][2],
                                 "entropy": round(H[nm], 3)} for nm in order]
        return [reps[nm][0] for nm in order]

    def _select_axis_order(self, reps, series, H):
        """Rank the deduped representatives into a PLOT-AXIS order: the structure-optimal
        (x, y) pair first (driver on X), then the rest by discounted relevance. `reps` maps
        name -> (var, entropy, members), `series` maps name -> sampled value list, `H` is the
        marginal entropy per name. Returns the ordered name list. The scoring is its own
        concern — separable from the sampling/grouping that produces `reps`/`series`."""
        # --- structure-based axis-pair selection (replaces mRMR) ---------------
        # mRMR (max-entropy + min-redundancy) is a feature-SELECTION criterion and is
        # the wrong tool for choosing PLOT AXES: entropy over-rewards trivial tick
        # counters, and the redundancy penalty avoids exactly the *correlated* pairs (a
        # cursor-vs-accumulator staircase, two co-moving counters) that make the best
        # picture. Instead: score each pair by a structure measure that REWARDS both
        # axes meaningfully varying and DISCOUNTS unit counters — and never penalizes
        # correlation. (See viz/reviews/selector-eval.md for the audit that motivated
        # this.)
        INDEX_DISCOUNT = float(os.environ.get("EVIDENT_VIZ_DISCOUNT", "0.8"))

        def is_unit_counter(name):              # tick proxy: a loop index / clock
            return self._is_unit_counter(reps[name][0]["kind"], series[name])

        W = {nm: (INDEX_DISCOUNT if is_unit_counter(nm) else 1.0) for nm in reps}
        relevance = {nm: W[nm] * H[nm] for nm in reps}             # discounted single-var score

        # functional-redundancy guard: a pair where one axis is a DETERMINISTIC function
        # of the other AND that dependent has tiny cardinality is a degenerate "flat line
        # with a step" (e.g. grep `done = line_no >= 4`). Demote it. A RICH functional
        # curve (a staircase, `sum = f(cursor)`) keeps full score — high cardinality means
        # it traces a real shape, not a step. (This is the targeted alternative to mRMR's
        # blanket redundancy penalty, which wrongly punished staircases too.)
        def determines(a, b):                  # does a determine b? (b is a function of a)
            m = {}
            for va, vb in zip(series[a], series[b]):
                if m.get(va, vb) != vb:
                    return False
                m[va] = vb
            return True

        def low_card(nm):
            return len(set(series[nm])) <= 3

        names = list(reps)
        best_pair, best_score = None, -1.0
        for i in range(len(names)):
            for j in range(i + 1, len(names)):
                a, b = names[i], names[j]
                if H[a] <= 0 or H[b] <= 0:                         # a constant axis = a flat line
                    continue
                s = relevance[a] * relevance[b]                    # both axes vary; counters discounted
                if (determines(a, b) and low_card(b)) or (determines(b, a) and low_card(a)):
                    s *= 0.4                                        # degenerate functional step (not a rich curve)
                if s > best_score:
                    best_pair, best_score = (a, b), s

        return self._order_axes(best_pair, names, series, relevance)

    def _order_axes(self, best_pair, names, series, relevance):
        """Assemble the final axis order: the structure-optimal pair first (driver on X),
        then the rest by discounted relevance. When no pair scored (a single varying axis),
        order everything by relevance. `series`/`relevance` are the per-name sampled values
        and discounted single-var scores from `_select_axis_order`."""
        def determines(a, b):                  # does a determine b? (b is a function of a)
            m = {}
            for va, vb in zip(series[a], series[b]):
                if m.get(va, vb) != vb:
                    return False
                m[va] = vb
            return True

        # state_vars = the structure-optimal axis pair first, then the rest by
        # discounted relevance (so the color/facet channels also avoid trivial counters).
        # Within the pair, the more-INDEPENDENT variable (it determines the other without
        # being determined — a driver/clock) goes FIRST = on X, matching the math
        # convention (independent → X, dependent → Y). When neither drives, order is kept.
        def indep_score(nm):
            return (sum(determines(nm, x) for x in names if x != nm)
                    - sum(determines(x, nm) for x in names if x != nm))

        if best_pair:
            a, b = best_pair
            ia, ib = indep_score(a), indep_score(b)
            # The driver (higher net-determination) goes on X. If NEITHER drives (a
            # relational/cyclic model, ia == ib), put the higher-information variable on
            # X — so a rich enum (dungeon's 7-room d.room) beats a 2-value bool (d.has_key)
            # rather than collapsing the scatter onto one or two vertical lines.
            if ib > ia or (ib == ia and relevance[b] > relevance[a]):
                a, b = b, a
            best_pair = (a, b)
            rest = sorted((nm for nm in names if nm not in best_pair),
                          key=lambda nm: -relevance[nm])
            return list(best_pair) + rest
        return sorted(names, key=lambda nm: -relevance[nm])

    @staticmethod
    def _is_unit_counter(kind, vals):
        """An integer that takes a DISTINCT, CONSECUTIVE value in every sampled state — a
        loop index / clock. Injective (one value per state) so a cyclic sawtooth isn't
        flagged; consecutive so an accumulator (monotone with gaps) isn't either."""
        if kind != "int":
            return False
        d = sorted(set(vals))
        return len(d) >= 3 and len(d) == len(vals) and (d[-1] - d[0] + 1) == len(d)

    def independence(self, states=None):
        """Functional-dependency analysis of the interface variables: which behave as an
        INDEPENDENT variable — a driver/clock that determines the others without being
        determined by them — vs DEPENDENT (computed from the drivers).

        Evident models are written to be relational (solve for any variable by leaving it
        unbound), but a difference-equation model usually smuggles in a driver: a cursor
        that advances on its own, with everything else computed from it. This surfaces it.
        A model with NO driver (a cycle, a nondeterministic graph — every variable
        co-determines) is reported as 'genuinely relational'.

        Returns {'verdict': 'driven'|'relational', 'driver': name|None,
                 'drivers': [names], 'dependents': [names], 'score': {name: net}} where
        net = #(vars it determines) − #(vars that determine it): positive = a driver,
        negative = a pure dependent, ~0 = mutual / cyclic."""
        vs = [v["name"] for v in self.interface_vars]
        # Reuse a caller-supplied reachable sample when available (the server already has reachable(400)
        # for the stats) — recomputing _sample_states' reachable(1500) here was ~0.6-0.9s of the
        # real-FSM analyze latency for nothing (Ana/Marek #217). 400 states is ample for a
        # functional-dependency check.
        if states is None:
            states = self._sample_states()
        if not vs or len({self._key(s) for s in states}) < 2:
            # nothing varies across the sample (constant / degenerate) — no driver to find
            return {"verdict": "relational", "driver": None, "drivers": [],
                    "dependents": [], "score": {n: 0 for n in vs}}
        if len(vs) == 1:
            # A lone carried variable that VARIES is its own clock: a deterministic
            # self-recurrence x_t = f(x_{t-1}). That is the MOST driven shape, not a
            # relational cycle — every FSM reads its own _prev, so self-carry must not be
            # mistaken for co-determination. (A nondeterministic lone var is caught by the
            # branching override in the banner upstream.)
            return {"verdict": "driven", "driver": vs[0], "drivers": list(vs),
                    "dependents": [], "score": {vs[0]: 0}}
        series = {n: [s[n] for s in states] for n in vs}

        def determines(a, b):                  # each a-value maps to a unique b-value
            m = {}
            for va, vb in zip(series[a], series[b]):
                if m.get(va, vb) != vb:
                    return False
                m[va] = vb
            return True

        det = {a: set(b for b in vs if a != b and determines(a, b)) for a in vs}
        score = {a: len(det[a]) - sum(1 for b in vs if a in det[b]) for a in vs}
        top = max(score.values())
        drivers = [a for a in vs if score[a] == top] if top > 0 else []
        dependents = sorted((a for a in vs if score[a] < 0), key=lambda a: score[a])
        driver = None
        if drivers:                            # canonical driver: prefer the unit counter
            kind = {v["name"]: v["kind"] for v in self.interface_vars}
            driver = sorted(drivers, key=lambda a: (
                not self._is_unit_counter(kind[a], series[a]), -len(det[a]), len(a)))[0]
        return {"verdict": "driven" if drivers else "relational", "driver": driver,
                "drivers": drivers, "dependents": dependents, "score": score}
