# Optimizing Evident Claims (incl. compiling to native functions)

## What this is

Making Evident claim queries fast. The most ambitious path —
compiling a claim to a native Rust function — is described in
detail later; it is **not the only option**, and probably not the
first one to build. This document lays out the full menu of
optimizations, then describes the native-compile path in depth
because that's where the design work has happened so far.

The motivating measurement: the dispatcher's Effect-ordering
toposort takes **0.010 ms in Rust** vs **521 ms via
`rt.query("Toposort<String>", …)`** for the same 26-node /
33-edge graph in Mario (see `docs/bench/dispatcher-toposort.md`).
That's a ~52,000× slowdown for an algorithm any first-year CS
student can write in 10 lines. Closing that gap — for toposort
and for any other claim whose Z3 cost dwarfs the work it actually
does — is what this doc is about.

## The full menu of optimizations

Performance wins are available at every layer of the pipeline, with
very different cost/benefit profiles. Native compilation is the
most powerful and the most expensive to build. Several cheaper
options should be considered first.

| Layer | Option | Build cost | Expected win | When it helps |
|---|---|---|---|---|
| **Caching** | Result cache `(claim_hash, given_hash) → bindings` | Tiny | 1000× on cache hit, 0 on miss | Repeated queries with identical given. Dispatcher toposort is the canonical example; the cache already exists there. |
| | Translation cache: AST → Z3 formula | Small | 2-10× by saving re-translation | Per-tick FSM solves where the body is identical and only `given` changes. |
| | Incremental solving (Z3 push/pop) | Small-medium | 2-20× depending on body size | Same as above — body asserted once, given pins push/popped per tick. Likely a major win for Mario's per-tick solves. |
| | Hot-path profile-guided cache | Medium | Wins what hot paths win | If a small set of (claim, given) shapes dominates runtime. |
| **Simplification** | Pre-translation simplification (constant folding, dead-constraint elim) | Small-medium | 1.2-3× | Bodies with redundant structure, hardcoded constants flowing through chains. |
| | Z3 simplify tactic at translation boundary | Tiny | 1.1-2× | Bodies with simplifiable expressions. Cheap; should just run unconditionally. |
| | Domain narrowing (Int ranges) before encoding | Medium | 1.5-5× | Bodies with bounded numeric vars; reduces Z3's search space. |
| | Quantifier instantiation hints (patterns) | Medium | 1.5-10× | Bodies with `∀` over large but structured domains. |
| **Solver tuning** | Per-claim tactic selection | Small | 1.2-3× | Bodies dominated by one theory (arith vs BV vs datatypes). `EVIDENT_Z3_ARITH_SOLVER` is one knob we already have. |
| | Solver parameter tuning per shape | Small | 1.1-2× | Same — shape-dependent. |
| **Symmetry breaking** | Build colored graph of formula → Bliss/Nauty for automorphism group → emit lex-leader constraints per orbit | Small-medium (Bliss subprocess + encoder) | 10-100× on symmetric formulas | Z3 has *no* built-in symmetry detection. Any claim with permutation-symmetric Seq, interchangeable enum values, or matrix row/column symmetry pays for redundant search Z3 will repeat over each orbit. Production-shipping in BreakID/SAT-comp winners. Distinct from the function-izer: keeps Z3, amplifies it. |
| **Algebraic evaluator** | Walk Z3's normalized formula (post-`solve-eqs`) without further solving | Medium | 100-1000× for function-shaped claims | When the body is deterministic enough to evaluate directly from the substitution chain. Simpler than native compile; covers most of the function-shape win at a fraction of the cost. Operates on the algebraic form, not the AST. |
| **Native compile** | Function-izer (this doc, below) | Large | 10,000-100,000× | Hot, function-shaped claims with stable input shape. |
| **Algorithm registry** | ~~Recognized shape → hand-written Rust~~ | ~~Per-algorithm small~~ | ~~10,000-1,000,000×~~ | **Rejected** — see "Why we rejected the algorithm registry" below. |
| **Generative synthesis** | Constraint model as oracle → CEGIS-train a function from (input, output) examples | Large + ML stack | Variable (1,000-100,000×) | Search-shaped claims that aren't directly compilable. Most experimental tier. Symbolic-regression flavor covers numeric `R^n → R` claims only (Mario per-tick physics, etc.); discrete-recursive claims (sort, toposort) need refinement-type synthesis (Synquid). |
| **Parallelism** | Rayon over claim queries / FSMs | Small | Up to N× cores | When you have many independent queries. Mario's 6 FSMs are an obvious target. |
| | SIMD via autovectorized emit | Architectural | 2-8× on vectorizable loops | `coindexed` / `edges` loops over Seqs of records with pure arithmetic — Mario's per-dot physics, exactly that shape. |

The tiers are **complementary, not alternatives**. A mature
runtime would have all of caching, simplification, algebraic evaluation,
native compile, and algorithm registry, applied in dispatch order
(cheapest hit wins; cheapest miss is cheap; expensive miss is the
last resort). What's described in the rest of this doc is the
most ambitious tier — and the one with the most design questions —
not the first thing to build.

## Algebraic evaluator — the middle tier

"Re-implement Z3 in Rust" is impractical at face value (Z3 is ~500K
LOC of decades of theory work), but there's an honest weaker
version: an **algebraic evaluator** that takes Z3's normalized
formula (the output of `simplify + propagate-values + solve-eqs`)
and evaluates it directly, without invoking Z3's search machinery
again.

For function-shaped claims, that's all that's needed. After
normalization, the residual is a chain of substitutions — `y₁ =
expr(inputs)`, `y₂ = expr(inputs, y₁)`, … — already in dependency
order. Evaluating that chain on concrete inputs takes microseconds.
No code generation, no rustc, no libloading. A few hundred lines
of Rust per theory we support evaluation for (arithmetic, datatype
match, sequence ops, set membership, ternary).

The evaluator is strictly weaker than Z3 — it can't search, so
search-shaped claims fall through to Z3 the way they would in the
native-compile path. But for the cases where function-ization
applies, it captures **most of the win at a fraction of the cost**:

- 100-1000× faster than Z3 (vs 10,000-100,000× for native compile)
- Days to a couple weeks to build (vs months for full native compile)
- No code-generation correctness questions (no `quote!`, no
  `syn::parse2`, no rustfmt, no libloading lifecycle)
- Same gate (2-copy UNSAT check) selects which claims go through it

The evaluator should be built **before** the native compile path,
both because it's cheaper and because the native compile path's
gate and extraction logic can be developed against it first — the
only difference is the final emit step (evaluator walks the
normalized formula directly; native compile emits Rust source
from the same formula).

**Critical: the evaluator's input is a Z3 formula, not an AST.**
That's the key correction over earlier drafts of this doc.
Equivalent claims with different source spellings normalize to the
same formula and evaluate identically. The evaluator is an
algebraic-structure walker, not a syntax-tree walker.

## What compiler optimizations enable, and what that means for emit

rustc + LLVM already do constant folding, dead-store elimination,
CSE, inlining, autovectorization — *for free*, on whatever native
code we emit. The architectural decision is whether to emit code
in a form that lets those optimizations fire.

Concretely:

- **For SIMD autovec**: emit `for x in xs.iter()` / `xs.iter().map(…).sum()`
  patterns, not recursion or manual indexing. Mario's
  `∀ (cur, nxt) ∈ coindexed(state.dots, state_next.dots) : …` is a
  textbook autovec target — pure arithmetic, no branches, contiguous
  Seqs. Emit it as an iterator chain.
- **For inlining**: emit small functions; let rustc decide. Don't
  manually CSE everything — duplicate small expressions and let
  LLVM merge them.
- **For PGO**: not worth pursuing until everything else is done.
  Adds workflow complexity for a small marginal win.

The SOA-vs-AOS question (Struct-of-Arrays vs Array-of-Structs) is
real but Evident pushes us toward AOS via record types. If SIMD on
something like "x-positions of all dots" matters, we'd need to
expose a parallel-Seq view at emit time —
`Seq(Dot) → (Seq(Int) x_positions, Seq(Int) y_positions, …)`.
That's an emit-side specialization, not a language change.

The algebraic evaluator doesn't benefit from any of this — it
evaluates the normalized formula directly, no Rust emit. The
native-compile path benefits from all of it, given the right
emit patterns. That's a real differentiator between the two
tiers, and one reason to build both rather than treating them
as alternatives.

## Generative synthesis — the constraint model as oracle

A claim that fails the direct compilation gate (2-copy SAT, or
`solve-eqs` leaves non-trivial residue) isn't necessarily a dead
end. The constraint model is still useful as a **test oracle**:
generate inputs, solve each via Z3 to get (input, output) pairs,
and synthesize a function from the corpus. This is the
**CEGIS pattern** — CounterExample-Guided Inductive Synthesis —
applied to function generation rather than program verification.

The pipeline:

```
1. SAMPLE INPUTS
   Generate diverse inputs from the input domain. Random,
   boundary-targeted, or constraint-guided (use Z3 to produce
   inputs that hit unexplored regions of the input space).

2. ORACLE-ANSWER EACH
   Solve the constraint model on each input via Z3; record the
   (input, output) pair. Z3 is acting as the oracle that tells us
   "for THIS input, here's a valid output."

3. SYNTHESIZE A CANDIDATE FUNCTION
   Train a function from the (input, output) corpus using any
   suitable synthesis technique:
     - Symbolic regression (gplearn, PySR, AI-Feynman)
     - Program synthesis (SyGuS, Sketch, Manthan)
     - Decision-tree / gradient-boosted regression
     - LLM-driven code generation against the corpus
     - Inductive logic programming
   The output is a candidate Rust function (or other lowering target).

4. VERIFY AGAINST THE CONSTRAINT MODEL
   Assert ∀input. constraint(input, candidate(input)) is valid.
   If yes: candidate is provably correct, install in cache and
   dispatch through it.
   If no: Z3 produces a counterexample (input where candidate
   diverges). Add it to the training corpus; go to (3).

5. CONVERGE OR GIVE UP
   With each counterexample the candidate gets stronger. If
   convergence stalls (no improvement after N iterations), give
   up and fall through to the solver.
```

The synthesized function need not be *the* minimal algorithm
(Kahn's, etc.) — just *a* function that satisfies the constraints.
For toposort, the synthesizer might rediscover Kahn's, or it might
emit a less elegant but correct routine; either is fine.

**Where this fits in the tier order:**

```
1. Algorithm registry        — pattern-match known shapes
2. Direct compilation         — gate + extract (this doc's main subject)
3. Algebraic evaluator        — same gate, walk the normalized formula
4. Generative synthesis       — oracle-based, for cases (1-3) reject
5. Z3 solver                  — final fallback
```

(4) is far more expensive per-claim than (2-3) but cheaper than
expecting a human to write the algorithm. It opens the door to
making *any* search-shaped claim fast, given enough oracle time
and a flexible-enough synthesizer.

**What this is NOT.** Generative synthesis does not bypass the
correctness story — the verification step (4) ensures the
synthesized function matches the constraint model on every input
Z3 can check. If verification can't be discharged (the verifier
returns "unknown" on some inputs), the synthesized function is
*candidate quality*: usable with a runtime safety check (compare
output against the solver on a sample of calls) but not provably
correct. That's a real tradeoff and the runtime should expose it
(e.g. `EVIDENT_TRUST_SYNTHESIZED=1` to skip the safety check).

### Classical symbolic regression — what it actually does

**Decision: no LLM-based synthesis.** The synthesis tier uses
classical techniques only — genetic programming, refinement-type
synthesis, CEGIS. LLMs make the verification story worse without
adding capability classical techniques don't have for *this*
problem shape.

A survey of non-LLM techniques (PySR, AI Feynman, EQL, DSO,
Aleph, Popper, Synquid, Sketch) yields a sharp partitioning by
output shape:

**Tier S.1 — `f: R^n → R` arithmetic claims.** Pure symbolic
regression (PySR / SymbolicRegression.jl is the strongest
implementation) handles closed-form algebraic expressions over
real inputs. SRBench results: exact recovery of all 100 Feynman
equations, Lorenz, pendulum, Lotka-Volterra. Operators are
arity-1/2 closed-form (`+ - * / sin cos exp log pow`). Typical
fit: seconds to minutes for ≤10-variable inputs. Strong fit for:
**Mario's per-tick physics**, easing curves, force calculations,
trajectory equations. Anything where a claim's content reduces
to "this scalar = this algebraic expression over these scalars."

**Tier S.2 — Boolean / enum-shaped claims.** Quine-McCluskey or
Espresso (logic-minimization tools) reduce any Boolean formula
to its canonical minimal DNF/CNF. For collision predicates,
dispatch tables, enum-valued match expressions — this is the right
tool, not SR. Decidable, fast, no training.

**Tier S.3 — Discrete-recursive claims (sort, toposort, parse).**
**Out of reach for SR.** The hypothesis space of pure SR is
arithmetic expression trees over a fixed operator set; it cannot
represent recursion, loops, or symbolic data. Documented across
PySR, AI Feynman, EQL, DSO papers — none has synthesized any
sort, traversal, or control-flow function from examples. The
ceiling is mathematical, not engineering.

For this tier, the classical options are:

- **Inductive logic programming (Popper, Metagol, Aleph)** —
  CAN learn recursive list / number programs. Caveat: only when
  background predicates like `partition`, `append` are *provided*.
  Without them, no system in this class invents sort de novo
  from `[3,1,2] → [1,2,3]` examples.
- **Refinement-type synthesis (Synquid)** — published synthesis of
  insertion-sort, merge-sort, BST insert/delete from polymorphic
  refinement-type specs. Spec engineering ≈ writing the function.
- **Sketch / SyGuS / CVC5** — fill `??` holes given a partial
  program skeleton. Same trade: the skeleton is most of the
  answer.

All of S.3's options require the programmer to supply enough
structure that they're roughly co-writing the function. That's
fine if our framing is "give the compiler enough hints" but it's
not "synthesis from examples alone."

### What this means for Evident's synthesis tier

For each new claim Evident wants to compile via synthesis:

1. **If the output is a single real or vector of reals over real
   inputs**: PySR via subprocess. Sample 1000 (input, output) pairs
   from Z3 (oracle), feed PySR, get back a closed-form expression,
   verify with Z3 (universal quantifier check), emit Rust. Strong
   fit for per-tick physics.
2. **If the output is Bool / small enum over Bool / enum inputs**:
   Use Espresso, not SR. Build the truth table from Z3 oracle calls,
   reduce, emit.
3. **If the output is discrete / recursive**: SR can't help. Either
   write the algorithm in Evident as recognizable piecewise idioms
   (loops + assignments — see "Piecewise reconstruction" above)
   so the function-izer compiles it directly, or write the
   constraint claim and accept Z3 cost.

The honest cost picture: synthesis is a useful tier for the narrow
slice of claims that are *purely arithmetic* — and that slice
overlaps with the most performance-critical parts of game-like
workloads (physics, animation curves). It's not a path to a fast
toposort.

**Tools we'd actually use:**

- **PySR / SymbolicRegression.jl** — https://github.com/MilesCranmer/PySR.
  Subprocess from Rust; Julia-backed. The fastest, most-published
  open-source SR engine.
- **AI Feynman 2** — https://github.com/SJ001/AI-Feynman. Higher
  recovery rate on physics-flavored equations via dimensional
  analysis and symmetry detection.
- **Espresso** — classical logic minimization; ships in EDA tools.
  Use for Bool-shaped claims.
- **Synquid** — https://github.com/nadia-polikarpova/synquid. The
  only published technique that synthesizes sort from a spec; would
  need our refinement types to plug in.

**Open questions for this tier:**

- Sampling strategy — Z3 returns one model per call; for diverse
  training data we need blocking-clause enumeration + hash-bucketing
  + size-stratified sweeps (covered in the input-sampling research,
  ~200 lines on top of the `z3` Rust crate).
- Subprocess overhead — PySR convergence is seconds; sample
  collection is also seconds; total compile-once cost is in the
  10-second range per claim. Amortizes only if the function is
  called many times.
- Generalization — a function trained on n=10 inputs may not
  generalize to n=1000. May need specialization per input shape.

This tier should land *after* the piecewise-reconstruction
function-izer is solid:
- The Z3 oracle for sampling is already in our runtime.
- Direct compilation handles the easy cases first; synthesis only
  gets called on the residual.
- Verification reuses the function-izer's gate machinery.

## Why "re-implement Z3" isn't the answer

The user-natural intuition — "Z3 is software, so we could
re-implement it in Rust and that'd be a function" — is correct in
the abstract but impractical in scope. Z3 is decades of decision
procedures, theory combinations, heuristics, and engineering. A
faithful reimplementation would itself be a multi-year project
and the result would be slower than Z3 for most inputs.

What's feasible is the **subset of Z3's behavior that our
function-shaped claims actually need**: evaluation of a normalized
formula along its substitution chain — arithmetic, datatype
dispatch, sequence operations, set membership, ternary folding.
That subset is the algebraic evaluator above. It's not "Z3 in
Rust"; it's "evaluate a normalized formula on concrete inputs,"
which is a much smaller and more tractable thing.

For search-shaped claims, the path forward is **algebraic pattern
recognition** — identifying mathematical structures (permutation,
fold, linear dependency, …) in the *normalized* formula and emitting
the corresponding native operation. See "Algebraic pattern
recognition" below.

## Why we rejected the algorithm registry

An earlier draft of this doc proposed an **algorithm registry**: a
table mapping recognized claim shapes (Toposort<T>, Sort<T>, …) to
hand-written Rust implementations. A prototype landed and worked —
it closed the dispatcher's 521ms → 0.029ms gap on Mario's toposort
dogfood path. Then we removed it.

The rejection reasons:

1. **It's FFI in disguise.** The programmer writes a constraint
   claim that the runtime *recognizes* and routes to a Rust
   function. That is the FFI contract — "here's a function name,
   here's the implementation in the host language." Wrapping it in
   structural matching changes the surface, not the substance. We
   already have FFI for cases where a Rust function is the answer;
   the registry was a second, less honest path to the same thing.

2. **It doesn't move self-hosting forward.** The long-term goal is
   Evident compiling its own runtime. Every algorithm in the
   registry is an algorithm *not* implemented in Evident. The more
   the registry grows, the further from self-hosting we are.

3. **The matcher is the actual problem.** A variations test (see
   `runtime/tests/toposort_variations.rs` in the rejected branch)
   exposed that even a "generous" matcher caught only 4 of 7
   plausible toposort spellings. The remaining 3 would need
   structural recognition of constraint patterns — exactly the
   piecewise-reconstruction work below. Once we have *that*, the
   registry adds nothing.

4. **Performance gains are illusory in expectation.** The registry
   accelerates exactly the claims we hand-coded for. Anyone who
   writes a slightly different shape gets no benefit and may not
   even know why. Worse, some variations produced silently-wrong
   answers because Z3 dropped malformed constraints — the registry
   would have rescued them, but only because we got lucky and
   the matcher fired. We don't want a perf feature whose absence
   is a correctness bug.

What we keep from the experiment: the variations-as-test discipline
(small `.ev` files exercising plausible spellings of a target
pattern) and the benchmark instrumentation (`EVIDENT_TOPOSORT_IMPL`,
`EVIDENT_DISPATCH_TIMING`). Both are useful for the genuine
function-izer when it lands.

## Decomposition: re-separating the composed model

Before any function-shape analysis happens, the function-izer's
first job is **decomposition** — undoing the syntactic composition
that produced the claim, recovering the independent sub-models the
program was built from.

### The framing

Evident programs are written by composition. A complex claim is
typically built up via `..ClaimName` passthrough, `ClaimCall`
invocation, names-match, and shared variables. Each individual
piece is often well-shaped on its own — small, focused, with
obvious algebraic structure. The composition flattens them into a
single body, but the *separability* of the original pieces is
still there algebraically, even if the source no longer makes it
visible.

We start with our models mostly separate, then we combine them.
The runtime's job — for execution efficiency, for analysis, for
compilation — is to **re-separate** them. The re-separation
recovers the natural decomposition the program intended, and lets
the rest of the pipeline (function-shape analysis, code emission)
operate on each piece independently.

This isn't just an optimization. It's the *correct architectural
framing*: composition is a writer's tool; the runtime un-does it
to recover structure.

### The algorithm

After pinning the `given` set, build a hypergraph:

- Nodes: free variables (not in `given`).
- Hyperedges: constraints, each connecting the variables it
  mentions.

Compute connected components via union-find. Each component is a
**separable sub-model** — its variables don't share any
constraint with variables outside the component, so it can be
analyzed and solved independently.

Linear time in the size of the formula. No Z3 calls.

```
1. Pin `given`; treat its variables as broadcast constants.
2. For each constraint c in the body:
     vars(c) ← free variables mentioned in c
     union-find: merge all elements of vars(c)
3. Read off connected components — each is a sub-model.
4. Recurse: each sub-model now becomes the input to the rest of
   the pipeline (function-shape analysis, pattern recognition,
   compilation), independently of its siblings.
```

### Why this is the right first step

- **Each sub-model is smaller**, making downstream analysis
  (per-variable functional closure, e-graph saturation,
  pattern recognition) much cheaper.
- **Components are independent by construction** — compiling
  one doesn't affect the others. Native code and Z3 calls
  coexist; we don't have to commit a whole claim to one path.
- **The decomposition is purely structural** — no semantic analysis
  needed, no Z3 calls. It's a one-pass union-find.
- **It exposes "potential function outputs"** — variables that
  weren't obviously function-shaped in the whole claim become
  obvious once isolated to their component.
- **It generalizes the "is this claim function-shaped?" gate**.
  Old framing: binary — the whole claim either is or isn't.
  New framing: per-component — each independent piece is judged
  on its own. A claim with one search-shaped component and ten
  function-shaped ones gets ten components' worth of native
  code; Z3 only runs on the one.

### Function-shape check per component

Once decomposed, each component is judged independently. For each
component:

```
1. 2-copy uniqueness check on the component's free variables:
   ∃ x, y₁, y₂. body_c(x, y₁) ∧ body_c(x, y₂) ∧ y₁ ≠ y₂
   UNSAT → component is function-shaped, compile it
   SAT/UNK → component is search-shaped, Z3 solves it

2. For function-shaped components, run the algebraic pattern
   recognition below to choose how to emit (linear functional
   dependency? fold? map? etc.).

3. At rt.query time:
   a) call native code for function-shaped components, in order
   b) augment given with their computed values
   c) hand the residual (search-shaped components + their
      shared variables, if any) to Z3
```

A component can fail the function-shape gate but still benefit
from further decomposition — e.g., a component that's
"search-shaped" globally but has a function-shaped *interior*
because some of its variables are determined by others within
the component. That's the per-variable functional-closure
analysis from earlier framings, now applied locally inside a
single component. The two compose: decompose into components,
then within each component apply functional closure if the
component as a whole isn't function-shaped.

### Connection to existing literature

This is well-trodden ground in constraint programming:

- **Hypergraph connected components** is the simplest case —
  Beeri / Fagin / Maier / Yannakakis (1983) studied this for
  acyclic hypergraphs and database joins.
- **Tree decomposition / treewidth** (Robertson & Seymour;
  Dechter, *Constraint Processing*, 2003) generalizes:
  components of width 1 are exactly connected components,
  higher widths admit bounded "interface" variables.
- **Yannakakis's algorithm** for acyclic conjunctive queries
  (Gottlob et al.) gives a linear-time solver when the
  hypergraph is acyclic.
- **AC-3 / AC-4 arc consistency** propagates within components
  before search.

For our v1, plain connected components is enough. Higher-width
tree decomposition is a future-work optimization for the rare
case where components are large and tightly coupled.

### Worked example

Consider a claim mixing physics, rendering, and a search:

```
claim FrameTick(world, world_next, frame_state, ...)
    -- physics (function-shaped)
    world_next.player.pos = world.player.pos + world.player.vel
    world_next.player.vel = world.player.vel + gravity

    -- rendering effects (function-shaped)
    sky_eff   = set_color(80, 160, 220)
    clear_eff = render_clear()

    -- a small constraint solve (search-shaped)
    frame_state.spawn_seed ∈ {0..100}
    frame_state.spawn_seed ≠ world.last_seed
```

After decomposition:
- Component A: physics variables (`world_next.player.pos`,
  `.vel`). Function-shaped. Compile to native arithmetic.
- Component B: rendering effects. Function-shaped (constant
  literals). Compile to native.
- Component C: `spawn_seed`. Search-shaped. Leave on Z3.

`rt.query` becomes: native(A) + native(B) + tiny Z3 call(C).
Most of the work runs at native speed; the residual Z3 call
sees only the constraint that actually needed search.

This is the function-izer working at the granularity it should
have all along — variable-level, not claim-level — and arrived
at via a structural step (decomposition) before any semantic
analysis.

### Empirical decomposition data (Mario + stdlib)

The decomposition pass has been implemented and run across our
codebase. See `runtime/src/decompose.rs` and the explorer at
`runtime/examples/explore_decomposition.rs`. The findings confirm
the architectural framing and surface a separate translator gap
worth flagging.

**Mario's FSMs decompose almost entirely into singletons plus one
"real" component**:

| FSM | Total components | Singletons | Multi-var components |
|---|---|---|---|
| `display` | 94 | 92 (97.9%) | **28-var** + 7-var |
| `game` | 76 | 74 (97.4%) | **45-var** + 2-var |
| `level_gen` | 91 | 90 (98.9%) | **18-var** |
| `keyboard` | 83 | 80 (96.4%) | 11 + 9 + 2 |

Each FSM has exactly one (occasionally two) multi-variable
components carrying the substantive computation — collision
detection for `game`, the rendering pipeline for `display`,
level selection logic for `level_gen`. The other 95%+ of
variables are isolated leaves: constants, baseline type
declarations brought in by `..` passthrough, intermediate
helpers that don't link to anything.

This validates the framing concretely: **the natural unit of
the function-izer is the substantive component, not the whole
claim.** A function-izer that compiles even just the biggest
component of each FSM gets at the substance; the 90%+ singletons
are already trivially determined and cost nothing.

The biggest cluster across the entire codebase is 45 variables
(Mario's `game` FSM physics). That's small enough that the
per-component 2-copy gate is cheap (a single Z3 call on a
45-variable formula completes in single-digit milliseconds);
the algebraic-pattern matcher can afford to be thorough.

**Stdlib's set-flavored claims surface a specific translator
gap** that the decomposition pass made visible:

| Claim | Components | Notes |
|---|---|---|
| `Toposort<Int>` | 3 singletons | `#items=#sorted`, `distinct(p)`, `∀ x ∈ p : x ∈ s` all dropped |
| `Permutation<Int>` | 2 singletons | same root cause |
| `propagate_int` | 83 singletons | `∃ i, j ∈ {0..body_len - 1}` dropped — depends on body_len pin |
| `infer_int_from_single_assignment` | 76, one 8-var cluster | partial — some structure survives |

These claims look "fully separable" not because they ARE, but
because key constraints didn't translate. The specific root
cause: **`#set` (Set cardinality) doesn't translate to Z3**.
That cascades through `Permutation<T>`'s body:

- `#s = #p` — `#s` doesn't translate (Set), so the equality drops
- `distinct(p)` — needs pinned `#p`, but without `#s = #p`
  flowing, `#p` isn't pinned, so the unroll fails
- `∀ x ∈ p : x ∈ s` — same: needs `#p` to unroll the ∀

The runtime's preprocess pass `apply_seq_lengths` only pins Seq
lengths from `#seq = N` literals. The parallel `apply_set_lengths`
that would handle `#set = N` doesn't exist. Building it is a
specific, contained task — once it lands, the cascade reverses
and Toposort/Permutation get their real constraints.

**Implication**: these claims are running *unsoundly* today,
not just slowly. `query` returns "satisfied" with garbage
`sorted` values that don't respect the dropped `distinct` or
edge-ordering constraints. The function-izer can't help
without the translator fix; it would inherit the unsoundness.

The decomposition pass surfaced this as a byproduct: a claim
that decomposes to all-singletons when the program clearly
intended significant linking structure is almost certainly
running with dropped constraints. **The pass doubles as a
"are my constraints actually reaching Z3?" diagnostic** —
an unintended but valuable side benefit.

Order-of-operations implication for self-hosting:

1. **Fix `#set` translation** (apply_set_lengths preprocess). Small,
   bounded, makes Toposort/Permutation actually solve correctly.
2. **Then the decomposition pass will show their real structure**
   — and the function-izer can target their multi-var components.
3. **Then native compilation** is meaningful on those claims.

Without step 1, steps 2-3 are working from a phantom model.

## Algebraic pattern recognition — not source patterns

The function-izer's job is not to walk the AST and translate
syntactic forms. It's to recognize **algebraic structure** in the
model and emit the corresponding computation. The two are
different — the same algebraic content can be written many ways,
and the right tool ignores the source spelling.

Examples of what the *same* algebraic property looks like in
different source spellings:

| Source spelling | Algebraic property |
|---|---|
| `y = 2*x + 1` <br> `2*x - y = -1` <br> `y - 1 = 2*x` | y is a linear function of x |
| `∀ x ∈ p : x ∈ s` ∧ `distinct(p)` ∧ `#p = #s` <br> `Permutation<T>(s, p)` <br> `multiset(p) = multiset(s)` | p is a permutation of s |
| `cond ⇒ y = a` <br> `y = (cond ? a : y)` <br> `¬cond ∨ y = a` | y conditionally equals a |
| `∀ i ∈ {0..#s - 1} : pred(s[i])` <br> `∀ x ∈ s : pred(x)` | pred holds universally over s |

A syntactic matcher catches one row of each table. An algebraic
matcher catches all of them, because all rows reduce to the same
canonical form under the normalization tactics.

### Tools for getting to the algebraic structure

We already have most of them:

- **Z3's tactic pipeline** for normalization. `simplify`,
  `propagate-values`, `solve-eqs`, `nnf`, `purify-arith` rewrite
  the formula into algebraic canonical forms. After applying the
  right tactic chain, equivalent claims look the same to a pattern
  matcher.
- **E-graphs / equality saturation** (the `egg` Rust crate). An
  e-graph represents equivalence classes of terms; matching a
  pattern hits *all* terms in the class, regardless of which
  representative Z3 happens to choose. For patterns that depend
  on associativity, commutativity, distributivity, the e-graph
  is the principled tool.
- **Term rewriting systems** — apply a confluent rewrite system to
  reach a canonical form, then match. Less powerful than e-graphs
  but simpler to reason about.

The matcher's input is **not the AST**; it's a Z3 formula (or an
e-graph over Z3 formulas) after normalization.

### The patterns library

A pattern is `(algebraic shape) → (code template)`. Examples of
the kinds of patterns we want to recognize:

**Linear functional dependency.** After `solve-eqs`, the residual
formula has the shape `y = c₀ + c₁·x₁ + c₂·x₂ + …` where the cᵢ
are known constants and the xᵢ are inputs. Emit `let y = c₀ + c₁ * x₁ + …`.

**Permutation.** A Seq variable is constrained to be a permutation
of a Set variable: same multiset, distinct elements. Emit an
enumeration of the set and a sampling step (Fisher-Yates with
optional seed).

**Pairwise ordering over a permutation.** A Seq is a permutation
of a Set, plus a relation that requires `position(a) < position(b)`
for every (a, b) in some edge set. Algebraically: toposort. Emit
Kahn's algorithm derived from the structure.

**Fold over a Seq.** A scalar variable `r` is constrained by
`r = base ∧ ∀ i : r = f(r, s[i])`, where the implicit binding
threads the accumulator through. Algebraically: a fold. Emit
`s.iter().fold(base, f)`.

**Map over a Seq.** A parallel Seq `t` is constrained by
`#t = #s ∧ ∀ i : t[i] = f(s[i])`. Emit `s.iter().map(f).collect()`.

**Linear program / convex region.** All constraints are linear
inequalities over reals, with a unique optimum. Use a simplex /
interior-point solver, not Z3.

Each pattern lives at the algebraic level. The matcher works on
the Z3 formula post-normalization. The code template generates
the corresponding native operation.

**Crucially, the algorithm is derived from the algebra, not just
dispatched by name.** Recognizing "permutation + pairwise ordering"
as toposort isn't naming a function; it's identifying the
mathematical structure for which Kahn's algorithm is the standard
realization. The template that emits Kahn's IS the function-izer's
output for that algebraic structure. If we want a different
realization (DFS-based toposort, parallel toposort), it's a
different code template against the same pattern.

### Why this is principled where the algorithm registry wasn't

- The matcher doesn't depend on source-level naming or arrangement
  of constraints. Anyone writing a claim that *means* toposort gets
  the optimization — they don't have to spell it the canonical way.
- The matcher composes. A wrapper claim with extra unrelated
  constraints still exposes the toposort substructure after
  normalization; the matcher fires on the substructure, the rest
  routes through other patterns or Z3.
- The library is **inherently extensible** — adding a new pattern
  doesn't require parser changes or new keywords; just a new entry
  in the algebraic-pattern → code-template table.
- It's expressible in Evident itself once we have AST reflection
  over normalized formulas. The function-izer becomes a
  self-hostable pass: an Evident program that takes a normalized
  formula and produces Rust source.

### Connection to existing literature

This framing has direct prior art:

- **E-graph rewriting in compilers** (Tate et al., "Equality
  Saturation: A New Approach to Optimization," POPL '09). The
  optimization passes don't walk the AST; they saturate an e-graph
  over the program and extract the best representative.
- **Term rewriting in CAS** (Mathematica, Maple, SymPy). Symbolic
  computation systems are built on algebraic-pattern matching, not
  source matching.
- **Z3's tactic combinators** (de Moura & Passmore, "The Strategy
  Challenge in SMT Solving"). The tactic framework is exactly the
  algebraic-rewriting machinery we want; we're using it more
  deeply than just "solve please."

The `egg` Rust crate (Willsey et al., POPL '21) gives us
production-grade e-graph + equality-saturation in our runtime
language. https://github.com/egraphs-good/egg.

### Toposort under algebraic recognition

Where the rejected algorithm-registry approach asked "does the
schema name + param names match the toposort fingerprint" — a
syntactic check — the algebraic approach asks "after Z3
normalization, does the formula have the structure of a
permutation plus a partial order?" Any claim whose normalized
form contains that structure gets the toposort code template,
regardless of how the source was written.

The function-izer might pull the toposort *substructure* out of a
larger claim (Schedule, LevelGen, etc.) and emit the loop just
for that substructure, while routing the surrounding constraints
through other patterns. This is what made the algorithm registry
brittle — it required the whole claim to match. Algebraic
pattern recognition matches local structure inside any larger
claim.

---

The rest of this document goes deep on the **native-compile tier**
specifically — the design that's been researched in depth. The
algebraic-evaluator tier shares most of the upstream architecture
(gate, extract, sample-recognition) and differs only in the final
step (evaluate the normalized formula directly vs emit Rust source
from it); assume the design below applies to both with that
caveat.

## Mathematical foundations

Across four areas — computational algebra, category theory, group
theory, and abstract interpretation — a coherent picture emerges
of what the function-izer actually is and what tools to use. The
areas don't compete; each addresses a different layer of the same
problem.

### What the function-izer IS, categorically (allegories)

A claim defines a relation `R ⊆ A × B` over its variables. The
relevant category is an **allegory** — locally-poset 2-category
with composition, intersection, and converse. The canonical
example is **Rel** (objects = sets, morphisms = relations); for our
typed setting, the allegory is built on top of the type structure.

A morphism in an allegory is a **map** when it satisfies *totality*
(`1 ⊆ R°;R` — every input has at least one output) and
*determinism* (`R;R° ⊆ 1` — at most one output per input). The
**map sub-category** of Rel is Set. Maps are *exactly* the
functional relations.

The 2-copy uniqueness check we already use — `∀x. ∃=1 y. R(x,y)`
— is **literally the determinism axiom** of an allegory. We're
doing the right categorical thing, with a different vocabulary.

What this framing makes obvious that we hadn't been asking:

- **Largest functional sub-relation.** A claim with mixed
  function-shape and search may admit a `map ⊆ R` covering part
  of the relation. The function-izer compiles the map; Z3
  handles the residue. Currently our gate is binary (compile or
  fall through); allegorically it's a *factorization* into a map
  + a residual.
- **Tabulation** (R = f;g° for maps f, g) is the categorical
  justification for input/output partition. The partition isn't
  arbitrary — it's a choice of tabulation.
- **Lenses are literal, not metaphor**, for the wrapper case.
  `Toposort<Rect>` IS a lens onto `Toposort<Int>`: `get` strips
  to indices, solve runs, `put` re-attaches. The GetPut / PutGet
  / PutPut laws give us the correctness conditions wrappers must
  satisfy.

Definitive reading: Freyd & Scedrov, *Categories, Allegories*
(1990); Bird & de Moor, *Algebra of Programming* (1996) — directly
applies allegories to deriving programs from relational specs,
which is our exact problem.

### How to RECOGNIZE structure (computational algebra)

The matcher operates on the *Z3-normalized* formula, not the AST.
The right toolkit, stacked from cheapest to most expensive:

- **Hermite Normal Form / Smith Normal Form** (via the `flint`
  library, Rust-bindable). Polynomial-time canonical form for
  linear systems over the integers. If `Ax = b` is the linear
  fragment of a claim's constraints, HNF of A immediately exposes
  which variables are functionally determined (pivot columns) and
  which are free. Linear functional dependency falls out
  decisively, no heuristics.
- **E-graph rewriting** via the `egg` Rust crate. Handles
  invariance under equivalent rewriting structurally — e-classes
  represent equivalence; pattern matching hits the whole class,
  not just one representative. Production-proven (Cranelift).
  This is the right substrate for the pattern library.
- **Gröbner bases** (Buchberger / F4 / F5) via `msolve`
  subprocess. For *polynomial* constraints, the reduced Gröbner
  basis under an elimination ordering literally exposes
  `z - g(x,y) = 0` as functional dependencies. Doubly-exponential
  worst case (Mayr-Meyer); reserve for small systems where
  HNF can't go.
- **Discrimination-tree term indexing** so the rule library scales
  beyond linear-scan-per-formula. Same idea every saturation-based
  theorem prover (Vampire, E, SPASS) already uses.
- **Knuth-Bendix completion** *offline* (not per-query) to compile
  a confluent rewrite system from the rule library so e-graph
  saturation converges predictably.

The combination gives: HNF for linear, Gröbner for polynomial,
e-graphs for everything else, term indexing for scale.

### Property decision via ABSTRACT INTERPRETATION

Our 2-copy uniqueness check, formalized: test `|γ(α(φ))| ≥ 2`
in the powerset lattice `2^D` of solutions. The *minimal lattice*
that decides this property is the **cardinality domain**
`{⊥, =0, =1, ≥2, ⊤}`. Lift the formula into that lattice,
evaluate `α(φ)`, read off the answer.

This is Cousot & Cousot's program (POPL '77) applied to
compilation: choose the cheapest abstract domain that decides
the property, evaluate, project. A function-izer that classifies
claims by "smallest domain that decides functionality on this
claim" is doing exactly this.

Mercury's mode/determinism analysis is the directly portable
prior art: a lattice of `{failure ⊏ det, semidet ⊏ multi, nondet}`
classifications, with abstract evaluation propagating modes
through the program. Mercury's compiler-facing example is
~25 years old and ships in production.

`solve-eqs` is a **Galois insertion**: `α` = normalize-to-
substitution-form, `γ` = re-expand; `α∘γ = id` because the
normalized form has unique representatives. This makes "did the
rewrite preserve meaning?" a discharge obligation rather than
a hope.

Skip: full domain theory (overkill for first-order constraints),
Stone duality, locales. The "opens = decidable observations"
intuition is worth one paragraph; the machinery doesn't pay off
for a constraint compiler.

### SYMMETRY breaking (group theory) — the immediately shippable tier

Z3 has *no built-in symmetry detection*. Many of our claims have
symmetries (permutation-symmetric Seqs, interchangeable enum
values, matrix row/column symmetries) that Z3 redundantly
searches over.

The fix is small and well-understood:

1. Build a colored graph from the constraint formula (variables
   and constraints as nodes, colors encode types and operators).
2. Run **Bliss** or **Nauty** (BSD-licensed C, milliseconds for
   our graph sizes) to compute the automorphism group of that
   graph — the formula's symmetry group.
3. Emit **lex-leader constraints** per orbit so Z3 only explores
   the canonical representative.
4. Hand the augmented formula to Z3.

This is **production-shipping today** in BreakID and SAT competition
winners. Empirical wins: 10-100× on symmetric SAT instances.
Weeks of engineering, not years. Different in kind from the
function-izer: it keeps Z3, just kills the symmetric redundancy
before Z3 sees the formula. The two compose.

Toposort's solution space is a coset family in S_n (the
symmetric group); **Schreier-Sims** gives polynomial-time
membership, order computation, and uniform random sampling.
If a Toposort claim recognized its space group-theoretically, it
could sample solutions uniformly instead of returning whatever
Z3 picks first.

Skip for now: Cayley-graph synthesis, groupoid-based optimization,
FFT-style transforms — research-grade, 5-year horizon.

### Where the four meet

- **Allegories** name *what* we're doing — finding maps in a
  relation.
- **AI** is *how to decide* the property — pick the right
  abstract domain.
- **Computational algebra** is *how to canonicalize* — get to a
  unique form where the pattern is recognizable.
- **Group theory** is *how to reduce redundancy* — kill
  symmetric duplicates before any of the above runs.

None solves the problem alone. Together they give:

1. A formal definition of what we're compiling (map sub-relation
   of the claim's allegoric morphism).
2. A canonical form to match against (Gröbner basis / HNF /
   e-graph saturation of the Z3-normalized formula).
3. A symmetry-breaking pass to reduce the work upstream
   (Bliss + lex-leader).
4. A decision procedure for "is this functional?" (cardinality
   domain over the solution-set lattice).

### Why decomposition belongs upstream of all four

The frameworks above analyze a *single* relation/formula/algebra.
Real Evident claims are composites — built up from smaller pieces
via `..` passthrough, claim invocation, and shared variables. The
composite's algebraic structure usually decomposes into
independent sub-relations that don't share variables (after pinning
`given`).

**Decomposition recovers that natural separation** as a purely
structural pass — hypergraph connected components, linear time,
no Z3 calls. After decomposition:

- Allegoric analysis runs per component.
- AI's lattice analysis runs per component.
- Pattern recognition runs per component.
- Symmetry breaking runs per component.

Each component is smaller, analyses are local, and a claim with
mixed function-shape / search-shape just becomes a list of
independently-classified components. The "is this whole claim
functional?" binary collapses into a per-component verdict.

This is the same principle as **tree decomposition** in CP
(Dechter, *Constraint Processing*, 2003): components of width 1
are exactly connected components; the four frameworks above
become per-component operations. For our v1, plain
connected-components is enough; higher-width decompositions are
future work for the rare case of tightly-coupled components.

### What's the highest-ROI intervention?

Two candidates with different shapes:

- **Decomposition** (this section) is structural, cheap, and
  makes everything else better. It's the *enabler*.
- **Symmetry breaking via Bliss** is the largest single-pass speedup
  on classically-symmetric problems — IF we have such programs.
  Our measurement (`docs/bench/`) found we currently don't:
  Mario's only candidate (3 platforms × `Jumpable`) is too small
  at n=3 for Z3 to pay the symmetry tax. Defer until evidence
  appears.

So: **decomposition is the unambiguous first thing to build.**
It's the structural prerequisite for the whole rest of the
function-izer, and it costs almost nothing to ship.

---



The constraint-programming and partial-evaluation communities have
been working on adjacent problems for decades. Reuse their terms.

| Concept | Term of art | Origin |
|---|---|---|
| The unit "claim + given-set" | **mode / procedure** | Mercury |
| Functional vs search | **determinism category** (`det`, `semidet`, `multi`, `nondet`) | Mercury |
| "x's value determines y's" | **functional dependency** | Mendelzon '85 |
| Per-constraint tag for "defines this var" | **`defines_var`** annotation | MiniZinc / FlatZinc |
| Pin-vs-derive partition | **binding-time annotation** | Partial evaluation |
| Extracting the function | **functional synthesis** / Skolem extraction | SMT synthesis |
| 2-copy uniqueness check | **self-composition** | Non-interference (Darringer & King '04), used everywhere since |

The natural name for the analysis is **mode analysis**. A claim has
multiple modes (input partitions); each mode gets a determinism
classification and, if the classification is good enough, a compiled
implementation.

## Why "one equation per output" was the wrong rule

An earlier sketch proposed a strict rule: each output variable must
have a defining equation `output = expr(inputs, already-defined-vars)`,
and the dependency graph of those equations must be a DAG.

That rule is too narrow. The real condition is "the relation,
restricted to the given-set, admits a unique witness" — which can
arise from:

- An explicit `y = expr(x)` equality (the easy case)
- A cluster of equalities that reduce to substitutions
  (`a + b = 7 ∧ a = 3` ⇒ `b = 4`)
- Algebraic isolation across multiple constraints
- Set-membership tightening narrowing a variable to a single value
- Quantifier elimination for theories that support it

Z3 answers "is the unique-witness property satisfied?" directly,
via the 2-copy assertion:

```
∃ x, y₁, y₂.  φ(x, y₁)  ∧  φ(x, y₂)  ∧  y₁ ≠ y₂        UNSAT  ⇒  functional
```

UNSAT means "no two distinct witnesses for the same input exist"
— i.e., the relation IS a function on the chosen partition.
SAT/UNKNOWN means it isn't (or Z3 couldn't decide), and the solver
path stays.

This generalizes immediately to records, Seqs, Sets, enums: `y₁ ≠ y₂`
becomes the disjunction "some component of y differs". Z3 already
supports this through standard datatype/sequence equality.

## The relaxation: "trivially samplable" outputs

Evident tolerates a controlled amount of randomness. A claim where
multiple valid outputs exist is still compilable to a fast function
*as long as the output set can be sampled cheaply, with optional
seeding for reproducibility*. We already use this pattern: the
dispatcher's toposort picks one valid ordering via Kahn's algorithm
with `EVIDENT_DISPATCH_SEED`-controlled tie-break.

Generalize: instead of asking "is the output unique?" ask "can a
valid output be produced in O(few-ms) given the inputs?"

Three samplable shapes the function-izer can recognize without
algorithm-substitution machinery:

**A. Free variable with no constraints.** After extraction, an
output is unconstrained → emit `rng.gen()` over its sort. Trivially
detectable: variable doesn't appear in any residual constraint or
substitution.

**B. Finite-domain narrowing.** An output is constrained to a known
finite set (`x ∈ {1, 5, 9}`) → emit `set.choose(rng)`. Detectable by
Z3's domain-tracking; if `solve-eqs` reduces an output's constraint
to a set-membership over a literal set, sample from it.

**C. Disjoint substitution branches.** The formula decomposes as a
disjunction `(C₁ ∧ chain₁) ∨ (C₂ ∧ chain₂) ∨ …` where each branch
is itself a substitution chain → emit branch selection by `rng`,
then evaluate the selected chain. Detection: after `solve-eqs`,
inspect the residual; if the top connective is `or` with mutually
exclusive guards, it's branch-samplable.

Anything outside A–C goes to the **algorithm registry** (next
section) or the solver.

### Determinism via seed

The compiled function takes an `rng: &mut StdRng` argument. With a
fixed seed it produces the same output for the same input; with
distinct seeds it samples freely from the valid set. The `seed`
becomes part of the cache key only when reproducibility matters;
in production runs it's typically thread-local.

This is what `EVIDENT_DISPATCH_SEED` already does for the
dispatcher. The function-izer formalises it as part of the API
rather than a global env-var.

## The pipeline

```
┌── 0. DECOMPOSE ───────────────────────────────────────────────────┐
│  Hypergraph connected components (after pinning `given`).          │
│  Union-find over constraint→variable hyperedges.                  │
│  Output: a list of independent sub-models.                         │
│                                                                   │
│  Each sub-model below runs through the rest of the pipeline       │
│  independently. Components don't share work or assumptions.        │
│                                                                   │
│  Linear time. No Z3 calls. Pure structural pass.                  │
└───────────────────────────────────────────────────────────────────┘
                           ↓ per sub-model
┌── 1. CLASSIFY ────────────────────────────────────────────────────┐
│  Given a sub-model, classify under Mercury-style                   │
│  determinism categories:                                          │
│    det      — UNSAT of 2-copy check; unique output                │
│    samplable — multiple outputs but sample-recognized pattern     │
│    nondet   — multiple outputs, no recognized sampler             │
│    failure  — body is UNSAT for this partition                    │
│                                                                   │
│  Implementation:                                                  │
│    Assert  ¬( ∀ X. ∃=1 Y. body(X, Y) )                            │
│    Z3.check() — UNSAT means det; SAT means try samplable;         │
│                  UNKNOWN means treat as nondet conservatively.    │
│                                                                   │
│  Per sub-model, not per whole claim — much smaller formulas,      │
│  much faster decisions, narrower failure modes.                    │
│                                                                   │
│  Sound but incomplete. IDP's exact technique (De Cat &            │
│  Bruynooghe 2013, TPLP), retargeted from SPASS-on-weakened-FO     │
│  to Z3 on full theories. Stronger than IDP's because Z3 sees      │
│  full datatypes/sequences/arrays.                                 │
└───────────────────────────────────────────────────────────────────┘
                           ↓ det or samplable
┌── 2. EXTRACT ─────────────────────────────────────────────────────┐
│  Tactic: (then simplify propagate-values solve-eqs)               │
│  Output: residual Goal + model converter holding x↦t pairs.       │
│                                                                   │
│  solve-eqs internally builds a dependency graph of                │
│  eliminate-able vars, topo-sorts, applies substitutions.          │
│  See z3 src/tactic/core/solve_eqs_tactic.cpp — collect+solve.     │
│                                                                   │
│  z3-sys binding: Z3_apply_result_get_model_converter.             │
│  If that's missing from our z3-sys 0.8, reconstruct:              │
│    chain = (original asserts) \ (residual asserts)                │
│  filtered for shape `x = t` with x ∉ free(t).                     │
│                                                                   │
│  For samplable claims, the residual MUST match patterns A/B/C     │
│  above. If it doesn't (residual is a non-trivial constraint),     │
│  demote to nondet → solver.                                        │
└───────────────────────────────────────────────────────────────────┘
                           ↓ chain + sampler spec
┌── 3. SHAPE ───────────────────────────────────────────────────────┐
│  Hash-cons the chain into a DAG. Topo-sort. Count use-sites.      │
│  Any node with use-count ≥ 2 → introduce a `let` at the           │
│  innermost binder dominating all uses (MetaOCaml discipline,      │
│  per Kiselyov's "Sharing and CSE" note).                          │
└───────────────────────────────────────────────────────────────────┘
                           ↓
┌── 4. EMIT ────────────────────────────────────────────────────────┐
│  Build a `proc_macro2::TokenStream` via the `quote!` macro.       │
│  Validate the whole function with `syn::parse2::<syn::File>()`    │
│  — this is the free type-check / parse-error gate.                │
│  Format with `prettyplease`.                                      │
│                                                                   │
│  ITE lowering:                                                    │
│    Bool scrutinee       → `if … else …`                            │
│    Enum-tag scrutinee   → `match …`                                │
│    Int-eq scrutinee     → lookup table (Vec / phf)                │
│                                                                   │
│  Sort → Rust type:                                                 │
│    Z3 Int           → i64 (with explicit overflow watch)          │
│    Z3 Real          → f64                                          │
│    Z3 Bool          → bool                                         │
│    Z3 Seq(Int)      → Vec<i64>                                     │
│    Z3 Set(Int)      → BTreeSet<i64>                                │
│    Z3 String        → String                                       │
│    Z3 Datatype(X)   → enum (generated alongside)                   │
│                                                                   │
│  Partial functions (Int/Int division, Seq index): emit explicit   │
│  guard or pre-prove the predicate at compile time and skip the    │
│  runtime check.                                                    │
└───────────────────────────────────────────────────────────────────┘
                           ↓
┌── 5. CACHE & DISPATCH (per claim, multiple sub-models) ───────────┐
│  Cache key: (claim_hash, sorted given-keys, optional seed-mode)   │
│  Cache value: ordered plan of per-component dispatch:              │
│    [ Component(i): {Native(fn_ptr) | Z3(formula)} ]                │
│                                                                   │
│  Hook in `rt.query` before the Z3 path:                            │
│    1. Look up plan in cache → if hit, execute steps:               │
│       a. For each Native(fn) step: call fn(given) → bindings;     │
│          merge into accumulator.                                    │
│       b. For each Z3(formula) step: solve with accumulator pinned │
│          as given.                                                  │
│    2. If miss, run pipeline (0-4):                                  │
│       - Decompose into components.                                  │
│       - Per component: classify, extract, shape, emit.             │
│       - Plan = list of dispatch steps in dependency order.         │
│       - Install plan in cache.                                      │
│                                                                   │
│  Compilation per native step: emit Rust source → invoke rustc →   │
│  load via libloading. Cost is per-shape, amortized over calls.    │
│                                                                   │
│  Mixed claims (some components Native, some Z3) work seamlessly — │
│  the plan just interleaves them. Components are independent by    │
│  construction.                                                     │
└───────────────────────────────────────────────────────────────────┘
```

## Architectural ordering of the dispatch paths

The dispatch order inside `rt.query`:

```
1. Function-izer cache — look up plan for (claim, given-keys);
                         execute plan steps (native + Z3 interleaved
                         per component) → bindings.
2. Function-izer cold  — run pipeline (decompose → classify-per-comp
                         → extract → emit). Install plan in cache.
3. Synthesis tier      — for arithmetic-shaped components that don't
                         compile directly; samples from Z3 oracle,
                         emits via PySR, verifies, caches.
4. Z3 solver           — fallback for components / claims that no
                         tier above could handle.
```

A "plan" is a per-component dispatch list: each component
independently dispatches as Native (compiled), Synthesis-emitted,
or Z3. A claim with mixed components (some functional, some
search) gets a plan that interleaves native and Z3 calls,
sharing the accumulated bindings between them.

Each step is sound: a hit means the result is identical to what Z3
would produce (modulo non-determinism in samplable claims, which is
also present in Z3's results). Each step's miss is cheap relative
to the work it avoids.

The deleted-Phase-2 algorithm-registry tier used to sit at the
front of this stack. See "Why we rejected the algorithm registry"
above.

## What this can't do

Confirmed boundaries from the research:

- **NP-hard cores** — scheduling with disjunctive-resource overlap,
  bin-packing, graph coloring, SAT proper. No functional direction;
  the function-izer's gate returns SAT, falls through.
- **Quantifier-alternation with arrays / EUF** — no closed-form
  Skolem function. `qe` returns "unknown."
- **Nonlinear integer arithmetic** — QE undecidable; `solve-eqs`
  skips equalities with non-invertible coefficients.
- **Bitvector with non-invertible multipliers** — `solve-eqs`
  silently leaves the equality in the residual.
- **Toposort, sort, and friends** — the constraint encoding fails
  the 2-copy gate (multiple valid outputs). Three honest paths:
  (a) implement the algorithm in Evident using piecewise-recognized
  idioms so the function-izer compiles it; (b) keep it on Z3 (slow
  but correct); (c) for arithmetic-shaped subproblems, route
  through the SR synthesis tier.

For each, the system degrades gracefully: the gate fails, the
extraction leaves residue, or both, and Z3 takes over. No
correctness risk — only the speedup is lost.

## Prior art and reading order

If you're implementing this, read in this order:

1. **De Cat & Bruynooghe 2013** — *"Detection and exploitation of
   functional dependencies for model generation"*, TPLP 13(4–5).
   The directly-lift-able paper. Their approach is the same as our
   step 1, just with SPASS-on-weakened-FO instead of Z3. They prove
   the rewrite is model-preserving and report order-of-magnitude
   wins on scheduling-style problems.
   - https://www.cambridge.org/core/journals/theory-and-practice-of-logic-programming/article/abs/detection-and-exploitation-of-functional-dependencies-for-model-generation/448D71378EBA254A8F94158CCF97778B
   - Their **output-definition trick** — keep the original
     relational form alongside the function so callers see what
     they wrote — should be ported directly.

2. **Z3 source: `src/tactic/core/solve_eqs_tactic.cpp`** — ~600
   lines, ground truth for what `solve-eqs` does. Read the
   `solve_eqs::collect` and `solve_eqs::solve` passes.
   - https://github.com/Z3Prover/z3/blob/master/src/tactic/core/solve_eqs_tactic.cpp

3. **de Moura & Passmore — "The Strategy Challenge in SMT
   Solving"** (2013) — design rationale for the tactic framework,
   including model-converter semantics.
   - https://leodemoura.github.io/files/strategy.pdf

4. **Mercury determinism documentation** — the closest precedent
   for the mode/determinism framing we're adopting.
   - https://mercurylang.org/information/doc-release/mercury_ref/Determinism-categories.html
   - Henderson, Somogyi, Conway, *"Determinism analysis in the
     Mercury compiler"* (1996).

5. **Rompf & Odersky — "Lightweight Modular Staging"** (GPCE 2010)
   — the engineering model for stage-1 (compile-time) vs stage-2
   (runtime) code generation, hash-consing, CSE.
   - https://infoscience.epfl.ch/record/150347/files/gpce63-rompf.pdf

6. **Kiselyov — "Sharing and CSE"** — canonical writeup on
   detecting sharing in tagless-final / EDSL compilation.
   - https://okmij.org/ftp/tagless-final/sharing/

7. **Poeplau & Francillon — "SymCC: Don't interpret, compile!"**
   (USENIX Sec '20) — engineering of "stop interpreting symbolic
   machinery, emit native code". Same instinct, different domain.
   - https://www.usenix.org/system/files/sec20-poeplau.pdf

### From the mathematical-foundations research

8. **Bird & de Moor — *Algebra of Programming*** (1996). Applies
   allegories to deriving programs from relational specifications.
   This is *literally* our problem in their vocabulary.
   - https://www.cambridge.org/core/books/algebra-of-programming/

9. **Freyd & Scedrov — *Categories, Allegories*** (North-Holland,
   1990). Definitive text on allegories. Maps in an allegory =
   functional sub-relations; the determinism axiom we already
   use is exactly the allegoric one.

10. **Cousot & Cousot — "Abstract Interpretation: A Unified Lattice
    Model…"** (POPL '77). Foundational. Our 2-copy check IS testing
    `|γ(α(φ))| ≥ 2` in the powerset lattice; the right minimal
    domain is the cardinality lattice `{⊥, =0, =1, ≥2, ⊤}`.
    - https://www.di.ens.fr/~cousot/COUSOTpapers/POPL77.shtml

11. **Davey & Priestley — *Introduction to Lattices and Order***
    (CUP, 2nd ed. 2002). Lattices, Galois connections, fixed-point
    theorems. The shared vocabulary across AI, mode analysis,
    program transformation.

12. **Willsey et al. — "egg: Fast and Extensible Equality
    Saturation"** (POPL '21). E-graphs as the substrate for
    invariant-under-rewriting pattern matching. Rust-native,
    production-proven (Cranelift).
    - https://egraphs-good.github.io/

13. **Cox, Little, O'Shea — *Ideals, Varieties, and Algorithms***
    (4th ed). Gateway to Gröbner bases. Reduced basis under
    elimination ordering reveals functional dependencies as
    `z - g(x,y) = 0` structurally.

14. **McKay & Piperno — Nauty/Traces** /
    **Junttila & Kaski — Bliss**. Graph automorphism in
    near-linear time on real graphs. Used by BreakID; ships in
    SAT competition winners.
    - https://pallini.di.uniroma1.it/
    - https://users.aalto.fi/~tjunttil/bliss/

15. **Devriendt et al. — BreakID**. Reference encoding of
    lex-leader symmetry-breaking constraints. The "how to use
    Bliss's output in practice" answer.
    - https://bitbucket.org/krr/breakid/

16. **Seress — *Permutation Group Algorithms*** (2003). Definitive
    on Schreier-Sims and computational permutation groups. Toposort
    solution space = coset family in S_n; this is its native
    vocabulary.

17. **Foster et al. — "Combinators for Bidirectional Tree
    Transformations"** (POPL '05). Lenses with GetPut / PutGet /
    PutPut laws. The categorical justification for the wrapper-claim
    pattern (`Toposort<Rect>` is literally a lens onto
    `Toposort<Int>`).

18. **Dechter — *Constraint Processing*** (Morgan Kaufmann, 2003).
    Tree decomposition, induced width, bucket elimination,
    AC-3/AC-4. The classical reference for "decompose a constraint
    network into independent or weakly-coupled pieces." Connected
    components is the width-1 case; everything we need for v1.

19. **Beeri, Fagin, Maier, Yannakakis — "On the Desirability of
    Acyclic Database Schemes"** (JACM 1983) and **Gottlob et al. —
    "The Complexity of Acyclic Conjunctive Queries"**. The
    foundational decomposition / hypergraph-acyclicity work the
    function-izer's decomposition pass rests on.
    - http://www.cs.toronto.edu/tss/files/papers/382780.382783.pdf

## Engineering pitfalls (catalogued from prior implementations)

- **Forgetting to name multi-use nodes** — emitted expression is
  exponential in DAG depth. Fix: use-count walk before emit,
  `let`-bind any node with ≥2 uses.
- **Z3 `Int` is unbounded, host `i64` is not** — silent wraparound
  on emitted code where the model proved a property only modulo
  unboundedness. Fix: emit explicit overflow checks, or use
  `i128` / `BigInt` for arithmetic that crosses i64 boundaries.
- **ITE-blasting at the wrong granularity** — deeply nested
  `if/else` ladders the host compiler can't fold, or one massive
  `match` past the exhaustiveness check. Fix: pick the lowering by
  scrutinee type (Bool/enum-tag/Int-eq → if/match/lookup-table).
- **Division / mod / array-index as total functions in SMT** —
  emitted Rust panics where Z3 was happy with the under-specified
  result. Fix: emit explicit guards or pre-prove the predicate at
  compile time.
- **Shadowing across nested `let`s when the emitter reuses a name
  counter per scope** — Rust accepts it, but `cargo clippy` lights
  up. Fix: use globally-fresh names from a single counter.
- **Z3 `Seq`/`Set`/`Real` have no 1:1 Rust mapping** — pick a
  representation up front (`Vec<T>` / `BTreeSet<T>` /
  `rug::Rational`). Don't try to polymorphize.
- **Scope extrusion (MetaOCaml's whole point)** — emitting
  `y_i = f(y_j)` where `y_j` was bound inside a now-closed branch.
  Fix: topo-sort over the flat substitution chain; never reference
  a name out of scope.
- **`if`-expressions as statements** — Rust's `if` IS an
  expression; emitting `;` after a tail `if` silently changes the
  function's return type to `()`. Fix: lint the emitted source
  with `syn::parse2::<syn::File>()` before writing.

## Rust emit idiom

Build a hash-consed expression DAG → run topo-sort + use-count →
emit through `proc_macro2::TokenStream` via `quote!`. NOT string
concatenation. `quote!` gives correct interpolation (`#ident`,
`#(#exprs),*`) and the resulting `TokenStream` round-trips through
`syn::parse2::<syn::File>()` — your free type-check / parse-error
gate before any file write. For final output, `prettyplease` (no
subprocess) or shelling out to `rustfmt` both work; `prettyplease`
is the standard inside `bindgen`-style tools.

Skip the intermediate Cranelift IR — it doesn't speak Rust types,
you'd be re-emitting from it anyway, and the host `rustc` is a
better optimizer than anything we'd hand-write.

Pipeline: substitution chain → hash-consed DAG → topo-sort →
`let`-insertion at use-count ≥ 2 → `quote!` walk → `syn::parse2`
validate → `prettyplease` → file → `rustc` → `libloading` →
function pointer.

## Status and next steps

**Don't lead with the native-compile path.** It's the most ambitious
and most expensive tier, with the most design questions still open.
Several cheaper tiers will pay off sooner. The honest sequencing:

### Phase 0 — Measure first

Before building any of this, get hard numbers on where current runtime
spends its time. Without that, we're optimizing blind.

- Add per-FSM solve timing to `effect-run` (or extend whatever
  `EVIDENT_LOOP_TIMING` already gives us). Mario for 60 ticks; break
  down wall time into FSM body solve, dispatcher, SDL dispatch,
  scheduling overhead.
- For each FSM body, classify against the 2-copy gate offline.
  We'd then know the fraction of runtime that *could* benefit from
  function-ization at all.

Half a day of work; informs every subsequent decision.

### Phase 1 — Quick wins (caching + incremental Z3)

The top rows of the menu table. Small builds with confident wins,
applicable regardless of what Phase 0 finds:

- **Translation cache** — AST → Z3 formula, keyed by (claim, shape
  of given). Per-tick FSM solves re-translate the same body every
  time today; this stops that.
- **Incremental Z3 (push/pop)** — assert the body once at FSM
  install, push the given pins per tick, solve, pop. Standard SMT
  pattern; should give 2-20× on per-tick solve depending on body
  size.
- **Generic result cache** — already present for the dispatcher
  toposort; consider promoting it to the runtime as a general
  facility.

### Phase 1.5 — Symmetry breaking via Bliss

The highest-ROI single intervention across the research waves.
Z3 has no built-in symmetry detection; we close that gap.

Scope:

- Pass: walk a Z3 formula, emit a colored graph (variables and
  subterms as nodes, types/operators as color labels).
- Subprocess to **Bliss** (https://users.aalto.fi/~tjunttil/bliss/)
  or **Nauty** (https://pallini.di.uniroma1.it/) → parse the
  generators of the automorphism group.
- Translate generators into **lex-leader constraints** per orbit
  (BreakID's documented encoding is the reference).
- Add the lex-leader constraints to the Z3 formula before solve.

Empirical 10-100× on symmetric problems. Weeks of work. Keeps
Z3, amplifies it. Different in kind from the function-izer;
they compose.

Watch for: any claim with permutation-symmetric `Seq(T)` (most
enumeration / allocation / scheduling problems), interchangeable
enum values (color-coloring, allocation), matrix row/column
symmetry.

### Phase 2 — *(removed: algorithm registry)*

Was: pattern-match recognized claim shapes and dispatch to
hand-written Rust. **Rejected** as FFI-in-disguise — see "Why we
rejected the algorithm registry" above. The intent (fast dispatch
for recognized algorithmic shapes) survives, but only via the
algebraic pattern recognition in Phase 3.

### Phase 3 — Function-izer (decompose → classify-per-component → emit)

The core of the function-izer. Operates on **Z3-normalized
formulas**, not on the AST, and works **per-component** rather
than per-whole-claim. Scope, in order:

- **3.1 Decomposition pass.** Build the constraint hypergraph
  for a given (claim, given-set); compute connected components
  via union-find. ~150 lines of Rust, no Z3 calls. This is the
  cheapest, highest-leverage step — it makes every downstream
  analysis component-local and small.
- **3.2 2-copy gate per component.** For each component, assert
  the determinism check. UNSAT → component is functional;
  SAT/UNKNOWN → leave to Z3.
- **3.3 Substitution extraction per functional component.**
  Apply `(then simplify propagate-values solve-eqs)` to the
  component's constraints; recover the substitution chain
  `y_i = expr(inputs, y_<i)`.
- **3.4 Algebraic pattern matcher.** Walk the normalized
  component formula (or its e-graph), match against the patterns
  library (linear functional dependency, permutation, fold, map,
  …), pick a code template.
- **3.5 Direct evaluator (interpret) OR Rust emit (compile).**
  For first ship, interpret — same logic, no codegen complexity.
  Native compile is a follow-on once the interpreter is solid.
- **3.6 Plan assembly.** Per claim, produce an ordered list of
  per-component dispatch steps (Native / Synthesis / Z3). Cache
  by (claim, given-keys).
- **3.7 Two-stage execution in `rt.query`.** Execute the plan:
  call native for functional components in dependency order,
  accumulate bindings, augment given, dispatch residual to Z3.

The matcher (3.4) is the central engineering question. Start
with **linear functional dependency** as the smoke test — a
trivial claim like `Pair(a, b ∈ Int, sum, prod ∈ Int)` with
`sum = a + b ∧ prod = a * b` should decompose to two components
(`{a, sum}` and `{a, prod}` if we cheat, or one component if we
don't pin away the shared `a`), each function-shaped, each
emitting `let sum = a + b` / `let prod = a * b`. Expand to
**fold/map** once basic dispatch works. **Permutation** and
**toposort** come later; they need the e-graph or extended
rewriting to recognize the multiset-equality structure.

A couple months of work. By the end of this phase, claims with
any functional components — and that's most non-trivial claims —
run those components at native speed, with Z3 only invoked for
the genuinely search-shaped residual.

### Phase 4 — Generative synthesis for arithmetic-shaped claims

Build the CEGIS pipeline against the same gate-and-extract
infrastructure. Bounded to **classical synthesizers only — no
LLMs.** The scope is what classical techniques can actually deliver:

- **`R^n → R` arithmetic claims** (per-tick physics, easing
  curves) — PySR via subprocess; sample 1000 (input, output) pairs
  via Z3 oracle, fit a closed-form expression, verify with Z3,
  emit Rust.
- **Boolean / small-enum claims** — Espresso logic minimization
  (not SR); build the truth table from oracle calls, reduce, emit.
- **Discrete-recursive claims (sort, toposort, parsers)** —
  *not in scope* via this tier. The hypothesis space of SR doesn't
  include them; ILP needs the primitives spoonfed. These either
  go through the algebraic-pattern matcher (if their structure is
  recognized) or stay on Z3.

Synthesis needs:
- A way to sample inputs (constraint-guided is best — blocking
  clauses + hash-bucketing + size-stratified sweeps, ~200 lines
  on top of the `z3` crate).
- A fast oracle (Phase 3's algebraic evaluator makes this
  tractable; calling Z3 for every example is too slow).
- A way to verify (same gate machinery as Phase 3).

So Phase 3 is a prerequisite. The synthesizer of choice is **PySR**
— Julia-backed, the fastest published open-source SR engine,
subprocess-callable. **AI Feynman 2** as a stronger alternative
for physics-flavored equations.

### Phase 5 — Native compile (this doc's main subject)

Build the function-izer's Rust-emit step on top of Phase 3's
algebraic pattern matcher. The differential win over Phase 3 is
another 10-100× via rustc + LLVM optimization. Only worth it for
the *hottest* claims.

Scope:

- For each matched algebraic pattern, the corresponding code
  template now emits a `proc_macro2::TokenStream` instead of
  walking the formula directly.
- Pipeline: matched pattern → code template → `quote!` walk →
  `syn::parse2` validate → `prettyplease` → file → `rustc` →
  `libloading` → function pointer.
- Cache by `(claim_hash, given_partition)`; first query pays the
  rustc cost, subsequent calls hit native.
- Extend gate detection to the samplable shapes (A/B/C). Branch
  sampling is more involved; punt until needed.

A couple months of work, and only worth doing once we know there are
hot enough function-shaped claims to justify it.

### Why this order

- Phase 0 prevents wasted work.
- Phase 1 wins are universal — they help every query, not just
  function-shaped ones.
- Phase 1.5 (symmetry breaking) is the highest-ROI single
  intervention. Production-shipping technique, weeks of work,
  10-100× on symmetric problems. Keeps Z3, amplifies it. Compose
  with everything below.
- Phase 2 is the rejected-and-removed algorithm registry. The
  intent survives in Phase 3.
- Phase 3 is the core function-izer. Algebraic pattern recognition
  on Z3-normalized formulas; captures most of the function-shape
  win for claims that pass the determinism gate.
- Phase 4 widens the function-izer to arithmetic-shaped claims via
  classical SR — only worth building once Phase 3's evaluator is
  fast enough to serve as oracle.
- Phase 5 is the long tail — only justified after Phase 3 has
  shown what's hot.

A native-compile tier without the algebraic-pattern matcher
beneath it is a common engineering trap: months of work emitting
code for patterns you haven't yet shown you can recognize. Build
the recognizer first; native emission is a downstream optimization.

## Connection to the broader self-hosting story

The "compile constraint models to functions" feature is one of two
load-bearing pieces for the project's self-hosting ambition. The
other is the stdlib being expressed in Evident itself (already
underway: `stdlib/ast.ev`, `stdlib/passes/`). With both in place:

- The desugar and inference passes are written in Evident.
- They get compiled to native Rust functions on first load.
- Subsequent compilations skip Z3 entirely for the passes.
- The runtime becomes self-hosted *at speed*, not just at semantics.

The project's earlier GLSL transpiler — `stdlib/glsl/transpile.ev`
(removed in Phase 2 but lives in commit `aff7003` if needed) —
demonstrates that non-trivial transpilers can be written as
Evident. That transpiler ran via Z3 because we had no other path.
With the algebraic evaluator or the function-izer, the same source
becomes a fast native transpiler. Toposort goes through the
algorithm registry; effect ordering, parser inference passes, AST
desugars all eventually flow through the same gate-and-extract
machinery — interpreted at speed for cold paths, native-compiled
for hot ones.
