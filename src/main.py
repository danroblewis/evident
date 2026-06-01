#!/usr/bin/env python3
"""Evident CLI.

Usage:
    python3 src/main.py FILE.ev               # parse, transpile, run
    python3 src/main.py --emit-smt FILE.ev    # just print the SMT-LIB

The runtime is the trampoline in runtime.py; the FFI bridge is ffi.py;
the parser is parser.py; the transpiler is transpile.py. Everything
else is library code in stdlib/ (written in Evident).
"""
import sys

import z3

from parser import parse
from transpile import transpile
from runtime import Runtime


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
