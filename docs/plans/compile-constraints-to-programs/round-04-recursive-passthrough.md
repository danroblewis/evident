# Round 4 — Recursive records + Passthrough inlining + check semantics

**Outcome:** PASS. Recursive simple records (AABB-style record-of-
records) work. Passthrough inlining lets the function-izer compile
claims that use `..ClaimName` to import another claim's body. A
critical correctness bug surfaced and was fixed: the function-izer
was returning SAT for queries that violated body-level constraints
on given-pinned variables.

## What was built

### 1. Recursive simple records

`is_simple_record_rec` is now recursive with cycle detection. A
record type is "simple" if all its field types are primitive OR
themselves simple records. Mario's `AABB(pos ∈ IVec2, size ∈ IVec2)`
qualifies; recursion through `type Body(aabb ∈ AABB, color ∈ Color)`
also qualifies.

Test: `recursive_record_compiles` — `Shift` claim with
`box ∈ AABB`, `nbox.pos.x = box.pos.x + 1`, etc. — 4 fields shifted,
all bindings verified.

### 2. Passthrough inlining

`is_pure_assignment_body_xl` accepts `BodyItem::Passthrough(name)`
when a predicate `is_pure_passthrough(name)` returns true. The
rt.query hook implements that predicate by recursively checking the
referenced claim's body, with a depth cap of 8 (no realistic
program nests passthroughs that deep).

`extract_chain_xl` walks all transitively passthrough'd bodies and
includes their equalities in the substitution-candidate pool.

### 3. Schema-wide consistency checks (correctness fix)

**The bug:** A conformance test `test_passthrough_unconditional_unsat`
failed:

```
claim GreetsHi { text ∈ String; text = "hi" }
type T { text ∈ String; ..GreetsHi }
query T given {text: "bye"} → expected UNSAT
```

The function-izer was returning SAT because:
- `text` is in given → excluded from component vars.
- Components = empty (no free vars).
- Chain steps = empty (nothing to substitute).
- Evaluator returned given as-is. SAT.

The body constraint `text = "hi"` was never enforced.

**The fix:** A new `extract_schema_wide_checks` walks all body
equalities (including inlined Passthrough bodies) looking for
constraints where either side mentions a given-pinned variable.
Those become **consistency checks** in `SubstitutionChain.checks`.
The evaluator now runs each check at runtime; a mismatch means
the body conflicts with the pin → return None → rt.query falls
through to Z3, which correctly returns UNSAT.

`SubstitutionChain` gained a `checks: Vec<(Expr, Expr)>` field.
`evaluate_chain_with_resolver` verifies each pair before declaring
success. The same evaluator also catches substitution conflicts
(if a chain step's variable is also in given AND the chain's
computed value differs from the pin, it's UNSAT).

## Bench results

```
record_typed_bench (Step with IVec2 vectors):
  Z3 query:    99.15 μs/call
  Native (fz):  1.06 μs/call
  Speedup:    94×

match_dispatch_bench (HelloStep with enum Match):
  Z3 query:    36.83 μs/call
  Native (fz):  0.76 μs/call
  Speedup:    48×
```

Both speedups stable across the schema-wide-check overhead. Native
path remains in the sub-μs range.

## Gate coverage

Still 54% on the probe_gate metric (which doesn't track
Passthrough acceptance because it uses the simpler `_full` gate).
Real-world Passthroughs in stdlib are mostly already gate-rejected
by ClaimCall or non-equality constraints elsewhere in the body.

To unlock Mario specifically, Round 5 will need:
- ClaimCall handling (Mario's `..Level` inlines but the FSM also
  calls subclaims).
- `∀ x ∈ seq / ∀ i ∈ {lo..hi}` finite unrolling.
- Seq-typed Memberships with composite-element types
  (`Seq(Effect)`, `Seq(Body)`).

## Round 5 candidates

The next high-leverage move:
1. **`∀ x ∈ {lo..hi}` with pinned bounds** (3-5 days) — unrolls
   the body at chain-extract time into N parallel substitutions
   per iteration. Unlocks coindexed iteration which Mario's
   per-entity ∀ loops use.
2. **Seq-typed Memberships with primitive elements** (2-3 days) —
   allow `Seq(Int)` etc. in the gate; Seq values come from given
   as `Value::SeqInt`.
3. **ClaimCall inlining** (3-5 days) — similar to Passthrough but
   with arg-binding.

Round 5 pick: ∀-range unrolling, since it unlocks coindexed
patterns which dominate Mario's display FSM.
