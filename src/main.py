#!/usr/bin/env python3
"""Evident CLI.

Usage:
    python3 src/main.py FILE.ev               # parse, transpile, run
    python3 src/main.py --emit-smt FILE.ev    # just print the SMT-LIB

The runtime is the trampoline in runtime.py; the FFI bridge is ffi.py;
the parser is parser.py; the transpiler is transpile.py. Everything
else is library code in stdlib/ (written in Evident).
"""
import os
import sys

import z3

from parser import parse
from transpile import transpile, register_ftis
from runtime import Runtime


def _load_prelude():
    """Parse every .ev file in prelude/ and register any FTI declarations.

    Run before transpiling the user program so that FSM bodies which
    bind FTI-typed variables (e.g. `s ∈ Stack(Int)`) find the FTI
    declaration in the registry and can inline it.
    """
    here = os.path.dirname(os.path.abspath(__file__))
    prelude_dir = os.path.normpath(os.path.join(here, "..", "prelude"))
    if not os.path.isdir(prelude_dir):
        return
    for name in sorted(os.listdir(prelude_dir)):
        if not name.endswith(".ev"): continue
        with open(os.path.join(prelude_dir, name)) as f:
            try:
                ast = parse(f.read())
            except SyntaxError:
                # Documentation-only .ev files (like seq.ev) may not parse
                # as a valid program; skip them. Real FTI files must
                # parse — a SyntaxError there is a real bug surfaced at
                # the user's next run.
                continue
            register_ftis(ast["decls"])


def main(argv):
    if len(argv) < 2:
        print("usage: evident [--emit-smt] FILE.ev", file=sys.stderr)
        sys.exit(2)

    emit_only = False
    args = argv[1:]
    if args[0] == "--emit-smt":
        emit_only = True
        args = args[1:]
    if not args:
        print("evident: missing FILE.ev", file=sys.stderr)
        sys.exit(2)
    path = args[0]

    _load_prelude()

    with open(path) as f:
        source = f.read()

    ast = parse(source)
    body = transpile(ast)

    if emit_only:
        sys.stdout.write(body)
        return

    r = Runtime(body)
    m = r.run()

    # Print the model: every declared const except `is_init`, the `_*`
    # previous-tick halves of state pairs, and the `effects` channel.
    for n in sorted(r.sorts):
        if n == "is_init": continue
        if n.startswith("_"): continue
        if n in r.effects_vars: continue
        try:
            v = m.eval(r.const[n], True)
        except Exception:
            continue
        print(f"{n} = {v}")


if __name__ == "__main__":
    main(sys.argv)
