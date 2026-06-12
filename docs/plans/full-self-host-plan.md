# Plan: full self-hosting — remove the oracle, one `compiler/` directory

**Goal.** Reach the point where the producing path is **kernel +
compiler-code only**: `compiler2/driver.ev` compiles its *own* flattened
source to the committed `.smt2` artifact, the Rust `evident-oracle` is
deleted, `compiler/` (the old self-hosted compiler) is retired, and
`compiler2/` is promoted to `compiler/`.

This plan covers reaching the **no-oracle milestone**. The kernel-shrink
waves (5a–5d: Z3 wrapper / libffi / functionizer / AOT into Evident) are
the *next* mountain and live in [`post-cutover-roadmap.md`](post-cutover-roadmap.md)
— explicitly out of scope here.

---

## 0. Current state (verified 2026-06-12)

- **The two compilers are import-decoupled already.** `compiler2/` imports
  nothing from `compiler/`; it carries its own `compiler2/parser.ev`
  (evolved) and `compiler2/lexer.ev` (a copy). The "cross-dependency"
  between the directories is gone — the only remaining tie is that
  `compiler/` still *builds the committed artifacts*.
- **`compiler/` is still load-bearing as an ARTIFACT SOURCE, not as code
  compiler2 depends on:**
  - `compiler.smt2` (the kernel-run compiler) is built from `compiler/*.ev`
    and drives `test.sh` Phases 4–6 (lang / kernel / seam).
  - `sample.smt2` is built from `compiler/sample.ev` (the sat-check path).
- **`compiler2/` is the new compiler**, exercised by the conformance gate
  (`.goalpost/bin/run-conformance.sh` emits `stage1.smt2` from `driver.ev`
  via the oracle, on the fly). It does **not** yet produce a committed
  artifact, and self-compile (driver.ev compiling its own flattened
  source) is **partial**: ~46/79 module wrappers clean; full `driver_main`
  not yet.
- **Debug + proof infra exists:** kernel FFI crash reporter (RC 71),
  compiler unresolved-ident diag (RC 9), `selfcompile-sweep.sh`,
  `prove-invariants.sh` + `invariant-gate.sh` (Phase 9).

**Reframing the directory question:** promoting compiler2→compiler is not
a dependency-untangling job (done) — it is the *last* step of
self-hosting, because `compiler/` can only be deleted once `compiler2`
takes over producing `compiler.smt2` + `sample.smt2`.

---

## Revised priority (operator, 2026-06-12) — proof first, then refactor

The original plan sequenced toward a *usable, fast* cutover (B throughput
gates D). The operator reprioritized for **maintainability risk**: the
compiler code is getting hard to work on, and more changes pile complexity
before it's been proven. New ordering:

1. **Milestone 1 — ONE good `driver_main` self-compile (slow is fine).**
   The whole driver compiles its own flattened source, correctly,
   end-to-end. A multi-hour one-time run is acceptable — this is a
   feasibility PROOF, not the production path. Needs Gate A (correctness)
   + full-driver integration + the `LibCall` remainder. **Explicitly does
   NOT need Gate B (throughput).**
2. **Milestone 2 — the refactor.** With feasibility proven, attack the
   readability/maintainability debt (finish de-prefix, extract
   abstractions, the passes-seam / out-of-awk) BEFORE it ossifies.
   **Unlocked by Milestone 1:** the self-compile FIXED POINT becomes the
   regression oracle — a refactor that leaves the compiler re-compiling
   itself byte-identically is provably behavior-preserving. So M1 doesn't
   just prove feasibility; it makes the refactor safe.
3. **Functionizer / Gate B (throughput) — opportunistic.** Pursue only if
   it looks straightforward to implement; otherwise defer. It is no longer
   a near-term blocker (it only gates the *fast* production cutover, not
   the proof).

So the five gates below still describe the full cutover, but the *near-term*
target is Milestone 1 = Gate A complete + a single end-to-end self-compile,
with B and the heavy C/D/E work reordered to follow the refactor.

## The critical path (five gates, in dependency order)

```
A. self-compile correctness   →  B. functionizer throughput
                                        ↓
C. out-of-awk passes  ───────────→  D. self-host build loop (drop oracle)
                                        ↓
                                  E. promote compiler2 → compiler, delete oracle
```

A and C can proceed in parallel (different files). B gates D (a self-host
that takes days is not a self-host). D gates E.

---

## A. Close the self-compile correctness gaps

`driver_main` must compile its own flattened source end-to-end (today only
~46/79 isolated module wrappers do). Named, clustered blockers:

1. **Bodyless-record field-const ordering (~14 modules).** *In flight* —
   A′ Steps 2–3 (`declaration-prescan.md`): an uncapped text field table
   lets pass 0 pre-declare `r.f`. Clears the whole `rc=7` cluster.
2. **Floor-ctor `LibCall` as a general expression.** `Exit(n)` landed
   (merged). The remainder is `LibCall`'s 3-arg `Seq(LibArg)` literal — an
   N-ary cons-list assembly reachable from `call3_items`
   (`floor-ctor-as-general-expr.md`). Unblocks driver_lex / workitems /
   argref (currently halt at `rc=9 ArgInt`).
3. **`rc=1` malformed-stage2 + `rc=3` crashers.** Downstream of (1)+(2) per
   the B-agent triage — re-sweep after A+floor-ctor land; expect most to
   fall out, then name whatever remains.
4. **Full-`driver_main` integration.** A module wrapper can't reproduce a
   sibling↔sibling carry back-edge; the real target is the whole driver
   self-compiling. Drive with `selfcompile-sweep.sh` for localization, then
   a full `driver_main` self-compile as the acceptance test.

**Gate:** `selfcompile-sweep.sh` → all (non-refusal) modules `rc=0`; then a
clean full `driver_main` self-compile of flattened `compiler2/driver.ev`.

## B. Functionizer throughput (the perf gate on D)

Self-compile runs the compiler-as-SMT every tick; interp throughput is
~0.5 ms/tick and the registry width multiplies it (STATE.md; the sample-rung
wall). A self-host build that takes days is unusable.

- Extend kernel **JIT (Cranelift) coverage** to the interp-only shapes the
  driver FSM emits (the `interp` bucket in `[functionizer]`), so the hot
  loop is JIT not interp.
- Re-baseline with `functionization-gate.sh` + `perf-profile.sh` on
  `driver.ev`; target a per-tick budget that makes a full self-compile
  minutes, not hours.

**Gate:** full `driver_main` self-compile wall-time under a fixed ceiling
(set it once measured); `[functionizer]` shows ~0 interp on the hot path.

## C. Out of awk — the pre-oracle passes in Evident

Four passes still run in `flatten-evident.sh` as shell/awk; self-hosting
wants them in Evident (`compiler2/passes/`). Status:

| pass | state |
|------|-------|
| `lower-bounded-seq.sh` | porting — Evident port at 20/20 byte-equiv (member/forall/index/card/dyn); ∃-form, record-element decls, keyed-projection/pin-family, refusals remain (`lowerseq-port-continuation.md`) |
| `expand-fsm-autocarry.sh` | ported to Evident, **unwired** pending B (throughput) |
| `flatten-body-records.sh` | awk; may shrink/retire once A′ field table lands |
| `hoist-decls.sh` | **being deleted** by A′ Step 3 (the two-pass build subsumes it) |

**The architectural prerequisite (the big one):** compiler2 is a *streaming*
translator — parse→lower→translate fused, no reified AST, so passes have
nowhere to live. The self-hosting-deliverable section of
`post-cutover-roadmap.md` calls for a real **`parse → reify BodyItemList
AST → run passes → lower → emit`** seam + a `compiler2/passes/` home. The
AST enums survive (`compiler2/parser.ev`); the `run()` fixed-point driver
and the marshaler were deleted with bootstrap and must be re-expressed
(either a minimal kernel `run()` capability or passes-as-tick-phases). This
is the largest single item in the plan and the thing that lets C finish
cleanly instead of accreting more awk.

**Gate:** each ported pass byte-identical via its equivalence gate AND
byte-identical flatten output on `compiler2/driver.ev`; then wire it into
the pipeline (one `flatten-evident.sh` line per pass), conformance 153/155.

## D. The self-host build loop — drop the oracle

Today: `evident-oracle emit <flat.ev> driver_main -o stage1.smt2`. Replace
with the self-hosted emit:

1. **Bootstrap stage:** use the *current* oracle once to emit
   `stage1.smt2` from `driver.ev` (today's path). Call this `compiler⁰`.
2. **Self-emit:** run `kernel compiler⁰` on flattened `compiler2/driver.ev`
   → `stage2.smt2` (`compiler¹`). This is the compiler compiling itself —
   gated by A+B+C.
3. **Fixed-point check:** `compiler¹` emits `compiler²` from the same
   source; assert `compiler¹ == compiler²` byte-for-byte (the classic
   self-host fixed point — proves the self-hosted output is stable).
4. **Equivalence to oracle:** assert `compiler¹` and the oracle agree on
   the whole conformance + seam corpus (behavior-equivalent emit), so the
   cutover changes nothing observable.
5. **Promote the artifact:** the committed `compiler.smt2` becomes
   `compiler¹` (self-emitted), no longer oracle-emitted. Replace
   `evident-oracle emit` in `flatten-evident.sh` / `evident-self` /
   `run-conformance.sh` with the kernel-self-emit path.

**Gate:** fixed-point byte-identity (step 3) + oracle-equivalence on the
full corpus (step 4) + all of `test.sh` green with the self-emitted
artifact.

## E. Promote `compiler2/` → `compiler/`, delete the oracle

Once D holds, the old `compiler/` is dead weight and the oracle is
unreferenced:

1. **Cover the sample path.** `compiler/sample.ev` → `sample.smt2` (sat-check)
   is the one compiler/ capability compiler2 doesn't yet replace. Either
   port a `sample` entry into compiler2 or keep a thin sample driver — decide
   and close before deleting compiler/.
2. **Delete `compiler/`** (the old self-hosted compiler) and rename
   `compiler2/` → `compiler/`. Update every reference: imports
   (`compiler2/…` → `compiler/…`), `scripts/*`, `test.sh`, `.goalpost/*`,
   `tests/compiler2_units/` → `tests/compiler_units/`.
3. **Delete the oracle:** remove `evident-oracle` build, `build-oracle.sh`,
   the bootstrap binary + Dockerfile stage, and any `EVIDENT_ORACLE`
   plumbing. Update CLAUDE.md (the "compiler.smt2 is a frozen artifact /
   no self-host build path" caveat goes away).
4. **Collapse the gates:** the conformance gate and the seam path now both
   run the same self-emitted artifact; fold `compiler2_units` into the main
   suite; `test.sh` Phase 3 (`IMPL=selfhost`) and Phases 4–6 unify.

**Gate:** full `test.sh` green; `grep -r compiler2 .` and `grep -r oracle`
return only history; one `compiler/` directory; no Rust outside `kernel/`.

---

## Sequencing & parallelism

- **Now (parallelizable):** A (A′ Steps 2-3 in flight; floor-ctor LibCall
  next) ∥ C-ports (lowerseq tiers). Different files.
- **Then:** B (throughput) — gates D; and the **passes-seam** (C's
  architectural item) — the long pole, do as a dedicated effort.
- **Then:** D (build loop) — needs A+B+C done.
- **Finally:** E (promote + delete oracle) — needs D.

## Risks / unknowns

- **Functionizer throughput (B)** is the schedule risk: if JIT coverage of
  the interp shapes is hard, the self-compile loop may stay too slow to be
  the production path even when correct. Measure early.
- **Passes-seam (C)** is the architecture risk: re-introducing a
  parse→passes→emit seam in a streaming translator is real surgery; a
  half-done seam breaks emit. Spike the execution-model choice (kernel
  `run()` vs tick-phases) before committing.
- **Oracle-equivalence (D step 4)** may surface oracle behaviors the
  self-host doesn't replicate (or vice versa) — budget a reconciliation
  pass; the conformance corpus is the arbiter.
- **`sample.smt2` (E)** is an easy-to-forget compiler/ capability — name it
  now so it doesn't block the delete.

## Acceptance (the milestone this plan reaches)

`./test.sh` green with a **self-emitted** `compiler.smt2`; oracle deleted;
single `compiler/` directory; the only Rust is `kernel/`. After this, the
roadmap is the kernel-shrink waves in `post-cutover-roadmap.md`.
