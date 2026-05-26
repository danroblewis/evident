# SMT-LIB as the compile target — the north star for self-hosting

> **Status:** strategic direction (2026-05). Not yet implemented; this doc
> is the north star the leaf-pass self-hosting work has been building
> toward. Pick it up here.

## The thesis

**Evident compiles to SMT-LIB** — not to Rust, not to assembly. SMT-LIB is
the canonical intermediate representation / compile target of the runtime.

The pipeline we're aiming at:

```
Evident source → (parse) → AST → SMT-LIB → Z3 (AST) → functionizer (JIT/…) → native
```

The job of the "translate" layer becomes: **emit SMT-LIB**. Z3 ingests
SMT-LIB natively (`parse_smtlib2_string`), produces its AST, and from there
the existing pipeline (functionizers, JIT, optimizations) is unchanged.

## Why SMT-LIB, and not Rust or Evident-all-the-way

1. **Z3 is the one unavoidable dependency.** We will almost certainly
   always have a constraint solver under us. Even if we someday swap Z3 for
   a faster solver, or build our own, **SMT-LIB is the portable, solver-
   agnostic interface** — virtually every SMT solver imports it. So SMT-LIB
   is the layer that *outlives the solver choice*. Targeting it decouples
   the whole runtime from any single solver's C API.

2. **It's the obvious axis to refactor the Rust along.** The refactoring
   rule falls right out: **whatever can be implemented as a Z3 solve
   *should* be — and we refactor it out of Rust into SMT-LIB.** A Rust
   function that's secretly a constraint problem stops being imperative
   Rust and becomes a constraint system expressed in a standard format.

3. **The optimizations survive untouched.** This is the load-bearing
   point: the functionizers (Cranelift JIT, symbolic, GLSL, …) operate on
   the **Z3 AST**, and they don't care where that AST came from. The
   current Rust `translate` layer builds it via the C API; an SMT-LIB file
   builds the same AST via Z3's parser; a future Evident generator builds
   the same AST via SMT-LIB. **All three converge on the same Z3 AST**, so
   every JIT/optimization investment applies regardless of source. We were
   always parsing Z3 AST data — SMT-LIB just changes *how it's authored*,
   not what the optimizer sees.

## The staged refactor: Rust → SMT-LIB → Evident

SMT-LIB is the **waypoint** between "in Rust" and "self-hosted in Evident."
You don't jump straight to Evident; you decouple in two steps:

- **Step 1 — Rust → SMT-LIB.** Take a Rust function that is (or can be)
  a Z3 solve and lift its logic *out of Rust* into an SMT-LIB constraint
  system (a static file, or a small generator). The Rust shrinks; the logic
  now lives in the portable format. This step alone is valuable — it's the
  decoupling, and it doesn't require any Evident self-hosting.
- **Step 2 — SMT-LIB ← Evident.** Find the equivalent **Evident program
  that generates that SMT-LIB**. Now the source of truth is Evident; the
  SMT-LIB is its compiled output. This is the self-hosting step.

You can stop after step 1 and still have won (Rust → portable IR). Step 2
is the self-hosting payoff.

## The bootstrap — and why it isn't a wall

Self-hosting a compiler is always circular ("how do you compile GCC with
GCC"), and the answer is always the same: a **first version in another
language** bootstraps the next, then is no longer needed at runtime.

For us:
- **Stage 0** (exists): the Rust front end (parse + translate to Z3).
- **Stage 1**: an Evident program that emits SMT-LIB (AST → SMT-LIB text).
- Stage 0 **AOT-compiles Stage 1** — and *the functionizer is our
  bootstrap compiler*. Functionize the Stage-1 program into a native
  artifact, and that artifact *is* the translator now; it runs without the
  Rust one. The Rust stage 0 is then only needed to re-bootstrap from
  scratch (like rustc shipping a `stage0`), not at user runtime.

**The crucial condition:** the loop closes *only if the self-hosted stage
is compiled (AOT), not interpreted via the solver.* An interpreted Stage 1
would need the solver to run, which needs the translator, which is Stage 1
— infinite regress. Compiling it to native (functionizer) breaks the loop.
This is why "circular" was never a wall; it's a bootstrap, and the
functionizer is the loop-breaker.

## What stays Rust forever

- **Z3 itself** (the solver) and the FFI binding to it.
- **The FFI/IO kernel** — real side effects, async, OS bridges.
- **The parser** as the bootstrap seed (string → AST; the other circular
  half — needs a seed in another language, same as any self-hosting
  compiler's front-end bootstrap).

Everything between parse and solve — the translate logic — is the
refactoring target: Rust → SMT-LIB → Evident.

## The gates (in order)

1. **String-theory performance.** Generating SMT-LIB (and parsing Evident)
   is string-heavy; today that's the Z3 string-theory blowup that bit the
   leaf passes. Cheap string handling is the prerequisite.
2. **AOT-compile-the-front-end.** Functionizing a *whole translator
   program* (much bigger than a leaf pass) into a standalone native
   artifact — the bootstrap mechanism above.
3. **Compile-time, not run-time.** `Evident → SMT-LIB text → Z3 parse`
   adds a serialize+parse round-trip vs the current in-memory AST build, so
   it's a **compile-once / AOT** path, not a per-tick path. (Once Stage 1
   is AOT'd to native, steady-state can emit the compiled artifact directly
   and skip the SMT-LIB text — SMT-LIB is the clean *bootstrap* interface,
   not necessarily the steady-state one.) This dovetails with the
   "compile the constraint model to native" perf plan.

## Relationship to existing docs
- [`minimal-runtime.md`](minimal-runtime.md) — the ~11K-LOC, FFI-first
  target this serves: refactoring Rust → SMT-LIB is the mechanism.
- [`self-hosting-inventory.md`](self-hosting-inventory.md) — the per-file
  port ladder; reframe its targets as "what can become a Z3 solve / SMT-LIB
  generator," not just "what can be a stack-FSM."
- [`event-sources-as-evident.md`](event-sources-as-evident.md) — the same
  FFI-first move for the I/O side (a generic awaiter + Evident sources).
- The leaf-pass self-hosting (validate, subscriptions, toposort, generics,
  desugar, inject, introspect, pretty) is the warm-up: it proved the
  stack-FSM traversal mode and the marshaler. SMT-LIB-as-target is the
  endgame those built toward.

## One-line summary
**Evident compiles to SMT-LIB; Z3 (or any SMT solver) runs it; the
functionizers optimize the resulting AST regardless of source — so the
refactor is "lift Rust logic into SMT-LIB, then have Evident generate it,"
with the functionizer as the bootstrap compiler.**
