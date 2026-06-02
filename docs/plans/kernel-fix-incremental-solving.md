# Kernel-fix proposal: incremental solving (one solver, push/pop per tick)

**Status:** LANDED (with a documented mechanism change — read the
"LANDED" section below before assuming push/pop). The fix removes the
per-tick re-parse of the SMT-LIB body (invariant #1), which was the
audit's stated dominant cost. The proposal's *push/pop incremental*
mechanism was implemented first, measured to regress badly, and
replaced with a *parse-once-into-cached-ASTs* mechanism that meets the
same invariant without the regression.

---

## LANDED — what was actually implemented (and why it differs)

The proposal sketched "one persistent solver, body asserted once at
base scope, `push`/`pop` the per-tick equality pins." That was
implemented exactly as written and measured. It **regressed
multi-tick fixtures ~36x** and a kernel test (`test_consolidated_lexer`,
~13 ticks) **timed out at 30s** (baseline: the whole 61-test kernel
suite runs in ~1.6s). So the literal push/pop form fails acceptance
criterion #2 of the task ("`./test.sh` fully green").

**Root cause.** The kernel carries FSM state across ticks by pinning
equalities. For datatype-typed state (e.g. the lexer's `TokenList`
enum), the carried value *grows each tick*, so the per-tick pins
include large nested datatype literals. A fresh one-shot solve (the
prior kernel, which re-parsed `body + pins` into a new solver each
tick) gets Z3's full preprocessing/simplification over those pins and
solves them fast. A *persistent* solver with `push`/`pop` is in
incremental mode and forgoes that one-shot preprocessing, so the same
constraints solve ~36x slower and blow past the tick budget. This is
not fixable within the task's constraints (invariant #4 + the explicit
"no `.simplify()` anywhere" forbid adding per-tick preprocessing).

Confirmed by isolation (a scratch z3-sys harness): push/pop with
*fixed dummy* pins is fast (~15 ms/tick); the slowdown only appears
with the *real growing datatype-state* pins — i.e. it is the
incremental-solve-without-preprocessing cost, not push/pop overhead or
the per-tick declaration re-parse.

**What landed instead.** Parse the body **once** and cache its
asserted ASTs (`Z3_ast_vector_inc_ref`). Each tick: create a fresh
solver, re-assert the **cached** ASTs (no re-parse), parse a tiny
`<declarations preamble> + <equality pins>` string and assert it,
`check`, read the model, drop the solver. This:

- satisfies invariant #1 ("the model / constraint system is parsed
  ONCE") — the audit's actual concern, the per-tick full-body
  re-parse, is gone;
- keeps each tick's one-shot preprocessing, so there is **no
  regression** (it is in fact slightly faster than the prior kernel,
  which re-parsed the body every tick);
- keeps `./test.sh` green (all 61 kernel tests pass, no timeouts).

The per-tick equality pins resolve to the cached body's variables via
Z3's within-context interning: re-declaring a symbol (primitive const,
datatype + its constructors, array-over-datatype) in a separate
`parse_smtlib2_string` call yields the *same* interned func_decl/sort,
so `(assert (= _x …))` constrains the cached body's `_x`. This was
verified empirically before relying on it (primitive consts,
`mk_const`, datatypes, and declaration-only symbols like
`is_first_tick`/`last_results` all intern). Declaration-only symbols
matter: bootstrap `emit.rs` hand-writes `is_first_tick`,
`last_results`, and the `Result` datatype even when the body never
references them, so an AST walk of the assertions could not have
recovered them — re-declaring from a textual preamble does.

**Deviation flag for the user.** This differs from the approved
*mechanism* (push/pop) but achieves the approved *goal* (stop
re-parsing the body every tick; "build the model once, reuse it").
The user's approval note — "that is what the previous code did" —
refers to the legacy runtime, which used `s.check(*pins)`
(check-with-assumptions), itself an incremental form that would share
the same regression on datatype-heavy state. If literal push/pop is
required regardless of the perf regression, revert
`kernel/src/tick.rs` and re-open this proposal; otherwise the landed
cached-ASTs form is the recommended fix.

Relative to invariant #3 ("no tick may rebuild the model"): the landed
form does create a fresh `Z3_solver` per tick, but it never re-parses
or re-builds the *constraint system* (the cached ASTs are reused
verbatim). The expensive, audited cost — the per-tick parse — is
eliminated.

---

## UPDATE (task #12): pre-loop `.simplify()` added; A confirmed default; B added env-selectable

Two pin mechanisms now live in `kernel/src/tick.rs`, chosen at runtime by
`EVIDENT_PIN_MECH`:

- **A (default; unset or `=A`)** — "cached-ASTs + a single pre-loop
  `.simplify()`". The body is parsed once, each assertion is run through
  `Z3_simplify` ONCE before the tick loop (architecture-invariants.md §4
  permits exactly one pre-loop simplify), and the simplified ASTs are
  re-asserted into a *fresh* solver each tick alongside the pin string.
  This is the landed cached-ASTs form plus the now-explicit setup
  simplify.
- **B (`=B`)** — "check-with-assumptions", the legacy FsmRunner's
  `s.check(*pins)` shape (legacy-python/docs/runtime-architecture.md
  §"Architecture A is a library pattern on Architecture B"). ONE
  persistent solver holds the simplified body for the program's life;
  each tick passes the pin ASTs as assumptions to
  `Z3_solver_check_assumptions`. ~60 LOC over A; the tick loop forks only
  at `apply_pins_a()` / `apply_pins_b()`.

Note on history: this branch's starting point (commit d11eaa9) was
already the cached-ASTs mechanism, *not* task #11's E (`Z3_substitute`
into the body) — E was never merged here, so there was no substitution
path to remove. Task #12 completed A by adding the pre-loop simplify and
added B alongside it.

### Benchmark: A vs B on a real, growing-datatype-state body

The task #11 benchmark used a tiny fixture and showed A/D/E within ~1 ms
— no signal. Task #12 re-measured on a REAL multi-tick body: the emitted
`test_consolidated_lexer` (`main`), a ~13-tick lexer FSM with 40 state
fields and a `TokenList` enum that *grows every tick* — the shape that
dominates self-hosting workloads. It was padded with real `declare-fun` +
equality asserts (the exact shape `evident emit` produces) to three total
sizes. Tick count is constant across pad sizes, so ms/tick growth is the
signal. Min wall-clock over 5 reps; `kernel/target/release/kernel`.

```
body size →             16 KB     64 KB    256 KB
A (cached+simplify):     2.15      7.10     27.97   ms/tick
B (check_assumpt):     946.90   1067.31   1347.71   ms/tick
ratio (B/A):           440.42    150.33     48.18
```

**Reading.** B is pinned near ~1 s/tick across all body sizes: a
persistent solver in incremental mode forgoes the one-shot preprocessing
that the growing `TokenList` pins need each tick — the identical root
cause that made the push/pop form time out (above). The ratio *shrinks*
with body size only because A's per-tick cost grows (fresh-solver one-shot
re-solve + decl-preamble re-parse are O(body), not the idealized O(K)),
while B's is already saturated by the datatype-state penalty. In absolute
terms A stays in single-to-tens of ms and B never drops below ~950 ms.

**Recommendation: A stays the default.** Per the user's stated tolerance
("performance is OK as long as it works and is correct") and "recommend by
growth rate," A is correct on both counts: it is two-to-three orders of
magnitude faster on the representative datatype-heavy body, and its growth
is a gentle low-ms slope versus B's saturated ~1 s/tick floor. B is
retained behind `EVIDENT_PIN_MECH=B` for future measurement (e.g. on
primitive-only state where the incremental penalty may not apply), not as
a default.

The user's task #12 quote that authorised this: *"At least test 2 of them
against each other, but yes we need a large model to be tested. It looks
like A, cached-ASTs + simplify, actually did better?"* — confirmed: A did
better, decisively.

---

## Original proposal (for reference)

**Status:** PROPOSAL — awaiting user approval per the `kernel/`
freeze in CLAUDE.md.

## Why this exists

The audit at `docs/plans/audit-kernel-z3-lifecycle.md` found that
the current kernel (`kernel/src/tick.rs:62-111`) violates the
user-confirmed Z3-lifecycle invariants stated in
`docs/plans/architecture-invariants.md`:

- Invariant #1 "Z3 model built ONCE": VIOLATED — body is re-parsed
  every tick.
- Invariant #2 "model reused; only equality pins change per tick":
  VIOLATED — full body re-asserted as text each tick.
- Invariant #3 "no tick may rebuild the model": VIOLATED — fresh
  `Z3_mk_solver` per tick; no `push`/`pop`.
- Invariant #4 "no `.simplify()` in the tick path": matches (no
  simplify anywhere).

The MVP comment in `tick.rs:5-7` acknowledges this is a stub. The
fix is well-localized but kernel code is frozen pending user
approval.

## What the fix changes (minimal sketch)

In `kernel/src/tick.rs`'s tick loop:

1. **Once per program** (currently lines 53-55 already do this for
   the `Context`; extend to the `Solver`):
   - Create the Z3 `Solver` once.
   - Parse the SMT-LIB body once (`Solver::from_string` or similar).
   - Assert the parsed body once.

2. **Per tick** (replace current re-parse path):
   - `solver.push()` — open a new scope.
   - Assert the tick-specific equality pins (`is_first_tick = …`,
     `_<state> = <previous model's value>`, `last_results = …`).
   - `solver.check()` for SAT.
   - Read the model.
   - `solver.pop()` — discard the tick-local equalities, retaining
     the parsed body and learned lemmas.

3. **No other changes** to behavior. Effect dispatch, manifest
   parsing, state extraction, and the halt rules are unchanged.

Estimated diff: ~50 LOC modified in `tick.rs`. No new files; no
crate dependencies added.

## Risk

- **Learned lemmas across ticks**: with `push`/`pop` the solver
  keeps unit clauses learned at the outer level. This is desired
  (faster subsequent solves) but may surface latent bugs in the
  current code that *depended* on solver state being fresh each
  tick. Risk: low; the current code asserts everything, including
  `is_first_tick`, fresh each tick, so a model state being implicitly
  preserved would be surprising. But any test that uses a constant
  whose value differs between ticks needs verification.

- **`pop` after `check`**: Z3 lets you read the model after `check`
  and before `pop`. Order matters; the fix has to keep the model
  read sandwiched between `check` and `pop`.

- **The `from_string` parse error path** changes location (now once
  at startup instead of per tick). Errors at startup are more
  visible; tick-time errors disappear.

## Why we can't avoid this fix

It's not optional. Without the fix:

- Every tick re-parses the SMT-LIB body. If the body is
  `compiler.smt2` (eventually multi-MB), that's the dominant cost
  per tick. Multiplied by the 100,000-tick limit, that's
  catastrophic for any non-trivial program.
- Every tick allocates a fresh `Z3_solver` + `CString` + AST vector.
- Learned lemmas are dropped on every tick — incremental solving
  is forfeited entirely.

The Z3-FTI Formula-builder approach (`legacy-python/docs/fti-z3.md`)
would also depend on incremental solving working correctly, since
the FTI builds ASTs via FFI and asserts them onto a persistent
solver. So this fix is on the critical path regardless of which
compiler output format we pursue.

## Alternative considered (and rejected)

**Use the bootstrap to skip the kernel re-parse problem entirely.**
Pre-parse SMT-LIB to a compact format (e.g. serialized Z3 AST),
ship that to the kernel, and have the kernel skip parsing. This
fixes the per-tick cost but adds a new build step and a new
on-disk format. Rejected — the push/pop fix is smaller and doesn't
introduce a new format.

## What does NOT change

- The kernel still reads SMT-LIB. The user clarified the SMT-LIB
  path is valid; the Z3-FTI Formula-builder is a separate parallel
  option.
- The manifest header convention is unchanged.
- The effects dispatch loop, halt rules, and state-carry
  conventions are unchanged.
- No new FFI surface, no new crate dependencies.
- `kernel/` LOC delta: roughly 0 (modifications, not additions).

## Decision needed

User to approve or reject:

1. **APPROVE** — coordinator launches a session to implement the
   fix per the sketch above. The session reports back with a diff
   and a before/after measurement on a long-tick fixture (e.g. an
   FSM with 1,000 ticks; today's behavior re-parses 1,000 times).
2. **REJECT** — the kernel stays as-is. Compiler work continues to
   produce SMT-LIB bodies that will become slow once they're not
   tiny. Z3-FTI Formula-builder work is blocked because that path
   also depends on the persistent solver.
3. **DEFER** — wait until self-hosting is closer to producing
   non-trivial bodies. Acceptable but the fix is so localized that
   "defer" mostly buys us a one-day delay later.

The coordinator will not implement this without explicit approval.

## Citations

- `docs/plans/audit-kernel-z3-lifecycle.md` — the audit that
  identified the gap.
- `docs/plans/architecture-invariants.md` — the user-confirmed
  rules.
- `kernel/src/tick.rs:5-7,53-55,62-111` — the relevant code.
- `legacy-python/docs/runtime-architecture.md` — minimal-runtime
  framing.
- `legacy-python/docs/fti-z3.md` — Z3-FTI depends on this fix.
