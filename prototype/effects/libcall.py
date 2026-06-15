"""libcall — generic FFI (the old LibCall effect) via a UserPropagateBase.

Prettified constraint model:

    ret = libcall("getpid")

`libcall(name)` is the effect: an uninterpreted function the runtime interprets by
dispatching to a REAL native function, resolved by name out of libc through
ctypes (Python's libffi binding) — exactly how the old Effect enum's
LibCall(name, args) dispatched through libffi. At the commit point the propagator
resolves the symbol, sets its C signature, and calls it.
Run:  python3 -m effects.libcall
"""
import ctypes
import z3
from effects.runtime import EffectProp

# ── real FFI: resolve a libc symbol by name and call it (ctypes IS libffi) ──
_libc = ctypes.CDLL(None)                   # the process's C symbols
_SIGS = {                                   # C signatures: name -> (restype, argtypes)
    "getpid":  (ctypes.c_int,    ()),
    "getppid": (ctypes.c_int,    ()),
    "strlen":  (ctypes.c_size_t, (ctypes.c_char_p,)),
}


def ffi(name, *args):
    """Generic foreign call: resolve `name` out of libc, set its signature, call."""
    restype, argtypes = _SIGS[name]
    fn = getattr(_libc, name)               # the real symbol lookup (dlsym)
    fn.restype, fn.argtypes = restype, list(argtypes)
    return fn(*args)


# ── the constraint model: the program makes one libcall ──
libcall = z3.Function("libcall", z3.StringSort(), z3.IntSort())   # name -> result
ret = z3.Int("ret")
s = z3.Solver()
s.add(ret == libcall(z3.StringVal("getpid")))


class LibCallRuntime(EffectProp):
    def __init__(self, s, name, args=()):
        super().__init__(s)
        self.name, self.args, self.done = name, args, False

    def on_final(self):                     # commit point: dispatch the real FFI call
        if self.done:
            return
        self.done = True
        result = ffi(self.name, *self.args)             # actual native call into libc
        print(f"[ffi] libcall {self.name}{self.args} -> {result}")


if __name__ == "__main__":
    LibCallRuntime(s, "getpid")
    s.check()
    # the dispatcher is generic — the same path resolves any registered symbol:
    print("[ffi] libcall getppid() ->", ffi("getppid"))
    print('[ffi] libcall strlen(b"hello") ->', ffi("strlen", b"hello"))
