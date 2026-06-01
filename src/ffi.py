"""libcall — the FFI bridge.

A LibCall is `(LibCall lib sym sig args ok_dest err_dest)`. `libcall` resolves
the library + symbol via ctypes, marshals the Z3 arg values to C, calls the
function, wraps the C return back into a Z3 value.

Sig grammar: r(args) where r and each a in args is one of:
    i  int (C int)
    l  long (C long, 8 bytes — also used for opaque pointers)
    d  double
    s  char* (NUL-terminated C string)
    v  void (return only)

Hand-parsed (5 chars, regular pattern). The earlier SMT-LIB-based parse_sig
FSM is unnecessary at this scale.
"""
import ctypes
import ctypes.util
import re

import z3


_SIG_PAT = re.compile(r"^([ildsv])\(([ildsv]*)\)$")
_CTYPE = {"i": ctypes.c_int, "l": ctypes.c_long,
          "d": ctypes.c_double, "s": ctypes.c_char_p, "v": None}
_LIBS = {}


def _parse_sig(sig):
    m = _SIG_PAT.match(sig.strip())
    if not m:
        raise ValueError(f"bad sig: {sig!r}")
    return m.group(1), m.group(2)


def _load_lib(name):
    if name in _LIBS:
        return _LIBS[name]
    # Allow either a name resolvable via dlopen ("z3") OR a full path
    # ("/opt/homebrew/lib/libz3.dylib"). The full-path form is needed on
    # systems where ctypes.util.find_library can't locate the library.
    path = ctypes.util.find_library(name.removeprefix("lib")) or name
    _LIBS[name] = ctypes.CDLL(path)
    return _LIBS[name]


def _z3_to_py(val):
    """Z3 AST → Python primitive for libcall args."""
    sort = val.sort()
    if sort == z3.IntSort():    return val.as_long()
    if sort == z3.RealSort():
        return val.numerator_as_long() / val.denominator_as_long()
    if sort == z3.StringSort(): return val.as_string().encode()
    raise TypeError(f"can't marshal {sort} to Python primitive")


def _py_to_z3(c_char, ret):
    """C return value → Z3 AST."""
    if c_char == "v": return None
    if c_char in ("i", "l"): return z3.IntVal(int(ret))
    if c_char == "d":        return z3.RealVal(repr(float(ret)))
    if c_char == "s":
        s = ret.decode() if isinstance(ret, bytes) else (str(ret) if ret else "")
        return z3.StringVal(s)
    raise ValueError(f"unknown return char: {c_char}")


def libcall(lib_name, sym, sig, args):
    """Real C call via ctypes. Returns (ok_z3_ast, err_z3_ast).

    args is a list of Z3 ASTs. For the sig grammar's primitive chars, each
    arg's z3 value is marshaled to a C primitive of the matching type.
    """
    r_char, arg_chars = _parse_sig(sig)

    lib = _load_lib(lib_name)
    fn = getattr(lib, sym, None)
    if fn is None:
        return None, z3.StringVal(f"unresolved symbol {sym!r}")

    fn.argtypes = [_CTYPE[c] for c in arg_chars]
    fn.restype = _CTYPE[r_char]

    if len(args) != len(arg_chars):
        return None, z3.StringVal(
            f"arity: sig wants {len(arg_chars)} args, got {len(args)}")

    marshaled = [_z3_to_py(a) for a in args]
    ret = fn(*marshaled)
    return _py_to_z3(r_char, ret), None
