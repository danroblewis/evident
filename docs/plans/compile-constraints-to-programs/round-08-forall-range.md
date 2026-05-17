# Round 8 — ∀-over-Range unrolling

**Outcome:** PASS for ∀ over a literal Integer Range. Mario's `game`
FSM uses ∀-over-Seq (`∀ p ∈ platforms`) which is a different
unroller — Round 9 work.

## What was built

### `substitute(expr, name, value) -> Expr`

A general AST substitution helper. Walks an Expr replacing every
`Expr::Identifier(name)` with `value`. Respects binders: doesn't
substitute through `∀ vars: body` when `name` is in `vars` (avoids
capturing).

### `try_unroll_forall_range(e)` and `expand_foralls(body)`

Given `∀ var ∈ {lo..hi} : inner`, where lo and hi resolve to
literal Ints (via `try_eval_const_int`), produce `hi - lo + 1`
copies of `inner` with `var` substituted by each value in the
range. Each copy becomes a fresh `BodyItem::Constraint`.

`expand_foralls` recursively walks a body, replacing every
unrollable ∀ with its expansion. Called after Passthrough
flattening so ∀s inside `..ClaimName` bodies also unroll.

### Gate accepts unrollable ∀

`gate_diagnostics` extended: if a top-level `BodyItem::Constraint`
is `Expr::Forall(...)` and `try_unroll_forall_range` succeeds, the
unrolled bodies are themselves checked for pure-Eq shape (each one
must be an equality). If yes, the ∀ passes; if no, the ∀ refuses
with the inner reason.

### `constraint_kind(e)` helper

Refactored the rejection-reason matcher out so it can be called
recursively (on unrolled bodies).

## Bench

`runtime/tests/functionize_forall.rs`:
- `forall_unrolling_unit_test` verifies `substitute()` produces
  the expected AST for `i → 0`.
- `forall_range_unrolls_and_compiles` — a Shift4 claim (4 parallel
  arithmetic substitutions) compiles cleanly.
- `forall_in_claim_body` — sanity check that simple claims still
  work.

All 422 cargo + 119 conformance + 12 lints pass with and without
EVIDENT_FUNCTIONIZE=1.

## Mario status

```
[fz] display:    rejected by gate (Membership win∈SDL_Window)
[fz] game:       rejected by gate (Forall (non-static bounds))
[fz] keyboard:   rejected by gate (Membership win∈SDL_Window)
[fz] level_gen:  rejected by gate (non-Eq Binary op Implies)
```

`game` rejection improved: from generic "Forall" to "Forall
(non-static bounds)". The ∀ in Mario's game body iterates over
`platforms` (a `Seq(Body)`), not a literal `{0..N-1}` range. The
∀-over-Seq unroller would:
1. Look up the seq's length from pinned `#seq` constraints or given
2. For each i in 0..N, substitute the loop var with `seq[i]`
   (a Field/Index expression resolving to env entries)

That requires Field/Index resolution beyond what we have. Round 9
work.

## Round 9 candidates

1. **∀-over-Seq unrolling** (unlock Mario game): needs (a) seq
   length resolution from body pins or given; (b) Field/Index
   expression evaluation in `eval_expr`. 3-5 days.
2. **Implies as guarded substitution** (unlock Mario level_gen):
   `cond ⇒ var = expr` becomes `var = (cond ? expr : <free>)`.
   Tricky soundness: the "free" branch leaves var unconstrained,
   but downstream may depend on it. 3-5 days.
3. **Top-level Ternary as branched substitution**: similar to
   Implies. Quick if we can get the soundness story right.

Pick: ∀-over-Seq, since `game` is the highest-cost Mario FSM (45-
var component, dominant per-tick cost in our earlier analysis).
