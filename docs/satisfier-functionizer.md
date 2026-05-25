# The SatisfierFunctionizer — sampling partially-constrained variables

The Cranelift JIT functionizer compiles a claim only when every output
is *defined* by an equation (`y = expr`). A claim with an
unbound-but-bounded variable —

```evident
claim setup
    x ∈ Int
    x ≥ 0
    x ≤ 10
    y ∈ Int = x * 3 + 5
```

— has no equation for `x`, so extraction refuses and the whole claim
falls to a full Z3 solve. But there's nothing to *solve* here: `x` is
just a value to be *picked* from `[0, 10]`. Picking a satisfying value
from a finite domain is what Z3 does internally when it builds a model;
the **SatisfierFunctionizer** does it directly, with a seeded PRNG, and
hands the deterministic remainder (`y = x*3+5`) to Cranelift.

The framing is **probabilistic programming**: an unbound-but-bounded
variable is a distribution, and a query draws one satisfying sample.

## What it recognizes (v1)

Opt-in via `EVIDENT_SATISFIER=1`. With it unset, behavior is identical
to the default Cranelift path — byte for byte.

| Shape | Step | Drawn value |
|---|---|---|
| `lo ≤ x ≤ hi` (scalar `Int`/`Nat`/`Pos`; `Nat`'s implicit `≥ 0` counts) | `SampleRange { var, lo, hi }` | uniform `Int` in `[lo, hi]` |
| `c ∈ EnumType` (no other constraint, nullary variants) | `SampleEnum { var, type_name }` | one of the enum's variants |
| `x ∈ {a, b, c}` (concrete finite set of Int/Bool) | `SampleSet { var, candidates }` | one of `candidates` |

A variable whose constraints aren't *exactly* one of these shapes is
left unbound and the claim falls through to the slow Z3 solve. That
includes:

- **A free relation** — `x < y` with `y` itself unbounded (needs a
  solve-order toposort; deferred).
- **A residual predicate on a derived var** — `y = x*2; y ≥ 100`. The
  sampler can't honor a constraint on a *computed* value by drawing
  `x`, so it refuses and lets Z3 validate it.
- **A half-open range** — only an upper or only a lower bound: the
  domain is infinite, nothing finite to sample.
- **Payload-bearing enum variants** — would need to sample the payload
  too (deferred).
- **Seq / quantified samplers** — `xs ∈ Seq(Int)` with free elements;
  IR + codegen both have to grow (deferred).

This conservatism is the correctness guarantee: the satisfier only ever
*adds* fast paths for shapes it can draw exactly; everything else keeps
the existing Z3 semantics.

## How it fits the pipeline

```
extract_program_partial            (z3_eval.rs)
  └─ recover_samplers               ← gated on EVIDENT_SATISFIER
        recognizes range/enum/set, emits Sample* steps,
        consumes the bound/membership predicates it subsumes

functionizer.compile               (query.rs calls it, unchanged)
  └─ SatisfierFunctionizer::compile (functionize/satisfier.rs)
        partition steps: Sample* → samplers, rest → stripped program
        refuse if any check/predicate survives sampling
        delegate the stripped program to CraneliftFunctionizer

CompiledFunction::call             (per query)
        draw each sampler (SplitMix64), inject into a clone of `given`,
        run the inner Cranelift fn (reads sampled vars as inputs),
        merge the drawn values into the result
```

Two design choices are worth calling out:

**Recognition lives in the extractor, not the functionizer.** The
production query path (`query.rs`) routes an unbound-but-bounded var to
the slow solve *before* `functionizer.compile` is reached. So the only
way to give the satisfier a chance is to make `recover_samplers` turn
the var into a `Sample*` step at extraction time — then the var counts
as "covered", and the program flows to `compile`. The env-var gate keeps
this invisible to the default Cranelift path (which would just refuse
the `Sample*` step anyway and route to the slow solve — the same
outcome as today).

**Reuse, not reimplementation.** The satisfier does not reimplement
integer arithmetic. It strips the `Sample*` steps and hands the
computed remainder to a real `CraneliftFunctionizer`. The sampled
variables become referenced-but-undefined names there, which Cranelift
treats as ordinary inputs read by name from the `given` map
(`cranelift::JitProgram::call`). At call time the satisfier injects the
drawn values into a clone of `given`, so the inner JIT consumes them
transparently.

## Determinism (non-negotiable)

The cross-tick value cache keys on `(claim, given-keys, given-values)`.
If the sampler weren't deterministic, repeated queries would return
inconsistent assignments and the cache would be poisoned.

The PRNG is a hand-rolled **SplitMix64** (no dependency, bit-stable
across platforms). The seed is

```
EVIDENT_DISPATCH_SEED  ⊕  program-shape salt  ⊕  given-values hash
```

- `EVIDENT_DISPATCH_SEED` — the same knob the effect scheduler uses;
  fixed default when unset.
- *program-shape salt* — FNV-1a over the sampler var names + bounds /
  type / arity, so two different sampler programs draw different
  sequences even for an identical `given`.
- *given-values hash* — FNV-1a over the key-sorted `given` map, so the
  draw varies with inputs (each tick reseeds via the changing prev-tick
  state) but is stable for a fixed input.

Samplers are drawn in a fixed (var-name-sorted) order, independent of
`HashMap` iteration order.

Same `(claim, given-values, seed)` → same assignment → the value cache
stays consistent and cross-validation is reproducible.

## Cross-validation against Z3

`runtime/tests/satisfier_functionizer.rs` builds each claim through the
real translate → simplify → extract pipeline, draws an assignment, and
**re-asserts the simplified body plus the drawn assignment in a fresh Z3
solver, requiring SAT.** This proves the sample genuinely satisfies the
constraints rather than merely landing in the declared bounds. Covered
shapes: range scalar, enum, finite set, `Nat` + computed, singleton
range. Plus determinism (repeated / fresh-compile / given-sensitivity)
and the two refusal paths.

## Demo + benchmark

`examples/test_33_satisfier.ev` spawns a batch of particles per tick,
each with a position sampled from the window bounds (range) and a colour
sampled from an enum, then derives an energy + warmth from the samples.
Measured (12 range + 6 enum samplers per tick, 21 ticks):

| Mode | wall | steady/tick |
|---|---|---|
| `EVIDENT_SATISFIER=1` | 3.84 ms | 0.01 ms |
| `EVIDENT_FUNCTIONIZE=0` | 16.53 ms | 0.72 ms |

≈ 72× faster steady-state per tick: drawing 18 bounded values is a PRNG
fan-out (sub-µs) for the satisfier but a real solve cycle (~0.7 ms) for
Z3.

## Limits / future work

- Scalar `Int` ranges only — `Real` ranges would need a float draw +
  precision policy.
- Set candidates are `Int`/`Bool` literals; `String` and composite
  candidates aren't decoded yet.
- The deferred shapes in the table above (free relations, Seq/quantified
  samplers, payload enums) are the natural next increments — each needs
  either solve-order toposort or an IR + codegen extension.
