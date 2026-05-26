# Constraining a child FSM's whole-run behavior — model checking, synthesis, CHC/Spacer, and the Z3 binding reality

> **What this is.** The user wants, on *every* `fsm`: a **parent constraint-model
> that constrains a child FSM's behavior over its whole run**, with the solver
> finding seeds/inputs/states that satisfy both the FSM's operation AND the
> parent's declarative properties. They have (correctly) re-derived that this is
> **model checking + synthesis**, that our existing unrolling is **Bounded Model
> Checking**, and that a hand-rolled **CEGAR** loop "re-implements parts of Z3."
>
> This report names the problem precisely, surveys the techniques
> (BMC / k-induction / IC3-PDR / CEGAR / CHC), deep-dives **Constrained Horn
> Clauses + Z3's Spacer engine** with the Horn encoding and a worked countdown
> example, and — the load-bearing practical gate — **actually inspects the
> installed `z3-0.12.1` and `z3-sys-0.8.1` crate sources** for the Fixedpoint/CHC
> API and the user-propagator API, returning a verdict *with file+symbol
> evidence, not a guess*. It then compares the realistic paths, recommends one
> decisively, and shows how `F(seed, fsm_state)` + parent invariants lowers onto
> it.
>
> **Reading prerequisites** (the design this slots into):
> [`../design/fsms-as-functions.md`](../design/fsms-as-functions.md) (the
> `result = F(init)` surface and the condensability→guarantee spectrum),
> [`../design/nested-fsm-strategies.md`](../design/nested-fsm-strategies.md) +
> [`../design/loop-functionizer.md`](../design/loop-functionizer.md) (the tier
> machinery), [`../design/cegar-scaffolding.md`](../design/cegar-scaffolding.md)
> (the hand-rolled CEGAR design),
> [`../perf/log-unroll-feasibility.md`](../perf/log-unroll-feasibility.md) (the
> BMC formula-growth measurement), and the existing BMC implementation
> [`runtime/src/fsm_unroll/compose.rs`](../../runtime/src/fsm_unroll/compose.rs).

---

## § 1 — Name the problem, and survey the techniques

### 1.1 The problem, stated precisely

An `fsm` denotes a **transition system** `M = (S, I, Tr)`:

- `S` — the state space (the FSM's state var(s): an `Int`, a record, an enum).
- `I ⊆ S` — the **initial states**, fixed by the *seed*: `I(s) ≜ (s = init)`, or a
  set when the seed is constrained rather than pinned (`I(s) ≜ s ≥ 0`).
- `Tr ⊆ S × S` — the **one-tick transition relation**, the FSM body:
  `Tr(s, s') ≜ (s' = step(s)) ∧ ¬halt(s)`. (Inputs fold in as extra arguments;
  for a pure function of the seed they are determined, so we keep `Tr` binary
  for the exposition.)

A **run** is a sequence `s₀, s₁, …` with `I(s₀)` and `Tr(sᵢ, sᵢ₊₁)`. A state is
**reachable** if some run reaches it. The FSM **halts** at the first `sₖ` with
`halt(sₖ)`; its **result** is `sₖ`.

The parent's declarative claims are a **property** `P` over states (or over the
result):

- A **safety property / invariant**: `P(s)` holds in *every* reachable state
  ("the count never goes negative", "`x_velocity` stays within ±max_speed").
- A **post-condition on the result**: `P(sₖ)` holds at halt ("the settled value
  is 0", "the walk visited every node").
- A **pre/post pair**: `Pre(s₀) ⇒ Post(sₖ)` ("for any seed ≥ 0, the result is 0").

There are two questions the user is conflating into one wish, and they have
different difficulty:

1. **Verification (∀):** *Given a seed-set, does the property hold over the whole
   run / all reachable states?* — `∀ reachable s. P(s)`. This is **model
   checking**.
2. **Synthesis (∃∀):** *Find a seed (or input) such that the property holds.* —
   `∃ seed. ∀ run-from-seed. P`. This is **program/parameter synthesis**, and the
   ∃∀ alternation is what makes it strictly harder than (1).

`fsms-as-functions.md` § 4 frames exactly this: **the fsm is the implementation,
the surrounding claims are the specification, and nesting checks the
implementation against the spec.** That doc's "recover the whole-output
guarantee flat FSMs gave up" *is* model checking; its "find a seed such that the
output satisfies the constraint" (§ 5 regime 3, the output-feedback / CEGAR case)
*is* synthesis. The present report is about *which engine* makes those two real.

### 1.2 The technique survey

All five techniques below attack "transition system vs property." They differ on
**bounded vs unbounded** (does the answer cover all run lengths, or only up to
some depth N?) and on **cost**. Each is mapped to "parent constrains child fsm."

#### BMC — Bounded Model Checking

*Biere, Cimatti, Clarke, Zhu, "Symbolic Model Checking without BDDs," TACAS
1999.*

Unroll `Tr` to a fixed depth `N`, conjoin the property's negation at some step,
and hand one big formula to the SAT/SMT solver:

```
I(s₀) ∧ Tr(s₀,s₁) ∧ Tr(s₁,s₂) ∧ … ∧ Tr(s_{N-1},s_N) ∧ ( ¬P(s₀) ∨ … ∨ ¬P(s_N) )
```

SAT ⇒ a concrete **counterexample trace** of length ≤ N. UNSAT ⇒ the property
holds **for the first N steps only** — *not* a proof for all depths. Bounded.

**This is exactly what we already have.** `runtime/src/fsm_unroll/compose.rs`
builds `Fᴺ` by exponentiation-by-squaring substitution (`build_f1` → `double` →
`series`) and asserts the halt aggregate / property on the outer solver
(`assert_halts_within`). The parent property maps to the `¬P` disjunct over the
unrolled states. **Cost:** the unrolled formula's size. For *affine* bodies it
collapses to closed form (ratio ≈ 1.0 — the pure counter stays 3 AST nodes at any
N); for *branching* bodies it grows ≈ 2× per doubling and BMC is no cheaper than
naïve unrolling — the measured wall in `log-unroll-feasibility.md` (conditional
update 1.97×, real Mario `game` 1.98×). `compose.rs`'s affine-step detector
(`classify`, ratio > 1.5 → `BranchingRefused`) is the honest cutoff.

#### k-induction

*Sheeran, Singh, Stålmarck, "Checking Safety Properties Using Induction and a
SAT-Solver," FMCAD 2000.*

Turns BMC into an **unbounded** proof with one extra query. Prove both:

- **base:** `P` holds for the first k steps from `I` (a BMC run of depth k), and
- **step:** for *any* k consecutive states satisfying `P` and linked by `Tr`, the
  (k+1)th also satisfies `P` — *without* assuming `I`:
  `P(s₀) ∧ Tr(s₀,s₁) ∧ … ∧ P(s_{k-1}) ∧ Tr(s_{k-1},s_k) ⇒ P(s_k)`.

If both pass, `P` holds at **all** depths. **Cost:** two BMC-shaped queries of
depth k — a *cheap strengthening of machinery we already have*: it reuses
`compose.rs`'s unroller verbatim, adding only the induction-step assertion (drop
the `I(s₀)` clause, assert `P` on the first k states, check `P` on the (k+1)th).
**Limit:** `P` must be *k-inductive* for some affordable k; many true safety
properties are not k-inductive until strengthened with auxiliary invariants
(which k-induction does not discover — that is IC3/CHC's job). Maps to "parent
constrains child" as: the parent property is the induction target; if it is
k-inductive over the child's `Tr`, you get an unbounded guarantee from the
bounded unroller.

#### IC3 / PDR — Property-Directed Reachability

*Bradley, "SAT-Based Model Checking without Unrolling," VMCAI 2011; Eén,
Mishchenko, Brayton, "Efficient Implementation of Property Directed
Reachability," FMCAD 2011.*

The breakthrough that **does not unroll at all.** IC3/PDR incrementally builds a
sequence of over-approximation *frames* `F₀ = I, F₁, F₂, …`, each an
over-approximation of the states reachable in ≤ i steps, each implying `P`. It
repeatedly asks Z3 small one-step queries ("is there a state in `Fᵢ` that steps
to a `¬P` / to a previously-blocked state?"), and when it finds a
**counterexample-to-induction**, it *generalizes* the blocking clause (drops
literals while staying relatively inductive) and pushes it back through the
frames. It terminates when two adjacent frames coincide (`Fᵢ = Fᵢ₊₁` — an
**inductive invariant** found, property proved, **unbounded**) or when a
counterexample reaches `I` (a real trace). **Cost:** many small SAT queries
instead of one giant unrolled formula — empirically far better than BMC on
hardware/software safety, and it *discovers* the auxiliary invariant k-induction
needs. This is the algorithm Z3's CHC engine generalizes.

#### CEGAR — Counterexample-Guided Abstraction Refinement

*Clarke, Grumberg, Jha, Lu, Veith, "Counterexample-Guided Abstraction
Refinement," CAV 2000 / JACM 2003.*

Don't model-check the concrete system; **abstract** it (e.g. predicate
abstraction — track only the truth of a finite set of predicates), model-check
the small abstraction, and if the abstraction yields a **spurious** counterexample
(one that doesn't correspond to a concrete run), **refine** the abstraction (add
predicates, classically via Craig interpolation) and repeat. Unbounded in
principle; cost lives in the refinement loop and the abstraction representation.

**This is GG's design** (`cegar-scaffolding.md`): the `Functionizer`-compiled
artifact is the *abstraction* (a fast, possibly-unsound candidate solver), the
full Z3 solve is the *oracle of truth*, and oracle counterexamples drive
refinement. GG's honest framing is the relevant one: the abstraction proposes,
the oracle checks, the counterexample refines — exactly the
**output-feedback / synthesis** regime (`fsms-as-functions.md` § 5 regime 3),
where the parent constrains the child's *output* and the satisfying input isn't
known up front. The user's remark — "hand-rolled CEGAR is re-implementing parts
of Z3" — is precise: predicate discovery, interpolation, and frame management are
exactly what IC3/PDR/Spacer already implement *inside* the solver.

#### CHC — Constrained Horn Clauses

*Bjørner, Gurfinkel, McMillan, Rybalchenko, "Horn Clause Solvers for Program
Verification," 2015; Grebenshchikov, Lopes, Popeea, Rybalchenko, "Synthesizing
Software Verifiers from Proof Rules," PLDI 2012.*

Not a new algorithm but a **uniform logical format** for verification conditions.
You express the proof obligation as a set of implications (Horn clauses) over
**uninterpreted predicates** that stand for the invariants you want to discover.
A CHC solver finds an *interpretation* of those predicates that makes all clauses
valid (= the inductive invariant ⇒ property proved, **unbounded**) or derives
`false` (= a concrete counterexample). The dominant CHC engine for software is
**Spacer** (next section), which is IC3/PDR generalized to clauses modulo SMT
theories. CHC is the **principled, unbounded** tool for the parent-constrains-
child question — and, decisively for this report, it ships *inside Z3*.

### 1.3 The throughline

For **safety** properties the proving power is a ladder:

```
BMC  ⊂  k-induction  ⊂  IC3/PDR  ≈  CHC/Spacer
(bounded)  (unbounded if      (unbounded, discovers     (unbounded, modulo
            k-inductive)        the invariant)            theories; the format)
```

CEGAR is orthogonal — an abstraction-refinement *loop* that several of these use
internally, and the right framing for the **synthesis / output-feedback** regime
where the child must be run as an opaque oracle. The user's three re-derivations
are all correct: our unrolling **is** BMC; CHC/Spacer **is** the principled
in-Z3 tool; and a hand-rolled CEGAR **does** re-implement machinery Spacer already
contains. The rest of the report establishes whether Spacer is *reachable* from
our Rust stack, and what to build.

---

## § 2 — CHC / Spacer deep dive

### 2.1 What a Constrained Horn Clause is

A **constrained Horn clause** is a first-order implication of the form

```
∀ x̄ .  ( φ(x̄)  ∧  P₁(x̄₁)  ∧  …  ∧  Pₙ(x̄ₙ) )  →  H(x̄)
```

where:

- `φ` is a **constraint** in a background SMT theory (linear integer arithmetic,
  reals, arrays, bitvectors, algebraic datatypes) — the *"constrained"* part;
- `P₁ … Pₙ` and `H` are **uninterpreted predicate symbols** (the *relations* /
  *invariants* to be discovered) applied to terms over `x̄`;
- the head `H` is **either** a predicate application `P(x̄)` **or** `false`
  (a *query* clause). "Horn" = at most one positive literal.

A **solution** is an interpretation that assigns each predicate `Pᵢ` a formula
(over its arguments, in the background theory) such that **every** clause becomes
a valid implication. Finding it = **proving the program/system safe**; the
predicate interpretations *are* the inductive invariants. If no solution exists,
the solver exhibits a **derivation of `false`** — a finite proof tree that, read
back, is a concrete counterexample trace.

CHC is exactly "verification condition as logic": the predicates are the holes
where invariants go, the clauses are the proof rules, and the solver's job is to
fill the holes.

### 2.2 Encoding a transition system + parent property

A safety problem `M = (S, I, Tr)` against property `P` lowers to **three clause
roles** over a single invariant predicate `Inv : S → Bool` (the set of reachable
states the solver must characterize):

```
(1) initiation   :   I(s)                    →  Inv(s)
(2) consecution  :   Inv(s) ∧ Tr(s, s')      →  Inv(s')
(3) safety/query :   Inv(s) ∧ ¬P(s)          →  false       (≡  Inv(s) → P(s))
```

Read in English: (1) every initial state is in the invariant; (2) the invariant
is closed under one tick of the FSM; (3) every state in the invariant satisfies
the property. A solution for `Inv` is precisely an **inductive invariant strong
enough to prove `P`**, and it covers runs of *every* length — the unbounded
guarantee BMC cannot give.

- **SAT** (clauses have a solution) ⇒ `Inv` found ⇒ **`P` holds in all reachable
  states, for all seeds in `I`, unbounded.**
- **UNSAT** (no solution; `false` is derivable) ⇒ a **concrete counterexample**:
  a seed and a trace `s₀ … sₖ` reaching a `¬P` state.
- **UNKNOWN / non-termination** ⇒ CHC is undecidable in general; the solver may
  give up (§ 2.6).

### 2.3 What Spacer is, and that Z3 ships it

**Spacer** (*Komuravelli, Gurfinkel, Chaki, "SMT-Based Model Checking for
Recursive Programs," CAV 2014*) is **IC3/PDR generalized to CHC modulo SMT
theories.** Where IC3/PDR was propositional/finite-state, Spacer works over LIA,
LRA, arrays, and ADTs: it maintains per-level over-approximations of the
reachable states, finds counterexamples-to-induction with Z3 SMT queries, and
generalizes them using **model-based projection** and **interpolation** instead
of pure clause-dropping. It is the default, production CHC engine inside Z3,
selected on the `fixedpoint` object via `fixedpoint.engine=spacer`.

Concretely confirmed in the installed tree: the bundled Z3 C source (vendored by
`z3-sys`, version **4.12.1.0**) contains the full Spacer implementation under
`…/z3-sys-0.8.1/z3/src/muz/spacer/` (e.g. `spacer_prop_solver.cpp`,
`spacer_generalizers.cpp`, `spacer_context.cpp`, …). So the linked `libz3` *has*
Spacer; the only question (§ 3) is whether the Rust crates expose the C API that
drives it.

### 2.4 Worked example — the countdown FSM

The canonical example in the corpus (`examples/test_34_halts_within.ev`'s
`decrement`, `fsms-as-functions.md`'s running counter):

```evident
fsm countdown(count ∈ Int, halt ∈ Bool)
    count = _count - 1            -- step: count' = count - 1
    halt  = (_count ≤ 0)          -- halt when the input count is ≤ 0
```

Parent property (the spec the user wants enforced over the *whole* run):

> *"For every seed ≥ 0, the settled (halted) result is exactly 0."*

`S = Int`, `Tr(c, c') ≜ (c' = c − 1) ∧ ¬(c ≤ 0)` (the FSM only advances while not
halted), the halt predicate is `halt(c) ≜ c ≤ 0`, and the result is the first
halted state. With `Inv(c)` the reachable-count predicate:

```
(1) initiation   :   c ≥ 0                       →  Inv(c)            -- any nonneg seed
(2) consecution  :   Inv(c) ∧ c > 0 ∧ c' = c-1   →  Inv(c')          -- step while not halted
(3) safety/query :   Inv(c) ∧ c ≤ 0 ∧ c ≠ 0      →  false            -- "halted ⇒ result = 0"
```

**What Spacer returns.** It searches for an interpretation of `Inv`. The
inductive invariant here is `Inv(c) ≡ c ≥ 0`:

- (1) holds: `c ≥ 0 → c ≥ 0`. ✓
- (2) holds: `c ≥ 0 ∧ c > 0 ∧ c' = c−1 ⇒ c' ≥ 0` (since `c > 0` over ℤ ⇒ `c ≥ 1`
  ⇒ `c−1 ≥ 0`). ✓
- (3) holds: `c ≥ 0 ∧ c ≤ 0 ⇒ c = 0`, so `c ≥ 0 ∧ c ≤ 0 ∧ c ≠ 0` is **UNSAT** —
  there is no such `c`, the query clause is vacuously valid. ✓

So Spacer reports **SAT (safe)**: the property holds **for every seed ≥ 0, at any
run length** — an unbounded proof, with **no `N`**. Contrast BMC, which could only
check seeds and depths up to a fixed bound and would never *prove* the ∀-seed
claim. (The invariant also explains *why* the property is true: it is the integer
`c ≥ 0` closure under `c−1` while `c > 0`. Flip the step to `count = _count − 2`
and the invariant `c ≥ 0` no longer forces `c = 0` at halt — odd seeds settle at
`−1` — and Spacer would instead return the counterexample seed. That parity
sensitivity is exactly the kind of whole-run bug `fsms-as-functions.md` § 1 says
flat FSMs hide and nesting must catch.)

### 2.5 The synthesis direction — and how much CHC expresses

The user also wants the **synthesis** face: *find a seed such that a property
holds at completion.* There are two distinct sub-questions, and CHC handles them
very differently:

**(a) "Find a seed that REACHES a goal state" — CHC does this directly, for
free.** Encode the goal as the query: `Inv(s) ∧ Goal(s) → false`. If the goal is
reachable, the clauses are **UNSAT**, and Spacer's **derivation of `false` *is*
the witnessing seed + trace.** This is the standard *synthesis-as-reachability*
trick: to synthesize an input reaching `Goal`, ask Spacer to "prove `Goal` is
unreachable"; it fails and hands back the concrete seed that reaches it. So the
**reachability / existential-witness** flavor of synthesis is exactly the
counterexample direction — no extra machinery.

**(b) "Find a *fixed* seed/parameter such that the property holds for ALL runs" —
genuine ∃∀, NOT direct.** `∃ seed. ∀ run-from-seed. P` has an existential over
the *invariant's free parameter* on top of the universal CHC body. Plain
CHC/Spacer is a **∀-Horn** solver: it proves "for all reachable states, `P`"; it
does **not** natively existentially-quantify a parameter that must work for all
runs. Expressing (b) requires either:

- **fold the parameter into the state** and reduce (b) to a reachability query
  (works when "find *a* seed" suffices — collapses to (a)); or
- an outer **∃∀ loop** — which is precisely **CEGIS / CEGAR** (*Solar-Lezama,
  "Program Synthesis by Sketching," 2008*): propose a seed, use CHC/SMT to check
  `∀ run. P`, and on failure use the counterexample to constrain the next seed.
  This is GG's loop again, now with Spacer (not a hand-rolled prover) as the
  inner ∀-check.

So: **CHC gives you unbounded verification (1) and reachability-witness synthesis
(a) outright; full ∃∀ parameter synthesis (b) needs an outer CEGIS loop wrapping
Spacer.** Be honest about that boundary — it is the same boundary
`fsms-as-functions.md` § 6 draws between forward dependency (determine then
check) and output-feedback (search).

### 2.6 How `F(seed, fsm_state)` + a parent invariant lowers to a CHC query

The lowering reuses machinery the runtime **already has**. `compose.rs::build_f1`
already does the hard extraction:

- it resolves the `(state, state_next)` pairs (`detect_state_pairs`),
- it pulls the per-output next-state expression `state_exprs[v]` — i.e. the body
  of `Tr` as a function of the input-state const — and the `halt_aggregate`
  (`build_f1`, `compose.rs:408-415`),
- all bottomed out in the input-state consts only (the forward-reference
  resolution fixed-point, `compose.rs:362-389`).

That `state_exprs` map **is** the `Tr` a CHC encoder needs. Instead of composing
`Fᴺ` symbolically (BMC), a CHC lowering emits, against a fresh `Z3_fixedpoint`:

```
declare  Inv : (sort of state) → Bool                        -- register_relation
rule  (1)  Init(s)                       → Inv(s)             -- seed / parent precondition
rule  (2)  Inv(s) ∧ ¬halt(s) ∧ s' = state_exprs(s)  → Inv(s') -- one tick of F
query (3)  Inv(s) ∧ halt(s) ∧ ¬ParentProp(s)        → false   -- parent spec at the result
```

where `ParentProp(s)` is the parent's declarative claim (the surrounding
`claim` body, names-matched onto the child's state fields) and `Init(s)` is the
seed pin (or the parent's precondition on the seed). `Z3_fixedpoint_query` then
returns `SAT` (proved), `UNSAT` (counterexample trace via
`Z3_fixedpoint_get_answer`), or `UNKNOWN`.

This is a **structural sibling of `assert_halts_within`**: same `build_f1`
front-end, same `Tr` extraction, but it emits *Horn rules into a fixedpoint
object* instead of *an N-fold composed Bool into the outer solver*. The
`halt_aggregate` becomes the `halt(s)` guard; the parent claim becomes
`ParentProp`.

### 2.7 Honest limits of Spacer

- **Safety is the sweet spot.** The (1)/(2)/(3) encoding is a *safety* obligation,
  and Spacer is strongest there. The parent-as-invariant case ("the child's run
  never violates `P`") is a perfect fit.
- **Liveness / termination is *not* this encoding.** "Does the child always halt?"
  is a liveness/well-foundedness property — it needs a ranking-function or
  well-founded-relation encoding (a different CHC shape, or a dedicated
  termination prover), not the safety triple above. Our existing `max_iters` /
  `halts_within(F, N)` guard is the bounded substitute and should stay.
- **∃∀ synthesis is not direct** (§ 2.5b) — needs the outer CEGIS loop.
- **Theory matters.** Spacer over **LIA/LRA** (the countdown) is mature and fast.
  Over **algebraic datatypes + recursion** (enum-state FSMs, the recursive
  tree-walk passes `pretty` / `subscriptions` / `validate`) it is far weaker, and
  unbounded recursion over an ADT is exactly the **recursion gap** the rest of the
  design flags as the case where *Z3 is not a sound oracle*
  (`loop-functionizer.md` § 6, COUNTEREXAMPLES #15). Do **not** expect Spacer to
  verify the tree-walk FSMs; those stay on the CEGAR / blocking-interpret oracle.
- **`unknown` / divergence is real.** CHC is undecidable; with nonlinear
  arithmetic, arrays, or deep recursion Spacer can diverge or return `unknown`.
  Any production use needs a timeout and a graceful fall-back to the bounded BMC
  answer.

---

## § 3 — Z3 API COMPATIBILITY CHECK: Fixedpoint / CHC (the load-bearing gate)

**Method:** direct inspection of the two installed crates. Locations:

```
~/.cargo/registry/src/index.crates.io-…/z3-0.12.1/        (high-level safe wrapper)
~/.cargo/registry/src/index.crates.io-…/z3-sys-0.8.1/     (raw extern "C" FFI)
```

The project pins `z3 = "0.12"` and `z3-sys = "0.8"` (`runtime/Cargo.toml:8-9`),
which resolve to exactly these directories.

### 3.1 High-level `z3` 0.12.1 — NO Fixedpoint wrapper

```
$ grep -rni "fixedpoint" z3-0.12.1/src/    →  0 matches
```

The `z3-0.12.1/src/` directory has files for `solver`, `optimize`, `tactic`,
`goal`, `model`, `ast`, `func_decl`, … but **no `fixedpoint.rs`**, and the string
`fixedpoint` does not appear anywhere in the safe crate. **There is no safe
wrapper for CHC/Spacer.** (Confirmed against the file listing of
`z3-0.12.1/src/` and a recursive case-insensitive grep.)

### 3.2 Raw `z3-sys` 0.8.1 — the FULL Fixedpoint/CHC API is bound

The raw FFI crate exposes the **complete** fixedpoint C API as `extern "C"`
declarations. Evidence (file `z3-sys-0.8.1/src/lib.rs`, with line numbers):

| Symbol | Line | Role in the CHC encoding |
|---|---|---|
| `Z3_mk_fixedpoint(c) -> Z3_fixedpoint` | `6215` | create the CHC/Spacer engine object |
| `Z3_fixedpoint_inc_ref` / `_dec_ref` | `6218` / `6221` | refcount management (object is not auto-managed) |
| `Z3_fixedpoint_register_relation(c, d, f)` | `6355` | declare an invariant predicate `Inv` |
| `Z3_fixedpoint_add_rule(c, d, rule, name)` | `6231` | add a Horn clause (1)/(2) |
| `Z3_fixedpoint_assert(c, d, axiom)` | `6258` | background axioms (PDR mode) |
| `Z3_fixedpoint_query(c, d, query) -> Z3_lbool` | `6271` | run the solve; query clause (3) |
| `Z3_fixedpoint_get_answer(c, d) -> Z3_ast` | `6297` | the invariant (SAT) or counterexample (UNSAT) |
| `Z3_fixedpoint_get_reason_unknown(c, d)` | `6302` | diagnose `UNKNOWN` |
| `Z3_fixedpoint_set_params(c, f, p)` | `6382` | **set `engine=spacer`** + tuning |
| `Z3_fixedpoint_from_string` / `_to_string` | `6429` / `6410` | SMT-LIB2 CHC round-trip (debug/escape hatch) |
| `Z3_fixedpoint_query_relations`, `_get_rules`, `_get_assertions`, … | 6281, 6371, 6374 | full surface |

`grep -ci "fixedpoint" z3-sys-0.8.1/src/lib.rs → 104 matches`. The declarations
sit in the crate's single big `extern "C"` block (the lines around 6215 are
bodyless `pub fn … ;` signatures, which only compile inside `extern`), and the
supporting types are defined: `Z3_fixedpoint` (`pub type … = *mut _Z3_fixedpoint`,
`lib.rs:278`).

Every supporting raw builder the encoding needs is also present:

```
Z3_mk_func_decl       : 1   (mint the Inv relation's func_decl)
Z3_mk_bool_sort       : 1   (Inv's range)
Z3_mk_params          : 1   Z3_params_set_symbol : 1   (engine=spacer)
Z3_mk_forall_const    : 1   Z3_mk_implies : 1   Z3_mk_app : 1   (build the Horn clauses)
Z3_global_param_set   : 1
```

And **Spacer itself is in the vendored, linked Z3 4.12.1** (`z3-sys-0.8.1/z3/
src/muz/spacer/`, § 2.3). So the engine the rules drive is present in `libz3`.

### 3.3 Bridging `z3::Context` → raw `Z3_context`: the precedent already exists

The raw functions take a `Z3_context`, but `z3::Context` keeps its `z3_ctx` field
**private** (`z3-0.12.1/src/lib.rs:76-77`, `z3_ctx: Z3_context` — no `pub`, no
accessor). **The project has already solved this**, and the precedent is the
template the CHC wrapper should copy verbatim:
`runtime/src/translate/exprs/string_ops.rs` reaches the unwrapped raw seq-theory
builders by transmuting the single-field newtype, guarded by a compile-time layout
assertion:

```rust
// runtime/src/translate/exprs/string_ops.rs
const _: () = {
    assert!(
        std::mem::size_of::<Context>() == std::mem::size_of::<z3_sys::Z3_context>(),
        "z3::Context is no longer a single-pointer newtype; raw_ctx is unsound"
    );
};
#[inline]
fn raw_ctx(ctx: &Context) -> z3_sys::Z3_context {
    unsafe { *(ctx as *const Context as *const z3_sys::Z3_context) }
}
```

And `z3::ast::Ast::get_z3_ast() -> Z3_ast` is **public** (`z3-0.12.1/src/ast.rs:197`)
and already used by `string_ops.rs`, so the Horn-clause bodies — built with the
*safe* `z3` AST API (`Bool::implies`, `forall_const`, `Dynamic`) — convert to the
`Z3_ast` that `Z3_fixedpoint_add_rule` wants with a single `.get_z3_ast()` call.
For the `Inv` relation predicate, either mint a `Z3_func_decl` directly via raw
`Z3_mk_func_decl(raw_ctx, …)` or wrap a `z3::FuncDecl` — the former avoids
`FuncDecl`'s private field entirely.

The `compose.rs` precedent is even closer: it *already mixes* `z3::{Context,
Solver, ast::*}` with `z3_sys::DeclKind` (`compose.rs:40-42`) and takes a
`ctx: &'static Context`. A CHC module sits naturally beside it.

### 3.4 VERDICT (§ 3) — **(b): usable via raw `z3-sys` FFI + a thin wrapper we write**

Stated against the SESSION's three options:

- **(a) safe `z3` API?** **No.** `z3-0.12.1` has zero `fixedpoint` references — no
  safe wrapper exists.
- **(b) raw `z3-sys` FFI + our own wrapper?** **YES — this is the answer.** The
  complete fixedpoint/CHC C API is bound in `z3-sys-0.8.1` (`Z3_mk_fixedpoint`
  `lib.rs:6215`, `…_register_relation` `:6355`, `…_add_rule` `:6231`, `…_query`
  `:6271`, `…_get_answer` `:6297`, `…_set_params` `:6382`), Spacer is in the linked
  `libz3` 4.12.1 (`z3/src/muz/spacer/`), the supporting builders are all present,
  and the `Z3_context` bridge is a copy of the **already-shipping** `raw_ctx`
  trick in `string_ops.rs`. Building the wrapper is the *same category of work*
  the project already did for string ops.
- **(c) not present / shell out?** **No** — it is present in `z3-sys`; no SMT-LIB
  subprocess or crate patch is needed. (SMT-LIB CHC via `Z3_fixedpoint_from_string`
  remains available as an in-process debug/escape hatch, `lib.rs:6429`.)

The single caveat is that we own the `unsafe` surface: refcounting
(`inc_ref`/`dec_ref`), the layout-assert guard, and pointer hygiene — exactly the
discipline `string_ops.rs` already demonstrates is maintainable here.

---

## § 4 — Z3 external-function / user-propagator API research

### 4.1 What the user-propagator API is

The **user propagator** (a.k.a. user theory-solver) API lets you bolt a *custom
theory* onto a Z3 solver by registering callbacks that fire **during** the search:

- `Z3_solver_propagate_init(c, s, ctx, push_eh, pop_eh, fresh_eh)` — register the
  propagator, supplying backtracking hooks (`push`/`pop`) and a fork hook.
- `Z3_solver_propagate_fixed(c, s, fixed_eh)` — fires when a *registered* term is
  assigned a fixed value by the search.
- `Z3_solver_propagate_eq` / `…_diseq` — fire on equality/disequality merges.
- `Z3_solver_propagate_final(c, s, final_eh)` — fires at a final check (model
  candidate complete), the place to do whole-assignment consistency checks.
- `Z3_solver_propagate_created` — a registered term was created.
- `Z3_solver_propagate_consequence(…)` — *the propagator calls back into Z3* to
  assert a learned consequence or a conflict (a justification clause).
- `Z3_solver_propagate_register(c, s, t)` — mark a term `t` for watching.

You watch a set of terms; when the search fixes enough of them you either
*propagate* an implied equality/literal or *raise a conflict*. It is how custom
theories (and opaque-but-semantic functions) get integrated without modifying
Z3's core (*Bjørner & Nachmanson, "Navigating the Universe of Z3 Theory Solvers";
the Z3 user-propagator tutorial*).

### 4.2 Could it serve as "evaluate the child FSM when its inputs are fixed"?

This is the "external function in Z3" the user asked about. The mapping is:
register the child's seed/input vars with `propagate_register`; in
`propagate_fixed`, once they are all fixed, **run the child FSM
(blocking-interpret) and `propagate_consequence` the result** (`output = computed
value`, with the fixed inputs as the justification). That is, literally, an
external deterministic function evaluated inside the solver.

**Honest assessment — sane but exotic, and the wrong tool for the headline
question:**

- **It only helps when inputs are *fixed by the search*.** For *verification* the
  inputs are pinned (the propagator is just a fancy constant fold). For
  *synthesis* the inputs are exactly what we're solving for, so the propagator
  fires late (only after the search guesses them) and merely *prunes* — the solver
  still searches the input space. It does **not** give the unbounded *proof* CHC
  gives; it gives a callable-mid-solve, not an invariant.
- **The discipline is fragile.** The callbacks must be consistent under
  backtracking (correct `push`/`pop` trail management), must produce sound
  justifications for every propagated consequence, and run re-entrantly inside the
  solver. A bug is a soundness bug, not a clean refusal.
- **It is best suited to "deterministic external function with a known value once
  inputs are fixed"** — which is *already* better served by `fsms-as-functions.md`'s
  **forward-only execute** (LL's pre-evaluate-to-a-constant) for the forward case,
  and by **CHC or CEGAR** for the feedback case. The propagator buys mid-solve
  evaluation, which neither of those needs.

So: a *real* mechanism, occasionally the right tool for embedding an opaque
semantic function, but **not** the principled answer to "parent constrains child
fsm over its whole run."

### 4.3 Compatibility check (same rigor as § 3)

**High-level `z3` 0.12.1:**

```
$ grep -rni "propagate\|Propagator" z3-0.12.1/src/   →  0 matches
```

No propagator support in the safe crate.

**Raw `z3-sys` 0.8.1:**

```
$ grep -ci "propagate" z3-sys-0.8.1/src/lib.rs       →  0 matches
```

and every specific symbol is **absent**:

```
Z3_solver_propagate_init        : 0      Z3_solver_propagate_created     : 0
Z3_solver_propagate_fixed       : 0      Z3_solver_propagate_consequence : 0
Z3_solver_propagate_eq          : 0      Z3_solver_propagate_register    : 0
Z3_solver_propagate_final       : 0      Z3_solver_register_on_clause    : 0
```

**But the symbols exist in the linked `libz3` 4.12.1.** The bundled C API header
declares the full propagator surface:

```
$ grep -c "propagate" z3-sys-0.8.1/z3/src/api/z3_api.h   →  30
  z3_api.h:6934  def_API('Z3_solver_propagate_init', …, (_fnptr(Z3_push_eh), _fnptr(Z3_pop_eh), _fnptr(Z3_fresh_eh)))
  z3_api.h:6951  def_API('Z3_solver_propagate_fixed', …, (_fnptr(Z3_fixed_eh)))
  z3_api.h:6916  def_API('Z3_solver_register_on_clause', …, (_fnptr(Z3_on_clause_eh)))
  …
```

So the propagator is **present in the shared library we link, but *unbound* by
the `z3-sys` 0.8.1 Rust crate.**

### 4.4 VERDICT (§ 4) — **NOT bound; reachable only by hand-rolling the FFI, and the wrong tool anyway**

To use it you would have to do **all** of:

1. **Hand-declare the entire `extern "C"` block** — every `Z3_solver_propagate_*`
   function *and* every callback function-pointer type (`Z3_push_eh`, `Z3_pop_eh`,
   `Z3_fresh_eh`, `Z3_fixed_eh`, `Z3_eq_eh`, `Z3_final_eh`, `Z3_created_eh`,
   `Z3_decide_eh`, `Z3_on_clause_eh`). None are in `z3-sys` 0.8.1. They *link*
   (present in `libz3` 4.12.1), so this compiles and runs, but it is a non-trivial
   FFI surface to author and keep in sync.
2. **Obtain the raw `Z3_solver`** — and here the `string_ops.rs` newtype trick
   **does not apply**: `z3::Solver` is a **two-field** struct
   (`{ ctx: &Context, z3_slv: Z3_solver }`, `z3-0.12.1/src/lib.rs:125-128`), so the
   offset-0 transmute that works for the single-field `Context` reads the wrong
   field. You would realistically have to construct and drive your *own* raw
   solver outside the project's `evaluate` / cached-solver / tactic-chain path —
   abandoning the runtime's solving infrastructure for the propagated queries.

Versus § 3's fixedpoint path — fully bound, single-field-newtype bridge already
shipping — the propagator path is **strictly more binding work, strictly more
unsafe surface, and conceptually the wrong mechanism** for the proof question.
Contrast with CHC/Spacer's verdict (b): the propagator is **(c)-flavored — not
bound by the crate; usable only via a from-scratch extern block plus raw solver
access the crate does not expose.**

---

## § 5 — Comparison and recommendation

### 5.1 The comparison table

| Path | Bounded safety | Unbounded safety | Liveness / termination | Synthesis | Rust binding (from §3/§4) | Engineering effort |
|---|---|---|---|---|---|---|
| **CHC / Spacer** | ✓ (subsumes BMC) | ✓ **(the win)** — discovers `Inv` | ✗ direct (needs ranking encoding) | reachability-witness ✓; ∃∀ via outer CEGIS | **(b)** raw `z3-sys` (full API, `lib.rs:6215+`) + `raw_ctx` bridge (precedent shipping) | **medium** — new `chc.rs`, reuses `build_f1`'s `Tr` extraction |
| **BMC unrolling (CC `fsm_unroll`)** | ✓ **(have it)** | ✗ (only depth ≤ N) | bounded "halts within N" only | bounded counterexample ✓ | safe `z3` (shipping) | **none new** — landed (`compose.rs`) |
| **k-induction on the unrolling** | ✓ | ✓ *if* P is k-inductive | ✗ | — | safe `z3` (shipping) | **low** — adds an induction-step assert to `compose.rs` |
| **Hand-rolled CEGAR (GG)** | ✓ | ✓ in principle (re-implements interpolation/frames) | ✗ | ✓ (the ∃∀ loop) — its purpose | safe `z3` + Functionizer (designed) | **high** — the refinement loop, predicate discovery |
| **User-propagator** | prunes only | ✗ | ✗ | prunes search only | **not bound** — hand-rolled extern + raw 2-field `Solver` (§4.4) | **high + fragile** |

### 5.2 The recommendation — decisive

**Build "parent constrains child fsm" on CHC/Spacer, reached via a raw `z3-sys`
wrapper, with the existing BMC unroller (+ a cheap k-induction add-on) as the
bounded fallback, and CEGAR/blocking-interpret reserved for the
non-condensable-recursive case where Z3 is not a sound oracle.** Specifically:

1. **Primary: CHC/Spacer** for the headline question — *the parent's declarative
   property holds over the child's whole run, for all seeds in the parent's
   precondition, unbounded.* It is the principled tool, it is **reachable** (§ 3
   verdict (b), with file+symbol evidence and a shipping precedent), and the
   encoding reuses `build_f1`'s already-extracted `Tr` (§ 2.6). It is the only
   path that delivers the *unbounded* guarantee `fsms-as-functions.md` § 4 wants
   to recover. Reachability-witness synthesis (§ 2.5a) comes for free.

2. **Bounded fallback: BMC (CC's `fsm_unroll`)** — already landed, already the
   right answer for **affine** bodies (they dissolve to closed form, ratio ≈ 1.0)
   and for cheap depth-bounded answers + counterexamples on anything. When Spacer
   returns `unknown` or diverges (§ 2.7), the BMC depth-N answer is the graceful
   degrade.

3. **Cheap strengthening: k-induction on the unroller** — low-effort, reuses
   `compose.rs` verbatim, and upgrades the bounded BMC answer to an *unbounded*
   proof whenever the parent property is k-inductive. Worth adding before the full
   CHC machinery as a stepping stone (it shares all the front-end).

4. **CEGAR (GG) / blocking-interpret** — **not a competitor to CHC for safety.**
   It is the **synthesis / output-feedback** loop (∃∀, § 2.5b) and the **only
   sound path for the recursive tree-walk class** where Z3 — and therefore CHC
   over unbounded ADT recursion — is *not* a sound oracle (`loop-functionizer.md`
   § 6, COUNTEREXAMPLES #15). Build it where CHC cannot encode the child, with
   blocking-interpret (tier 3) as the ground-truth oracle. The user's instinct is
   right that hand-rolling CEGAR *for the safety question* re-implements Spacer —
   so don't; use it only for the cases Spacer structurally can't reach.

5. **User-propagator — do not build.** Worst binding story (§ 4.4), fragile, and
   the wrong mechanism (it gives mid-solve evaluation, not an invariant proof).

### 5.3 Why this ordering and not "CHC for everything"

Spacer's power is in **LIA/LRA safety**. Two of the corpus's three reasons to want
this capability are not that: (i) the recursive tree-walk passes are ADT-recursion
(Spacer-weak, Z3-unsound oracle); (ii) the branching game FSMs are exactly the
bodies BMC's measurement shows don't condense, and whose invariants Spacer may not
infer. So CHC is the *principled core* for the affine/arithmetic + condensable
feedback case, BMC+k-induction is the always-available bounded floor, and CEGAR is
the escape hatch for the genuinely opaque recursive case. That triad maps exactly
onto the existing **condensability → guarantee spectrum**, which § 6 makes precise.

---

## § 6 — How it plugs into the surface and the selector

### 6.1 `F(seed, fsm_state)` + parent invariants → the recommended mechanism

`fsms-as-functions.md` already gives the surface: an `fsm` referenced in an
equation is **function application to completion** (`result = F(init)`), and the
surrounding `claim`s are the **specification** conjoined around it. To add
"parent constrains child over the whole run," the parent claim's constraints on
`result` (and any invariant over the run) become the `ParentProp` in the CHC
query:

```evident
final ∈ CountState = countdown(seed)     -- child: the implementation (F)
seed ≥ 0                                  -- parent precondition  → Init clause
final.count = 0                           -- parent postcondition → query clause (3)
```

lowers (§ 2.6) to: register `Inv`; rule (1) from `seed ≥ 0`; rule (2) from
`build_f1`'s `state_exprs` guarded by `¬halt`; query (3) from `halt ∧ ¬(count =
0)`. The same `build_f1` front-end that today feeds `assert_halts_within` feeds a
new `chc::prove(F, parent_prop)` instead — emitting Horn rules into a
`Z3_fixedpoint` object rather than an N-fold Bool into the outer solver. The
parent's claim body is the spec; the child's transition is the implementation;
Spacer is the checker.

### 6.2 How the nested-fsm strategy selector chooses

Extend the existing tier selector (`nested-fsm-strategies.md` § 3) — which already
mirrors the `query.rs` functionizer fall-through — with the **forward-vs-feedback
rule** (`fsms-as-functions.md` § 6) as the top-level branch:

```
result = F(init), with parent claims around result
   │
   ├─ NO feedback (parent does NOT constrain F's output; init determined up front)
   │     → FORWARD-ONLY EXECUTE: pre-evaluate F to a constant (LL tier-3 / tiers 1-2),
   │       check the constant against the parent claims once. UNSAT on violation, no retry.
   │
   └─ FEEDBACK (parent constrains F's output; the satisfying seed is NOT known up front)
         │
         ├─ CONDENSABLE (affine step; compose.rs detector accepts) OR step encodes
         │   in a Spacer theory (LIA/LRA, simple ADT):
         │     → CHC / SPACER  (unbounded proof of the parent property over the whole run)
         │        with BMC (compose.rs) as the bounded fallback on `unknown`/divergence,
         │        and k-induction as the cheap unbounded-from-bounded strengthening.
         │
         └─ NON-CONDENSABLE + RECURSIVE (tree-walk; Z3 not a sound oracle):
               → CEGAR (GG) with blocking-interpret (tier 3) as the ground-truth oracle.
```

This is the natural completion of the condensability→guarantee spectrum
(`fsms-as-functions.md` § 5):

| Regime | Dependency | Recovered guarantee | Engine |
|---|---|---|---|
| **Dissolve** | forward, affine | full — one solve | BMC closed-form / CHC |
| **Forward-execute** | forward, branching | checked (UNSAT on violation) | pre-evaluate + check |
| **Feedback, condensable/arithmetic** | output-feedback | **unbounded proof** | **CHC / Spacer** |
| **Feedback, recursive** | output-feedback, ADT recursion | searched (bounded) | CEGAR + blocking-interpret |

The selector's decision inputs are the three already computed by existing
machinery — the body shape (`detect_state_pairs` / `MainShape`), the affine-step
detector verdict (`fsm_unroll/detector.rs`), and now a **theory classifier** (is
the step's state + transition within a Spacer-friendly theory?) — plus one new
bit: **does the parent constrain the child's output?** (forward vs feedback),
which a read/write-set analysis over the embedding equation already has the
ingredients for.

---

## § 7 — Open questions, risks, and a concrete first slice

### 7.1 Risks

- **`unknown` / divergence (the headline CHC risk).** Spacer is incomplete;
  nonlinear arithmetic, arrays, and deep recursion can make it diverge or return
  `unknown`. *Mitigation:* a wall-clock/`rlimit` budget on the fixedpoint query,
  `Z3_fixedpoint_get_reason_unknown` (`lib.rs:6302`) for diagnostics, and a
  guaranteed fall-back to the **bounded BMC answer** (we already have it). Never
  let a CHC `unknown` silently become a "property holds."
- **Theory mismatch on ADT-state FSMs.** Enum-state and recursive tree-walk FSMs
  are precisely where Spacer is weak and where Z3 is *not* a sound oracle. *Do not
  route those to CHC* — the selector's theory classifier (§ 6.2) must send them to
  CEGAR/blocking-interpret. The clean first targets are `Int`/`Real`-state FSMs
  (the countdown, frame counters, affine accumulators).
- **We own the unsafe FFI.** No safe wrapper means we maintain refcounting
  (`Z3_fixedpoint_inc_ref`/`_dec_ref`), the `raw_ctx` layout-assert guard
  (a build break if `z3::Context` ever grows a field — the guard makes that loud,
  not silent), and pointer hygiene. The `string_ops.rs` precedent shows this is
  maintainable, but it is genuinely `unsafe` surface a future `z3`-crate upgrade
  could simplify (if a later `z3` version adds a safe `Fixedpoint` wrapper — not
  present in 0.12.1, and not verified for newer releases here).
- **Faithfulness of the encoding.** The `Tr` extracted by `build_f1` must match
  the FSM's actual per-tick semantics (halt read on the *input* state, the
  first-halt-wins convention `compose.rs` already implements for `halted_state`).
  The CHC consecution clause must use the *same* `¬halt`-guarded step, or the
  proof is about a different machine than the one that runs.
- **∃∀ synthesis scope creep.** Reachability-witness synthesis is free (§ 2.5a);
  true fixed-parameter ∃∀ synthesis is an outer CEGIS loop. Scope v1 to
  verification + reachability-witness; defer ∃∀ to the CEGAR build.

### 7.2 Binding gaps (the honest ledger)

- **CHC/Spacer:** *no gap* — fully bound in `z3-sys` 0.8.1; bridge precedent
  ships. The only "gap" is that we write the wrapper.
- **User-propagator:** *real gap* — unbound by the crate; needs a hand-rolled
  extern block + raw 2-field `Solver` access. Recommended: don't.

### 7.3 A concrete first implementation slice

Mirror exactly how `string_ops.rs` proved the raw-builder approach — smallest
end-to-end thing that proves both the binding and the encoding faithfulness:

1. **New module `runtime/src/fsm_unroll/chc.rs`** (sibling of `compose.rs`),
   behind an `EVIDENT_CHC` env gate. It exposes
   `prove_safety(fsm_name, parent_prop, ctx, schemas, registry, enums)
   -> ChcResult` where `ChcResult ∈ { Proved, Counterexample(trace), Unknown(reason) }`.
2. **Reuse `compose.rs::build_f1`** to get `state_exprs` (`Tr`), the input consts,
   and `halt_aggregate`. (Refactor `build_f1` to `pub(super)` if needed — it is
   already the shared resolution point for `assert_halts_within` and
   `collapse_run`.)
3. **Raw-FFI fixedpoint glue:** `raw_ctx(ctx)` (copy the `string_ops.rs` trick +
   layout assert); mint the `Inv` relation via `Z3_mk_func_decl`
   (`lib.rs` builder present); build clauses (1)/(2)/(3) with the **safe** `z3` AST
   API (`forall_const`, `Bool::implies`) and `.get_z3_ast()` them into
   `Z3_fixedpoint_add_rule` (`:6231`) / `Z3_fixedpoint_query` (`:6271`); set
   `engine=spacer` via `Z3_mk_params` + `Z3_fixedpoint_set_params` (`:6382`).
   Wrap in `inc_ref`/`dec_ref`. Decode the answer via
   `Z3_fixedpoint_get_answer` (`:6297`).
4. **Validate on the countdown** (§ 2.4): assert Spacer returns **Proved** for
   "∀ seed ≥ 0, settled result = 0", and **Counterexample** for the `−2`-step
   parity variant (settles at `−1` for odd seeds) — the same shape
   `tier1_jit.rs` uses to validate `collapse_run` against the blocking-interpret
   oracle. This is the minimal proof that the binding works *and* the Horn
   encoding is faithful to the FSM the runtime actually executes.
5. **Then, and only then,** wire it into the selector (§ 6.2) behind the
   forward-vs-feedback branch, and extend to multi-var / record state.

After slice 1, the report's central claim is no longer a paper verdict: it is a
green test showing Z3's Spacer engine proving a parent property over a child
FSM's whole run, reached from our Rust stack through the same FFI discipline the
runtime already uses for string ops.

---

## Appendix — evidence ledger (file + symbol)

| Claim | Evidence |
|---|---|
| High-level `z3` has no CHC wrapper | `grep -rni fixedpoint z3-0.12.1/src/` → **0**; no `fixedpoint.rs` in `src/` listing |
| Raw `z3-sys` binds the full CHC API | `z3-sys-0.8.1/src/lib.rs`: `Z3_mk_fixedpoint:6215`, `_register_relation:6355`, `_add_rule:6231`, `_query:6271`, `_get_answer:6297`, `_set_params:6382`, `_get_reason_unknown:6302`; 104 `fixedpoint` matches |
| Supporting raw builders present | `Z3_mk_func_decl`, `Z3_mk_bool_sort`, `Z3_mk_params`, `Z3_params_set_symbol`, `Z3_mk_forall_const`, `Z3_mk_implies`, `Z3_mk_app`, `Z3_global_param_set` — each 1 match in `z3-sys` lib.rs |
| Spacer engine in linked libz3 | bundled Z3 **4.12.1.0** (`z3-sys-0.8.1/z3/CMakeLists.txt`); `z3/src/muz/spacer/` source present |
| `Z3_context` bridge precedent | `runtime/src/translate/exprs/string_ops.rs` — `raw_ctx()` + `size_of` layout assert; `z3::ast::Ast::get_z3_ast` public at `z3-0.12.1/src/ast.rs:197` |
| `z3::Context` is single-field (trick valid) | `z3-0.12.1/src/lib.rs:76-77` — `struct Context { z3_ctx: Z3_context }` |
| `z3::Solver` is two-field (trick invalid for propagator) | `z3-0.12.1/src/lib.rs:125-128` — `struct Solver { ctx, z3_slv }` |
| High-level `z3` has no propagator | `grep -rni "propagate\|Propagator" z3-0.12.1/src/` → **0** |
| Raw `z3-sys` does NOT bind the propagator | `grep -ci propagate z3-sys-0.8.1/src/lib.rs` → **0**; all 9 `Z3_solver_propagate_*` / `_register_on_clause` / `_mk_solver_propagate` → 0 |
| …but propagator IS in linked libz3 4.12.1 | `z3-sys-0.8.1/z3/src/api/z3_api.h` → **30** `propagate` decls (`Z3_solver_propagate_init:6934`, `_fixed:6951`, `_register_on_clause:6916`) |
| Existing BMC reuses `z3` + `z3-sys` together | `runtime/src/fsm_unroll/compose.rs:40-42` (`z3::{ast,Context,Solver}` + `z3_sys::DeclKind`); `build_f1:237` extracts `Tr` |
| Project pins | `runtime/Cargo.toml:8-9` — `z3 = "0.12"`, `z3-sys = "0.8"` → resolve to 0.12.1 / 0.8.1 |

### Canonical sources

- **BMC** — Biere, Cimatti, Clarke, Zhu, *Symbolic Model Checking without BDDs*,
  TACAS 1999.
- **k-induction** — Sheeran, Singh, Stålmarck, *Checking Safety Properties Using
  Induction and a SAT-Solver*, FMCAD 2000.
- **IC3** — Bradley, *SAT-Based Model Checking without Unrolling*, VMCAI 2011;
  **PDR** — Eén, Mishchenko, Brayton, *Efficient Implementation of Property
  Directed Reachability*, FMCAD 2011.
- **CEGAR** — Clarke, Grumberg, Jha, Lu, Veith, *Counterexample-Guided
  Abstraction Refinement*, CAV 2000 / JACM 2003.
- **CHC** — Bjørner, Gurfinkel, McMillan, Rybalchenko, *Horn Clause Solvers for
  Program Verification*, 2015; Grebenshchikov et al., *Synthesizing Software
  Verifiers from Proof Rules*, PLDI 2012.
- **Spacer** — Komuravelli, Gurfinkel, Chaki, *SMT-Based Model Checking for
  Recursive Programs*, CAV 2014.
- **CEGIS** — Solar-Lezama, *Program Synthesis by Sketching*, 2008.
- **User propagators** — Bjørner & Nachmanson, *Navigating the Universe of Z3
  Theory Solvers*; the Z3 user-propagator API tutorial.
