---
name: types-carry-invariants
description: Evident types should bind their fields with membership invariants, not just group variables
metadata:
  type: feedback
---

A good Evident `type` is not a bare field bag — it is "a named set defined by
membership conditions." The defining move is to attach the invariants that
*bind the relationships between its fields* as body constraints. `type
FtiBuffer(base, count)` with no body is anemic (just a tuple); the real type
adds `0 ≤ count`, `0 ≤ base`, and the bounds relationship.

**Why:** in a constraint language the type's value is that the solver
re-checks its invariant every tick. A violated invariant on carried state
surfaces as a loud `UNSAT` (exit 2) instead of silent corruption — e.g. a
buffer overrun or a null Z3 handle becomes a kernel halt, not a wrong answer.
The user flagged the Phase-1 types (driver_ir.ev) as deficient for lacking this.

**How to apply:** when introducing a `type`, ask what must always be true of
its fields and write it in the body. Verified the oracle instantiates type-body
invariants: `x ∈ T` injects T's body constraints over `x`'s fields, and a
violation forces UNSAT (probed 2026-06-08). Caveat for *carried* state types:
the invariant must hold even during the uninitialized window (tick 0, pre-latch
handles are legitimately 0), so universal `handle ≠ 0` is unsafe — use
non-negativity (`≥ 0`) or *conditional/relational* invariants like
`sol ≠ 0 ⇒ (ctx ≠ 0 ∧ cfg ≠ 0)` (vacuously true during init, binds the
lifecycle once live). Gate with conformance (137/138) + the type's carry unit
test, NOT the byte-identical emit gate (adding invariants is a behavior change
by design). A conformance drop means an invariant is actually false somewhere —
that is real signal worth investigating.

**PERF CAVEAT — the trap is `≠`, NOT `⇒` (corrected 2026-06-08):** the
implication functionizes fine. The killer is **disequality (`≠`)**: it is
non-convex (Z3 reads `x ≠ 0` as `x < 0 ∨ x > 0` and case-splits), and on hot
carried state the model references everywhere (the compiler's Z3 handles) that
case-splitting compounds every tick across thousands of ticks and explodes —
`sol ≠ 0 ⇒ …` took conformance fixture-001 from 19s to a >30-min timeout. The
SAME implication as `sol > 0 ⇒ (ctx > 0 ∧ cfg > 0)` stays fully functionized
(0.0 ms z3, 20s). Rule: **never put `≠` on hot carried state — write `> 0`/`< 0`
when the sign is known.** Convex comparisons (`> ≥ ≤ < =`) and implications are
cheap. (A bare satisfiable `≠` on a lightly-referenced var is also fine — the
cost is `≠` × heavy entanglement.) Guard with scripts/functionization-gate.sh;
profile with scripts/perf-profile.sh (ranks constraints by marginal solve ms +
rlimit-count, `--bisect` finds the dominant one). Related:
[[record-carry-recipe]].
