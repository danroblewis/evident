# Task: Implement `stdlib/fti/stack.ev` — a Stack FTI using direct libc FFI

## Why this matters

Recursive tree walks in the compiler (translator passes that walk
an arbitrary Expr / AST) need a work-stack pattern carried across
ticks. The naive approach — accumulate a `Seq` in state — works
mechanically but grows the Z3 model per push. The FTI pattern keeps
the backing memory in C-land via libc calls, while the Evident-side
state holds only a small handle (a pointer + length).

This is the FIRST FTI in the new repo. It proves the pattern works
on the current single-channel-effects kernel without any kernel
extensions. Subsequent FTIs (Queue, etc.) follow the same shape.

## Required reading

Before writing a single line of code, read:

1. `CLAUDE.md` (freeze rules).
2. `docs/plans/architecture-invariants.md` (Z3 lifecycle + FTI rules).
3. `legacy-python/docs/runtime-architecture.md` (the trampoline +
   LibCall + state-pair model that the current kernel implements).
4. `legacy-python/docs/fti-composition.md` (the FTI inlining
   pattern from tiny-runtime — the source of the Stack/Queue
   design).
5. `docs/notes/python-branch-techniques.md` (coordinator-level
   summary of what we learned; corrections to the naive FTI model).

Cite at least #2 and #4 in your report-back.

## What you're building

`stdlib/fti/stack.ev`: an Evident `claim` (or whichever shape fits)
named `Stack(T)` that:

- Maintains a logical `Seq(Int)` view of the stack contents
  (`contents`) across ticks via the state pair `_contents` /
  `contents`.
- Allows the host FSM to express transitions: push, pop, no-op.
  Use the same legal-transition disjunction shape from
  `legacy-python/docs/fti-composition.md` §"Stack." Note that the
  current kernel has ONE `effects` channel with single-writer +
  `++` composition; this differs from the multi-channel design in
  tiny-runtime, and the FTI body must respect it (per
  `docs/plans/architecture-invariants.md`).
- Emits `LibCall("libc", "malloc", …)` on the first tick to
  allocate a backing region (initial capacity ~1024 bytes is fine).
- On pushes: emits `LibCall("libc", "memcpy", …)` (or per-element
  store; pick what works with available signatures) to write the
  new value into the backing region at the right offset.
- On pops: no FFI call needed; the next-tick `_contents` is
  `init(contents)` (drop last) which already encodes the pop.
- On unsupported transitions (e.g. reverse): the legal-transition
  disjunction goes UNSAT — the FSM halts. This is the "honest
  declaration" pattern from tiny-runtime.

The Stack lives in `stdlib/`, not `compiler/`, because it is a
reusable language-level data structure (Stack is to the language
what Vec is to Rust), not a piece of the compiler being built.

## Where it gets used

Add a tiny test fixture `tests/kernel/test_fti_stack.ev` that:

- Imports `stdlib/fti/stack.ev`.
- Uses a Stack to push 3 values across 3 ticks, then pops 2.
- Verifies via diagnostic puts that `len(contents)` evolves
  correctly: 0 → 1 → 2 → 3 → 2 → 1.
- `-- expect:` header so the existing kernel test framework
  catches regressions.

The fixture goes in `tests/kernel/` (the existing kernel-test
location). Wire it into the existing test runner — no new
infrastructure needed.

## Acceptance

All of:

1. `stdlib/fti/stack.ev` exists.
2. `tests/kernel/test_fti_stack.ev` exists and passes.
3. `./test.sh` is fully green (61 kernel tests + 6 conformance
   features + lang + legacy Python all pass).
4. Your diff touches only:
   - `stdlib/fti/stack.ev` (new)
   - `tests/kernel/test_fti_stack.ev` (new)
   - Possibly `docs/plans/blocked-stack-fti.md` if you hit a
     genuine block.
5. Diff DOES NOT touch `bootstrap/`, `kernel/`, `compiler/`, or
   anything else.
6. The FTI body uses the single-channel `effects` model with `++`
   composition. Per `docs/plans/architecture-invariants.md` no
   namespaced `*_effects` channels.

## Forbidden

- Editing `kernel/`, `bootstrap/`, or `compiler/`.
- Adding new Python files.
- Adding a `__mem__` synthetic library (we use direct `libc`).
- Using the multi-channel `*_effects` pattern from tiny-runtime
  (kernel doesn't support it).
- Calling `.simplify()` anywhere (kernel invariant).
- Building a recursive constraint or rebuilding the Z3 model in
  the tick body (kernel invariant).

## Reporting back

Final message:

- Branch pushed.
- One sentence: did the Stack FTI pattern work on the current
  kernel, yes/no?
- File paths added.
- Output of the new `test_fti_stack.ev` (the diagnostic puts
  showing 0→1→2→3→2→1 or actual progression).
- `./test.sh` final line.
- `scripts/check-deletable.sh` blocker count after changes.
- Cite which docs justified your approach.

Do NOT paste full source; the coordinator reads files.

## If you get stuck

Common blockers, in expected order of likelihood:

1. The kernel re-parses the SMT-LIB body every tick. Per
   `docs/plans/audit-kernel-z3-lifecycle.md`, this is a known
   violation of the invariants. It does NOT block this FTI from
   working correctly; it just makes it slow. If your fixture's
   wall-clock is >5s for 3 ticks, note that in your report and
   continue.

2. libffi sig grammar (`v(ll)`, `i(s)`, etc.) for `libc::malloc`
   / `libc::memcpy` may need new shapes. Look at how existing
   `stdlib/kernel.ev` `BuildPrintln`/`BuildPrint`/`BuildTime`
   call libc. If a sig you need isn't expressible, that's a
   `docs/plans/blocked-stack-fti.md` write-and-stop.

3. The "first tick only" alloc emission pattern needs to use the
   `is_first_tick` Bool. See existing usage in test fixtures.

4. State carry of the `base` (malloc'd pointer) across ticks uses
   `Int` (it's a pointer-as-integer). Make sure `_base` carries
   correctly.

If any blocker stops you, write `docs/plans/blocked-stack-fti.md`
with what you tried and what would unblock you, then stop.
