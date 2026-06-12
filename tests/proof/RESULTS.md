# prove-invariants over the compiler's real carried types

`scripts/prove-invariants.sh` run against the four carried body-types that
`compiler2/driver.ev` actually carries across ticks (the zinit latch banks +
the FTI buffer). Each is exercised by its `tests/compiler2_units/types/*_carry.ev`
fixture. The headline: **none of the four type-body invariants is 1-inductive on
its own** — they are runtime safety nets (re-checked every tick, halt on
violation), and three of the four become statically provable once you supply the
latch step↔field ordering as an auxiliary lemma.

| carried type | fixture | invariant | plain step | with lemma |
|---|---|---|---|---|
| `Z3SolverCtx` | `z3_solverctx_carry.ev` | `sol>0 ⇒ ctx>0 ∧ cfg>0` | sat | **unsat (PROVEN)** |
| `Z3Sorts` | `z3_sorts_carry.ev` | `rsort>0 ⇒ isort>0 ∧ bsort>0 ∧ ssort>0` | sat | **unsat (PROVEN)** |
| `Z3Numerals` | `z3_numerals_carry.ev` | `four>0 ⇒ zero>0 ∧ one>0 ∧ two>0 ∧ three>0` | sat | **unsat (PROVEN)** |
| `FtiBuffer` | `fti_buffer_carry.ev` | `0 ≤ count ≤ cap` (`cap`=2048) | sat (`_count 2048`) | n/a — runtime net |

## Why the latch banks aren't 1-inductive (and the lemma that fixes it)

The handles latch in **step order** (`cfg@step1`, `ctx@step2`, `sol@step3`). On
any *reachable* carry the higher handle is live only once the lower ones already
are — so `sol>0 ⇒ ctx>0` holds. But 1-induction starts from an *arbitrary*
invariant-satisfying carry: from `(cfg,ctx,sol)=(0,0,0)` a single symbolic step
with a free `step` can land on the sol-step, setting `sol>0` while `ctx` is still
its carried `0`. That state is unreachable, but 1-induction can't see reachability.

The certificate is the step↔field correspondence — `step ≥ k ⇒ field_k > 0` —
which pins down which handles must already be live at a given step. Supplied as
the 4th arg to the tool (`tests/proof/lemmas/*_latch_order.smt2`), it discharges
all three. This is textbook k-induction strengthening.

## The buffer is a genuine non-inductive (the safety-net case)

`fti_buffer_carry.ev` increments `count` unconditionally each tick. `0 ≤ count ≤
cap` is preserved up to `count=cap`, then the next `count++` breaks it — the
counterexample `_buf_count 2048` is exactly that. The fixture is correct because
it `Exit`s after three ticks (never nears `cap`); the invariant is doing its real
job as a **runtime halt** (the kernel's UNSAT/exit-2 overrun guard), not as a
static guarantee. Making it 1-inductive would require guarding the increment
(`count < cap ⇒ count' = count+1`) — a behavior change, not a lemma.

## Takeaway

The tool cleanly separates the two reasons a type invariant can be `sat`:

1. **needs a lemma** — sound on reachable states, provable with an ordering/
   monotonicity fact (the latch banks). The lemma is itself a proof obligation.
2. **runtime net** — relies on the program halting before the bound is hit (the
   buffer). No lemma; the invariant *is* the guard.

The tool classifies the two automatically: on a `sat` step it pins the
counterexample and prints `totality: has-successor` (needs a lemma) or
`totality: STUCK` (real no-successor / exit-2 overrun).

Reproduce: `tests/proof/lemmas/*.smt2` are the lemmas;
`scripts/prove-invariants.sh tests/compiler2_units/types/<fix>.ev main <pfx> [lemma]`.
Standing gate over all four (pins this baseline): `scripts/invariant-gate.sh`.
