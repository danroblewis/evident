"""model_codec.py — the value <-> z3 codec for a loaded Model (a mixin).

The transition-query layer (evident_viz.Model) talks to z3 in two directions: it
PINS a Python state value as a z3 literal (to fix `_state` for a successor query) and
DECODES a solved z3 model back to a Python value. Both are driven by each carried
leaf's `kind` (int/bool/real/string/enum) — plus `seq`, whose const is an
`(Array Int <elem>)` handled element-wise here:

  * decode  → read Select(arr, i) for i in 0..len-1, each by the element kind → a list
  * pin     → assert Select(prev_arr, i) == lit(value[i]) for each i
  * block   → exclude an already-found successor on its OBSERVABLE cells (0..len-1)
  * sort    → ArraySort(Int, elem_sort) for a leaf the smt2 parser dropped

Kept out of evident_viz.py (the load/decode core) as a single-concern mixin, mirroring
the RankingMixin / AnalysisMixin / QueryMixin / TemporalMixin split. Reads only the
instance attributes Model.__init__ sets: `consts`, `carried`, `two_tick_vars`,
`_enum_lit`.
"""
import z3


class CodecMixin:
    # ---- one scalar value <-> z3 -------------------------------------------
    def _scalar_lit(self, kind, name, value):
        """A z3 literal for one SCALAR value of `kind`. `kind` is the var's own kind
        for a scalar, or the ELEMENT kind for a Seq element. `name` resolves an enum's
        per-var variant table."""
        if kind == "int":
            return z3.IntVal(int(value))
        if kind == "bool":
            return z3.BoolVal(bool(value))
        if kind == "real":
            return z3.RealVal(value)
        if kind == "string":
            return z3.StringVal(value)
        if kind == "enum":
            return self._enum_lit[name][value]
        raise ValueError(f"unknown kind {kind}")

    def _scalar_read(self, mv, kind):
        """Decode one already-evaluated SCALAR z3 value `mv` of `kind` to Python."""
        if kind == "int":
            return mv.as_long()
        if kind == "bool":
            return z3.is_true(mv)
        if kind == "real":
            # A diverging continuous map (logistic, Lotka-Volterra) drives Z3's exact
            # rational to an astronomical numerator/denominator; float() of that Fraction
            # raises OverflowError. Clamp to a large finite magnitude so decoding never
            # crashes a renderer — a blown-up value reads as ±1e18, not an exception.
            CLAMP = 1e18
            try:
                fv = float(mv.as_fraction())
            except (OverflowError, ValueError):
                frac = mv.as_fraction()
                fv = CLAMP if frac > 0 else (-CLAMP if frac < 0 else 0.0)
            if fv != fv:  # NaN guard
                return 0.0
            return max(-CLAMP, min(CLAMP, fv))
        if kind == "string":
            return mv.as_string()
        if kind == "enum":
            return mv.decl().name()
        raise ValueError(f"unknown kind {kind}")

    # ---- a carried leaf's value <-> z3 (scalar OR seq) ---------------------
    def _lit(self, var, value):
        # A Seq has no single literal — it is pinned element-wise (see _pin_prev),
        # so _lit is never called on one. Guard so a stray call fails loudly rather
        # than silently dropping the constraint.
        if var["kind"] == "seq":
            raise ValueError("seq has no scalar literal; pin element-wise via _pin_prev")
        return self._scalar_lit(var["kind"], var["name"], value)

    def _read(self, model, var):
        c = self.consts[var["name"]]
        if var["kind"] == "seq":
            # The const is an (Array Int <elem>); read Select(arr, i) for i in 0..len-1,
            # decoding each by the element kind. Returns a Python list (tuple-ified in
            # _key for hashing). `len` is pinned in the schema entry for a carried Seq.
            n = var.get("len", 0)
            elem = var.get("elem", "int")
            return [self._scalar_read(model.eval(z3.Select(c, z3.IntVal(i)),
                                                 model_completion=True), elem)
                    for i in range(n)]
        mv = model.eval(c, model_completion=True)
        return self._scalar_read(mv, var["kind"])

    # ---- pinning a previous-tick value -------------------------------------
    def _pin_prev(self, solver, state):
        # Pin only the leaves the caller supplied; a renderer may pass a PARTIAL
        # state (e.g. just the deduped axis vars), leaving the rest free. Pinning
        # all of self.carried would KeyError on a leaf the caller omitted.
        for v in self.carried:
            if v["name"] in state:
                self._pin_one(solver, v, v["prev"], state[v["name"]])

    def _pin_one(self, solver, var, const_name, value):
        # Pin a single carried leaf's z3 const (named `const_name` — usually the prev
        # twin `_x`, but `__x` for the two-ago path) to `value`. A Seq has no scalar
        # literal: assert Select(arr, i) == lit(value[i]) element-wise instead.
        c = self.consts[const_name]
        if var["kind"] == "seq":
            elem = var.get("elem", "int")
            for i, ev in enumerate(value):
                solver.add(z3.Select(c, z3.IntVal(i)) ==
                           self._scalar_lit(elem, var["name"], ev))
        else:
            solver.add(c == self._lit(var, value))

    def _pin_prev2(self, solver, prev_state):
        # Pin the TWO-ticks-ago twin (`__x`) for the hist-2 leaves. Only the two-tick
        # vars have a `__x` const; one-tick vars have nothing two ticks back.
        for v in self.two_tick_vars:
            if v["name"] in prev_state:
                if ("__" + v["name"]) in self.consts:
                    self._pin_one(solver, v, "__" + v["name"], prev_state[v["name"]])

    # ---- blocking an already-found successor -------------------------------
    def _block_clause(self, mod):
        # "Differ from THIS model on some observable carried leaf" — the clause that
        # blocks an already-found successor so the next solve yields a genuinely
        # distinct one. Block against the model's EXACT value of each const, not a
        # re-literal of the decoded Python value: for reals, _read collapses an exact
        # rational (175/3) to a lossy float, and re-blocking with RealVal(float) never
        # excludes the true value — so a deterministic FSM would report one successor
        # 64× as 'distinct'. model.eval is exact for every kind.
        #
        # A Seq's const is an unbounded (Array Int <elem>); blocking the WHOLE array
        # (`arr != model_arr`) is trivially satisfiable by flipping a tail index ≥ len
        # that no one observes — the same 64× mislabel. Block on the OBSERVABLE cells
        # (Select(arr, 0..len-1)) instead, which is exactly what _read decodes.
        terms = []
        for v in self.carried:
            c = self.consts[v["name"]]
            if v["kind"] == "seq":
                for i in range(v.get("len", 0)):
                    sel = z3.Select(c, z3.IntVal(i))
                    terms.append(sel != mod.eval(sel, model_completion=True))
            else:
                terms.append(c != mod.eval(c, model_completion=True))
        return z3.Or(terms)

    # ---- z3 sorts ----------------------------------------------------------
    @staticmethod
    def _scalar_sort(kind):
        return {"int": z3.IntSort(), "bool": z3.BoolSort(),
                "real": z3.RealSort(), "string": z3.StringSort()}.get(kind, z3.IntSort())

    def _var_sort(self, var):
        # The z3 sort to synthesize for a carried leaf the parser dropped (declared but
        # unused in the transition). A Seq is an (Array Int <elem>); everything else is
        # the scalar sort for its kind.
        if var["kind"] == "seq":
            return z3.ArraySort(z3.IntSort(), self._scalar_sort(var.get("elem", "int")))
        return self._scalar_sort(var["kind"])
