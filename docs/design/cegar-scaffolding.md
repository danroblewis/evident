# CEGAR scaffolding via the Functionizer trait

> Scaffolding for layering CEGAR (Counterexample-Guided Abstraction
> Refinement) on top of the existing `Functionizer` interface. The
> framing: the Functionizer is already shaped like a theory-solver
> interface — it takes a `Z3Program`, returns a callable that answers
> queries cheaply. Treat that callable as the *abstraction*; treat
> the full Z3 solver as the *oracle of truth*; add a refinement loop
> that uses oracle counterexamples to improve the abstraction. The
> rest of this doc fleshes out what that buys us and how the
> interfaces would look.
>
> Companion reading: [`compile-claims-to-functions.md`](compile-claims-to-functions.md)
> (the Functionizer-as-cheap-evaluator argument) and
> [`../perf/log-unroll-feasibility.md`](../perf/log-unroll-feasibility.md)
> (the empirical wall this technique routes around).

## 1. The framing: Functionizer is already an abstraction

`runtime/src/core/functionizer.rs` defines a 50-line trait:

```rust
pub trait Functionizer {
    fn name(&self) -> &'static str;
    fn compile(&self,
               program:   &Z3Program,
               enums:     &EnumRegistry,
               datatypes: &DatatypeRegistry)
        -> Option<Rc<dyn CompiledFunction>>;
}

pub trait CompiledFunction {
    fn call(&self, given: &HashMap<String, Value>)
        -> Option<HashMap<String, Value>>;
}
```

That's the entire interface between "the constraint model" and "some
artifact that answers queries against it." The Cranelift JIT
(`runtime/src/functionize/cranelift.rs`) is the only implementation
today; the docs sketch others (algebraic evaluator, LLM-generated,
C transpiler, GLSL emit). The runtime calls `compile` once, then
`call` per query. If the compiled artifact refuses an input
(`call` returns `None`), the runtime falls through to a full Z3 solve.

That fall-through is the seed of CEGAR. The current runtime treats
`None` as a binary refusal — either the JIT handles it or Z3 does.
But there's a richer story:

| Today's contract | CEGAR's contract |
|---|---|
| `call(given) -> Some(answer)` — trust it | `call(given) -> Some(answer)` — abstraction's *candidate* |
| `call(given) -> None` — fall through to Z3 | Same — abstraction admits ignorance |
| (no concept) | "is `answer` consistent with the full constraint?" — ask the oracle |
| (no concept) | If not: oracle returns a counterexample; *refine* the abstraction |

The abstraction may be **unsound** (returns a candidate Z3 disagrees
with) or **incomplete** (returns `None` on inputs Z3 could answer).
Both modes get repaired by the refinement loop. Today's Cranelift
JIT is engineered to be sound on its supported subset and refuse
otherwise — i.e., 100% incomplete on the unsupported subset, never
unsound. CEGAR loosens that engineering constraint: an abstraction
that's *occasionally wrong but cheap* can be productive, because the
oracle catches the mistakes.

The shift is from "an honest cheap solver" to "a cheap solver that
the oracle audits and improves."

## 2. Three concrete use cases

### a) Verifying `halts_within(F, N)` on a branching FSM

This is the one the log-unroll measurement
([`../perf/log-unroll-feasibility.md`](../perf/log-unroll-feasibility.md))
forces us to confront. CC's affine-step detector rejects FSMs whose
per-tick body branches on the carried state (Mario's `game`, the
dot-physics demos, anything with collisions). For those, asking
Z3 directly whether the FSM halts within N ticks blows up at N≈16,
because each tick nests the next tick's `ite` tree inside the
previous one and the formula grows ~2×/doubling.

CEGAR routes around this:

- **Abstraction**: the first K unrolled ticks (K small, say 4 or 8),
  compiled by the existing JIT. Answers "does halt happen in the
  first K ticks?" cheaply.
- **Oracle**: the full Z3 unroll. Answers "is there a reachable
  state from which halt *doesn't* happen within N?"
- **Counterexample**: a concrete initial state + trace that
  reaches a non-halting cycle past tick K.
- **Refinement**: extend the unroll depth, OR strengthen the
  abstraction with an invariant covering the counterexample
  family (e.g., "Mario's `x_velocity` is bounded by ±max_speed
  after tick 1") so the next round doesn't ask the oracle about
  states already known to halt.

The point isn't that this verifies Mario in one shot — it almost
certainly doesn't. The point is that we get *partial* answers
("halts within 4 ticks for these initial states") plus *targeted*
counterexamples ("here's a specific input that loops") instead of
"Z3 timed out after 30 seconds." That's a usable verification
loop where today we have a brick wall.

### b) JIT correctness: Z3 finds the corner case the codegen got wrong

The Cranelift JIT and the Z3 model are supposed to agree on every
input. They don't, always — Cranelift's i64 arithmetic wraps where
Z3's `Int` is unbounded; sequence indexing past `#seq` is undefined
in one and constrained in the other; floats vs. reals differ at
denormals; etc. Today these gaps surface as flaky tests or wrong
runtime answers nobody notices.

CEGAR turns the gap into a regression test:

- **Abstraction**: the JIT-compiled function.
- **Oracle**: full Z3 solve on the same `Z3Program`.
- **Counterexample**: an input where `jit.call(x) ≠ z3.call(x)`.
- **Refinement**: this is where it gets interesting. The codegen
  is fixed (a Cranelift instruction-selection rule, an overflow
  guard, a `None` return for the unsupported case). The
  counterexample becomes a row in a property-test corpus. The
  "refinement" is human-driven for v1, automated later.

This use case doesn't need a full refinement loop — it needs the
*detection* half (oracle disagrees with abstraction) wired into CI.
But the interface is the same: an `Oracle::check` call after each
JIT result on a representative input distribution surfaces every
Cranelift codegen bug as a concrete failing input rather than a
production miscompile.

### c) LLM-generated functions: hallucinated abstractions caught at the gate

Session-W's `SatisfierFunctionizer` + session-Y3's LLM path
(`--functionizer llm`) make a different bet from the JIT:
"a language model writes Rust that approximates the claim, we
trust it conditionally." Today's gate is a 2-copy UNSAT check
on a small sample of inputs — coarse, cheap, and catches the
obvious hallucinations. It misses the *interesting* ones: a
function that's right on 99% of inputs and wrong on a structured
1% the gate didn't sample.

CEGAR is the right escalation:

- **Abstraction**: the LLM-generated function.
- **Oracle**: Z3 solve on the `Z3Program`.
- **Counterexample**: a specific input where the LLM's output
  diverges from the Z3 model.
- **Refinement**: feed the counterexample back into the prompt
  as a worked example. "Here's an input you got wrong: input
  `{x: 7, y: -3}`, you returned `0`, the correct answer is
  `4`. Try again." Each round adds a few-shot example; the
  next generation has a tighter prior.

This is CEGIS dressed in CEGAR clothing — the literature treats
the two as duals (one synthesizes the program, the other
synthesizes an abstraction; both alternate candidate-find +
counterexample). For Evident's purposes they collapse to one
interface: the Refiner consumes a counterexample and emits a
better Abstraction.

## 3. Interface sketch

Three traits, layered above what exists. None of these are
implemented yet — this section is the API target.

```rust
/// What you ask of any approximate solver. `CompiledFunction`
/// is exactly this trait under another name.
trait Abstraction {
    fn query(&self, given: &HashMap<String, Value>) -> Option<Value>;
    //                                                ^^^^^^
    // None means "I don't know" (abstraction admits ignorance).
    // Some(v) is the candidate answer, which may or may not be
    // consistent with the full constraint.
}

/// What the oracle does: given a candidate answer, check whether
/// it's actually consistent with the full constraint system.
trait Oracle {
    /// Returns `Some(counterexample)` if the abstraction's answer
    /// is inconsistent with the full constraint; `None` if it's
    /// consistent (abstraction accepted).
    ///
    /// The counterexample is a model that the oracle exhibits to
    /// prove the inconsistency: typically a binding of inputs +
    /// the "real" answer the abstraction missed.
    fn check(&self,
             given:  &HashMap<String, Value>,
             answer: &Value)
        -> Option<HashMap<String, Value>>;
}

/// What you do with a counterexample: produce a stronger
/// abstraction that handles the counterexample (and, ideally,
/// its whole equivalence class).
trait Refiner {
    /// Update the abstraction in light of a counterexample.
    /// Returns the new abstraction, or `None` if the refinement
    /// strategy is exhausted (caller falls back to Oracle for the
    /// remaining queries).
    fn refine(&mut self, ce: HashMap<String, Value>)
        -> Option<Box<dyn Abstraction>>;
}
```

The wiring is the small CEGAR loop, sketched in the same pseudo-Rust:

```rust
fn cegar_query(abs:     &mut Box<dyn Abstraction>,
               oracle:  &dyn Oracle,
               refiner: &mut dyn Refiner,
               given:   &HashMap<String, Value>,
               max_rounds: usize)
    -> Option<HashMap<String, Value>>
{
    for _ in 0..max_rounds {
        match abs.query(given) {
            None      => return oracle.fallback(given),       // abstraction admits ignorance
            Some(ans) => match oracle.check(given, &ans) {
                None     => return Some(extract_bindings(&ans)),  // accepted
                Some(ce) => match refiner.refine(ce) {
                    Some(new_abs) => *abs = new_abs,
                    None          => return oracle.fallback(given),  // refinement exhausted
                },
            },
        }
    }
    oracle.fallback(given)                                    // ran out of rounds
}
```

The implementation budget for v1 is one method on `EvidentRuntime`:
`oracle_check(&self, schema, given, candidate) -> Option<Model>`.
Z3 is asked "is `candidate` consistent with the full assertion list,
given the inputs?" and returns a counterexample model when not.
Everything else is policy on top of that.

`Abstraction` is the existing `CompiledFunction` trait renamed.
We could just rename it; we could also keep both names and have
`Abstraction` be a marker that the result is *advisory* rather than
authoritative. The latter is probably the right move — the JIT-with-CEGAR
mode is a different contract than today's JIT-or-fallthrough mode,
and naming makes that visible.

## 4. Refinement strategies

Four strategies cover the three use cases above. Each is a
`Refiner` implementation. v1 picks one per use case; future work
chains them.

### a) Unroll-depth refinement

When the abstraction is "the first K ticks of an FSM," the
refinement is "make K bigger." If the counterexample reaches a
state outside the first K ticks, double K and rebuild the
abstraction. Eventually either: the FSM halts within some K we
can afford (great, done), or K hits a budget and we report
"didn't halt within K=N ticks, here's the trace at K=N."

This is the natural answer to "CC's affine-step detector
rejected this FSM." It mirrors bounded model checking, but
*adaptive* — only as much unroll as the counterexample forces.

### b) Predicate refinement

When the abstraction is "the JIT-compiled function plus a
predicate Φ on inputs," the refinement is "add a new predicate
to Φ that excludes the counterexample's region." If Z3 produced
the counterexample `{x: 7, y: -3}` because the abstraction
returns wrong answers for `y < 0`, the refinement adds `y ≥ 0`
to Φ. The abstraction now answers only the `y ≥ 0` subset;
`y < 0` falls through to oracle. Over time, Φ partitions the
input space into "covered by abstraction" and "covered by
oracle."

This is the classical predicate-abstraction shape from the
SLAM/BLAST line of work. The predicates can be hand-curated,
inferred from counterexamples (interpolation), or learned —
v1 doesn't need any of that machinery; a single "type-bounds"
predicate inferred from the counterexample's domain is enough
to be useful.

### c) Domain refinement

A degenerate special case of (b): the abstraction is sound on
a smaller subset of inputs; restrict the domain, fall through
for the rest. No predicate inference, just "the JIT only
handles 32-bit-safe inputs; if Z3 says `x = 2^40`, mark `x` as
oracle-only from now on." The simplest possible refiner: a
bitmap of "trust the abstraction for input shape X" indexed by
the counterexample's domain features.

This is the right v1 refiner for the JIT correctness use case
(b in §2). The abstraction is engineered to be sound on a
subset and refuses otherwise; the refiner narrows the subset
when the oracle finds counterexamples.

### d) Re-prompt refinement (LLM functionizer)

For the LLM-generated abstraction (§2c), refinement is
re-generation with the counterexample added to the prompt as
a worked example. The Refiner owns the prompt template; each
call appends the counterexample to a growing few-shot list and
re-invokes the model. The new function is a new abstraction;
the loop continues until the model produces something the
oracle accepts (or we hit the round budget).

The cost model is different here — each refinement is a
network round-trip to the model — so the round budget is
small (3–5 rounds) and the prompt assembly is the interesting
engineering work, not the loop itself. The LLM functionizer
already has prompt+sample machinery; CEGAR just adds "and
here are the counterexamples from previous rounds."

## 5. What's NOT in v1

The CEGAR literature has 30 years of refinements. Most of them
are out of scope:

- **Craig interpolation** for predicate discovery. The textbook
  way to get strong predicates from a counterexample; requires
  an interpolating prover (Z3 supports it but the integration is
  non-trivial). v1 uses concrete counterexamples directly without
  generalization.
- **Predicate-discovery algorithms** (Houdini, ICE-learning).
  Active learning of invariants from positive + negative
  examples. v1 uses hand-coded predicate templates.
- **Unbounded refinement**. v1 is bounded: at most K refinement
  rounds (say K=8), then fall through to the oracle for that
  query. Unbounded refinement requires a termination argument
  (e.g., refining toward a finite abstract domain) we don't
  have for arbitrary Evident claims.
- **IC3 / PDR** (property-directed reachability). The right
  algorithm for "verify this property holds in all reachable
  states." Subsumes finite-unroll BMC. v1 starts with BMC; IC3
  is the right v2.
- **CEGAR for synthesis** (CEGIS proper). The dual where the
  *abstraction* is what we're synthesizing, not just refining.
  The LLM use case (§4d) is the closest we get in v1, and even
  there we're not formally synthesizing — we're regenerating.

The recommended posture: bounded BMC-flavored CEGAR with concrete
counterexamples and human-readable refinement strategies. If we
need the full toolkit later, the interfaces in §3 should compose
with it without redesign — `Refiner` is just a different policy.

## 6. Recommended implementation order

Two sessions, roughly:

**Session 1: Oracle.** One method on `EvidentRuntime`:

```rust
impl EvidentRuntime {
    /// Ask Z3 whether `candidate` is a satisfying assignment for
    /// the given claim under the given input pins. Returns `Ok(())`
    /// if it is; `Err(counterexample_model)` if not.
    pub fn oracle_check(&self,
                        schema:    &str,
                        given:     &HashMap<String, Value>,
                        candidate: &HashMap<String, Value>)
        -> Result<(), HashMap<String, Value>>;
}
```

Internally: take the schema's assertion list, push the given
bindings *and* the candidate's output bindings as additional
constraints, solve. If SAT, the candidate is consistent (return
`Ok`). If UNSAT, negate one of the candidate bindings and solve
again — the new model is a counterexample where the constraints
hold but the candidate's answer is wrong.

Test target: a hand-crafted `Z3Program` plus a wrong candidate;
`oracle_check` should return the counterexample model. No CEGAR
loop yet — just the oracle primitive that the loop will call.

**Session 2: Unroll-depth refinement.** Wire `oracle_check` into
the FSM `halts_within(F, N)` flow:

1. Try CC's affine-step detector. If it accepts, do log-unroll
   (no CEGAR needed).
2. If rejected, start CEGAR. Initial abstraction: unroll to
   depth K₀ (say 4), JIT it, ask "does it halt in K₀?"
3. If yes, ask oracle whether the answer is consistent with
   the full N-tick unroll. If yes, accept. If no, the CE is a
   trace of length > K₀ that doesn't halt — double K and retry.
4. If the abstraction says "doesn't halt in K₀," ask oracle
   whether any trace longer than K₀ halts. If yes, the CE
   is a halting trace at length > K₀; the abstraction was
   incomplete — double K.
5. Stop at K_max (say 256) and report what we know.

Test target: a small branching FSM where naive Z3 unroll
times out at N=64 but CEGAR converges in <5 rounds with
K growing from 4 to 64. Use one of the dot-physics demos as
a concrete case.

After session 2, the loop generalizes: §4b/c/d are
incremental — same `Oracle::check`, different `Refiner`.

## 7. Honest caveats and prior work

CEGAR is well-trodden ground in academic hardware and software
verification:

- **Clarke, Grumberg, Jha, Lu, Veith (2000)** — the original
  CEGAR paper, framed for finite-state model checking with
  predicate abstraction. The Refiner uses Craig interpolants;
  the abstraction is a predicate lattice.
- **Bradley (2011), IC3** — property-directed reachability,
  the algorithm that supersedes BMC + CEGAR for safety
  properties. If we want unbounded verification, this is the
  right target.
- **Solar-Lezama (2008), CEGIS** — counterexample-guided
  inductive synthesis. The synthesis dual of CEGAR; same loop
  shape, the abstraction *is* the program being synthesized.
  Evident's LLM-generation use case is CEGIS-flavored. See
  also [`smt-languages-research.md`](smt-languages-research.md)
  §1 for the Sketch / CEGIS framing.
- **Henzinger, Jhala, Majumdar (2005), BLAST** — lazy abstraction;
  the abstraction is only as fine-grained as the
  counterexamples demand. Worth studying when v2 wants to
  reduce the per-round refinement cost.

**What's the Evident twist?** The Functionizer trait gives us
*many possible abstractions, one oracle*. The literature
typically pins the abstraction representation (predicates, BDDs,
intervals) and varies the refinement. Here the abstraction can
be JIT'd native code, an algebraic evaluator, an LLM-generated
function, a partial unroll, or a hand-written Rust function —
each registered as a `Functionizer` implementation, each
participating in the same CEGAR loop with the same oracle. The
oracle is the Z3 solver, which is fixed and authoritative.

That's the design lever the existing trait gives us for free.
We didn't build it specifically for CEGAR, but it's already
shaped right. The work in this doc is recognizing the shape and
adding the missing two pieces (oracle, refiner) so the trait
can carry the verification weight.

## Cross-links

- [`compile-claims-to-functions.md`](compile-claims-to-functions.md)
  — the full menu of "make queries fast"; this doc is the
  verification dual.
- [`multi-fsm.md`](multi-fsm.md) — multi-FSM execution; verifying
  FSM properties is the primary CEGAR use case.
- [`../perf/log-unroll-feasibility.md`](../perf/log-unroll-feasibility.md)
  — measurement showing log-unroll fails on branching FSMs; the
  literal motivation for CEGAR.
- [`smt-languages-research.md`](smt-languages-research.md) — CEGIS
  background from the Sketch line.
