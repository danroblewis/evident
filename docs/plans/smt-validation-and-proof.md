# SMT validation & proof in Evident — what's real, what's reachable

Status: design note / reference (2026-06-08). Not a committed plan — a
map of what "validation" and "proof" can mean in a language whose
programs *are* SMT constraint models, why most of it overlaps with "just
writing tests," and the one capability that is strictly stronger and
sits unused under our feet.

Context: this came out of asking what "FTI formal validation" would even
be, once you notice that an Evident program is already a logical model Z3
solves — so "put the safety constraint in the model" is not, by itself, a
proof of anything. Related: `fti-formal-validation-feasibility.md` (the
bounds-proof slice), `driver-subsystem-map.md` §3 (state scoping).

---

## 0. The core tension

In a normal verification setup you have two artifacts: a **program** (in
some imperative language) and a **specification** (properties you want).
SMT proves a relationship between them — the program satisfies the spec.

In Evident the program **is** the constraint model. Program and logical
model are the same artifact. That collapses the usual program-vs-spec
split and creates the puzzle:

> If the FTI model already *contains* `write_slot < capacity`, I haven't
> *proven* safety — I've *assumed* it. The solver then refuses unsafe
> states because I forbade them, not because I showed they can't arise.

That skepticism is correct. "Validation is just what's already in the
model" is the **null case**. Real proof-content has to come from
somewhere the assertion isn't. There are exactly three such places, and
they are the three classic SMT verification modes.

---

## 1. The three proof modes

### 1.1 Entailment — prove an *emergent* property

Show the model **implies** something you did not directly assert:

```
Model ∧ ¬P   →  UNSAT      means   Model ⇒ P   is a theorem
```

Value scales with how non-obvious `P` is. Asserting `slot < cap` and
proving `slot < cap` is vacuous. Asserting `slot < cap` *and*
`addr = base + slot*8` separately, then proving the **consequence**
`base ≤ addr < base + cap*8`, is a real theorem — it shows the pieces
*together* exclude the bad state. This is the existing `unsat_*` / `sat_*`
convention (`sat_*` proves the model isn't vacuously empty; `unsat_*`
proves the bad state is excluded). It is **per-claim / per-tick**.

### 1.2 Equivalence / refinement — prove two models agree

Given two models, prove they have the same solution space (equivalence)
or that one only does what the other allows (refinement). This is
**translation validation** / **equivalence checking** (hardware &
compiler verification).

```
Functional equivalence (same inputs ⇒ same outputs):
    M_ref(in, o1) ∧ M_prod(in, o2) ∧ o1 ≠ o2   →  UNSAT
Refinement (every production behavior is a reference behavior):
    M_prod ∧ ¬M_ref                            →  UNSAT
```

Primary motivation is **performance and safe optimization**:
- Write a **slow, obviously-correct reference** model and a **fast,
  complex production** model; prove them equivalent once; run the fast
  one trusting it.
- Prove a constraint **redundant** (`M_without_C ⇒ C`) so it can be
  dropped — fewer constraints, faster solves.
- Proving a subsystem equivalent to a closed-form function is what would
  let the **functionizer** safely replace it.

Honest limit: you can only prove equivalence to a **reference model you
write**, never to the *actual* external machine (real OpenGL/socket/heap
is unmodeled). The reference *is* your spec; equivalence checking proves
production conforms to spec, not to reality.

### 1.3 Induction — prove a property over *all ticks*

This is the one that is strictly stronger than tests, and the one we are
closest to and not using.

An Evident FSM's per-tick constraint — the relation between `_x` (prev
state) and `x` (next state) — **is a symbolic transition relation
`T(s, s′)`**. That is exactly the input format SMT-based model checkers
(k-induction, IC3/PDR) consume. We have, by accident, built a model
checker's input language and are using it only as a one-shot solver.

Prove an invariant `P` holds on every reachable tick by induction:

```
Base:  Init(s0) ∧ ¬P(s0)          →  UNSAT     (holds at tick 0)
Step:  P(s) ∧ T(s, s′) ∧ ¬P(s′)   →  UNSAT     (preserved by a tick)
```

If both discharge, `P` holds on **every reachable tick** — proven without
executing the run. Not "this write is in bounds" but "**`count ≤ capacity`
is invariant across the entire execution**." That is the qualitative jump
from "assertion you assumed" to "theorem about all behaviors."

`_x → x` is `s → s′`; `is_first_tick` gates `Init`. The transition
relation is already written — it's the program. What's missing is the
*induction harness* that asserts `T` over symbolic `s` (not a concrete
prev-state read from the model) and checks the step obligation.

---

## 2. How this relates to "just writing tests"

The operator's observation: this sounds like what you'd get from being
able to write tests — which the carry-preserving fsm-composition fix
(`fsm-composition.md`) unlocks anyway. That's right, and the relationship
is precise:

| | What it checks | Coverage | Cost |
|---|---|---|---|
| **Unit test** (fsm compose) | one concrete scenario → expected output | the cases you write | run once, ms |
| **Entailment proof** (§1.1) | a property holds for one tick / one claim | all states of that claim, one tick | one solve |
| **Induction proof** (§1.3) | an invariant holds on *every* tick | all reachable states, all ticks | two solves |
| **Equivalence** (§1.2) | two models agree | all inputs | one solve |

Tests and §1.1 proofs cover **specific cases**; §1.2/§1.3 cover **all
cases**. So:

- **Most day-to-day value is tests**, now unlocked: a subsystem is a
  carry-owning fsm you compose in isolation, drive with an input, and
  assert stdout/exit (the `tests/fsm_compose/` shape). This catches the
  *logic* bugs (the kind the `Z3_mk_ite` null-operand crash was) by
  reproducing them in milliseconds. **Do this first; it's the 80%.**
- **Proof is the extension past testing**: when "for the cases I wrote"
  isn't enough and you need "for *all* cases" — an unbounded safety
  invariant (induction), or a guarantee that an optimized model didn't
  change behavior (equivalence). You reach for it when a property is
  load-bearing enough that sampled cases won't do.

The same `.ev` artifact serves both: a pure helper claim (e.g.
`FtiInvariant`) is the *unit under test* for a `sat_/unsat_` obligation
**and** the body the production FSM composes — the property proven is
literally the claim the code runs, not a re-implementation that can drift.

---

## 3. FTI as a remote state machine (the framing this came from)

An FFI/FTI is an interface to a **remote state machine** — memory is one,
but so are OpenGL state, sockets, files, locks/semaphores. The FTI is an
**abstraction / simulation** of that machine, *including its fault modes*
(memory: a write outside `[base, base+cap*8)` SIGSEGVs; we model that as a
state and constrain reachability). Validation, in SMT terms, is then:

- **Soundness of the abstraction** — the model never admits a "safe"
  state the real machine would crash on (its fault states are reachable
  only where the real machine's are).
- **Consistency** — the model is satisfiable (not over-constrained) and
  non-vacuous (`sat_*`).
- **Counterexample search** — model the fault and ask `Model ∧
  reachable(fault)` SAT; the solver *hands you the crashing inputs*. This
  is a debugging tool, not just a gate.
- **Refinement to a reference** — prove the detailed/fast production FTI
  conforms to a simple reference spec of the remote machine (§1.2).
- **Whole-run safety** — the fault state is unreachable on every tick
  (§1.3 induction).

The boundary, stated honestly and repeatedly: every one of these proves
things about your **model / reference** of the remote machine, never
about the real external thing. The external heap, the real GL driver, the
real socket are **inputs/assumptions** to the model, not theorems of it
(same shape as the cross-allocation aliasing gap and the AST-liveness gap
in `fti-formal-validation-feasibility.md` §1.2–1.3).

---

## 4. Concretely buildable, in priority order

1. **Per-claim entailment proofs** (`sat_/unsat_`) — exists today; the
   FTI bounds suite (`tests/fti_proofs/`) is the worked template. Cheap,
   catches the recurring sizing/bounds class.
2. **Unit tests via fsm composition** — the 80% case, now unlocked. Each
   subsystem extracted by the decomposition becomes a testable fsm; write
   a fixture per bug we hit (the `Z3_mk_ite` repro is the first customer).
3. **Reference-vs-production equivalence** per FTI — the "two models,
   prove equal solution spaces" idea. Catches divergence and underpins
   performance swaps / functionizer replacement.
4. **Inductive safety harness** — assert the per-tick model as `T(s,s′)`
   over symbolic state and discharge base+step. The strictly-stronger
   capability; the FSM substrate is already there, only the harness is
   missing. Highest ceiling, most new machinery.

`#1` and `#2` pay off immediately and compose with current work. `#3` and
`#4` are the "past testing" tier — worth keeping in view, not on the
critical path. The carry-preserving fsm-composition fix is the
prerequisite that makes a subsystem a standalone fsm — i.e. the unit you'd
write the reference model against (`#3`) and run induction on (`#4`).
