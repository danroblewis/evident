# Kernel-fix proposal: incremental solving (one solver, push/pop per tick)

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
