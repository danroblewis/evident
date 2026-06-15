# effects — dispatching effects during a Z3 solve (UserPropagateBase)

Three example programs where the program is a Z3 constraint model and the
**effects fire during `check()`**, dispatched by a `UserPropagateBase`
propagator — the "loop inside Z3" rather than a host tick loop. Each example file
has its prettified constraint model in the header comment.

- `runtime.py` — `EffectProp`, a thin base over `UserPropagateBase` (handles the
  push/pop/fresh scaffolding; you override `on_fixed`/`on_final`).
- `log.py` — logs each event as the solver fixes it, and at the complete model.
- `echo.py` — reads a line from stdin and prints it at the commit point.
- `libcall.py` — the old `LibCall` effect as **real** generic FFI: resolves a C
  symbol by name out of libc through `ctypes` (Python's libffi binding), sets its
  signature, and calls it (`getpid`/`getppid`/`strlen`) — the way
  `LibCall(name, args)` went through libffi.

```bash
python3 -m effects.log
echo "hello" | python3 -m effects.echo
python3 -m effects.libcall
```

**Environment caveat:** this repo's z3py is the 4.8.12 wrapper over libz3 4.15.4,
whose user-propagator ABI disagree — the `fixed` callback's term-id doesn't match
`add()`, so feeding a value *back* into the model (`propagate`) is unreliable
here. The effects themselves fire fine (that's what these demo); feeding results
back needs a matched z3 (pip `z3-solver` >= ~4.12). See
`../../docs/z3-effects-in-check.md`.
