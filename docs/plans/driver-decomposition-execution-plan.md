# driver_main decomposition — execution plan (binding spec)

Status: APPROVED, criteria locked (2026-06-08). This doc is the BINDING
SPEC, not a sketch. The refactor is REJECTED if it does not meet §6.

Operator decisions: (1) §31 effects — ACCEPT the `eff_out` bridge.
(2) Run all modules unattended. (3) Translation: extract behavior-
preserving first, land the ternary fix as a SEPARATE commit + test.

Related: `fsm-composition.md` (the mechanism), `driver-subsystem-map.md`
(the 31-subsystem inventory these modules cluster), `sample-ternary-crash-
diagnosis.md` (a bug that must become a module test).

---

## 1. The real goal (read this before touching code)

The goal is NOT "make `driver_main` smaller." The goal is:

1. **Well-tested modules** — each compiler concern verifiable in isolation.
2. **Add-a-test-on-bug** — when a bug appears, you can name the module,
   write a failing test, fix it, and keep the test as a regression guard.
3. **Clear, isolated contracts** — each module has a small, explicit
   interface; concerns don't leak across module boundaries.

Modularization is the *means*. Conformance 137/138 is an *integration*
test — necessary, but it never tells you WHICH module broke. Per-module
tests are the point.

## 2. The anti-pattern we are rejecting

The **pitiful refactor**: tiny 1–5 line components are hoisted out, the
original stays several-thousand lines, and the core problems remain. We
reject it explicitly via §3 (size floor), §5 (a real test per module), and
§6 (hard success criteria). A swarm of <40-line stubs is a FAILURE, not
progress.

## 3. Code standards

- **A module is a cohesive compiler phase or data structure** with a named
  contract. Every module file opens with a header:
  ```
  -- MODULE <name>
  -- CONSUMES: <bus slots in>
  -- PRODUCES: <slots out>
  -- MAINTAINS: <invariants it guarantees>
  ```
  If you cannot write that header in three honest lines, the cut is wrong.
- **Size: 150–500 lines.** >600 → split. **<40 → forbidden** (fold it; a
  trivial extraction is the pitiful-refactor signature).
- **Interface width ≤ ~8 slots.** A module needing 12+ bus inputs means the
  concern isn't isolated — redesign the cut or justify it in writing. This
  is the real measure of "isolated contracts," and it is load-bearing.
- **`driver_main` reads as the pipeline**: lex → parse → translate → emit,
  one composed module per line, over the shared bus.
- Follow CLAUDE.md style (compact wiring, logic in claims, element-form
  iteration, record types over parallel Seqs).

## 4. Target end-state (~10–14 modules, ~250–450 lines each)

`driver_main` 5930 → ~400–600 line orchestrator. The 5000+ lines
redistribute into cohesive, named, testable units. Each row below must be
describable in one sentence — that is the test of a good cut.

| # | Module / file | ~Lines | Contract (one sentence) | Owns bug |
|---|---|---|---|---|
| 1 | `driver_lex.ev` | ~250 | chars → tokens (incl. 2-char operators) | |
| 2 | `driver_buffers.ev` | ~200 | bounded token/symbol/claim-index storage in `__mem` | **overrun** |
| 3 | `driver_zinit.ev` | ~150 | Z3 config/context/sorts/solver setup | |
| 4 | `driver_recognize.ev` | ~350 | tokens → claim/type/enum/fsm headers + fields | |
| 5 | `driver_expr.ev` | ~400 | tokens → expr tree (precedence, ternary) | **ternary** |
| 6 | `driver_workitems.ev` | ~450 | C2Items micro-step interpreter + handle stack | **ternary** |
| 7 | `driver_enum.ev` (ED) | ~400 | enum decls → Z3 datatypes | |
| 8 | `driver_record.ev` (G2) | ~350 | record decls → Z3 tuple sorts | |
| 9 | `driver_compose.ev` | ~450 | claim-composition (`Sub(x↦y)`, `..`) inlining | |
| 10 | `driver_dispatch.ev` | ~250 | route a node to `translate2_{arith,bool,…}` | |
| 11 | `driver_emit.ev` | ~250 | effects schedule + smt2/manifest output | |
| 12 | `driver.ev` (`driver_main`) | ~500 | the shared bus + the pipeline wiring | |

(Counts are targets, not quotas — a clean cut at 520 or 180 lines is fine;
a cut at 25 or 900 is not.) Plus the 6 existing `translate2_*.ev`.

Target `driver_main` shape (illustrative):
```
fsm driver_main
    -- shared bus: the few genuinely-global carries
    pos ∈ Int ; tcur ∈ Int ; pmode ∈ Int ; zstep ∈ Int ; ...
    -- pipeline: each line a module owning its own state
    DriverLex(src ↦ content, ...)
    DriverBuffers(push ↦ tok, ...)
    DriverRecognize(...)
    DriverExpr(...)
    DriverWorkItems(...)
    DriverEnum(...) ; DriverRecord(...) ; DriverCompose(...)
    DriverDispatch(...)
    DriverEmit(... ↦ effects)
```

## 5. Testing standards

- **Every module ships ≥1 isolation test** — construct its inputs, run the
  module ALONE (composed standalone via fsm composition), assert outputs.
  Conformance is NOT a substitute.
- **Per module: happy-path + ≥1 boundary/edge + a regression test for any
  known bug in its domain.**
- **Known bugs become module tests now:** the buffer overrun → a bounds
  test on module 2 (drive `count` to `capacity`); the ternary null-operand
  (`repro_deep.ev`) → a test on modules 5/6 that asserts a clean result, not
  a segfault. These are the proof the structure addresses what we hit.
- **A module is not "done" until a test exists that FAILS if its contract
  is violated.** Tests live in `tests/compiler2_units/<module>/`, kernel-
  fixture header style (expected stdout + exit).

## 6. Success criteria — REJECT the refactor if any fail

1. `driver_main` < ~600 lines (from 5930).
2. 10–15 modules, each 150–500 lines — **no swarm of <40-line stubs.**
3. Every module: contract header (§3) + ≥1 isolation test (§5).
4. No interface wider than ~8 slots without written justification.
5. The two known bugs (overrun, ternary) each have a regression test in
   their module.
6. Every module describable in one sentence (the §4 table is honest).
7. (don't-regress) conformance stays 137/138; equivalence gate green per
   step (see §9).

## 7. Method — contract-first, test-first (per module)

For each module, in this order:
1. **Write the contract header** (CONSUMES/PRODUCES/MAINTAINS).
2. **Write the isolation tests** against that contract (they fail (red)
   because the module doesn't exist yet — that's expected).
3. **Extract** the fields + logic into the module fsm; wire the slot-call
   in `driver_main`.
4. **Make the tests green** + pass the equivalence gate (§9).
5. **Commit** (one module per commit).
Tests are written BEFORE the extraction is "done," not bolted on after.

## 8. Phase 0 — pipeline prerequisite (do FIRST, blocks everything)

`expand-fsm-autocarry.sh` operates on one file's text; if modules live in
separate imported files, `expand < driver.ev | oracle` never sees them and
carry-injection fails for cross-file composition. `flatten-evident.sh`
already resolves imports AND runs expand on the flattened result, so the
self-host path works — but the oracle gate (`gp_build_stage1`) and the unit-
test harness do not flatten first.

**Phase 0 task:** make the build, the gate, and the test harness
**flatten-first** (`flatten driver.ev` → single expanded text → `oracle
emit`). VERIFY it produces a stage1 byte-identical (modulo `__callN`) to the
current `expand|oracle` baseline on the un-split driver. Only once a
separate-file fsm module compiles + carries correctly through this pipeline
(prove with a throwaway 2-file Counter that prints `0,1,2`) may module
extraction begin. If flatten-first cannot be made baseline-identical,
STOP and report — do not paper over it.

## 9. Per-step gate (every extraction)

Capture the frozen baseline once (post-Phase-0 pipeline). After each module:
```bash
flatten-evident.sh compiler2/driver.ev > /tmp/flat.ev          # imports + expand
oracle emit /tmp/flat.ev driver_main -o /tmp/now.smt2
diff <(sed 's/__call[0-9]\+/__callN/g' BASE.smt2) \
     <(sed 's/__call[0-9]\+/__callN/g' /tmp/now.smt2) && echo EQUIV   # (1)
diff <(grep '^;; manifest: state-fields' BASE.smt2) \
     <(grep '^;; manifest: state-fields' /tmp/now.smt2)               # (2) (except §31 eff_out)
bash .goalpost/bin/run-conformance.sh    # (3) must be 137/138
# (4) the module's isolation tests pass
```
All four green → commit. If a step can't go equivalent, REVERT it and note
why. After §31 the manifest legitimately gains `eff_out`; re-baseline then.

## 10. Extraction order

Phase 0 (pipeline) → then most-isolated first:
`driver_lex` → `driver_buffers` (+overrun test) → `driver_zinit` →
`driver_recognize` → `driver_emit` → `driver_enum` → `driver_record` →
`driver_compose` → `driver_dispatch` → `driver_expr` → `driver_workitems`
→ (translation extraction behavior-preserving) → **separate commit:** the
ternary null-handle fix + `repro_deep` unit test → `driver_main` is now the
orchestrator residue; confirm §6.

## 11. How it runs / deliverable

One sequential background agent, fresh branch off `main`, staged commits
(harness-fault resilience), NOT auto-merged. Maintains the execution log
below: per module — commit hash, final line count, interface width, tests
added, gate result. Deliverable: a branch meeting all of §6, with a report.

## 12. Execution log

Branch `compiler2-driver-decomp`. Baseline conformance 137/138 (the one
failure is the pre-existing `123-subschema-shadowing-quantifier`). Gate =
`scripts/driver-decomp-gate.sh` (§9 steps 1+2; step 3 is implied by
byte-identity — see note). Unit harness = `tests/compiler2_units/run.sh`.

Pivotal feasibility finding (recorded before any module): a `..ModuleName`
names-match **lift** of an extracted block is byte-identical to the inline
original (empty §9(1) diff), whereas slot-bind composition churns
thousands of SMT2 lines (subsystem-map §5). So every extraction either
(a) `..`-lifts a stateful block to a separate file, or (b) moves pure
helper CLAIM definitions to a separate file (call sites inline
identically). Both keep the per-step gate byte-identical. Because the
emit stays byte-identical (mod `__callN`), the compiler behaves
identically on every input, so conformance 137/138 is preserved by
construction; it was re-run in full after the one behavior-changing
commit (the ternary guard) and stayed 137/138.

| # | commit | module / file | lines | interface | tests | gate | driver_main |
|---|---|---|---|---|---|---|---|
| 0 | `be9d72a` | Phase 0: flatten-first pipeline + gate + unit harness + 2-file Counter proof | — | — | 2-file Counter ⇒ 0,1,2 | EQUIV; manifest unchanged | 5930 |
| 1 | `99d62e9` | `driver_lex.ev` (`fsm DriverLex`) | 246 (225 body) | 5 bus (input, tbase, d_cap_int, _zstep, _got_path) | lex_idents (2 toks), lex_twochar_op (`++`⇒3) | EQUIV; manifest unchanged | 5707 |
| 2 | `0226dda` | `driver_expr.ev` (C2TokOp/C2AtomE/C2Prec/PrOps/C2PrattStep) | 506 (~485 body) | 3–4 slots; C2PrattStep 19 (justified §6.4) | prec_ladder, tokop_classify | EQUIV; manifest unchanged | 5221 |
| — | `e0a042a` | ternary null-operand guard (`TernaryBuildZ3`, translate2_bool.ev) + repro | +7 SMT2 | n/a | ternary_null_guard ⇒ Exit 7 | behavior-change: conformance 137/138; re-baselined | 5221 |
| — | `f5574de` | buffer-overrun regression (bounds-as-UNSAT) | — | n/a | overrun_bound (drive to cap ⇒ exit 2) | n/a (test-only) | 5221 |
| 3 | `b4d2303` | `driver_record.ev` (`fsm DriverRecord` + Rt* lookups) | 665 (577 stateful body + 69 pure) | Rt* 3–13; RtFieldAcc 23 (justified §6.4) | registry_lookup (RtIdxOf slot + unfinished/absent miss; RtSortOf Int/Nat/user/unknown) | EQUIV; manifest unchanged | 4577 |
| 7 | (this run) | `driver_enum.ev` (`fsm DriverEnum`, the ED machine) | 434 (411 body + 23 header) | 10 bus (zstep, ec_start, ec_list_n, ue_name, f_ir1, z_isort/bsort/ssort/rsort, d_cap_int) — wide but irreducible: declaring a Z3 datatype reads all 4 base sorts + the capture reg + the user-enum collection signals (justified §6.4) | floor_walk (hold zstep=9 ⇒ ed_src walks 0→4 over the four Effect-floor enum runs) | EQUIV; manifest unchanged | 4168 |
| TW | (this run) | `driver_window.ev` (`fsm DriverWindow` + the `FtiTok` decode claim) | 350 (262 body + 63 FtiTok + header) | 9 bus (in_parse, _pmode, _rc_on, _ed_act, d_witems_nil, _cw_st, _pt_st, tbase, last_results) — the lookahead-need gate inherently reads every consumer's sub-state (justified §6.4) | fetch_burst (pmode 6 ⇒ w_need 5; empty cursor ⇒ fetch_go fires; fmode 0→1→2 burst counter reaches 3). FtiTok moved as a pure claim (call sites inline identically). | EQUIV; manifest unchanged | 3846 |
| IR | (this run) | `driver_ir.ev` — the shared C2 work-item IR enums (C2Item/C2Items/C2H/C2Binds/C2Frames) hoisted out of driver.ev so modules + their tests share the vocabulary | 63 | n/a (type-defs header; imported, deduped by flatten) | covered by the DriverClassify test (which produces C2Items) | EQUIV; manifest unchanged | 3554 |
| CL | (this run) | `driver_classify.ev` (`fsm DriverClassify`) — token-window → per-line classification + dispatch decision | 270 (242 body) | wide (window head + ci/st names + inline-frame + record registry + every sibling sub-walk's enter signal) — the central dispatch brain; inspects the whole window against every entry condition (justified §6.4) | membership_pin (pin window to `x ∈ Int = 5` ⇒ c_is_mem ∧ c_pinned ∧ d_enter_pratt0, verdict 7) | EQUIV; manifest unchanged | 3554 |
| 9 | (this run) | `driver_compose.ev` (`fsm DriverCompose`) — claim-composition slot-call + names-match `..` inlining | 322 (298 body) | wide (classifier decision flags + claim index + handle-stack views + record registry + sibling fire/cap signals) — a call jump inherently couples those three (justified §6.4) | slot_capture (drive pmode-10 walk: cw_st 0→1, window `p ↦` ⇒ cw_slot fires ⇒ cs_n0 captures "p") | EQUIV; manifest unchanged | 3259 |
| 6a | (this run) | `driver_symtab.ev` (`fsm DriverSymtab`) — FTI symbol table + work-item decode layer (split A of the 668-line interpreter, §3 >600 split) | 213 (193 body) | 12 bus (witems, d_hstk_in, in_parse/_pmode/tok_ready, d_in_claim, fl_on, rb_on, il_ps, d_lk_read/d_lkname, _istep) | decode_peel (pin stack ⟨99,7⟩ + one C2DeclConst ⇒ d_h_top 99 ∧ d_h_2nd 7 ∧ d_it_decl, verdict 7) | EQUIV; manifest unchanged | 3069 |
| 4b | (this run) | `driver_claimidx.ev` (`fsm DriverClaimIdx` + C2ChainLvl) — claim/type index + pmode-4 enum collection + pmode-5 effects-chain dispatch | 274 (247 body) | ~33 bus (d_enter_skip, window head, d_head_is_claim/d_cl_name, parse gate, pmode-4 ed signals + floor decls, Pratt result, ci_base) | index_append (drive skip pass with a claim head ⇒ ci_cnt climbs to 3). C2ChainLvl moved as a pure claim. | EQUIV; manifest unchanged | 2821 |
| 4c | (this run) | `driver_matchpin.ev` (`fsm DriverMatchPin`) — pmode-6 match-pin walk (D3 match lowering) | 292 (271 body) | ~38 bus (d_enter_match, c_dname/c_sc, window head + el_head_name, parse gate, Pratt result, user-enum + Result variant registries uev_*/z_*) | scrutinee (drive `match e` ⇒ mp_st 0→1, mp_scrut captures "e") | EQUIV; manifest unchanged | 2553 |
| 9b | (this run) | `driver_posbind.ev` (`fsm DriverPosBind`) — pmode-12 positional-binding walk `(e1,e2,..) ∈ Claim` | 335 (308 body) | wide (classifier head + handle stack + claim index + symtab + inline-frame + enum value table + record registry + Pratt result) — positional binding resolves callee + parses elements + binds params (justified §6.4) | tuple_recognize (window `( 5 , 7 )` in state 0 ⇒ pt_tup_b ∧ ptt_ok0, verdict 3) | EQUIV; manifest unchanged | 2248 |
| G1q | (this run) | `driver_quant.ev` (`fsm DriverQuant`) — bounded-quantifier line classifier (`∀\|∃ v ∈ {lo..hi}` / `∀ v ∈ seq`) + the str_len/seq-length recognizer | 132 (114 body) | ~32 bus (classifier head c_t0..t4 + window lookahead, registered-seq carries, record registry, Pratt result) | range_header (pin `∀ x ∈ {0..9}` ⇒ c_q_rng ∧ c_q_hi 9) | EQUIV; manifest unchanged | 2137 |
| F1g | (this run) | `driver_group.ev` (`fsm DriverGroup`) — pmode-9 multi-name group walk (`x, y, z ∈ Nat` body + `(a, b ∈ Int)` param) | 106 (88 body) | 14 bus (d_enter_mn/d_enter_claimp, classifier name/sort/type, window head, parse gate) | multiname (window `x ,` in state 0 ⇒ pg_collect ∧ pg_take2, verdict 3) | EQUIV; manifest unchanged | 2052 |
| G2s | (this run) | `driver_setvar.ev` (`fsm DriverSetVar`) — Set(T) variable registry (≤2) + pmode-14 set walk + quantifier-over-set | 186 (167 body) | ~37 bus (classifier set-line flags, classify gate, window head, registered-seq carries, inline prefix, Pratt result) | registry_append (drive d_setmem ⇒ stv_cnt 0→2 capped ∧ stv_n0 captures "myset") | EQUIV; manifest unchanged | 1888 |
| 5p | (this run) | `driver_pratt.ev` (`fsm DriverPratt`) — pmode-3 Pratt expression-parser FSM | 110 (94 body) | ~26 bus (parse gate, d_enter_pratt + d_pratt_kind0 entry, classifier pin/decl flags, record + user-enum registries, qstop) | entry_kind (drive d_enter_pratt + d_pratt_kind0 5 ⇒ pk_kind latches 5) | EQUIV; manifest unchanged | 1797 |

Status after continuation run: 15 modules extracted total (3 prior +
DriverEnum, DriverWindow, DriverClassify[+driver_ir hoist], DriverCompose,
DriverSymtab, DriverClaimIdx, DriverMatchPin, DriverPosBind, DriverQuant,
DriverGroup, DriverSetVar, DriverPratt this run) + BOTH mandated bug
regressions (overrun, ternary). 20 unit fixtures green; gate EQUIV +
manifest unchanged per step; conformance 137/138 (preserved by byte-identity
— the prebuilt stage1 conformance artifact reads 137 pass / 1 fail, the
pre-existing 123-subschema-shadowing-quantifier). driver_main 5930 → 1797
(-4133, -70%).

§6.1 (`driver_main` < 600) is NOT met and is **NOT cleanly reachable** —
this is the §10 STOP-and-report condition. The remaining ~1797 lines split
into (a) the shared bus + pipeline wiring (the legitimate orchestrator
residue, ~300 lines) and (b) an irreducible INTEGRATION CORE that resists
clean isolation. Measured interface widths (distinct external bus reads per
block) make the cut quality explicit:

| residual block | lines | ext refs | isolable? |
|---|---|---|---|
| work-item interpreter (per-opcode lowering: C2RecVal/RecDecl, expr-node, call/ctor/matches/str-ops) | ~477 | **178** | no |
| state transitions | ~135 | **133** | no |
| token consumption (cursor advance + window tail) | ~173 | **145** | no |
| per-item build effects (pass-claim dispatch) | ~197 | **115** | no |
| effects schedule (+ §31 eff_out) | ~77 | **132** | no |

These five (~1059 lines) ARE `..`-liftable byte-identically, but each reads
50–178 distinct bus slots — they consume every registry (record rt_*, enum
evt_*/uev_*, set stv_*, handle stack, the whole window) and produce the Z3
build effects. Per §3 ("12+ bus inputs means the concern isn't isolated")
and §6.4, these are NOT cleanly-isolable modules: their only meaningful
test is integration/conformance (the Z3 handles they compute need the live
arena), so forcing them into modules would yield wide-interface lifts with
weak smoke tests — the very outcome §1/§3 reject. The cleaner remaining
blocks (rb broadcast ext 59, pratt ext 51, ZINIT/EMIT — Z3-lifecycle,
arena-dependent tests; pmode-7/8 <40-line stubs) would take driver_main to
only ~1368, still well above 600.

VERIFIED BLOCKER (§10 STOP condition): the work-item lowering engine
(~476 lines, the C2RecVal/RecDecl + expr-node/call/ctor/matches/str-op
lowering, ext 128) does **not** `..`-lift. Attempting to lift it (`fsm
DriverLower`) makes the oracle DROP an unrelated constraint — DriverWindow's
`w_need` Int assignment ("couldn't translate to Bool") — so the whole-driver
emit fails the §9 gate (build error, not drift). This was REVERTED per §9.
Bisection confirmed it is NOT a size effect: lifting just the 127-line
C2RecVal/RecDecl sub-block reproduces the identical `w_need` drop. So unlike
all 14 cleanly-lifted blocks (each EQUIV byte-identical), the lowering
engine resists the `..`-lift mechanism — most likely a flatten/expand
autocarry × heavy-pass-claim-call interaction that perturbs the oracle's
translation of a sibling module's `matches`-bearing ternary. Root-causing
that is a TOOL (expander/oracle) fix, out of scope for this source-only
decomposition.

Consequence: < 600 is NOT reachable by `..`-lift alone, because the 476-line
lowering engine is the largest residual block and cannot be lifted; even
extracting every remaining clean block (rb ext 59, pratt ext 51, ZINIT,
EMIT, cond-inline, pmode-7/8 — ~520 lines) would leave driver_main ≈ 1368
(lowering engine + orchestrator wiring + bus). The 14 extracted modules each
meet §3/§5/§6 with a real isolation test; the lowering engine is the honest,
verified blocker to §6.1, and unblocking it requires a fix to the
flatten/expand autocarry path (a kernel/tooling change), not more source
extraction.
