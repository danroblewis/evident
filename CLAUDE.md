# Z3-first toward a general-purpose set-theoretic language — read this first

## The end goal (never lose sight of this)

We are building a **general-purpose programming language** — one you could write
*any* program in, not a constraint DSL or a solver frontend. Three hard
requirements define it:

1. **General-purpose.** It must express any programming task — algorithms, data
   structures, IO, real programs — the way Python or Rust can. Clean constraint
   problems are the easy case; the language has to do the rest too.
2. **Set-theory-based.** Its computational paradigm is set theory — sets,
   membership, relations — not imperative statements or lambda calculus. This is
   the distinctive bet, and it constrains everything.
3. **Z3 + a minimal harness as the runtime.** The SMT solver does the work; a
   small harness around it handles effects, the solve loop, and FFI. No heavy
   compiler, no large runtime. (This is what the deleted Evident kernel was
   reaching for — ~880 lines around Z3.)

The language we still call *Evident* is that goal. Holding all three at once —
general-purpose **and** set-theoretic **and** running on Z3+minimal-harness — is
the whole challenge. Keep all three in mind for every decision.

## What this project is now

We are **not building the Evident compiler anymore.** The kernel,
`compiler.smt2`, the self-hosted `compiler2/*.ev`, stdlib, and the whole
`.ev → smt2 → kernel` stack have been deleted (branch `prototype-z3-python`).
What changed is the **order of operations**, not the goal:

```
  PHASE 1 (now):  prove Z3 + a minimal harness CAN be the runtime for a
                  general-purpose, set-theoretic language — in Python — and
                  uncover the PRINCIPLES that make it work.
  PHASE 2 (later): design Evident as SUGAR over the proven Z3 substrate.
```

So today we prototype directly in **Python over the Z3 library**, keep the
prototypes honest and measured, and write down what we learn. The recurring
phase-1 question for any experiment is: **does this move us toward a
general-purpose set-theoretic language on a Z3+minimal-harness runtime?** A fast
trick that only works for constraint puzzles, or that needs a heavy runtime, or
that abandons the set-theoretic paradigm, is not progress — flag it as such.
Evident is the goal we're earning the right to design — not the thing we're
building this week.

## Working style: bias hard toward autonomous background research

**Default to action, not permission, for anything read-only or throwaway.**
We are optimizing for more work in less wall-clock time. When a request — or
your own analysis of one — implies research, exploration, benchmarking, or
prototyping, **start it immediately as background work without asking.** Do not
narrate a plan and wait for a go-ahead; launch the jobs, then report findings.

What to start autonomously, no approval needed:

- **Research / exploration** — reading across the tree, grepping for prior art,
  reading Z3/library source, web lookups. Fan these out as parallel `Agent`
  subagents (and `Explore` for broad searches) in a single batch.
- **Prototyping** — write a throwaway script under `prototype/` (or a scratch
  dir / git worktree), run it, measure. A prototype that lives in its own file
  and touches nothing load-bearing is free to create and run.
- **Benchmarking / measurement** — sweeps, profiles, perf comparisons. Always
  `run_in_background: true` for anything over ~30s and keep working; poll later.
- **Parallel investigation of open questions** — if a task raises 2+
  independent sub-questions, spawn one background agent per question at once
  rather than serializing them. When the user gives a strategic/ambiguous
  prompt, kick off the obvious supporting research *while* you reason about it.

Be greedy about parallelism: prefer many concurrent background jobs/subagents
over one long serial chain. Batch independent tool calls into one message. When
in doubt about whether a piece of research is worth doing, **do it in the
background** — an idle background agent is cheaper than a missed insight.

Where the bias does NOT apply (still confirm first): anything hard to reverse or
outward-facing — pushing/PRs, publishing to external services, deleting or
overwriting work you didn't create. Aggressive on explore/prototype/measure;
conservative on irreversible/outward. Surface what the background work found,
including dead ends, so the user can steer.

## Tree layout

| Path                  | What it is                                                              |
| --------------------- | ---------------------------------------------------------------------- |
| `prototype/`          | **The work.** Python-over-Z3 experiments and the benchmark suite.      |
| `prototype/benchsuite/` | The re-runnable suite as a package (tasks · tactics · harness · runner · profiling · report). |
| `prototype/run.py`    | CLI: `run` / `report` / `profile` / `list`.                            |
| `prototype/results/`  | Generated artifacts (CSV / markdown / smt2 dumps).                      |
| `prototype/FINDINGS.md` | Consolidated cross-theory conclusions (the principles, with numbers).  |
| `prototype/z3-capabilities.md` | Reference: every Z3 theory, sort, predicate.                    |
| `prototype/set-lowering-via-z3.md` | The `blast_select_store` finding + Z3 source refs.          |
| `docs/`               | Mostly **legacy Evident** design notes. Forward-vision docs worth keeping: `docs/plans/claims-as-sets.md`, `docs/plans/relations-as-tuple-sets.md`. Treat the rest as historical until reviewed. |
| `STATE.md` / `ARCHITECTURE.md` | Legacy Evident snapshots — stale on this branch.              |

## Running the suite

```bash
cd prototype
python3 run.py list                 # tasks / 14 theories / tactic counts
python3 run.py run --max-len 2       # the combinatorial sweep → results/run.{csv,md}
python3 run.py report results/run.csv --markdown out.md
python3 run.py report results/run.csv --translations results/set-theory.md \
        --theory set --single-file   # focused per-theory translation+diff report
python3 run.py profile dispatch set 200 --tactics blast   # model AST diff under a tactic
```

`z3` is the system `python3-z3` package (4.15.4). There is no pip in this
environment; don't try to install the wheel.

---

# The principles we're uncovering

These are the durable, measured facts — the payload of phase 1. Each is backed
by a fixture in `prototype/`; cite the measurement when you lean on one.

1. **A set-theoretic surface is viable for performance *only paired with a
   lowering*.** Naïve set-of-tuples membership is ~1000× slower than an ite
   chain. The whole game is lowering it.

2. **Keep the surface, change the lowering.** A slow encoding is a missing
   lowering rule, never a reason to abandon the clean surface. `blast_select_store`
   rewrites set-membership (`select` over a `store`-chain, which is how Z3
   represents `Set` = `Array(T, Bool)`) into a flat disjunction of equalities —
   ~340× on bounded dispatch — and our sweep *discovers* it automatically.

3. **No theory is universally fastest — pick per problem shape.** Coloring wants
   Booleanization (one-hot/enum ~2ms; `Int` + `≠` is 13ms); arithmetic wants the
   opposite (`Real`/LRA 0.6ms; `BitVec` 36ms); reachability wants
   bounded-unroll-to-Bool (5ms) over `TransitiveClosure` (10ms) or a set frontier
   (59ms). The benchmark table *is* the lowering rule book.

4. **`≠` is a trap on anything the solver searches.** Disequality is non-convex
   (`x ≠ 0` ⟺ `x < 0 ∨ x > 0`), so it case-splits. Prefer convex comparisons
   (`> ≥ ≤ < =`) and finite-domain Booleanization. When you mean "nonzero" and
   the sign is known, write `> 0`.

5. **Bounded ⇒ decidable and fast; unbounded ⇒ semi-decidable.** A `Seq`/quantifier
   with a literal size bound lowers to cheap Array+len / bounded-quantifier form;
   unbounded ones are semi-decidable (expect `unknown`/timeouts). The Array+len
   lowering for a bounded `Seq` is *solver-internal* — no goal-rewrite tactic
   exposes it (measured: the swept tactics only reorder a `seq.len` goal).

6. **Tactics can help OR hurt — always measure, never assume.** A tactic that
   shrinks a model can speed it up (distinct→equalities) *or* a tactic can push a
   solvable case into timeout (seen in fp / reachability). Time tactic-apply
   separately from solve. Zero soundness violations across 2394 cases is the bar.

7. **Outputs must be covered by an assignment, not defined by implication.** A
   value defined only by guards (`A ⇒ out = x`) leaves the solver to search;
   a covering `=` (ternary select chain, or a lowered keyed-projection) is
   extractable and fast. `⇒`/`∨` belong *inside* a defining expression, never as
   the thing that defines the output.

When a new shape teaches us a new rule, add it here with its number and a
one-line measurement, and back it with a fixture.

---

# Constraints on the prototype Python (keep it Evident-translatable)

The Python we write has **two layers**, and only one of them is constrained:

- **Harness / tooling layer** — the sweep runner, timing, argparse, file IO,
  report/diff generation, profiling, unique-name counters. This is throwaway
  scaffolding that *orchestrates* experiments and will never be Evident. Use
  Python freely here.

- **Model-construction layer** — the functions that build a Z3 `Goal`/constraints
  from problem parameters (today: the `build(scale)` encodings in
  `benchsuite/tasks.py`; tomorrow: anything that defines a "claim"). **This is the
  part Evident will eventually express, so keep it in the Evident-expressible
  subset.**

**The positive rule:** a model-construction function should be a *pure function
from problem parameters to a set of constraints*, built only from: named typed
variables; records (fixed-field tuples); enums / sum types (incl. recursive
datatypes); bounded sequences and sets; exact `Int`/`Real` arithmetic; bounded
quantifiers (`∀`/`∃` over a static range); guards (conditional constraints); and
composition of sub-claims. If the result depends only on the *inputs* (not on
Python execution order, object identity, or host computation), it will sugar.

**Avoid these in model-construction code** — each is something Evident can't (yet)
express, so reaching for it is a smell that the prototype won't translate:

| # | Avoid | Why / use instead |
|---|---|---|
| C1 | **Imperative mutation of model state** that depends on order/aliasing | Evident values are immutable. Folding to build a `Sum`/`And` is fine (declarative); mutating a shared accumulator others read is not. "To add to a set, union a new set" — the old set stays fixed. |
| C2 | **Host computation baked in as model logic** | Python may compute *inputs/parameters* (a fixed graph, a scrambled map — that's fixture data). It may NOT compute a value the *model reasons about* and freeze it as a literal. If it should be a constraint, write it as one. |
| C3 | **Reflection / metaprogramming / dynamic names** (`eval`, `getattr` tricks, names built by arbitrary string computation) | Evident has static names + composition. Simple indexed families (`f"c{u}"` over a bounded range) are fine — that's a `∀`; arbitrary dynamic schema generation is not. |
| C4 | **`None` / sentinel threading** | Evident has no null. Model absence with an explicit enum variant (Option-like). |
| C5 | **Exceptions as control flow** in model building | Evident constraints are total. (try/except around *solver calls* in the harness is fine.) |
| C6 | **Python `float` arithmetic leaking into Int/Real constraints** | Evident is exact (rationals). Modeling FP *as the Z3 `Float32` theory* is fine — that's exact bitwise; the ban is inexact host floats becoming literals. |
| C7 | **Unbounded / dynamic structure** whose size depends on a prior solve | Evident models are statically bounded. The one sanctioned dynamic form is the tick/fixpoint loop (carried `_x` state, solve-until-`Done`); use that, not an ad-hoc Python loop sized by solver output. |
| C8 | **Heterogeneous / ad-hoc dicts as model data** | Prefer records (fixed fields) and typed collections. A Python dict holding a fixed, known set of named Z3 vars is fine (it's a record); a dict with dynamic/heterogeneous keys maps to nothing clean. |
| C9 | **IO / side effects interleaved with constraint construction** | Evident separates effects (the Effect enum + solve→dispatch loop). Model builders must be pure. |

When a prototype genuinely *needs* a Z3 feature with no Evident surface, that is
itself a finding: **record it as an "Evident surface gap: X"** (in `FINDINGS.md`
or a `docs/notes/` file) rather than silently depending on it. The gap list is
phase-2 input — it tells us what surface we'll have to design.

These rules are provisional — we don't yet know the full set of structures that
won't translate. When you hit a case that's ambiguous, flag it for discussion
rather than guessing; we'll harden the list as we learn.

---

# Limitations of prototyping in Python + Z3 (know these going in)

- **The surface gap is the big one.** Python + Z3 can express far more than
  Evident currently can. A fast result that relies on a Z3 feature with no
  planned surface is *not yet reachable from Evident* — so a win in the prototype
  isn't automatically a win for the language. Always ask "could Evident produce
  this model?" and flag (C-rules, surface-gap notes) when the answer is no.

- **Z3 Python API gotchas** (already paid for, don't re-pay):
  - `statistics()['rlimit count']` is **cumulative per global context**, not
    per-solve — track deltas (`harness.py` does).
  - The default context is **global**: re-declaring an `EnumSort`/`TupleSort`
    name errors, and sort names collide across builds — use a unique-name
    counter (`tasks._uid`).
  - Use a **fresh `Solver` per measurement** for isolation; take **min over reps**
    to cut noise; time **tactic-apply separately** from solve.

- **Solver semi-decidability & nondeterminism.** Unbounded quantifiers, nonlinear
  arithmetic, and unbounded seq/string are semi-decidable → `unknown` and
  timeouts are normal results, not bugs. Timings and even sat/unsat *latency* vary
  across Z3 versions; pin libz3 (system 4.15.4) and re-measure after any change.

- **Eager construction invites C2 violations.** Python evaluates as it builds, so
  it's easy to accidentally compute-then-freeze something that should be a
  constraint. The model-construction rules above exist to catch this.

- **Measurement validity.** Python build time ≠ Z3 solve time; a "slow" case may
  be slow to *build* in Python and fast to *solve*, or vice versa. Separate the
  two before drawing a conclusion.

---

# The forward vision: a general-purpose set-theoretic language on Z3

The end goal restated as a target, not just a phase-2 note: **Evident is a
general-purpose programming language whose paradigm is set theory and whose
runtime is Z3 + a minimal harness.** Every phase-1 experiment should be read as
evidence for or against that target being reachable.

The likely shape of the language (see `docs/plans/claims-as-sets.md`,
`docs/plans/relations-as-tuple-sets.md`):

- **Claims as sets** — a named predicate is the set of assignments satisfying it;
  composition is set algebra. This is how *all* abstraction works in the
  language, not just constraints — functions, types, and modules are sets too.
- **Relations as tuple-sets** — dispatch / mappings / grammars / lookup tables as
  membership in a set of tuples, lowered (not interpreted as control flow).
- **Under-determined, bounded solution spaces** — partial constraint; the solver
  fills the rest; somewhere between "totally free" and "all equalities."
- **Solve-until-`Done`** — a fixpoint/tick loop with carried state and an effect
  trace, on top of whichever lowering backend. This is how the language reaches
  *general-purpose* territory: unbounded computation, IO, and real programs live
  in the loop + effects, since a single Z3 solve is bounded by construction.

What general-purpose demands that pure constraint-solving does not — the phase-1
questions that decide whether the end goal is reachable:

- **Effects & IO** on a solver runtime — the minimal harness must dispatch reads,
  writes, and FFI from solver output (the old Effect-enum + solve→dispatch idea),
  without growing into a heavy runtime.
- **Unbounded computation** — algorithms that don't fit one bounded solve must
  decompose across the `Done`-loop; we need to show that's expressible and fast.
- **Everyday data structures & algorithms** in set-theoretic terms — can a
  hashmap, a parser, a sort be *sets/relations* and still perform? Each is a
  phase-1 prototype waiting to be written.

The open phase-2 design question (already under discussion): **where the lowering
decision lives** — a solver-side tactic portfolio (auto-tuner), a compiler-side
shape-directed encoder (our benchmark table as rules), or a relational surface
over a bounded-unroll backend. We have the data to start building the
shape-directed router today; the surface syntax is still open.
