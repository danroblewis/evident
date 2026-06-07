# Project handoff — what to know, what to do next

This is the entry point for an agent picking up the project cold. Read
it in full before doing anything. Then read `CLAUDE.md` (project
north star) and `STATE.md` (most recent verified state). Then check
recent commits with `git log --oneline -20` to see what's actually
landed.

## What Evident is

Constraint programming language. Programs are collections of
constraints over named variables. A Z3 SMT solver finds satisfying
assignments. The central abstraction is `claim` / `type` / `schema`:
a named set defined by membership conditions.

Compilation pipeline:
```
source.ev ──→ flatten ──→ kernel + compiler.smt2 ──→ output.smt2 ──→ kernel ──→ exit / stdout
```

- `kernel/` (~880 LOC Rust): trampoline + libffi + Z3 wrapper. Reads
  a .smt2 file, drives a multi-tick FSM (per-tick: parse pins, solve,
  read model, dispatch effects). The minimal native runtime.
- `compiler.smt2` / `sample.smt2` (committed artifacts at repo root):
  the self-hosted compiler + sat-check driver, compiled from
  `compiler/*.ev` once.
- `compiler/*.ev`: the Evident-source compiler. The .smt2 artifacts
  are built from these.

## Critical context

### Bootstrap is gone — the seam is the build path

There used to be a Rust crate `bootstrap/runtime/` that compiled
`.ev` → `.smt2`. It was deleted (commit 76dc491), restored as a
crutch (c83afb1), re-deleted (c218dca). Don't restore it. The dev
loop is now:

- Run an `.ev` program: `kernel/target/release/kernel <file.smt2>`
  after producing the `.smt2` via the seam wrapper.
- Compile `.ev` → `.smt2`: `scripts/build-sample-smt2-candidate.sh`
  (builds `sample_new.smt2` via `kernel + compiler.smt2`; promote
  with `mv` only when tests pass). For other files, the wrapper at
  `scripts/evident-self bin` returns an ephemeral script that runs
  `flatten | kernel compiler.smt2`.

Git is the safety net. If you produce a broken `sample.smt2` or
`compiler.smt2`, `git checkout <file>` restores. **Never push a
broken artifact** — test before `mv`-ing a candidate into place.

### Memory growth — SOLVED (2026-06-07, commit 4552527)

The 2+ GB / 10+ min / OOM-killed seam compiles were NOT a
pin-string or startup-simplify problem. Root cause: `16eea4d`'s
persistent solver put every per-tick check-sat on Z3's raw
incremental core, which gets NO preprocessing. compiler.smt2's
7851 ground functional asserts solve in 0.7 s with preprocessing
(solve-eqs eliminates every variable) but the incremental core
churns 12.9M added-eqs and never terminates — 153 GB RSS observed.
"Stuck in startup body-simplify" was this, misattributed; startup
is instant (verified with EVIDENT_PHASE_TRACE=1).

Fix: **Mech T** (default since 4552527) — fresh
`Z3_mk_solver_from_tactic("default")` per tick, cached body ASTs +
pins asserted fresh, solver freed at tick end. Verified end-to-end:
smoke_effects.ev compiles (~6700 ticks, ~161 ms/tick, RSS bounded
~1.5 GB) and the emitted program runs, exit 0.

Still-relevant fix paths, now for SPEED rather than survival:
  1. **Functionizer coverage** (tracked task): it refuses
     compiler.smt2 ("an output had no covering assignment") so all
     7852 asserts stay residual and every tick is a full Z3 solve.
     Covering them collapses ~18 min compiles to seconds.
  2. **Pivot `compiler/translate_*.ev` to build Z3 ASTs via libcalls**
     (`docs/plans/z3-sugar-inventory.md`). `compiler/translate_arith.ev`
     is done (commit `7278bdb`); ~55 BuildZ3* claims remain. Long-term
     architecture direction.
  3. **FTI Stack pivot** (`docs/plans/sample-ev-fti-pivot.md`):
     DEPRIORITIZED — the memory cliff it targeted is gone. Term-table
     growth is ~30-60 KB/tick now. Revisit only if compile length
     demands it.

### Architecture direction (long term)

Stop having the compiler build SMT-LIB text. Have it build Z3 ASTs
directly in memory via libcalls (`BuildZ3MkAdd`, `BuildZ3MkEq`,
etc.), then ask Z3 to serialize via `Z3_ast_to_string` for output.

This is what `compiler/translate_arith.ev` already does — it emits
`LibCall("libz3", "Z3_mk_*", ...)` effects to build the AST in
memory. The hand-written proof at
`tests/kernel/wave-5a-arith/translate_arith_via_z3.smt2` works
end-to-end (exits 0, builds `(+ 3 (* 4 5))` in Z3's memory, asks
Z3 to print it back, compares string).

Why this matters:
- Every Z3 predicate is automatically available — no per-op
  translation case needed (the wave-5-style "we listed 26
  predicates" becomes "we don't write SMT-LIB, Z3 does").
- No string-escape bugs.
- Compiler shrinks dramatically — translate_*.ev files become Z3
  AST builders instead of text emitters.

The other translate_*.ev files (`translate_bool`, `translate_ctor`,
`translate_match`, `translate_seq`, etc.) all still build strings.
Pivoting each one is the work.

## What's working end-to-end (verified)

- Wave 5 a-d Evident-source proofs all exit 0 (commits 8bf39be,
  70410f9, c007bc2, 9fcee9e, bfaa09a). These are the foundational
  primitives: libz3 from Evident, libffi from Evident, the
  functionizer recognizer leaf, codegen via asm→dylib→ffi_call,
  evidentc cache.
- `compiler/translate_arith.ev` pivoted to build Z3 ASTs (7278bdb).
- `stdlib/fti/token_stack.ev` (3898806): TokenStack FTI runtime
  proven (test fixture exits 0).
- `kernel/src/libcall.rs`: `__mem`, `__dlsym`, `__cstr` pseudo-libs
  for FTI use.
- 22+ Z3 predicates work via bootstrap's translation path (the
  bootstrap fix in commit c817c6c lives only in the committed
  `sample.smt2` — see "Known gotcha" below).

## Tasks (use `TaskList` if available)

### Highest priority — compile speed (memory is solved, see above)

1. **Functionizer coverage for compiler.smt2** — it refuses with
   "extract_program: an output had no covering assignment", leaving
   all 7852 asserts residual (full Z3 solve every tick, ~161 ms).
   `EVIDENT_FUNCTIONIZE_STATS=verbose` + the `[functionizer-why]`
   stuck-vars report show which output lacks coverage. Fixing this
   collapses ~18 min seam compiles to seconds.
2. ~~EVIDENT_NO_PRESIMPLIFY measurement~~ — moot: with z3 4.15.4
   the presimplify pass takes 0.1 s.
3. ~~Pin-cap measurement~~ — deprioritized: post-Mech-T term-table
   growth is ~30-60 KB/tick (~400 MB/compile), bounded enough.

### Medium priority — architecture direction

4. **Pivot `compiler/translate_bool.ev`** next (easiest, smallest
   surface). Pattern is in `compiler/translate_arith.ev`. The
   sugar inventory is at `docs/plans/z3-sugar-inventory.md` — every
   op needed by bool is in §4.
5. **Pivot remaining `translate_*.ev` files** progressively. The
   spicy ones are `translate_ctor.ev` and `translate_match.ev`
   (datatypes — §7 of the inventory has the marshaling shape).

### Tracked tasks (status as of handoff)

- #353 PENDING: Port `expr_as_var` extension into `compiler/sample.ev`
  (the bootstrap-Rust fix that lives only in the committed
  `sample.smt2`, will be lost on next rebuild).
- #358 PENDING: `TokenList` → FTI Stack pivot (the big refactor).

### Don't do

- Don't restore bootstrap.
- Don't add Python anywhere.
- Don't edit `kernel/` to add language-runtime features that
  belong in `compiler/*.ev` or `stdlib/*.ev` — only add kernel
  capabilities the runtime needs (Z3 lifecycle, FFI dispatch,
  trampoline, the `__mem`/`__dlsym`/`__cstr` pseudo-libs).
- Don't overwrite committed `.smt2` artifacts mid-iteration. Build
  candidates with `scripts/build-sample-smt2-candidate.sh`; promote
  only when tests pass.
- Don't claim work is done without verification. The user values
  honest reporting much more than false claims of completion.

## How to verify your work

For a fresh `.ev` test fixture:
```
flat=$(mktemp); scripts/flatten-evident.sh path/to/file.ev > "$flat"
# Compile via the seam — the second stdin line is the CLAIM NAME and
# must match the fixture (test_hello.ev's claim is `hello`, NOT
# `main`!). A nonexistent claim "succeeds" with a 12-line stub
# (empty state-fields, max-effects 0) — do not misread that as a
# translator bug; it cost an afternoon once.
printf '%s\nmain\n' "$flat" | kernel/target/release/kernel compiler.smt2 > /tmp/out.smt2
# Run the result:
kernel/target/release/kernel /tmp/out.smt2; echo "exit: $?"
rm "$flat"
```

`EVIDENT_PHASE_TRACE=1` on the kernel prints startup-phase markers,
tick progress, and per-effect dispatch (ReadLine/ReadFile/Exit) to
stderr — first thing to reach for when a seam run looks stuck.

For the smoke test: `scripts/run-seam-smoke.sh` (~4 min baseline).

For lang tests: `scripts/run-lang-tests.sh` (slow — 15+ min per
test through the seam).

## Critical reading list (in this order)

1. `CLAUDE.md` — project north star, freeze rules, language spec.
2. `STATE.md` — current state in prose.
3. `docs/plans/post-cutover-roadmap.md` — wave 5 a-d roadmap.
4. `docs/plans/sample-ev-fti-pivot.md` — the FTI cascade plan + the
   "honest accounting" of the per-sub-claim cost.
5. `docs/plans/z3-sugar-inventory.md` — what each `translate_*.ev`
   needs to pivot.
6. `docs/plans/wave-5a-z3-in-evident.md` through `wave-5d-...` —
   the foundational wave 5 design docs.
7. `compiler/translate_arith.ev` + `tests/kernel/wave-5a-arith/translate_arith_via_z3.smt2`
   — the reference implementation for the "build Z3 model" pattern.
8. `git log --oneline -30` — what's actually been committed
   recently. Read commit messages — they explain the *why* behind
   most decisions.

## Background orchestration (subordinate sessions)

`docs/briefings/orchestrator.md` documents a pattern for
dispatching work to isolated subordinate `claude -p` sessions
running in their own git worktrees. **The patterns are alive and
usable**; the *specific goal* the briefing was written for
(deleting bootstrap) is obsolete since bootstrap is already gone.

What's still applicable from that briefing:

- `scripts/coordinator.sh status` and `scripts/coordinator.sh
  launch docs/briefings/tasks/NN-name.md` for spawning subagents
  in isolated worktrees.
- The "write a terse task spec, never read the subagent's full
  transcript, read its final report + the files it produced"
  context-preservation pattern.
- The "merge from `agent-NN-name` branch, run `./test.sh`, push to
  main" merge workflow.
- The `<<autonomous-loop-dynamic>>` wakeup pattern for
  long-running coordinated work.

What's obsolete:

- The deletion goal itself + `scripts/check-deletable.sh` (gone).
- References to `docs/plans/DELETION-CHECKLIST.md` (likely gone).
- The "freeze rules" framing — bootstrap is already deleted, not
  frozen.

In-process subagents via the `Agent` tool also work fine for
shorter tasks (used twice this session — once for the
`translate_arith` pivot, once for a verification check). Use those
for tasks under a single context window; use the
`scripts/coordinator.sh` flow for tasks that need a fresh process
with isolated git state. The coordinator flow has resumed once or
twice when the parent session crashed, so it's resilient.

## Tone the user prefers

- Honest reporting of what worked, what didn't, what's unverified.
- Don't claim completion without verification — the user checks.
- When something doesn't work, surface it directly with the
  evidence and propose options.
- Long-running task is fine — "we have all the time in the world"
  was a user quote. Don't optimize for short sessions.
- Default to small, atomic commits that work in isolation. Don't
  bundle 5 unrelated changes.

## Known gotchas

- **`expr_as_var` Rust fix** — committed `sample.smt2` has it baked
  in, but the SOURCE for it lived in bootstrap's Rust (gone). Future
  rebuilds via the seam will LOSE the capability until it's ported
  to `compiler/sample.ev`. Track is task #353.
- **macOS allocator never returns freed memory** to the OS. RSS is
  high-water-mark. Freeing Z3 contexts doesn't drop RSS. Don't
  conclude "I freed it" from RSS readings.
- **`cmd1 | cmd2` env vars** only apply to `cmd1` in bash —
  `VAR=val cmd1 | cmd2` means `cmd2` doesn't see `VAR`. Use
  `env VAR=val cmd2` or wrap in a subshell.
- **Rust stderr to file is fully buffered**, flushes at process
  exit. Per-tick `eprintln!` won't appear until the kernel exits.
  For long-running diagnostics, write to a file directly or use
  `cargo run` with stderr to terminal.
- **The kernel's main thread has a 128 MB worker stack** (commit
  b1a38b9) to prevent stack overflow on real-size compiler.smt2
  inputs. Don't reduce it unless you know what you're doing.

## Quick-start commands

```bash
cd /Users/danroblewis/evident   # or wherever the repo lives

# Build kernel
(cd kernel && cargo build --release)

# Run an existing test
kernel/target/release/kernel tests/kernel/wave-5a/z3_solve_x42.smt2

# Check status
git status; git log --oneline -10

# Run all tests (slow)
./test.sh

# Build a candidate sample.smt2 (slow + memory-heavy)
scripts/build-sample-smt2-candidate.sh /tmp/sample_new.smt2
```
