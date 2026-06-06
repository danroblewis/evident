# STATE

## Post-bootstrap-deletion (re-deleted at <new-commit>; corrected understanding)

The producing path is `kernel + compiler.smt2` end-to-end. Bootstrap
was deleted (76dc491), restored (c83afb1) as a misjudged crutch
while iterating on Goal 1, then re-deleted with the corrected
understanding: **the dev loop is not fragile**. Git is the safety
net for `compiler/*.ev` edits; `compiler.smt2`/`sample.smt2` are
committed artifacts that can be restored or kept-as-is when a new
build is broken.

When the seam (`kernel + compiler.smt2`) can't compile a given
shape today, that is a capability gap in `compiler.smt2` to track
and fix at the source level (in `compiler/*.ev`) — not something
to route around by restoring bootstrap.

Known open capability gap: the `expr_as_var` fix from Goal 1 part 1
(commit c817c6c) lived in bootstrap's Rust, NOT in `compiler/*.ev`.
The current committed `sample.smt2` was produced with that fix
baked in; subsequent seam re-builds of `sample.smt2` from
`compiler/sample.ev` will lose the fix until the same logic is
ported to `compiler/sample.ev`. That porting is the right work and
is tracked as a follow-on task.

```
  source.ev ──→ flatten ──→ kernel + compiler.smt2 ──→ output.smt2 ──→ kernel ──→ exit / stdout
```

## What runs the project

- `kernel/` — ~880 LOC Rust (Cargo crate). Builds with the bundled
  linker patch (`scripts/cc-wrapper.sh`).
- `compiler.smt2` — ~2 MB / 42k lines of SMT-LIB at the repo root.
  Committed artifact; rebuilding it from `compiler/compiler.ev` is
  the wave-5 closure.
- `sample.smt2` — sister artifact for the sat-check verb.
- `scripts/evident-self bin` — single resolution point for the
  compiler CLI; every test/bench script asks it.

## Verified this session

- `tests/seam/smoke_effects.ev` compiles through the seam in
  ~3:40 wall-clock and emits the full effects body. Phase 6 of
  test.sh gates this on every run.
- `tests/kernel/test_hello.ev` compiles through the seam in
  ~3:45 and the kernel runs the emitted `.smt2` to print
  "hello world" exit 0.
- `./test.sh --rust-only` is green (4 kernel unit tests).

## Important behavior fix landed this session

`scripts/mem-cap.sh` (polling RSS watchdog) was spawning its child
with `"$@" &`. Bash's default for backgrounded jobs is to redirect
stdin from `/dev/null`, so kernel processes that read stdin via the
`ReadLine` effect saw instant EOF, halted at tick 1, and emitted a
truncated 11-line program. This looked exactly like a "compiler that
silently drops constructor arguments" — which is what STATE.md
(pre-fix) and a prior session's diagnosis said it was. It was not a
compiler bug. The two-character fix is `"$@" <&0 &`.

Side-effect: the mem-cap default cap was raised from 3 GB to 12 GB.
Real compiles peak around 4 GB; the tight cap was killing
legitimate runs.

## What's next

`docs/plans/post-cutover-roadmap.md` sequences the four feasibility
plans that are already on disk:

1. Wave 5a — Z3 wrapper in Evident (FFI to libz3). Solve loop HIGH;
   model-readback needs two named primitives.
2. Wave 5b — Trampoline + libffi in Evident. Path A (libffi as a C
   dep) HIGH and depends on 5a; Path B (mmap + mprotect codegen)
   MEDIUM and is the prerequisite for 5c option Z.
3. Wave 5c — Functionizer in Evident. Recognizer HIGH; codegen
   option X (emit asm → `as` → dlopen) HIGH and depends on 5b's
   dlopen sugar.
4. Wave 5d — AOT functionizer binary cache. MEDIUM. Materializes 5c
   into a side-car `.evidentc` format.

Suggested order: 5a → 5b → 5c → 5d. See the roadmap for cross-wave
blockers.

## Operational guards retained

- `scripts/mem-cap.sh` — polling watchdog (default 12 GB).
- `scripts/run-{lang,kernel}-tests.sh` default parallelism: 4
  (was sysctl `hw.activecpu` ≈ 12). Each kernel-on-compiler.smt2
  child can briefly use 3–6 GB; high fanout swamps the host even
  with the per-child cap.
- `tests/conformance/features/runner.sh` has a known-fails
  allowlist for `IMPL=selfhost` covering the genuine arithmetic-in-
  ctor cases that compiler.smt2 doesn't yet handle (a real
  capability gap, not the masquerading bug above).

## Open known issues (not blockers; documented for the next session)

- `compiler.smt2` cannot yet compile arithmetic inside constructor
  args (`Exit(3 + 4)` emits `(Exit 3)`). 16 conformance features
  fail under selfhost because of this. Allowlisted today; a real
  fix in `compiler/translate_ctor.ev`'s `RenderExprL0` extends the
  ctor-arg expression renderer.
- Per-emit wall-clock is ~3:40 on the smoke fixture. The kernel
  phase under full `./test.sh` (~140 fixtures × 3–4 min / 4
  parallel) is ~2 hours. Acceptable for now; targeted by wave 5d's
  AOT cache.
