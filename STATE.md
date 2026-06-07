# STATE

## The "memory growth" problem is solved (2026-06-07)

The multi-GB / hours-long / OOM-killed seam compiles were **not** a
pin-string or startup-simplify memory problem. Root cause, found by
phase-tracing a run in a Linux container:

Commit `16eea4d` ("persistent solver + push/pop") put every per-tick
`check-sat` on Z3's **raw incremental smt core, which gets no
preprocessing**. compiler.smt2's body is 7851 ground functional
asserts (zero quantifiers) — `solve-eqs` eliminates every variable
and the preprocessed solve is **0.7 s** (z3 CLI, verified on both
4.8.12 and 4.15.4), but the incremental core churns 12.9M added-eqs
on the same formula and **never terminates** (153 GB RSS observed
before kill). HANDOFF's "stuck in startup body-simplify" diagnosis
was this, misattributed: startup is instant; tick 0's check-sat was
the thing that never returned. Every post-16eea4d observation
("presimplify takes minutes", "pin-cap never measurable", "real
load stuck at startup") traces to this.

**The fix — Mech T (commit `4552527`, new default):** fresh
`Z3_mk_solver_from_tactic("default")` per tick; cached body ASTs +
pin ASTs asserted fresh each tick; solver freed at tick end. Every
tick gets the full preprocessing pipeline. This restores the
pre-16eea4d per-tick discipline (a fresh non-incremental solver is
the combined solver, i.e. the tactic path) while keeping the
cached-AST reuse. `EVIDENT_PIN_MECH=A|B` still selects the old
mechanisms for comparison.

## Verified end-to-end (Linux container, 24-core aarch64, z3 4.15.4)

- `tests/seam/smoke_effects.ev` seam compile: ~6700 ticks at
  ~161 ms/tick (~18 min wall), RSS bounded at ~1.5 GB (was:
  unbounded growth, never finished). Emitted .smt2 contains the
  full effects body and **runs under the kernel, exit 0**.
- Deterministic: repeated compiles produce byte-identical output.
- `./test.sh --rust-only` green.

**Update, same day: the functionizer now covers compiler.smt2**
(commit `c8e7d9b` — five fixes: recognizer constructor from decl
parameters, XOR-shape intermediate capture, tick-0 carry seeding
from the verify model, guard-tree recursion for nested else-if
effect writers, prev_results threaded into the fast path). With it:

- Seam compile: **~35 s** (was ~18 min Z3-path) — ~5 ms/tick
  interpreted, ZERO per-tick Z3 fallbacks, 7852 steps extracted
  (810 JIT / 6450 interp / 45 residual predicates).
- Output byte-identical to the Z3-path ground truth; emitted hello
  prints "hello world" exit 0; canonical seam smoke passes in 35.6 s.
- The Z3 path (Mech T) remains the verification baseline and the
  fallback for any tick the fast path refuses.

## Gotcha that cost an afternoon

The emit claim name must match the fixture: `test_hello.ev`'s claim
is `hello`, not `main`. Asking the seam for a nonexistent claim
produces a structurally valid **12-line stub** (empty state-fields,
max-effects 0) and exits 0 — easy to misread as a translator bug.
HANDOFF's verify recipe hardcoded `main`.

## Environment note (container vs Mac)

The dev container's Debian bookworm ships z3 4.8.12 (2021), which
dies with "Overflow encountered when expanding vector" under load.
Dockerfile.dev now installs the official Z3 release per-arch
(arm64 → 4.15.4, amd64 → 4.14.1; newest whose glibc floor fits
bookworm's 2.36). `kernel/src/libcall.rs` had a `*const i8` that
broke the Linux aarch64 build (c_char is u8 there) — fixed.

## Diagnostics added (kernel, env-gated)

- `EVIDENT_PHASE_TRACE=1` — startup-phase markers (parse /
  presimplify / decls / solver setup / functionize), tick-progress
  lines, and per-effect dispatch logging (ReadLine/ReadFile results,
  Exit codes). This is what found the misattribution.
- `EVIDENT_NO_PRESIMPLIFY=1` — moot on z3 4.15.4 (presimplify is
  0.1 s); kept as a switch.
- `EVIDENT_PIN_DEPTH_CAP` — deprioritized; per-tick term-table
  growth is now ~30-60 KB/tick (~400 MB per compile).

## What runs the project

- `kernel/` — ~1 kLOC Rust (Cargo crate).
- `compiler.smt2` — ~2 MB / 42k lines of SMT-LIB at the repo root.
  Committed artifact; rebuilding it from `compiler/compiler.ev` is
  the wave-5 closure.
- `sample.smt2` — sister artifact for the sat-check verb.
- `scripts/evident-self bin` — single resolution point for the
  compiler CLI; every test/bench script asks it.
- `compiler.smt2.evidentc` — functionize-simplify cache side-car,
  regenerated locally (not committed; keyed on src-hash + codegen
  version — NOT keyed on Z3 version, keep in mind when swapping Z3).

## What's next

1. **Run the full test suite** (`./test.sh`) — the kernel/lang
   phases that were ~2 hours at 3-4 min/fixture should now be
   minutes at ~35 s/fixture. Expect and triage any divergences.
2. Port `expr_as_var` into `compiler/sample.ev` (pre-existing task,
   unchanged).
3. `translate_bool.ev` pivot to Z3-AST building (wave-5 direction,
   reference: `compiler/translate_arith.ev`).
4. TokenList → FTI pivot: deprioritized — the memory cliff it
   targeted is gone; revisit only if compile times demand it.

`docs/plans/post-cutover-roadmap.md` still sequences waves 5a-5d.

## Operational guards retained

- `scripts/mem-cap.sh` — polling watchdog (default 12 GB).
- `scripts/run-{lang,kernel}-tests.sh` default parallelism: 4.
- `tests/conformance/features/runner.sh` known-fails allowlist for
  `IMPL=selfhost` (genuine arithmetic-in-ctor gaps).

## Open known issues

- `compiler.smt2` cannot yet compile arithmetic inside constructor
  args (`Exit(3 + 4)` emits `(Exit 3)`). 16 conformance features
  allowlisted; real fix is in `compiler/translate_ctor.ev`'s
  `RenderExprL0`.
- Per-emit wall-clock ~35 s in the container on the functionized
  fast path (was ~18 min Z3-path, ~3:40 on the Mac pre-16eea4d).

## compiler2 milestone (2026-06-08): first census fixtures green

compiler2/driver.ev (P3a, merged) compiles real .ev sources via
Z3-AST building end-to-end: lex (reused) → parse → work-stack walk
→ P2 per-node claims → Z3_solver_to_string emit. 026-arithmetic-add
and 008-boolean-and — census FAILURES under the legacy artifact —
compile through it and RUN with expected exits (independently
re-verified on main). The dropped-compound-expression class is gone
by construction. Descopes + next steps:
docs/plans/compiler2-driver-notes.md. The census
(docs/plans/conformance-census-2026-06-07.log, 14/138) is the
scoreboard; widening the driver's surface is the work.

Update (P3b merged): compiler2 now flips 18 census fixtures
(016 in P3b: memberships, comparisons, chained, implies forms,
bool-as-constraint — incl. 2 formerly-vacuous UNSATs now genuinely
solved; spot-verified 026/037 independently on main). Next per
driver-notes: Pratt expression parser (the shape zoo is at its
limit) + FTI lexer pivot (token_stack.ev) before big sources.
Driver entry claim is `driver_main`.
