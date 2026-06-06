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

### Memory growth is the major open problem

Running `kernel + compiler.smt2` to compile real-size programs
(e.g. `compiler/sample.ev`, `tests/kernel/test_translate_arith_via_z3.ev`)
uses 2+ GB RSS and 10+ minutes wall-clock, sometimes OOM-killed by
macOS jetsam. We've diagnosed the cause and tried fixes:

- **Root diagnosis**: `compiler.smt2`'s manifest lists 47 cons-list /
  datatype state-fields (27 `TokenList`, 17 `Token`, 3 misc). Every
  tick the kernel reads each whole list out of the Z3 model and
  re-emits it as a nested `(TLCons t1 (TLCons t2 ...))` pin string.
  Z3 parses each pin string into AST nodes which accumulate in
  the context's term hash-cons table. Plus a one-shot per-assertion
  `Z3_simplify` over 7800 body assertions at startup that ALSO grows
  the term store and takes minutes.

- **Things tried and committed (none of which actually fix the
  memory)**:
  - **Kernel push/pop scoping per-tick pins** (`16eea4d`):
    correctness-clean discipline, doesn't help — manages solver
    state, not the term hash-cons table.
  - **Periodic Z3 context teardown** (built then reverted, see commit
    history): doesn't help because macOS allocator doesn't return
    freed memory to the OS.
  - **`EVIDENT_PIN_DEPTH_CAP=<N>`** (`13b6464`): truncates cons-list
    pin rendering at depth N. Code is right (verified on a synthetic
    fixture in /tmp), but never measured under real load because
    real load is stuck in startup body-simplify, not per-tick. So
    pin-cap targets the wrong layer.
  - **`EVIDENT_NO_PRESIMPLIFY=1`** (uncommitted as of handoff —
    check `git status`): skips the per-assertion `Z3_simplify` pass
    at startup. This was being measured when the session ended.

- **Real fix paths** (documented in `docs/plans/`):
  1. **Pivot `compiler/*.ev` to use FTI Stack** (see
     `stdlib/fti/token_stack.ev`, `docs/plans/sample-ev-fti-pivot.md`)
     so `TokenList` state-fields become `(base ∈ Int, depth ∈ Int)`
     with contents in libc memory. Substantial multi-stage refactor;
     order matters (compiler.ev first, then one painful rebuild,
     then sample.ev). Stage 4 (sub-claim pivot) honest cost analysis
     in the doc.
  2. **Pivot `compiler/translate_*.ev` to build Z3 ASTs via libcalls**
     instead of building SMT-LIB strings by `++` concatenation
     (`docs/plans/z3-sugar-inventory.md`). `compiler/translate_arith.ev`
     is the first one done (commit `7278bdb`); ~55 BuildZ3* claims
     remain. This is the long-term architecture direction — the
     compiler stops needing to "know" SMT-LIB syntax, Z3 owns
     canonical serialization.

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

## Tasks (use `TaskList` if available; #353/358 still pending)

### Highest priority — the memory bound

1. **Continue measuring `EVIDENT_NO_PRESIMPLIFY=1`** (commit it
   first — currently uncommitted as of handoff). Run the seam smoke
   with it set, observe if startup is faster and whether
   correctness holds. If yes, this is a free win — make it the
   default for compiler.smt2 runs.
2. **If presimplify-skip helps startup but per-tick is still
   bottleneck**, the `EVIDENT_PIN_DEPTH_CAP=16` measurement
   becomes possible — diagnostic logging is in place (commit
   `13b6464` + uncommitted diag). Watch for `pin-cap-diag` and
   `truncating` events.
3. **If pin-cap helps** make it the default. If not, the FTI
   cascade in `docs/plans/sample-ev-fti-pivot.md` is the next path.

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
# Compile via the seam:
printf '%s\nmain\n' "$flat" | kernel/target/release/kernel compiler.smt2 > /tmp/out.smt2
# Run the result:
kernel/target/release/kernel /tmp/out.smt2; echo "exit: $?"
rm "$flat"
```

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
