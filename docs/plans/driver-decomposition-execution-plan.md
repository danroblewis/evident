# driver_main decomposition — execution plan (for review)

Status: APPROVED — executing unattended (2026-06-08, 03:5x).
Operator decisions: (1) §31 — ACCEPT the eff_out bridge. (2) Scope — run
ALL phases A→E unattended. (3) Crash fix — extract E1 behavior-preserving,
land the ternary fix as a SEPARATE commit + unit test after E1.
Supersedes the Probe-C-only approach in `driver-subsystem-map.md` §4 now
that carry-preserving fsm composition exists (`fsm-composition.md`).

## Goal

Turn `compiler2/driver.ev`'s `driver_main` (~5930 lines) from one
monolith into ~10–14 per-file, **independently unit-testable** subsystems.
Each becomes a **carry-owning `fsm`** in its own file; `driver_main`
shrinks to wiring (the shared bus + one slot-call per subsystem).

## Why now / why this differs from the held Probe-C branch

The held branch (`worktree-agent-a488863a`, 221 lines moved) used Probe-C:
declarations stay in `driver_main`, only bodies move. That left the carry
scaffolding stranded — which is why it barely shrank. With carry-preserving
fsm composition, a subsystem **owns** its `x`/`_x`: we write the latch/
machine as a standalone `fsm`, compose it `Sub(x ↦ busvar)`, and the
transform injects `_x ↦ _busvar` so the carry travels with the logic.
That's the genuine isolation, and each subsystem is then a unit you can
test and prove against in isolation.

## Per-step protocol (every extraction, no exceptions)

1. Move one subsystem's fields + transition logic into a new
   `compiler2/driver_<name>.ev` as an `fsm`; replace it in `driver_main`
   with a slot-call.
2. `expand-fsm-autocarry.sh < driver.ev | oracle emit driver_main` →
   stage1.
3. **Equivalence gate (all three must hold):**
   - `sed 's/__call[0-9]\+/__callN/g'` diff vs frozen baseline == empty
     (semantic identity modulo the oracle's call-counter renaming),
   - manifest `state-fields` line byte-identical (no field added / dropped
     / retyped),
   - conformance == **137/138**.
4. Write a unit test for the extracted fsm in `tests/compiler2_units/`
   (compose standalone, drive an input, assert output/exit).
5. Commit (one subsystem per commit). If a step can't be made equivalent,
   **revert it** and record why in this doc.

Frozen baseline = stage1 from `main`'s `driver_main` captured once at start.

## Extraction order (smallest blast radius → biggest payoff)

**Phase A — pure muxes (zero carry; reuse held-branch work where clean)**
- A1. §27 state transitions → `driver_state.ev` (port from held branch)
- A2. §28 token consumption → `driver_consume.ev` (port from held branch)
- A3. §31 effects schedule — *blocked by the single-writer validator*; see
  Decision 1.

**Phase B — carry-owning latches (redo as fsms, NOT Probe-C)**
- B1. §2 ZINIT `z_*` latches (34 fields) → `driver_zinit.ev` (carry-owning fsm)
- B2. §3/§4 ED/G2 `d_cap_int` latches (49 calls) → fold into the same shape

**Phase C — the big machines (the real bulk; the point of the exercise)**
- C1. §3 ED machine step bodies (66 carry fields) → `driver_ed.ev`
- C2. §4 G2 RD record registry (58 fields) → `driver_g2.ev`

**Phase D — walks**
- D1. §10–13, §20–26 pmode-N walk bodies → `driver_pmode_*.ev`

**Phase E — translation (where the live crash is)**
- E1. expression / handle-stack translation incl. `C2Ite` → `driver_translate.ev`
- E2. the ternary-crash unit test (from `repro_deep.ev`) + the fix (null
  then/else handles), as a SEPARATE commit after E1's behavior-preserving
  extraction — so the equivalence gate stays meaningful (see Decision 3).

**Stays in `driver_main` (the shared bus):** `d_cap_int`, `pmode`, `zstep`,
`tcur`/`wend`/window, `st_*`, `il_*` — passed as input slots to whatever
needs them. Not "owned" by any subsystem.

## How it runs

One long-running background agent on a fresh branch off `main`: sequential
(single file, gated per step), staged commits for agent-fault resilience.
NOT parallel (one file = worktree conflicts). I check in between phases and
report. Deliverable: a branch with N gated commits + per-subsystem unit
tests, a `driver_main` that reads as wiring, and a report — left for review,
not auto-merged.

## Decisions needed from you (defaults recommended)

1. **§31 effects schedule.** The emit validator enforces the single-writer
   rule syntactically (`effects` must be one SeqLit-equality in
   `driver_main`). Extracting it needs a local `eff_out ∈ Seq(Effect)` +
   `effects = eff_out` bridge — adds ONE manifest field, conformance-green
   but not call-N-identical. *Recommend: accept the bridge* (only way to
   extract it; provably benign). Alternative: leave §31 inline.

2. **Scope tonight.** *Recommend: run A→C* (the bulk / real payoff), stop,
   and let you review before D→E. Alternative: run all A→E unattended.

3. **Ternary fix coupling.** *Recommend: keep extraction behavior-
   preserving (gate green) and land the fix as a separate commit + unit
   test after E1.* Keeps the equivalence gate meaningful. Alternative: fold
   the fix into E1 (gate then expected to change at E1).
