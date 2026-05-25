# `halts_within(F, N)` — FSM halt as a constraint

`halts_within(F, N)` is a body-item directive that asserts the FSM body
named `F` reaches its halt state within `N` ticks. It is *static
verification*: the runtime proves (or refuses to prove) the property by
composing `F`'s transition with itself, without ever running the FSM.

```evident
claim decrement
    count, count_next ∈ Int
    halt ∈ Bool
    count_next = count - 1
    halt = (count ≤ 0)

claim sat_decrement_halts_by_100
    count ∈ Int = 50
    halts_within(decrement, 100)      -- SAT: 50 reaches 0 by tick 51
```

This is the surface that Z's log-unroll feasibility measurement
(`docs/perf/log-unroll-feasibility.md`) was the justification for. That
measurement established *when* the technique works; this is the feature
that exploits it, gated so it refuses cleanly when it doesn't.

`halts_within` is the **verify**-side sibling of running an FSM for a
*value*: it asks "does `F` halt within `N`?" and answers SAT/UNSAT,
without ever ticking `F`. The **execute**-side question — "run `F` from
this initial state; what is its final state?" — is
[`nested-fsm-strategies.md`](nested-fsm-strategies.md), whose tier-1
strategy reuses this doc's exponentiation-by-squaring composer
(`fsm_unroll/compose.rs`) to extract the composed final-state *value*
rather than the halt witness. Both are faces of one idea — `result =
F(init)` — unified in [`fsms-as-functions.md`](fsms-as-functions.md) (the
capstone): `halts_within` is its **verify** face (the step-relation
composed symbolically), `run` its **execute** face (the
run-to-completion function).

## The halt convention

`F` must be a `claim` (not an `fsm` — see below) that declares:

- **A state pair** `x, x_next ∈ T` for each carried state variable. The
  bare name is the tick's *input* state; the `_next` name is its
  *output*. The composer pairs them by the `_next` suffix.
- **A `halt ∈ Bool`.** Required. It is the halt witness.

`halt` is read on each tick's **input** state. So `halts_within(F, N)`
holds iff `halt` is true at the *start* of some tick in `1..=N`:

> `halts_within(F, N)`  ≡  `∃ k ∈ [1, N] : halt_k`

where `halt_k` is `halt` evaluated on the state entering tick `k`. For
the counter above, starting at `count = 50`, the input count is 0 at the
start of tick 51, so it first halts at tick 51 — `halts_within(decrement,
51)` is SAT and `halts_within(decrement, 50)` is UNSAT.

### Why `F` is a `claim`, not an `fsm`

FSM-ness is determined by the `fsm` parse-time keyword, and `fsm`-tagged
claims are auto-instantiated by the multi-FSM scheduler. The transition
under verification should never be *run* by the scheduler — it's a pure
state→state function we want to reason about. Declaring it `claim` keeps
it out of the scheduler while leaving it in the schema registry, where
`halts_within` looks it up by name. The runnable, effect-emitting form of
the same logic is a separate `fsm` (see `examples/test_34_halts_within.ev`,
where `countdown` is the operational twin of `decrement`).

The initial state is supplied by the *enclosing* claim through
names-match: an outer `count ∈ Int = 50` binds the composed body's
tick-0 `count`. Leaving it free makes the claim an existential over all
initial states.

## Lowering: exponentiation by squaring

`runtime/src/fsm_unroll/` lowers the directive. The strategy is the
"closed-form composition" from Z's measurement — compose the transition
as a Z3 *expression*, not as repeated goal cloning.

1. **Extract the function shape (`build_f1`).** Translate `F`'s body once
   via the normal `build_cache` pipeline (so every body shape the rest of
   the runtime supports — record lifts, ternaries, lookup tables — works
   here too), then `simplify_assertions`. The simplified body is a set of
   `(= out expr)` equalities; pull the RHS for each `x_next` and for
   `halt`. Resolve forward references (`halt = count_next ≤ 0` →
   substitute `count_next`'s definition) so every expression bottoms out
   in the input consts. If any output lacks a clean defining equality,
   refuse with `NotFunctionShape` — the body isn't a pure vector function
   (guarded assignments, quantifier outputs, …).

2. **Cache the powers.** `F^1, F^2, F^4, …, F^(2^p)` where `2^p ≤ N`.
   `F^(2k)` is built from `F^k` by pure substitution: replace `F^k`'s
   input consts inside `F^k`'s own output expressions (`double`). Each
   state expression is `.simplify()`'d after composition — *this* is
   where an affine body collapses (`count − 1` composed with itself is
   `count − 2`, still 3 AST nodes).

   The halt witness composes as a disjunction: `halt_{2k} = halt_k ∨
   halt_k[inputs := state after k ticks]` — "halt fired in the first half
   OR the second half."

3. **Assemble `F^N` (`build_unrolled`).** Pick the cached powers by `N`'s
   binary expansion (e.g. `1000 = 512 + 256 + 128 + 64 + 32 + 8`) and
   chain them in series. The state collapses; the halt aggregate is the
   OR of all the per-half witnesses.

4. **Bind and assert.** Substitute `F^N`'s input consts with the outer
   claim's matching vars (names-match), then assert `halt_aggregate =
   true` on the outer solver. When the initial state is pinned, this
   collapses to a constant.

### The state collapses; the halt aggregate is O(N)

The state-transition node count is **constant in N** for an affine body
(`count − N` is always 3 nodes). The halt aggregate is **O(N)** — it's a
disjunction with one disjunct per tick (`count≤0 ∨ count≤1 ∨ … ∨
count≤N-1`), and Z3's term simplifier does not subsume nested intervals.
This is fine in practice:

- When the initial state is pinned (the typical verification case), the
  whole disjunction folds to `true`/`false` at the final
  `substitute().simplify()`. SAT/UNSAT is immediate.
- Building the O(N) disjunction is O(N) total work across the O(log N)
  doublings (each doubling roughly doubles the disjunct count), with one
  `.simplify()` per doubling. N = 16,000 builds in tens of milliseconds.

A future improvement could keep the halt aggregate small for monotone
halts via a subsumption tactic, but it isn't needed for the cases this
targets.

## The affine-step detector (gating)

Log-unroll is only a win when the state collapses. Z's measurement showed
two clean regimes and — crucially — that **one doubling is not enough to
tell them apart**:

| shape           | f1 | f2 | f4 | f8 | f2/f1 | f4/f2 | f8/f4 |
|-----------------|----|----|----|----|-------|-------|-------|
| pure counter    |  3 |  3 |  3 |  3 | 1.00  | 1.00  | 1.00  |
| linear recur.   |  5 |  5 |  5 |  5 | 1.00  | 1.00  | 1.00  |
| Fibonacci       |  3 |  6 | 11 | 11 | 2.00  | 1.83  | 1.00  |
| conditional upd |  6 |  9 | 15 | 27 | 1.50  | 1.67  | 1.80  |
| 3-state machine |  9 | 16 | 33 | 69 | 1.78  | 2.06  | 2.09  |

The first-doubling ratio (`f2/f1`) misclassifies *both* ways: it buckets
Fibonacci (affine, 2.00) as branching and the conditional update
(branching, exactly 1.50) as affine. The detector therefore **probes 3
doublings** (to `F^8`, capped at `N`'s largest power) and classifies on
the *last* doubling ratio (`f8/f4`): by then every affine shape has
collapsed to 1.00 and every branching shape is still ≥ 1.67. Threshold
1.5 sits in the gap.

The detector measures only the **state** node count, never the halt
aggregate — the halt disjunction grows O(N) by construction and would
make even a pure counter look branching.

Probing to `F^8` costs at most 3 doublings even on a branching body
(~8× `F^1`, still tiny), so the refuse path stays cheap. On a verdict of
**Branching**, the lowering returns `BranchingRefused`; the inline walker
prints a `log-unroll declined` diagnostic to stderr and asserts `false`
so the enclosing claim resolves **UNSAT** — an honest "I can't prove this
via log-unroll," never a wrong SAT. The user should reach for a per-tick
functionizer (the Cranelift JIT path) or CEGAR instead.

## Trace output

`EVIDENT_FSM_UNROLL_TRACE=1` prints the per-doubling state-node count,
the detector verdict, the binary-expansion of N, and the final
halt-aggregate size:

```
[fsm_unroll] decrement: target N=1000
[fsm_unroll]   f^1    state_nodes=3     ratio=    —
[fsm_unroll]   f^2    state_nodes=3     ratio= 1.00
[fsm_unroll]   f^4    state_nodes=3     ratio= 1.00
[fsm_unroll]   f^8    state_nodes=3     ratio= 1.00
[fsm_unroll]   detector: last-doubling ratio=1.00 (probed to f^8) → Affine
[fsm_unroll]   ... continues to f^512 (largest power ≤ 1000)
[fsm_unroll] composing N=1000 from cached powers: 512 + 256 + 128 + 64 + 32 + 8
[fsm_unroll] final halt-aggregate node count: 2002 (O(N) disjunction; collapses when initial state is pinned), time: 4ms
```

## Where it lives

| Concern | File |
|---|---|
| Surface syntax (`halts_within(F, N)` at body-item level) | `runtime/src/parser/body_item.rs` |
| AST node (`BodyItem::HaltsWithin`) | `runtime/src/core/ast.rs` |
| Lowering dispatch (pass 2 inline walk) | `runtime/src/translate/inline/walk.rs` |
| Composer (build_f1 / double / series / assemble) | `runtime/src/fsm_unroll/compose.rs` |
| Affine-step detector | `runtime/src/fsm_unroll/detector.rs` |
| Demo + static sat_/unsat_ claims | `examples/test_34_halts_within.ev` |
| Tests | `runtime/tests/fsm_unroll.rs` |

## Limits

- **`F` must be a pure vector function.** Clean `x_next = expr` and
  `halt = expr` equalities. Guarded/conditional *state* updates make the
  detector refuse (they don't collapse); a body whose outputs aren't
  top-level equalities refuses with `NotFunctionShape`.
- **Affine only.** Branching bodies are refused by design — that's the
  whole point of the detector. Use the JIT or CEGAR for those.
- **Scalar state.** Int / Bool / Real / enum state vars. The composer
  threads each `x`/`x_next` pair; record-typed state would need per-leaf
  pairing (not yet wired).
- **No transient-halt subtlety beyond the OR.** The `∃ k` semantics is
  exact: halt firing at *any* tick in range satisfies, even if the FSM
  would "un-halt" later. If you need sticky-halt semantics, encode it in
  the body (`halt` monotone in the state).
- **`N` is a non-negative integer literal.** `halts_within(decrement,
  100)` — not `halts_within(decrement, my_n)`. The parser intercepts the
  literal shape at the body-item level (`runtime/src/parser/body_item.rs`);
  a pinned-identifier `N` is recognized but falls through to the regular
  call parse (and surfaces as a dropped constraint). Resolving a pinned
  `N` at translate time is a small follow-up — the lowering already takes
  `n: i64`, so it's a parser + AST-shape change, deliberately left out of
  v1 to keep the self-hosted AST encoding (`stdlib/ast.ev`
  `BIHaltsWithin(String, Int)`) simple.
