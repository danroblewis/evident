"""log — simple logging via a UserPropagateBase.

Prettified constraint model:

    e0
    e0 ⇒ e1
    e1 ⇒ e2

The propagator logs each event as the solver fixes it during the solve, and logs
once the program reaches a complete model. Run:  python3 -m effects.log
"""
import z3
from effects.runtime import EffectProp

# ── the constraint model: three events, each implying the next ──
e0, e1, e2 = z3.Bools("e0 e1 e2")
s = z3.Solver()
s.add(e0, z3.Implies(e0, e1), z3.Implies(e1, e2))


class Logger(EffectProp):
    def on_fixed(self, value):
        print(f"[log] event fixed -> {value}")

    def on_final(self):
        print("[log] program reached a complete model")


if __name__ == "__main__":
    Logger(s).track(e0, e1, e2)
    print("[log] solving (effects fire during check) ...")
    print("[log] result:", s.check())
