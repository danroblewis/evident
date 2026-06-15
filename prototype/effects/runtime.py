"""EffectProp — a thin UserPropagateBase base that dispatches effects DURING a
solve. Subclass and override on_fixed(value) and/or on_final(); call .track(...)
to register the Bool/BitVec terms whose assignment should fire on_fixed. The
push/pop/fresh scaffolding the propagator requires is handled here.

ENVIRONMENT CAVEAT: this repo's z3py is the 4.8.12 wrapper over libz3 4.15.4, and
their user-propagator ABI disagrees — the `fixed` callback's term-id does not
correspond to `add()`, so feeding a value BACK into the model (`propagate`) is
unreliable here. The EFFECTS still fire during the solve (that is what these
examples demonstrate); on a matched z3 (pip `z3-solver` >= ~4.12) the
value-feedback works too. See docs/z3-effects-in-check.md.
"""
import z3


class EffectProp(z3.UserPropagateBase):
    def __init__(self, s):
        super().__init__(s)
        self._lim = []
        if s is not None:
            self.add_fixed(self._fixed)
            self.add_final(self._final)

    def push(self):
        self._lim.append(0)

    def pop(self, n):
        for _ in range(n):
            if self._lim:
                self._lim.pop()

    def fresh(self, ctx):
        return EffectProp(None)

    def _fixed(self, _id, value):
        self.on_fixed(value)

    def _final(self):
        self.on_final()

    # ── override these ──
    def on_fixed(self, value):
        pass

    def on_final(self):
        pass

    def track(self, *terms):
        for t in terms:
            self.add(t)
        return self
