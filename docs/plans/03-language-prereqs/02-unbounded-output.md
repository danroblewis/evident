# Phase 3.2: Unbounded output Seq from passes

## Goal

A claim's output Seq's length depends on the input — it isn't pinned
at parse time via `#out = N`.

## Today's limitation

`collect_seq_lengths` in `translate/preprocess.rs` walks the body
for `#seq = expr` constraints and pins the length so quantifier
unrolling can fire. For a claim that builds a result Seq of unknown
length, this preprocessor finds nothing and the constraint becomes
underdetermined.

GLSL transpiler example: a shader of N statements should produce a
GLSL output string of (roughly) N lines. The output length isn't
in the input; it has to be derived.

## Design sketch

Two parts:

1. **Length inference**: when the output Seq's length is uniquely
   determined by Z3 (e.g. tail-recursive walks where each Cons
   adds one element), let Z3 derive it instead of requiring a pin.
2. **Quantifier semantics for unbounded ranges**: `∀ i ∈ {0..#s - 1}`
   where `#s` is symbolic. This needs the translator to NOT unroll;
   instead emit a Z3 universal quantifier with a bounded range.

(2) is risky for solver performance — Z3 quantifier instantiation is
expensive. Limit it to ranges over auto-derivable lengths.

## Files touched

- `runtime/src/translate/preprocess.rs` — relax length-pinning
- `runtime/src/translate/exprs.rs` — symbolic-range quantifiers
- New tests for unbounded-output programs

## Test it

A claim that builds a Seq(Int) from a recursive enum:

```evident
enum List = Nil | Cons(Int, List)

claim collect(list ∈ List, out ∈ Seq(Int))
    list = Nil ⇒ #out = 0
    ∀ h ∈ Int, t ∈ List :
        list = Cons(h, t) ⇒
            ∃ rest ∈ Seq(Int) :
                collect(t, rest)
                out = ⟨h⟩ ++ rest
```

## Acceptance

- [ ] List → Seq round trip works for variable-length lists
- [ ] LOC: +~200 Rust
- [ ] Performance acceptable on existing tests (no regression)

## Notes

Performance is the big risk. Profile aggressively. If Z3
quantifier instantiation tanks performance, fall back to a bounded
unrolling with a high default depth (e.g. 64) and document the
limit.
