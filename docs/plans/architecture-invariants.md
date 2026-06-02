# Architectural invariants for the kernel and compiler

User-confirmed invariants that constrain every subordinate session's
design space. Read these before designing anything that touches FSM
ticks, the Z3 model lifecycle, or FTI bodies.

## Z3 model lifecycle

1. **The Z3 model is built ONCE.** A program's constraint system is
   loaded into a Z3 context at startup — by parsing the program's
   SMT-LIB (current path) or by calling Z3 constructors through FFI
   (the Z3-FTI path; equivalent and the kernel doesn't care which).
2. **The model is REUSED across all ticks.** Per tick, the ONLY
   allowed change is *adding equality constraints to pin variables*
   (state-carry premises like `_x = 7`, `is_first_tick = false`,
   `last_results = ⟨…⟩`).
3. **No tick may rebuild the model.** Rebuilding is allowed only
   through an FTI that calls Z3's constructors via FFI; that is a
   separate sub-system from "the main FSM's tick."
4. **No tick may call `.simplify()` on the model.** Simplification
   is too expensive to run in the tick loop. Once an FSM is in its
   loop, it lives with the parsed-and-loaded form forever.
   **However: a single `.simplify()` pass BEFORE entering the tick
   loop IS allowed and desired** — that's setup work, not tick
   work. The kernel should simplify the body once after parsing,
   before any tick runs. The constraint above is about *per-tick*
   simplification. **IMPLEMENTED** (task #12): `kernel/src/tick.rs`
   runs `Z3_simplify` over each cached body assertion exactly once,
   before the loop; the simplified ASTs are what every tick re-uses.
   No per-tick simplify is introduced. The kernel ships two pin
   mechanisms, A (default) and B (`EVIDENT_PIN_MECH=B`,
   check-with-assumptions); a real-body benchmark (datatype-heavy
   lexer, 16–256 KB) found A 48–440× faster, so A is the default.
   See `docs/plans/kernel-fix-incremental-solving.md` §"UPDATE
   (task #12)".

Implication: subordinate sessions designing tick bodies must NOT
introduce constraints that *vary* in shape per tick. Only the values
of pinned variables vary. The body's *structure* is fixed at load
time.

Implication for the kernel: the loaded program's parse must NOT be
redone every tick. **FIX LANDED** in `kernel/src/tick.rs`: the body is
parsed ONCE and its asserted ASTs cached; each tick re-asserts the
cached ASTs (no re-parse) plus the equality pins into a fresh solver.
This satisfies invariant #1 (parse once). Note the landed form does
NOT use `push`/`pop`: the proposal's push/pop incremental mechanism was
implemented and measured first but regressed datatype-state fixtures
~36x (a kernel test timed out at 30s) because incremental mode forgoes
the one-shot preprocessing those growing pins need — so it was replaced
with the cached-ASTs mechanism, which keeps `./test.sh` green and is
faster than the prior re-parsing kernel. Full write-up (including the
deviation flag for the user) at
`docs/plans/kernel-fix-incremental-solving.md`; the original violation
is documented at `docs/plans/audit-kernel-z3-lifecycle.md`.

## Compiler output: SMT-LIB string OR Z3 AST (whichever is faster)

The compiler may emit:

- **SMT-LIB strings.** Kernel parses via Z3's SMT-LIB importer. This
  is the current `compiler/translate_*.ev` direction.
- **Z3 ASTs constructed via FFI.** Kernel skips parsing because the
  Z3 context is populated by the FTI's `LibCall("libz3", "Z3_mk_…", …)`
  sequence. This is the Z3-FTI Formula-builder described in
  `legacy-python/docs/fti-z3.md`.

Both paths produce the same Z3 model and the kernel accepts either.
Choice between them is a performance question, measured later. The
SMT-LIB path is the default and stays valid; the Z3-FTI path is an
optimization to explore in parallel.

## FTIs are pure Evident + FFI; no kernel additions

FTIs (Foreign Type Interfaces) are implemented as Evident claims in
`stdlib/fti/*.ev` or `compiler/fti/*.ev`. Their bodies are the same
shape as any other Evident claim. They produce effects that are
direct `LibCall`s into available C libraries (`libc`, `libz3`, …) —
no synthetic-library shims in the kernel, no namespaced channels.

If an FTI conceptually wants a separate effects channel, it shares
the host's single `effects` Seq via `++` composition with a single
top-level ternary whose branches are *literal* effect sequences.
The `match`-into-ternary-`++`-with-literal pattern is a current
translator constraint (a ternary as a `++` operand fails to
translate; `++` flattens at load time over literal operands only).
Concretely:

```evident
-- Host FSM:
effects = host_part ++ stack_part

-- Inside the Stack FTI body, the FTI exposes BuildXyz sugars that
-- produce literal LibCall Seqs. The FTI's effects expression is a
-- single ternary whose arms are concrete literals:
stack_part ∈ Seq(Effect) = (push_detected ? ⟨LibCall("libc", "memcpy", …)⟩
                          : ⟨⟩)
```

`stdlib/fti/stack.ev` (the first FTI, shipped) is the worked example
of this pattern. The key constraints discovered there:

- Seq(T) carried via state pair does not work for unbounded
  contents — use an enum cons-list + an `Int depth` for state carry.
  See `stdlib/fti/stack.ev` for the pattern.
- The FTI cannot own the host's `effects` channel directly
  (validators require a literal `effects =` in the host); the FTI
  exposes `BuildXyz` sugars and the host `++`-composes them.
- `match` cannot wrap a `++` expression — write a single ternary
  whose arms are the literal effect sequences.

That keeps the kernel's single-writer rule intact and adds no kernel
infrastructure.

## Single-channel effects + `++` composition

The kernel has one `effects` Seq per FSM. Multiple writers compose
with `++`. Single-writer rule prevents UNSAT-by-overconstrain.
Subordinate sessions designing FTIs and translator passes must
respect this — no introducing `*_effects` channels.

## State carry is via the `_<name>` convention

Top-level primitive fields (`Int`/`Bool`/`Real`/`String`) get a
companion `_<name>` field that is the previous tick's value, pinned
by the kernel before each solve. Sessions implement FSM memory
through this pattern, not through any kernel-side mechanism.

## What "the compiler" produces

A `.smt2` file that the kernel can read directly today (via Z3's
SMT-LIB importer), or eventually a stream of `LibCall("libz3", …)`
that builds the same Z3 model in memory. Either way the manifest
header tells the kernel which top-level fields are state, which is
`effects`, which is `last_results`. That header convention is
unchanged.

## Where these invariants are referenced

- This file is required reading in `docs/briefings/foundation.md`
  for any session touching the compiler, FTIs, or kernel architecture.
- `legacy-python/docs/runtime-architecture.md` and
  `legacy-python/docs/fti-composition.md` are the longer-form
  explanations; this file is the abbreviated rule-list.
- A session that violates these invariants (introducing tick-time
  `.simplify()`, adding `*_effects` channels, rebuilding the Z3
  model in the FSM body, etc.) has produced unusable work
  regardless of test status.
