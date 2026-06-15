"""EffectProp — a thin UserPropagateBase base that dispatches effects DURING a
solve, with full call-tracing. Subclass and override on_fixed/on_final/on_eq/
on_diseq; call .track(...) to register terms. The push/pop/fresh scaffolding the
propagator requires is handled here.

Every method Z3 drives is logged to stderr with a `[prop dN]` prefix (N = current
scope depth) so you can watch the solver call the propagator: __init__, track/add
(us), then Z3 calling push, pop, fresh, _fixed, _final, _eq, _diseq. Set
`EffectProp.LOG = False` to silence it. The trace goes to stderr, so program
effects on stdout stay clean.

ENVIRONMENT CAVEAT: this repo's z3py is the 4.8.12 wrapper over libz3 4.15.4, and
their user-propagator ABI disagrees — the `fixed` callback's term-id does not
correspond to `add()`, so feeding a value BACK into the model (`propagate`) is
unreliable here. The EFFECTS still fire during the solve (that is what these
examples demonstrate); on a matched z3 (pip `z3-solver` >= ~4.12) value-feedback
works too. See docs/z3-effects-in-check.md.
"""
import sys
import z3


class EffectProp(z3.UserPropagateBase):
    LOG = True                              # set False to silence the call trace

    def __init__(self, s):
        self._lim = []                      # set before super so a stray push is safe
        super().__init__(s)
        self._log("__init__  (registering callbacks)")
        if s is not None:
            self.add_fixed(self._fixed)     # Z3 -> _fixed when a tracked term is set
            self.add_final(self._final)     # Z3 -> _final at a complete model
            self.add_eq(self._eq)           # Z3 -> _eq when two terms become equal
            self.add_diseq(self._diseq)     # Z3 -> _diseq when two become distinct

    def _log(self, msg):
        if self.LOG:
            print(f"[prop d{len(self._lim)}] {msg}", file=sys.stderr)

    # ── methods Z3 calls (the propagator interface) ──────────────────────────
    def push(self):                         # solver opens a decision scope
        self._log("push   (solver entered a new decision scope)")
        self._lim.append(0)

    def pop(self, num_scopes):              # solver backtracks out of scopes
        # cap to real depth: on this build the ABI hands a garbage num_scopes
        # (huge int); the solver can never pop more scopes than exist, so min()
        # is correct here and a no-op on a matched build.
        n = min(num_scopes, len(self._lim))
        for _ in range(n):
            self._lim.pop()
        self._log(f"pop {n}  (solver backtracked to depth {len(self._lim)})")

    def fresh(self, ctx):                   # solver forks a context (parallel)
        self._log("fresh  (solver forked a context)")
        return EffectProp(None)

    def _fixed(self, term_id, value):       # a tracked term was assigned
        self._log(f"_fixed  value={value}  (term id={term_id})")
        self.on_fixed(value)

    def _final(self):                       # a complete candidate model
        self._log("_final  (complete model — a commit point)")
        self.on_final()

    def _eq(self, x, y):                    # two terms became equal
        self._log(f"_eq    x={x} y={y}")
        self.on_eq(x, y)

    def _diseq(self, x, y):                 # two terms became distinct
        self._log(f"_diseq x={x} y={y}")
        self.on_diseq(x, y)

    # ── what we call (logged for completeness) ───────────────────────────────
    def track(self, *terms):
        for t in terms:
            self._log(f"track  add({t})  (register term for _fixed callbacks)")
            self.add(t)
        return self

    # ── override these ───────────────────────────────────────────────────────
    def on_fixed(self, value):
        pass

    def on_final(self):
        pass

    def on_eq(self, x, y):
        pass

    def on_diseq(self, x, y):
        pass
