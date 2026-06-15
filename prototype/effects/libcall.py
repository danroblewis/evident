"""libcall — generic FFI (the old LibCall effect) via a UserPropagateBase.

Prettified constraint model:

    ret = libcall("answer")

`libcall(name)` is the effect: an uninterpreted function the runtime interprets by
dispatching to a registered host/native function (the FFI table) — exactly how the
old Effect enum's LibCall(name, args) dispatched through libffi. At the commit
point the propagator looks the call up and runs it. Run:  python3 -m effects.libcall
"""
import os
import z3
from effects.runtime import EffectProp

# ── the FFI table: name -> host function. Generic; register anything. ──
FFI = {
    "answer": lambda: 42,
    "pid": os.getpid,
    "double": lambda n: n * 2,
}

# ── the constraint model: the program makes one libcall ──
libcall = z3.Function("libcall", z3.StringSort(), z3.IntSort())   # name -> result
ret = z3.Int("ret")
s = z3.Solver()
s.add(ret == libcall(z3.StringVal("answer")))


class LibCallRuntime(EffectProp):
    def __init__(self, s, name, args=()):
        super().__init__(s)
        self.name, self.args, self.done = name, args, False

    def on_final(self):                     # commit point: dispatch the FFI call
        if self.done:
            return
        self.done = True
        result = FFI[self.name](*self.args)             # the generic FFI dispatch
        print(f"[ffi] libcall {self.name}{self.args} -> {result!r}")


if __name__ == "__main__":
    LibCallRuntime(s, "answer")
    s.check()
