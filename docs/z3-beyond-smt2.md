# Z3 beyond smt2 — what the API exposes that the text format cannot

A map of everything Z3 (4.15.4) provides, organized around one question: **what is
available in Z3 that you cannot write in an smt2 file?** This is the coverage map
for "a pretty language over Z3" — it tells us which parts are syntax for a model
(prettify smt2) and which parts are operations on Z3's runtime objects (the
language has to add them).

The short version: smt2 expresses **the declarative model plus a command
script**. The API adds five tiers on top — **objects**, **live control**, **the
tactic/probe algebra**, **engine internals**, and **user-code extension**. Tiers
1–5 aren't syntax for a problem; they're a programming surface over Z3's objects,
which is why "support everything Z3" forces the pretty thing to be a real
language, not a prettier file format.

---

## What smt2 *does* cover (so we don't over-scope)

Z3's smt2 dialect goes well past the SMT-LIB standard. All of this is textual:

- **Sorts** — `Bool`, `Int`, `Real`, `BitVec`, `Array`, `Seq`, `String`, `RegEx`,
  `FloatingPoint`, uninterpreted sorts, and algebraic **datatypes**
  (`declare-datatypes`).
- **All three function kinds** — `declare-fun` (uninterpreted), `define-fun`
  (macro), `define-fun(s)-rec` (recursive).
- **Assertions** including named ones (`:named` for unsat cores) and quantifier
  patterns/triggers (`:pattern`).
- **Solving** — `check-sat`, `check-sat-assuming`, `push`/`pop`/`reset`.
- **Answers** — `get-model`, `get-value`, `get-unsat-core`, `get-proof`,
  `get-objectives`, `get-info :all-statistics`, `get-info :reason-unknown`.
- **Z3 extensions** that surprise people: **optimization**
  (`maximize`/`minimize`/`assert-soft`), the **Fixedpoint/Horn engine**
  (`declare-rel`/`declare-var`/`rule`/`query`), and **tactic application**
  (`check-sat-using`, `apply`, with `then`/`or-else`/`par-then`/`repeat`/
  `using-params` combinators and probe conditionals), plus the `simplify` command
  and pseudo-Boolean `((_ at-most k) …)`.

So the model, the optimization, the fixpoint rules, and a good chunk of tactic
scripting are already expressible as text.

---

## View A — by access method (the five API-only tiers)

What you can reach *only* through the API, ordered shallow → deep.

### Tier 1 — Everything-as-manipulable-objects
smt2 emits a model and answers fixed queries; the API hands you the *things*.
- **ASTs as values**: walk children, `decl()`/`kind()`, `substitute`, `simplify`
  with params, `translate` to another context. (Our prettifier lives here — it's
  the AST-walking half.)
- **Goals**: apply a tactic and *consume* the resulting subgoals as objects
  (`Goal.get`, `size`, `depth`, `precision`, `inconsistent`, `convert_model` to
  lift a subgoal's model back to the original).
- **Models as objects**: `eval` any expression with `model_completion`, iterate
  `decls()`, and read a function's interpretation as a `FuncInterp` (its entries
  plus the else-value) — where smt2's `get-value` is only fixed queries.
- **Proofs / unsat cores / consequences** as structures you traverse.

### Tier 2 — Live runtime control and introspection
smt2 is a batch script; the API is a live handle on a running solver.
- **interrupt / cancel** a solve in flight; set **per-call resource limits**
  (`rlimit`) and timeouts.
- read `statistics` and `reason_unknown` mid-solve.
- `consequences` (implied literals under assumptions), **cube-and-conquer**
  splitting (`cube`/`cube_vars`), inspect the solver's internal assignment
  (`trail`, `units`, `non_units`, `num_scopes`), export `dimacs`.
- `translate` a problem into a fresh context for **parallel/portfolio** solving.

### Tier 3 — The tactic & probe algebra as first-class values
This build has **116 tactics and 42 probes** (tabled below). smt2 lets you
*apply* a pipeline; the API lets you *build* one as composable values —
`Then`/`OrElse`/`Repeat`/`With`/`ParThen`/`TryFor`, **custom probes** that measure
a goal, and `Tactic.solver()` to turn any pipeline into a solver.

### Tier 4 — Engine-object internals
- **Fixedpoint** (Spacer/PDR) exposes its guts: `get_answer`/`get_ground_sat_answer`
  as objects, the inductive `add_cover`/`get_cover_delta`, `get_num_levels`,
  `get_rules_along_trace`, `set_predicate_representation`.
- **Optimize** lets you read objective bounds *as the solve progresses*
  (`lower`/`upper`/`lower_values`/`upper_values`), enumerate a Pareto front, and
  register `set_on_model` (a callback on every improving solution).

### Tier 5 — Extension via your own code (the deep one)
- **`UserPropagateBase`** — write a *custom theory* in the host language that
  participates in Z3's CDCL(T): callbacks on equality (`add_eq`), value-fixing
  (`add_fixed`), final check (`add_final`), and you can raise a `conflict` or
  assert a `propagate`. **Impossible to express in smt2 in any form** — it runs
  your code inside the solver loop.

---

## View B — by purpose

The same capabilities, grouped by what you're trying to *do*.

| purpose | smt2 | API-only |
|---|---|---|
| **Model** a problem (sorts, functions, assertions, datatypes, quantifiers) | ✅ full | build/inspect/rewrite the AST as objects |
| **Transform** a problem (simplify, lower, normalize) | `simplify`, `apply` | the tactic algebra as values; consume subgoals; custom probes |
| **Route** a problem (pick strategy by shape) | probe conditionals in `apply` | probes as values; custom probes; `Tactic.solver()` |
| **Solve** | `check-sat`, `check-sat-using` | interrupt, per-call limits, cube-and-conquer, consequences, portfolio via `translate` |
| **Read the answer** | `get-model`/`get-value`/`get-proof`/`get-unsat-core` | model/FuncInterp/proof/core as objects; `eval` with completion |
| **Optimize** | `maximize`/`minimize`/`assert-soft` | live bound queries, Pareto enumeration, `set_on_model` |
| **Fixpoints / verify** | `rule`/`query` | the answer/invariant as an object, covers, trace levels |
| **Introspect** | `get-info :all-statistics` | live statistics, `reason_unknown`, trail/units, memory probe |
| **Extend the solver** | — | `UserPropagateBase` (custom theory in host code) |

---

## View C — by object handle

Every API-only capability hangs off a handle. The language's value types are
essentially this list.

| handle | what it is | the powers it adds over smt2 |
|---|---|---|
| `AstRef` (terms/sorts) | an expression as a DAG | walk, `decl`/`kind`, `substitute`, `simplify`, `translate` |
| `Goal` | a mutable set of assertions | `apply` a tactic → subgoals you consume; `size`/`depth`/`precision`; `convert_model` |
| `Tactic` | a goal→subgoals transform | compose (`Then`/`OrElse`/`Repeat`/`With`/`ParThen`); `solver()` |
| `Probe` | a goal→number/bool measure | route tactics by shape; build custom ones |
| `Solver` | an incremental solving session | `push`/`pop`, `consequences`, `cube`, `trail`/`units`, `statistics`, `interrupt`, `translate`, `proof`, `unsat_core` |
| `Model` / `FuncInterp` | a satisfying assignment | `eval` w/ completion, iterate `decls`, function tables as objects |
| `Optimize` | objective solving | `maximize`/`minimize`/`add_soft`, `lower`/`upper`, Pareto, `set_on_model` |
| `Fixedpoint` | the Datalog/Spacer engine | `rule`/`query`, `get_answer`, `add_cover`, levels, rules-along-trace |
| `Statistics` | solver metrics | read counters programmatically mid/post solve |
| `UserPropagateBase` | a host-code theory | `add_eq`/`add_fixed`/`add_final`/`conflict`/`propagate` inside CDCL(T) |

---

## The 116 tactics

Tactics are goal→subgoals transforms. You compose them into a pipeline that
**lowers** a problem toward a form a final solver handles fast (the
`blast_select_store` win in the benchmark suite is one `simplify` tactic firing).
Probes (next section) pick *which* pipeline by inspecting the goal's shape.

### Builtin per-logic strategies — "just solve a goal in this fragment"
The end-stage solvers, each a tuned pipeline for one SMT-LIB logic. Use as the
last step, or standalone when you know the fragment.

| tactic | for |
|---|---|
| `default` | default strategy when no logic is set |
| `qfbv` / `bv` / `ufbv` | bit-vectors (quantifier-free / with quantifiers / + UF) |
| `qfufbv` / `qfufbv_ackr` / `qfaufbv` | BV + uninterpreted funcs (+ Ackermannization / + arrays) |
| `qfuf` | quantifier-free uninterpreted functions (EUF) |
| `qfidl` / `qflia` / `lia` / `ufnia` | integer linear arithmetic (difference / QF / quantified / + UF) |
| `qflra` / `lra` / `uflra` | real linear arithmetic (QF / quantified / + UF) |
| `qfnia` / `qfnra` / `nra` / `qfnra-nlsat` | nonlinear arithmetic (int / real / quantified real / via nlsat) |
| `lira` / `auflia` / `auflira` / `aufnira` | mixed int+real, and array+UF+arith combinations |
| `qffp` / `qffpbv` / `qffplra` | floating point (alone / + BV / + LRA) |
| `qffd` / `pqffd` / `smtfd` | finite-domain (QF / parallel / SMT-reduced-to-FD) |

### General solving engines
The cores the per-logic strategies call into, plus search variants.

| tactic | what it does |
|---|---|
| `smt` | the SAT-based SMT solver (the general workhorse) |
| `sat` / `psat` | SAT solver (serial / parallel) |
| `sat-preprocess` | SAT preprocessing (resolution, BCP, 2-SAT, subsumption) |
| `nlsat` / `nlqsat` | nonlinear-arithmetic solver / its quantified (QSAT) form |
| `qsat` | a QSAT solver (quantified) |
| `psmt` | SMT in parallel |
| `qfbv-sls` / `sls-smt` | stochastic local search (BV / SMT) — fast model-finding |
| `horn` / `horn-simplify` | solve / simplify Horn clauses (the Fixedpoint front) |
| `subpaving` | test harness for the subpaving (interval) module |

### Simplification & propagation — the workhorses
Run early and often; they shrink the goal and expose structure for the solver.

| tactic | what it does |
|---|---|
| `simplify` | apply the rewrite-rule simplifier (the big one) |
| `ctx-simplify` / `ctx-solver-simplify` | contextual simplification (rules / solver-backed) |
| `dom-simplify` | dominator-based simplification |
| `propagate-values` / `propagate-values2` | propagate constants |
| `propagate-ineqs` | propagate bounds, drop subsumed inequalities |
| `solve-eqs` | solve for variables and eliminate them |
| `elim-uncnstr` / `elim-uncnstr2` | eliminate unconstrained variables |
| `elim-and` | rewrite `and` to `not/or` |
| `euf-completion` | simplify using known equalities |
| `demodulator` | extract equalities from quantifiers and rewrite with them |
| `reduce-args` / `reduce-args2` | drop function args that are always a value |
| `elim-predicates` | eliminate predicates, macros, implicit defs |
| `injectivity` | apply injectivity axioms |
| `special-relations` | detect and replace orders/closures with special relations |
| `unit-subsume-simplify` / `solver-subsumption` | drop subsumed units / assertions |
| `purify-arith` | remove `-,/,div,mod,rem,is-int,^,root` in favor of basics |
| `normalize-bounds` | shift a var by its lower bound (`x' = x - k`) |
| `recover-01` / `add-bounds` | recover 0-1 vars hidden as Bool / bound unbounded vars |
| `propagate-bv-bounds` / `propagate-bv-bounds2` / `bv_bound_chk` | BV bound propagation / inconsistency check |
| `reduce-bv-size` / `bv-slice` | shrink BV widths / simplify via slices |

### Normal forms, if-then-else & Boolean structure
Put the goal in a canonical shape a downstream engine expects.

| tactic | what it does |
|---|---|
| `elim-term-ite` | replace term-level `ite` with fresh defs |
| `blast-term-ite` / `cofactor-term-ite` | hoist / cofactor-eliminate term `ite` |
| `nnf` / `snf` | negation / skolem normal form |
| `occf` | one-constraint-per-clause normal form |
| `tseitin-cnf` / `tseitin-cnf-core` | CNF via Tseitin (with / without pre-simplification) |
| `aig` | simplify Boolean structure with And-Inverter Graphs |
| `max-bv-sharing` | maximize sharing of BV adders/multipliers |
| `split-clause` | split a clause into subgoals (case split) |
| `symmetry-reduce` | symmetry-breaking |

### Theory lowering / reduction — turn one theory into another
The "change the representation to a faster theory" tactics — the benchmark suite's
whole thesis as a toolbox.

| tactic | reduces |
|---|---|
| `bit-blast` / `bv1-blast` | BV → SAT / BV → width-1 BV |
| `bvarray2uf` | BV arrays → uninterpreted functions |
| `dt2bv` | finite datatypes → bit-vectors |
| `elim-small-bv` | expand small quantified BVs |
| `ackermannize_bv` | full Ackermannization of BV instances |
| `fpa2bv` | floating point → bit-vectors |
| `nla2bv` | nonlinear arithmetic → BV (under-approx, for model finding) |
| `lia2pb` / `lia2card` | bounded ints → 0-1 vars / cardinality constraints |
| `pb2bv` / `card2bv` | pseudo-Boolean / cardinality → bit-vectors |
| `eq2bv` | finite-domain integers → bit-vectors |
| `pb-preprocess` | Davis-Putnam-style PB preprocessing |
| `degree-shift` / `factor` | reduce polynomial degree / factor polynomials |
| `fm` | Fourier–Motzkin variable elimination |
| `fix-dl-var` | difference-logic: pin the most-used var at 0 |
| `diff-neq` | specialized bounded integer ≠ solver |

### Quantifiers & quantifier elimination
| tactic | what it does |
|---|---|
| `qe` | quantifier elimination |
| `qe2` / `qe_rec` | QSAT-based QE (flat / recursive) |
| `qe-light` | light-weight QE |
| `der` | destructive equality resolution |
| `distribute-forall` | push `forall` over conjunctions |
| `macro-finder` / `quasi-macros` | find & apply (quasi-)macros from quantifiers |
| `ufbv-rewriter` | UFBV demodulation rewriting |

### Control / plumbing atoms
The combinators you build pipelines from.

| tactic | what it does |
|---|---|
| `skip` | do nothing (identity) |
| `fail` / `fail-if-undecided` | always fail / fail unless the goal is decided |
| `collect-statistics` | gather stats as a pipeline step |

---

## The 42 probes

Probes measure a goal and return a number or bool. Their job is **routing**:
`If(probe, tacticA, tacticB)` picks a strategy from the goal's shape — exactly the
shape-directed lowering router we want, built from Z3's own measurements.

### Size & structure metrics
| probe | returns |
|---|---|
| `size` | number of assertions |
| `depth` | goal depth |
| `num-exprs` | number of sub-terms |
| `num-consts` | non-Boolean constants |
| `num-bool-consts` | Boolean constants |
| `num-arith-consts` | arithmetic constants |
| `num-bv-consts` | bit-vector constants |
| `memory` | memory used (MB) |

### Arithmetic shape
| probe | returns |
|---|---|
| `arith-max-deg` / `arith-avg-deg` | max / avg polynomial total degree |
| `arith-max-bw` / `arith-avg-bw` | max / avg coefficient bit width |

### Fragment classifiers (`is-…`) — "which logic is this goal in?"
The router's main inputs. All return true/false.

| probe | true when the goal is… |
|---|---|
| `is-propositional` | propositional logic |
| `is-pb` / `is-quasi-pb` | a pseudo-Boolean / quasi-PB problem |
| `is-ilp` | integer linear programming |
| `is-unbounded` | has int/real consts with no lower/upper bound |
| `is-qfbv` / `is-qfbv-eq` / `is-qfaufbv` | QF_BV / its `=`,extract,concat fragment / QF_AUFBV |
| `is-qflia` / `is-qflra` / `is-qflira` / `is-qfauflia` | QF linear int / real / int+real / + arrays+UF |
| `is-lia` / `is-lra` / `is-lira` | (quantified) linear int / real / int+real |
| `is-qfnia` / `is-qfnra` / `is-qfufnra` | QF nonlinear int / real / real+other-theories |
| `is-nia` / `is-nra` / `is-nira` | (quantified) nonlinear int / real / int+real |
| `is-qffp` / `is-qffpbv` / `is-qffplra` | QF floats / floats+BV / floats+LRA |

### Capability & config flags
| probe | true when… |
|---|---|
| `has-quantifiers` | the goal has quantifiers |
| `has-patterns` | the quantifiers carry patterns/triggers |
| `produce-proofs` / `produce-model` / `produce-unsat-cores` | that output is enabled |

### Specialized
| probe | returns |
|---|---|
| `ackr-bound-probe` | upper bound on Ackermann lemmas the formula may generate |

---

## Tying it together: a general-purpose language from the non-smt2 features

The thing to see is that smt2 is only the **model** layer — one of five — and the
API-only tiers are not peripheral conveniences, they are the **compiler**,
**runtime**, and **extension** layers a general-purpose language needs. A program
is a constraint model, and the non-smt2 features are how you compile it, run it,
and extend it.

**Layer 1 — Surface & model (smt2-expressible).** Programs are written as claims
and constraints. A claim is a `define-fun(s)-rec` (a named, reusable, recursive,
composable function/predicate). Pure computation is defined functions Z3
evaluates; **effects are uninterpreted functions** — a `declare-fun` is a
description of behavior the model doesn't compute, i.e. a read/write/LibCall the
runtime must fulfill (world-indexed so each call is distinct). This whole layer is
what the prettifier already renders.

**Layer 2 — The compiler (Tactics + Probes, Tier 3).** A program is a `Goal`.
Probes inspect its shape (`is-qfbv`, `num-consts`, `arith-max-deg`, …) and route
it; tactics lower it to the measured-best form (`bit-blast` here, `blast_select_store`
there, `qe-light` for quantifiers, `lia2card` for counting). This is the
shape-directed lowering router from the benchmark work — and it's **Z3's own
algebra**, so we expose it as the language's optimization pipeline rather than
building one. The benchmark suite is the data telling the router which tactic wins
per shape.

**Layer 3 — The runtime (Solver + Model + control, Tiers 1–2).** Execution is
driving Z3 over time: `check`, read the `Model` (which effects the program is
requesting, via `eval`-with-completion), dispatch those effects to the world, pin
the results, advance, repeat — all on **one reused solver via `push`/`pop`**, so
the footprint stays flat (the tick loop, done with Z3's incremental API instead of
our crude fresh-solver-per-tick). Three modes fall out, and the hard two are
exactly what Z3 is best at:
- **prove** — does the property hold for *every* allowed behavior? (`check`
  unsat / Spacer invariant) — correctness over the whole under-determined space.
- **refute / chaos** — find an allowed behavior that violates it (`check` sat → a
  counterexample ordering): the solver as a perfect error-injector that *proves*
  the bad interleaving exists, the SRE's "make it happen so we can fix it."
- **run** — pick one behavior, deterministically, only at the production finale.

`interrupt`, per-call `rlimit`, `statistics`, and `reason_unknown` are the
runtime's resource governance and observability; `translate` into fresh contexts
runs prove and refute in **parallel**.

**Layer 4 — Built-in engines (Optimize, Fixedpoint).** Some programs are an
optimization (`maximize`/`minimize`, `assert-soft` for preferences) — `Optimize`
with its live bound queries and Pareto fronts. Some are a fixed point or a safety
property — `Fixedpoint`/Spacer computes closures and **synthesizes the inductive
invariant** (our "case 1," handed to us). These are engines the language calls,
not things it reimplements.

**Layer 5 — Extension (`UserPropagateBase`, Tier 5).** The language's FFI *into
the solver*: a user-defined theory in host code that participates in CDCL(T).
This is where a domain-specific effect or propagation rule that isn't a clean
constraint plugs in — `add_fixed`/`add_eq` callbacks let the outside world (or a
custom decision procedure) steer the search. It's the one tier with no smt2 analog
and the deepest lever for making the language extensible.

The synthesis: **the model is smt2; the compiler is the tactic/probe algebra; the
runtime is the solver/model/control surface in prove/refute/run modes; the engines
are Optimize and Fixedpoint; and extension is the user propagator.** Four of the
five layers are *bindings over objects Z3 already implements* — the language's job
is pretty syntax and orchestration, not reimplementation. What's genuinely left
for us to build is small and specific: the surface syntax, the effect vocabulary
(uninterpreted functions as world-indexed effects + the dispatch table), the
mode-driven runtime loop, and a thin router that consults the benchmark data. The
rest of "everything Z3 provides" is already in the box; we are exposing it, not
rebuilding it.
