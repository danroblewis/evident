# Relational extraction — can Z3 rearrange constraints into functionizer steps?

Date: 2026-06-10. Feasibility study; no kernel edits. All probes run on
z3 4.15.4 (CLI + C API) against hand-written relational `.smt2` and REAL
oracle-emitted tick formulas (`tests/compiler2_units/perf/fti_buffer_loop.ev`
→ 38-line `.smt2`; `compiler2/driver.ev` `driver_main` → 24,803 lines /
3,289 asserts). Probe files were under `/tmp/relx/`; every load-bearing
S-expression is reproduced inline below.

## The question

The kernel functionizer (`kernel/src/functionize/mod.rs`) extracts steps
**syntactically**: `extract_program` captures literal `(= var expr)`
covering assignments, guarded `(=> g (= var e))` branches, seq
length/element pins, and the bool-XOR special case. Its tactic chain is
only `simplify` + `propagate-values` (`simplify_assertions`) — no
`solve-eqs`, no `macro-finder`. Anything relational (`x*y = a+b` with
`y` the output; `count' - count = 1`) is refused and the tick falls to
Z3. The hope under test: Z3-as-CAS can rearrange relations into the
directed `out := f(inputs)` definitions the extractor needs, as a
**normalization pre-pass** before extraction.

## Experiment 1 — macro-finder: irrelevant to our formulas

`macro-finder` operates only on **universally quantified** definitions of
uninterpreted *functions*. Our tick formulas are 100% ground constants
(compiler.smt2: 7,851 ground functional asserts, zero quantifiers —
STATE.md). Measured:

```smt
; ground (our shape): COMPLETE NO-OP — goal returned unchanged
(assert (= y (+ a b)))
(assert (= (* x y) (+ a b)))
(apply macro-finder)            ; → goal unchanged, no model-add

; its home turf: literal ∀-definition — extracted and inlined
(assert (forall ((x Int)) (= (f x) (+ x 1))))
(assert (= c (f 5)))
(apply macro-finder)            ; → goal: (= c 6)
                                ; → (model-add f ((x!1 Int)) Int (+ x!1 1))

; quantified but RELATIONAL: refused even on home turf
(assert (forall ((x Int)) (= (- (f x) x) 1)))
(apply macro-finder)            ; → goal unchanged
```

**Verdict: macro-finder extracts nothing from ground tick formulas, and
is literal-LHS=RHS even where it applies. Dead end for this purpose.**

## Experiment 2 — solve-eqs: a real CAS for linear shapes

`(apply (then simplify solve-eqs) :print_model_converter true)` per probe:

| probe | result |
| --- | --- |
| `(= y (+ a b))` plain assignment | eliminated **`a`**, not `y`: `(model-add a () Int (- y b))` — rearranged a covering assignment *backwards* |
| `(= (* x y) (+ a b))` nonlinear, target `y` | eliminated `a` (the linear var): `(model-add a () Int (- (* x y) b))`; never solves for `y` |
| `(= (+ y x) (+ a b))` linear | `(model-add y () Int (- (+ a b) x))` |
| guarded-pin family `(=> g1 (= out 1)) …` | **untouched** (with and without `:context-solve true`) — solve-eqs never builds the covering ternary |
| chained `0 ≤ count ≤ cap` | untouched — inequalities are not equations, stay predicates |
| bool relation `(= (xor out a) b)` | refused — no bool isolation |

Direction-forcing: rewriting every *input* occurrence as an arity-1
application (`(in 0)` for `_x`) makes inputs non-eliminable — solve-eqs
only eliminates 0-arity constants — so the output is the only candidate:

```smt
(assert (= (+ y (in 0)) (+ (in 1) (in 2))))   ; forced direction
; → (model-add y () Int (- (+ (in 1) (in 2)) (in 0)))     WORKS
```

Inversion side-conditions — solve-eqs **refuses, never drops, never
guards**:

```smt
(= (* (in 0) y) (+ (in 1) (in 2)))   ; var coefficient, Int AND Real: goal unchanged
                                      ; (no y := (a+b)/x, no x≠0 guard — refused)
(= (* 2 y) (in 0))                    ; Int constant coeff: refused (divisibility)
(= (* 2.0 y) (in 0))                  ; Real constant coeff: y := (/ (in 0) 2.0)  (exact, safe)
(= (ite g (+ y 1) (- y 1)) (in 0))   ; output spread across ite branches: refused
```

So the operator's hoped-for `x*y = a+b ⟶ y = (a+b)/x` does **not**
happen — and that is the safe behavior: solve-eqs only produces
definitions that are unconditionally valid.

### The money shot: solve-eqs on a REAL emitted tick formula

On the oracle-emitted `fti_buffer_loop` formula (unmodified), solve-eqs
extracted **exactly the three manifest outputs**, with correct
universally-valid, ite-aware, transitively-substituted definitions:

```smt
(model-add done      () Bool (>= (ite is_first_tick 0 (+ 1 _buf.count)) 800))
(model-add buf.count () Int  (ite is_first_tick 0 (+ 1 _buf.count)))
(model-add buf.base  () Int  (ite is_first_tick 100 _buf.base))
```

Inputs survived untouched because the carry equations are not invertible
through the `ite` — the emit shape *structurally* forces the right
direction most of the time. Residual goal: the invariant inequalities and
the guarded `effects` writers (per-tick predicates today as well).

At scale, on the emitted compiler2 driver (3,289 asserts):
`(then simplify solve-eqs)` runs in **3.2 s** and produces **3,223
model-add definitions** — load-time-budget compatible. But 3 of them are
**wrong-direction** (e.g. `(model-add _rec_count () Int (- rec_slot (- 1)))`
— a carry *input* defined from an output), confirming input-wrapping is
mandatory for a sound pre-pass, not just the lucky structure.

### The API problem, and two ways around it

solve-eqs's definitions live in the apply-result's **model converter**,
which the C API does not expose symbolically (`:print_model_converter`
is a CLI printing option; `Z3_apply_result_*` gives goals only). Two
viable kernel-side mechanisms, both verified:

**(a) The cancellation trick — pure `Z3_simplify`, no tactics.** For an
uncovered equality `L = R` and a candidate output `v` occurring linearly
with coefficient ±1: `def := Z3_simplify(v - (L - R))` (or `v + (L - R)`
for −1). If the result no longer mentions `v` (the existing
`mentions_name` gate), it IS the rearrangement; otherwise refuse.
Measured:

```smt
(simplify (- y (- (+ y c) (+ a b))))   ; → (+ (* (- 1) c) a b)        accepted: y := a+b-c
(simplify (- y (- (* x y) (+ a b))))   ; → (+ y (* (- 1) x y) a b)    still mentions y: refused
(simplify (+ y (- (- c y) a)))         ; → (+ c (* (- 1) a))          coeff −1 works
```

Soundness per def is one load-time `check-sat`: `¬((L = R) ⇔ (v = def))`
must be UNSAT. The existing tick-0/1 verify gate stays as the outer net.

**(b) `Z3_solver_solve_for(c, s, variables, terms, guards)`** — present
in the installed 4.15.4 C API ("only linear solutions are supported…
may be presented in triangular form"). Works **only on
`Z3_mk_simple_solver`** (the default tactic solver's preprocessing
eliminates the vars before the theory sees them → empty result), after a
`Z3_solver_check`. Strictly stronger than solve-eqs on Int coefficients —
it synthesizes divisibility side conditions:

```
2*y = a, solve for y  →  y := (div a 2)
                         guard: (and … (= (mod a 2) 0))
```

But at whole-formula granularity it is **trail-relative** (like the
congruence API): on the real fti formula it returned
`buf.count := (+ _buf.count 1)` (only the ¬first-tick branch — the
current model's branch), `buf.base := 0` (a model *value*), and a
circular `_buf.count := (+ 1 _buf.count (- 1))`. **Usable only
per-equation in a fresh simple solver**, where the guard returned is
exactly the consumed equation (+ divisibility). Guards slot in as
per-tick predicates, which the fast path already evaluates natively.

## Experiment 3 — determinism

Fully deterministic, and insensitive to declaration order, variable
names, and equation side order: `(= y (+ a b))` eliminates `a` across
3 identical runs, decl-order swap, rename `a→z` (eliminates `z`), and
side flip `(= (+ a b) y)`. The pick is a fixed internal traversal
heuristic — stable but **not controllable**, and (per Exp 2) sometimes
an input. A pre-pass must force direction (input-wrapping or
per-equation `solve_for`/cancellation with the target chosen by us);
relying on solve-eqs's own choice is reproducible but semantically
arbitrary.

## Experiment 4 — lookup tables for small-domain outputs

Shape: at load, enumerate the provably-small input domain; for each
entry, one `check-sat` + `get-value` to read the output and polarity/
disequality probes to confirm the output is *functionally determined*
(refuse the table if any entry is underdetermined). Measured with
incremental push/pop in one z3 process:

| workload | queries | wall |
| --- | --- | --- |
| synthetic char-classifier, implication-defined (the exact guarded-pin shape that kills the extractor), 128 inputs | 128 | **14 ms** |
| same, 1,024 inputs | 1,024 | **53 ms** |
| REAL fti formula: `done` over is_first_tick × `_buf.count ∈ 0..2048` (= 4,098 entries), find + 2 uniqueness probes each | 12,294 | **0.59 s** |

~50 µs/query, even on the real datatype-laden formula. **Feasible at
load time for input domains up to ~10⁵ entries** (a few seconds, and
cacheable in the `.evidentc` side-car keyed on src-hash). Scope limits:

- output must be Bool / enum-code / bounded-Int (table-valued);
- the *input* domain must be provably bounded (type-body invariants like
  `0 ≤ count ≤ cap` with literal cap give exactly this);
- per-entry functional determination must hold, else refuse;
- composite outputs (`effects`) are out of scope.

Notably this covers the implication-defined guarded-pin family — the
documented COVERED-vs-implication perf trap (CLAUDE.md) — *when the
input domain is small*: the table is an alternative to hand-rewriting
into ternary chains for, e.g., char classification in the FTI lexer.

## Feasibility verdict — a normalization pre-pass in the kernel functionizer

**FEASIBLE, with a specific shape.** Not "run solve-eqs and read its
answers" (model converter is C-API-opaque; direction uncontrolled).
The workable design, ordered by cost:

1. **Stage N0 — cancellation pre-pass** (cheap, highest value/effort):
   in `extract_program`, when an equality assertion fails today's
   capture, for each manifest-output / intermediate var mentioned
   linearly, try `Z3_simplify(v ∓ (L − R))` + `mentions_name` gate +
   one load-time UNSAT equivalence check. Accepted defs become ordinary
   `StepBody::Scalar` entries; refused assertions stay predicates as
   today. Pure addition: extraction can only grow, never lose soundness
   (tick-0/1 verify unchanged). Handles ±1-coefficient linear
   rearrangement — `total = x + y` with `y` the output, carry deltas
   `count' - count = 1`, etc.
2. **Stage N1 — per-equation `Z3_solver_solve_for`** (fresh
   `Z3_mk_simple_solver` per equation): adds Int-coefficient division
   with synthesized `mod`-guards; guards become per-tick predicates.
   Only if N0's coverage proves insufficient.
3. **Stage N2 — lookup-table extraction**: for refused outputs whose
   input domain is bounded by type invariants, enumerate at load
   (≤ ~10⁵ entries, ~50 µs/query), cache in `.evidentc`. New
   `StepBody::Table` evaluates as a slot-indexed read — JIT-trivial.

What this unlocks for surface style (cross-ref
`docs/plans/relational-style-exploration.md`, sibling draft in flight):
the COVERED-assignment rule (CLAUDE.md perf trap, 2026-06-09) could
relax from "every output needs one literal covering `=`" to "every
output needs one *linearly invertible* equation" — relations like
`buf.count - _buf.count = 1` or `lo + width = hi` become legal hot-path
style instead of traps. The guarded-pin keyed-projection family stays a
trap *except* where N2's bounded-domain table applies.

**What stays out of reach** (confirmed refusals, not gaps in effort):

- nonlinear inversion (`x*y = c` → `y = c/x`): refused by every
  mechanism at every sort, no side-condition synthesis exists;
- guarded-pin families → covering ternary: solve-eqs (incl.
  `context-solve`) never merges branches; only N2 tables or the
  existing surface lowering (`lower-bounded-seq.sh`) cover this;
- bool isolation through `xor`/`=`;
- inequalities as definitions (chained `0 ≤ count ≤ cap` correctly
  stays a predicate);
- ite-spread outputs (`(ite g (+ y 1) (- y 1)) = a`).

**Future options, deliberately not experimented here:** symbolic
regression (fit `f(inputs)` to enumerated samples — subsumes N2 with
generalization risk; would need its own verify story) and LLM-generated
candidate definitions (cheap to validate with the same per-def UNSAT
check, but a build-time external dependency). Both are candidate-
*generators* in front of the same load-time equivalence check; the
verification machinery N0 introduces is the reusable piece.
