# Walls hit porting the pre-oracle passes to Evident

Status: the fsm autocarry pass is fully ported and gate-green on
correctness (2026-06-10), but NOT wired into `flatten-evident.sh` —
the performance budget gate failed. This note records the wall
precisely so the wire-in decision (or a kernel fast-path) can be made
deliberately.

## What was ported

`scripts/passes/` holds the port of `scripts/expand-fsm-autocarry.sh`
as three kernel-run Evident programs plus a shared helper library:

| File                    | Role                                              |
| ----------------------- | ------------------------------------------------- |
| `autocarry_analyze.ev`  | one pass over the flat source → tagged registry record stream (bare decls, call bindings, body-reference carry marks) |
| `autocarry_fix.ev`      | record stream → carry fixpoint → 2-line edit script (inserts + injects, sorted by line) |
| `autocarry_apply.ev`    | edit script + source → expanded source            |
| `autocarry_lib.ev`      | single-expression scan claims (ws-skip / word-end / trim-end / comment step / paren step / body probes / digit parse) safe to multi-instantiate |
| `autocarry-evident.sh`  | the stdin→stdout filter wiring the three kernels  |
| `build-autocarry.sh`    | rebuilds the committed `.smt2` artifacts (awk pipeline + frozen oracle = the bootstrap) |

Gates that PASS:
- **Corpus parity**: byte-identical to the awk pass on all 236 real
  pipeline inputs (every compiler2_units/conformance/seq/fsm_compose
  stream + the full compiler2/driver.ev stream).
- **Self-application**: identical output on all four pass sources'
  own flattened streams.
- **Behavioral**: `tests/fsm_compose/run.sh` 5/5 with the Evident pass
  substituted into the flatten pipeline.

Gate that FAILS — the ≤1 s perf budget:
- 8468-line driver stream: **1.46 s** wall (awk: ~60 ms).
  analyze 0.79 s · fix 0.26 s (overlapped in a pipe with analyze:
  A|C = 1.06 s) · apply 0.39 s · ~0.15 s kernel startups (the
  functionizer's two verify solves per program dominate startup).

## The wall: per-step interp cost × one-line-per-tick

The kernel's functionizer interp evaluates every program step each
tick at ~0.5–0.7 µs/step (Z3-AST node walks through FFI), and a
ReadLine-driven filter spends one tick per input line. A text
transform needs ~50–90 steps live (analyze: 82), so the floor is
~60 µs/line — ~0.6 s on a 9k-line stream before any real work, per
program in the chain.

Measured facts that any future attempt should keep (all 2026-06-10,
driver stream, release kernel):

- Idle guarded step ≈ 0.42 µs/tick (60-pad experiment); `_`-prefixed
  (un-carried) locals ≈ 0.18 µs — naming transients `_t_*` skips the
  state-carry machinery and is worth ~25 %.
- ITE / ∧ / ∨ ARE lazy in the interp (eval.rs), so 64-deep unrolled
  scan chains cost only the distance actually scanned — but each
  char step is ~1–2 µs (≈ 10–20 FFI node visits), so char-by-char
  scanning is ~50× slower than C-side `index_of`/`starts_with`.
- Carried-string holds clone at ~0.1 µs/KB/tick — streaming records
  out instead of accumulating registries in carried state (analyze →
  fix) bought ~0.4 s and lets the two kernels run concurrently.
- Enum-typed reads of `last_results` (`r ∈ Result = last_results[k]`)
  make the whole program Z3-residual; `match` arms extract fine.
- Effects must be literal seqs of NAMED Effect vars under NAMED Bool
  guards; inline `LibCall(...)` leaves or compound guards fall off the
  guarded-seq extractor (25 ms z3/tick vs 0.0).

## What would close the gap

1. **Two-lines-per-tick batching** (⟨…, ReadLine, ReadLine⟩ + dual
   line-proc, fall back to single on submode entry): halves the
   dominant line-tick count for ~+30 steps; projected ~0.9–1.0 s.
   High complexity on the analyze state machine.
2. **A kernel fast-path for FSM text filters** (wave 5b/5c adjacent):
   batch step evaluation or Cranelift-compile string steps; the JIT
   currently covers int/bool scalars only.
3. Accepting a relaxed budget: at 1.46 s/driver-stream the suites
   stay functional (conformance fixtures cost ~0.25 s each through
   the 3-program chain vs ~30 ms awk).

## Stretch goal not attempted

`scripts/lower-bounded-seq.sh` (~16 rule families + completeness
check) was not ported — the autocarry perf wall consumed the budget,
and the same wall applies with a larger step count.

## Purism findings (evident-critic, pre-commit 2026-06-10)

The port carries two deliberate purity BLOCKERs, both perf-driven and
both needing an operator ruling before this style spreads:

1. **`_t_`-prefixed transient locals** use the spec-reserved `_<name>`
   carry namespace for non-carries, to skip the kernel's per-tick
   state-carry cost (~0.25 µs/var/tick). Ruling needed: a blessed
   surface for "un-carried local", or accept the cost and drop the
   prefix.
2. **Unbounded carried String buffers** (the analysis registries, the
   claim body buffer, the edit script): FSM doctrine says unbounded
   data lives on the tape (FTI buffers via effects), not in carried
   state. The tape design was not attempted — it multiplies effects
   per tick and the pass is already over its perf budget.

Plus tolerated WARNs: delimiter-encoded record registries in Strings
(blocked on Seq-of-records lowering gaps), hand-unrolled char-scan
chains and digit parsing in `autocarry_lib.ev` (no blessed string-scan
surface exists — a `str_span`-like builtin would erase ~700 lines of
generated chain).


## RESOLVED (operator ruling 2026-06-10): the `_t_` transient class

Ruled a violation; fixed by renaming all `_t_*` transients to ordinary
carried variables (`tk_*`). Measured on the lowered-IR kernel: full
driver flatten 0.72 s (awk) vs 1.00 s (Evident pass, everything
carried) — the pun was buying ~40 ms. Output byte-identical to awk
after the rename; gates green. `_x` without a base `x` is now a
purism BLOCKER (§3.6). The unbounded-carried-String registry class
remains tolerated-tracked, pending the bounded Seq-of-String carry
lowering and an operator ruling.


## RESOLVED (2026-06-10): the unbounded-carried-String registry class

Every carried registry across all three stages is now statically
bounded — the §1.2 "FSMs stay finite" BLOCKER (V15) is cleared. Two
shapes, chosen by how each registry is *read*:

- **Bounded Seq(String)** where the read is equality membership. The
  fsm-name set was `str_contains(_fsm_set, "⟨F⟩")` over a concatenated
  String; it is now `fsm_set ∈ Seq(String)` + `#fsm_set ≤ 104` read by
  `∃ e ∈ fsm_set : e = key` — the blessed §2.5 existential. Same for
  analyze's `hdr_pend` (cap 16). These are honest sets; the surface
  reads as set theory.

- **Length-bounded String** where the read is char-offset cursor
  scanning (`index_of`/`substr` into a concatenated registry). These
  keep the delimiter encoding (the pre-existing tolerated WARN, NOT
  introduced here) and gain a literal `#reg ≤ N` bound. The bound makes
  the carried state finite; an overflow is loud — analyze (61-64) and
  fix (70-79) clamp the append and emit a distinct BuildEprint+BuildExit
  code, apply's per-record reassignments trip the kernel's per-tick
  invariant re-check (UNSAT, exit 2). All caps ≥ 4x the corpus maxima
  measured on driver.ev (the largest stream); the per-stage exit-code +
  cap tables live in each program's MODULE header as wire facts.

Why NOT Seq-of-records for the cursor-scanned registries: those hold
500–734 records each (bind 529, slot 734 on driver); a keyed-projection
read would lower to a per-tick N-way ∃-unroll over 2000+ slots,
multiplied across ~19 read sites and thousands of ticks — categorically
past the ≤1 s pass budget. The bounded String keeps the registry finite
(the BLOCKER's actual requirement) without that explosion. The
delimiter-encoding remains a tolerated WARN pending a `str_span`-style
scan builtin or a Seq-of-records lowering that the functionizer can
extract without the unroll.

Verification (2026-06-10): corpus parity 302/302 byte-identical (awk vs
Evident, every pipeline stream); self-application identical on all four
pass sources; autocarry pass 0.43 s on the 8.9k-line driver stream
(≤1 s budget, GREEN); all three stages 0.0 ms z3; overflow guards
verified to fire (fix exit 70 + stderr, apply exit 2); conformance
153/154 (123 known-fail), 0 timeouts; compiler2_units 67/67;
fsm_compose 7/7; functionization gate GREEN.
