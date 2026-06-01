"""The trampoline — Evident's one-and-only runtime kernel.

Takes an SMT-LIB body and runs it as an FSM. Each tick:
  1. Pin `is_init` and every state pair's `_X` to its previous tick's `X`.
  2. Solve. Z3 finds the satisfying assignment, which is `this tick's state`.
  3. Read any emitted effects (a `(Seq Effect)` channel named `effects` or
     `*_effects`) and execute them via the registered effect handlers.
  4. Halt when both:
       - no state pair changed (`X` equals last tick's `X`), AND
       - no effects were emitted.
  5. Otherwise loop with the new state.

This file knows about exactly one effect: `LibCall`, dispatched to
ffi.libcall. All other "effects" patterns (CellRead, CellWrite, channel
operations, anything else) live in Evident-level libraries that lower to
LibCall — see docs/runtime-architecture.md.
"""
import re

import z3

from ffi import libcall


def seq_as_list(seq_expr):
    """Z3 sequence literal → Python list of element ASTs. Walks the seq.unit
    / seq.++ / seq.empty form Z3 produces for concrete sequence values."""
    decl = seq_expr.decl().name() if seq_expr.num_args() > 0 else None
    if seq_expr.num_args() == 0:
        return []  # empty sequence
    if decl == "seq.unit":
        return [seq_expr.arg(0)]
    if decl == "seq.++":
        out = []
        for i in range(seq_expr.num_args()):
            out.extend(seq_as_list(seq_expr.arg(i)))
        return out
    # Fallback: single value treated as unit.
    return [seq_expr]


def default_for(sort):
    """Sort-shaped zero. Used to initialize non-state, non-effects consts
    (FFI return registers) before any libcall has populated them."""
    if sort == z3.IntSort():    return z3.IntVal(0)
    if sort == z3.BoolSort():   return z3.BoolVal(False)
    if sort == z3.RealSort():   return z3.RealVal("0.0")
    if sort == z3.StringSort(): return z3.StringVal("")
    if sort.kind() == z3.Z3_SEQ_SORT: return z3.Empty(sort)
    return None


class Runtime:
    """The trampoline. Construct with an SMT-LIB body string; call `run()`
    to tick to halt and return the final-tick model."""

    def __init__(self, body):
        # Touch-all-consts trick: every declared const gets a trivial
        # (= n n) clause so the consts appear in the assertion graph and
        # are discoverable by walking. Also keeps Z3 from dropping them
        # during preprocessing.
        names = re.findall(r"\(declare-const\s+([^\s()]+)", body)
        if names:
            body += "\n(assert (and " + " ".join(
                f"(= {n} {n})" for n in names) + "))\n"

        self.s = z3.Solver()
        self.s.from_string(body)

        # Discover declared consts by walking every assertion's AST.
        # Z3's Python Solver doesn't expose declared functions/consts
        # directly; this keeps the runtime self-contained.
        self.const = {}
        self.sorts = {}
        for asrt in self.s.assertions():
            self._collect_consts(asrt)

        # State pairs: any `_X` declared alongside a matching `X`.
        self.state = [(f"_{n}", n) for n in self.sorts
                      if f"_{n}" in self.sorts]

        # Effects channels: anything named `effects` or `*_effects`.
        self.effects_vars = sorted(n for n in self.sorts
                                   if n == "effects" or n.endswith("_effects"))

        # Special names (not defaulted, not pinned to defaults each tick).
        self.special = ({n for pair in self.state for n in pair}
                        | {"is_init"} | set(self.effects_vars))

    def _collect_consts(self, expr):
        """Walk a Z3 AST; record every uninterpreted 0-ary constant."""
        if z3.is_const(expr) and \
           expr.decl().kind() == z3.Z3_OP_UNINTERPRETED:
            name = expr.decl().name()
            if name not in self.sorts:
                self.sorts[name] = expr.sort()
                self.const[name] = expr
            return
        for i in range(expr.num_args()):
            self._collect_consts(expr.arg(i))

    def run(self):
        """Tick to halt. Returns the model from the halting tick."""
        # `given` accumulates: state-pair carryover (`_X` ← previous `X`),
        # FFI return-slot bindings (from libcall ok_dest/err_dest), and —
        # on tick 1 only — `_X` defaulted to its sort's zero so that FTI
        # bodies can rely on a defined initial state (empty Seq, 0 Int,
        # false Bool, etc.) without having to spell it out in every body.
        # Subsequent ticks rely on state-pair carryover, not these defaults.
        given = {}
        for _p, p in self.state:
            d = default_for(self.sorts[_p])
            if d is not None:
                given[_p] = d

        is_init = z3.BoolVal(True)
        last_model = None
        while True:
            pins = [self.const["is_init"] == is_init] + \
                   [self.const[n] == v for n, v in given.items()]
            result = self.s.check(*pins)
            if result != z3.sat:
                # FSM has no valid next transition — treat as a halt.
                # On the very first tick this is a real error (the body
                # itself is unsatisfiable); after that, it's the FSM's
                # natural end-of-life.
                if last_model is None:
                    raise RuntimeError(
                        f"first tick is UNSAT — body is unsatisfiable:\n{self.s}")
                return last_model
            m = self.s.model()
            last_model = m

            # Compute next-tick state values from this tick's model.
            next_state = {_p: m.eval(self.const[p], True)
                          for _p, p in self.state}

            # Dispatch effects; may inject results into `given`.
            had_effects = self._dispatch_effects(m, given)

            state_changed = any(p not in given or not z3.eq(next_state[p], given[p])
                                for p, _ in self.state)
            if not state_changed and not had_effects:
                return m

            given.update(next_state)
            is_init = z3.BoolVal(False)

    def _dispatch_effects(self, m, given):
        """Read every effects channel; execute LibCall effects; route results
        into `given` so the next tick sees them."""
        had_any = False
        for ev in self.effects_vars:
            effs = seq_as_list(m.eval(self.const[ev], True))
            if not effs:
                continue
            had_any = True
            for eff in effs:
                name = eff.decl().name()
                if name == "LibCall":
                    # The args field is `(Seq FFIArg)` — a Z3 sequence
                    # value. `seq_as_list` walks the seq.unit/seq.++
                    # form to get a Python list of FFIArg ASTs.
                    ok, err = libcall(
                        eff.arg(0).as_string(),     # lib
                        eff.arg(1).as_string(),     # sym
                        eff.arg(2).as_string(),     # sig
                        seq_as_list(eff.arg(3)),    # args (Seq of FFIArg ASTs)
                    )
                    ok_dest, err_dest = eff.arg(4).as_string(), eff.arg(5).as_string()
                    if ok_dest  and ok  is not None: given[ok_dest]  = ok
                    if err_dest and err is not None: given[err_dest] = err
                # Any other effect constructor is silently ignored. This is
                # intentional: unknown effects are an Evident-level concern;
                # add new top-level effect kinds only when something genuinely
                # cannot be expressed as a LibCall.
        return had_any
