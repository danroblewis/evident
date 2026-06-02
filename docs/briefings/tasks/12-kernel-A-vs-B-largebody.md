# Task: Kernel — switch default to A, add B env-selectable, benchmark on large body

## Authorisation

Another rare exception to the `kernel/` freeze. The user
explicitly authorised additional kernel work because the previous
session (task #11) picked E based on a vague "applying pins to the
model" criterion that doesn't actually have a measurable
performance benefit. The user's quote:

> *"At least test 2 of them against each other, but yes we need a
> large model to be tested. It looks like A, cached-ASTs + simplify,
> actually did better?"*

Your edits are limited to what this spec authorises.

## The problem with task #11's choice

Task #11 picked E (substitute pins into body AST). On the
benchmark fixtures the table showed all of {A, D, E} within ~1 ms
of each other — no meaningful perf difference. E's selection
came from a *stylistic* read of "applying pins to the model,"
not from data.

Architecturally, A is better:

- **A per-tick cost:** assert cached body ASTs (no parse, no walk
  — solver already holds them by reference) + add `K` pin
  equalities. Pin work is `O(K)`, independent of body size.
- **E per-tick cost:** walk the body AST to substitute pins (`O(body_size)`),
  then assert the substituted form. Body walk happens every tick.

For tiny test fixtures this is invisible. For self-hosting
workloads — `compiler.smt2` is the body, multi-KB to multi-MB —
E's per-tick body walk grows linearly with body size; A's doesn't.

## What you're producing

Three deliverables in one session:

### Deliverable 1: switch tick.rs default A→A

Currently the kernel implements E. Switch it to A:

- A is "cached-ASTs + `.simplify()` once before the loop." Each tick:
  fresh solver, assert the cached simplified body ASTs (Z3 keeps
  ASTs interned per context; passing the same AST handle re-asserts
  cheaply), assert a tiny `(declarations preamble) + (pin equalities)`
  string, check.
- The .simplify() pre-loop pass stays.
- Remove the `Z3_substitute` code path used by E.

### Deliverable 2: add B as env-selectable

Implement B (`check_with_assumptions`, tiny-runtime's pattern) as a
kernel feature selectable at runtime by `EVIDENT_PIN_MECH=B`. Default
remains A (`unset` or `=A`).

- Persistent solver after the pre-loop `.simplify()` pass.
- Per tick: build a `Z3_ast` array of equality constants, call
  `Z3_solver_check_assumptions` (or the z3-sys equivalent), read the
  model.
- This is the literal form the user's "we can use the other solver to
  apply the pins" comment described.

Code organisation: split the per-tick body into a small
`apply_pins_a()` / `apply_pins_b()` choice at the top of the tick
loop. Don't fork the entire tick function. ~60 LOC delta over
deliverable #1.

### Deliverable 3: benchmark A vs B on a REAL large body

Today's benchmarks (task #11) ran on `test_consolidated_lexer.ev`
(~13 ticks, tiny body). That doesn't surface body-size growth.

Generate at least one large body by running the existing bootstrap
compiler on a substantial `.ev` source:

```bash
ls -lS compiler/*.ev stdlib/*.ev | head -5    # find the largest .ev files
# then for each:
bootstrap/runtime/target/release/evident emit <chosen.ev> main -o /tmp/big.smt2
wc -l /tmp/big.smt2
```

Pick a body that's at least 1000 lines of SMT-LIB. Aim for 5K+ if
available.

For each pin-mechanism A and B, run a kernel benchmark that:

- Loads the chosen `.smt2`.
- Asserts a synthetic state and runs at least 10 ticks.
- Measures total wall-clock and per-tick mean.

Repeat on at least two body sizes (e.g. one ~1 KB, one ~10 KB+).
The goal is to *show the growth rate*. A's per-tick should stay
roughly constant; B's may grow worse (the incremental-mode penalty
on pin assumptions) or stay flat. Whichever wins on the big body
is the user's data-driven answer.

Report a clear table:

```
body size →   1 KB   10 KB   100 KB
A (cached+simplify): X.X     Y.Y    Z.Z    ms/tick
B (check_assumpt):   X.X     Y.Y    Z.Z    ms/tick
ratio (B/A):         ...     ...    ...
```

If B is competitive or better on large bodies, recommend B as the
default. If A wins comfortably, A stays.

## Acceptance

1. `kernel/src/tick.rs` default is A (E's substitution path removed).
2. `kernel/src/tick.rs` supports `EVIDENT_PIN_MECH=B` to switch.
3. `./test.sh` is fully green under default and under `EVIDENT_PIN_MECH=B`.
   (Run it both ways and report both final lines.)
4. The benchmark table above is in your report.
5. Recommendation: A as default OR switch to B, with the table as
   evidence.
6. Diff limited to:
   - `kernel/src/tick.rs` (modified)
   - `docs/plans/kernel-fix-incremental-solving.md` (LANDED section
     updated)
   - Possibly `docs/plans/architecture-invariants.md` (clarifying
     notes).
7. `scripts/check-deletable.sh` output unchanged (kernel-internal task).

## Forbidden

- Editing any kernel file OTHER than `kernel/src/tick.rs`.
- Adding crate dependencies.
- Editing `bootstrap/`, `compiler/`, `stdlib/`, except optional
  modification to a large-body fixture under `tests/kernel/` if
  needed for the benchmark.
- Implementing C (push/pop), D (reset+assert), or F (alt-tactic).
  This task is A-vs-B only.
- Calling `.simplify()` INSIDE the tick loop. Pre-loop only.
- Re-using task #11's "perf is tiebreaker → prefer faster"
  heuristic. Use the user's actual stated tolerance: "performance
  is OK as long as it works and is correct." Recommend by growth
  rate, not by absolute speed on tiny fixtures.

## Reporting back

Final message (terse):

- Branch pushed (`agent-12-kernel-A-vs-B-largebody` or similar).
- Table: body size × mechanism × ms/tick (described above).
- Recommendation in one sentence (A default / switch to B).
- Diff stat (`git diff --stat HEAD~1`).
- `./test.sh` final line under default.
- `./test.sh` final line under `EVIDENT_PIN_MECH=B`.
- `scripts/check-deletable.sh` blocker count unchanged.
- Cite `docs/plans/kernel-fix-incremental-solving.md`,
  `legacy-python/docs/runtime-architecture.md`, and the user's
  quote above.

Do NOT paste tick.rs source. Do NOT paste raw benchmark logs.
