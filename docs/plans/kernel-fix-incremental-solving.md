# Kernel-fix: pre-loop `.simplify()` + pin-application exploration

**Status:** LANDED. The kernel now (a) `.simplify()`s the parsed body
ONCE before the tick loop (invariant #4, clarified to allow exactly
this), and (b) applies the per-tick pins by **substituting** them into
the cached body AST. Both came out of an explicit exploration of six
pin-application mechanisms (task #11), superseding the earlier
cached-ASTs landing (task #06) recorded further below.

---

## LANDED (task #11) — pre-loop simplify + substitute pins

The user authorised a second kernel exception to test *all* the ways to
apply pins to a Z3 model, with a pre-loop `.simplify()`, accepting that
"performance is bad is OK as long as it works and is correct." Six
mechanisms were implemented behind a `KERNEL_PIN_MECH` env switch (that
scaffold is preserved at the first commit of branch
`agent-11-kernel-pin-exploration`; the committed kernel keeps only the
winner) and benchmarked. **Every mechanism runs the same single
`.simplify()` pass on the body before the loop;** they differ only in
*how the per-tick equality pins reach the solver.*

### Benchmark (median of 10 runs, wall-clock ms, release kernel)

Fixtures: `test_consolidated_lexer` (13 ticks, growing datatype
`TokenList` state — the one that timed out under push/pop in task #06),
`test_fti_stack` (7 ticks, cons-list state), `test_tokens_carry` (short
init/push/done). "suite" = wall-clock of the full 63-test kernel suite.

| # | Mechanism | lexer | stack | tokens | suite | correct? |
|---|-----------|------:|------:|-------:|------:|----------|
| — | baseline (task-06 cached-ASTs, **no** simplify) | 27.9 | 14.2 | 11.0 | ~2.3s | ✓ |
| A | cached-ASTs **+ simplify** (fresh solver, assert body+pins) | 28.7 | 14.6 | 10.7 | 2.4s | ✓ |
| B | simplify **+ check-with-assumptions** (persistent solver, `s.check(*pins)`) | **12642** | 21.1 | 9.0 | 28.9s | ✓ (slow) |
| C | simplify **+ push/pop** (persistent solver) | 1155 | 22.8 | 8.8 | **>600s** | ✗ timeout |
| D | simplify **+ reset+assert** (persistent solver) | 28.3 | 14.4 | 10.6 | 2.3s | ✓ |
| **E** | **simplify + substitute pins into body AST** (fresh solver) | 28.8 | 14.0 | 10.5 | 2.3s | ✓ ← **picked** |
| F | simplify **+ tactic solver** (`simplify;smt`, fresh per tick) | crash | crash | crash | — | ✗ SIGABRT/SIGSEGV |

### What the numbers say

- **Pre-loop `.simplify()` alone changes nothing** measurable (A vs
  baseline: 28.7 vs 27.9 ms). It is cheap, harmless, and now done once —
  but it is not where the cost lives. The cost is the per-tick solve over
  growing datatype-state pins.
- **The incremental persistent-solver forms (B, C) reproduce the task-06
  regression even WITH pre-loop simplify.** B (check-with-assumptions —
  *literally tiny-runtime's `s.check(*pins)`*) is **451× slower** than the
  fresh-solver forms on the lexer (12.6 s vs 28 ms); C (push/pop) is 41×
  slower and its full suite never finished in 600 s. The pre-loop simplify
  did help C relative to task-06 (1.15 s vs a 30 s timeout on the lexer),
  but not enough. Root cause is unchanged from task #06: a persistent
  solver is in incremental mode and forgoes the one-shot
  preprocessing/simplification a *fresh* solve applies to the large nested
  datatype literals the carried state grows into each tick. A *single*
  pre-loop simplify cannot substitute for the per-solve preprocessing the
  incremental solver skips on every tick.
- **F (tactic-built solver) crashes** (SIGABRT / SIGSEGV) — a
  `mk_solver_from_tactic` model does not interoperate with the kernel's
  raw-z3-sys model-read path here. Not pursued further.
- **The fresh-solver forms (A, D, E) are all correct and ~equal in
  speed** (within noise of each other and the baseline). They keep each
  tick's one-shot preprocessing because each tick gets a fresh solver.

### Why E (substitute) was picked

The task's selection rule: correctness is the hard gate; then closest to
the user's design intent (`check(*pins)` canonical; descending preference
B > C > E > D > A > F); then perf is a tiebreaker — *except* "if one
mechanism is 100× slower than another while both are correct, prefer the
faster one," and "if multiple are correct and within 2× on perf, prefer
the one closer to tiny-runtime's design."

- C and F fail the correctness/time gate (suite timeout / crash) → out.
- B is correct but **451× slower** than the fresh-solver forms → the
  explicit ">100× → prefer faster" rule demotes it below A/D/E despite
  being the most design-canonical form. (B *does* pass `./test.sh`: its
  worst single test is 12.6 s < the 30 s "too-slow" threshold. It is a
  valid fallback if a future requirement insists on the literal
  `s.check(*pins)` shape and accepts the datatype-state cost.)
- Among the fast correct set {A, D, E}, all within 2× of each other, the
  design-intent ranking prefers **E (substitution) > D (reset) > A
  (cached)**. So **E** is committed.

### What E does (the committed mechanism)

Parse the body once; `Z3_simplify` each asserted formula once before the
loop and cache the results. Each tick: build the pins as uniform
`(= lhs rhs)` equalities (including `is_first_tick`, written as an
equality), parse that tiny `<decl preamble> + <equality asserts>` string,
split the equalities into (i) **substitutions** whose lhs is a nullary
const (`_x`, `is_first_tick`, `last_results__len` — interned to the
cached body's vars via Z3's per-context hash-consing of sorts/func_decls)
and (ii) **residual asserts** whose lhs is a compound term
(`(select last_results i)`, not a single replaceable subterm).
`Z3_substitute` inlines the substitutions into the cached simplified body
ASTs; the residuals are asserted normally; a fresh solver checks the
result. Substitution applies the pins *directly to the model* (the
user's framing — "several ways to apply pins to a Z3 model") and produces
ground terms Z3 constant-folds, so it is as fast as the cached baseline.

Declaration-only symbols matter: bootstrap `emit.rs` hand-writes
`is_first_tick`, `last_results`, and the `Result` datatype even when the
body never references them, so an AST walk of the assertions could not
recover them — re-declaring from the textual preamble does (verified by
the suite passing).

Relative to the invariants: invariant #1 (parse once) ✓ — the body is
parsed and simplified a single time. Invariant #4 (no per-tick simplify)
✓ — simplify runs exactly once, before the loop. Invariant #3 ("no tick
may rebuild the model") — like task #06's form, a fresh `Z3_solver` is
created per tick and the body is substituted-then-asserted, but the
*constraint system* is never re-parsed; the cached simplified ASTs are
reused verbatim. The audited dominant cost (per-tick full-body re-parse)
remains eliminated.

**Deviation flag for the user.** The user's literal design is
`s.check(*pins)` = mechanism B. B works and is correct but is 451× slower
on growing datatype-state fixtures (12.6 s on a 13-tick lexer); the
committed kernel uses substitution (E) instead, per the task's explicit
">100× → prefer faster" rule and the E > D > A design ranking among the
fast correct forms. If the literal `s.check(*pins)` shape is required
regardless of cost, set `KERNEL_PIN_MECH=B` against the exploration-
scaffold commit, or re-introduce mechanism B as the default — it is a
~20-line change and passes `./test.sh`.

---

## LANDED (task #06) — parse-once cached-ASTs (superseded by E above)

The task-06 proposal sketched "one persistent solver, body asserted once
at base scope, `push`/`pop` the per-tick equality pins." That was
implemented exactly as written and measured. It **regressed
multi-tick fixtures ~36x** and a kernel test (`test_consolidated_lexer`,
~13 ticks) **timed out at 30s** (baseline: the whole 61-test kernel
suite runs in ~1.6s). So the literal push/pop form fails acceptance
criterion #2 of the task ("`./test.sh` fully green"). Task #11 above
re-measured push/pop (mechanism C) *with* the pre-loop simplify and
confirmed it is still far too slow.

**Root cause.** The kernel carries FSM state across ticks by pinning
equalities. For datatype-typed state (e.g. the lexer's `TokenList`
enum), the carried value *grows each tick*, so the per-tick pins
include large nested datatype literals. A fresh one-shot solve (the
prior kernel, which re-parsed `body + pins` into a new solver each
tick) gets Z3's full preprocessing/simplification over those pins and
solves them fast. A *persistent* solver with `push`/`pop` is in
incremental mode and forgoes that one-shot preprocessing, so the same
constraints solve ~36x slower and blow past the tick budget.

**What landed in task #06.** Parse the body **once** and cache its
asserted ASTs (`Z3_ast_vector_inc_ref`). Each tick: create a fresh
solver, re-assert the **cached** ASTs (no re-parse), parse a tiny
`<declarations preamble> + <equality pins>` string and assert it,
`check`, read the model, drop the solver. Task #11 evolved this into
mechanism A (same + pre-loop simplify) and then E (same, but substitute
the pins into the cached ASTs instead of asserting them).

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
