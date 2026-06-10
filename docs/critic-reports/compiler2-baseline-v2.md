# Evident critic — compiler2/ + scripts/passes/ baseline review v2

- **Date:** 2026-06-10
- **Commit reviewed:** `3417a783ded4efe4acd1fa86b4db9f46da99da34` (main)
- **Rulebook:** `docs/evident-purism.md` @ HEAD (includes V18
  numbered-variable-families, the §3.6 `_x`-without-base ruling, §6 /
  V16 / V17); calibration `docs/evident-purism-calibration.md`.
- **Prior baseline:** `docs/critic-reports/compiler2-baseline.md` (v1,
  `ce8d7b4`). Every finding below is marked **NEW** / **CARRIED** /
  noted **RESOLVED-since-v1** where v1 rows closed.
- **Scope:** surface text of all 35 `compiler2/*.ev` (~7,000 lines) AND
  all 4 `scripts/passes/*.ev` (~1,055 lines — now production surface).
  Pipeline acceptance, transforms, and performance are inadmissible /
  out of scope per the rulebook header.

## Verdict

`VIOLATIONS: 4 BLOCKER / 59 WARN / 48 NOTE` (111 findings)

No new-grammar constructions found — nothing `requires operator
ruling`. (The bracket alphabets `⟦⟧⟨⟩❲❳⦂❨❩❰❱` in scripts/passes live
inside string literals — wire data, not grammar.)

Zero V2 (silently-vacuous) findings: no capitalized `True`/`False`, no
bare Seq membership, no unwrapped `⇒`-consequent / boolean-`=`
near-misses anywhere, including the four pass programs. No
`_x`-without-base names (§3.6 BLOCKER class): every `_x` reference in
compiler2 and passes has its base; the `_t_` transient class v1 tracked
was retired by the 2026-06-10 operator ruling (HEAD commit `3417a78`,
renamed to ordinary `tk_*` variables).

### V18 classification rule applied (per operator instruction)

- **BLOCKER** — numbered family whose blessed bounded-Seq surface
  exists **today, proven in-repo** (Seq(Int)/Seq(record) decls incl.
  String-field records, element/range ∀, keyed access, variable
  indexing `xs[i]`, claim-call instantiation under range-∀, slot values
  as expressions).
- **WARN-with-named-gap** — the shape is genuinely blocked; each row
  names its gap from the open set: **Seq(enum) elements** (Effect/
  Token/Result-typed element families), **bounded Seq→cons folds**,
  **cons-list carry** (incl. Seq nested in enum payloads, e.g. `binds`
  inside `CFCons`), **bounded Seq-of-String carry** (plain
  `Seq(String)`; record-wrapped String fields are proven carried —
  `param_names ∈ Seq(PbStr)`, `rec_params ∈ Seq(RcParam)`).
- The unbounded-carried-String class in scripts/passes is
  **tolerated-tracked (no operator ruling)** — listed WARN with that
  annotation, never BLOCKER.

## Summary table

### By severity / violation class

| Class | Rule | BLOCKER | WARN | NOTE |
|---|---|---|---|---|
| Numbered families with blessed surface proven in-repo (`recdecl_ty/sort0..5`, `enum_h_field0..5`, `vf_t0..4`, `enum_fieldsym0..5`+`enum_fld_*` selection) | V18 (V3 applies) | 4 | — | — |
| Numbered families / hand-unrolled folds behind a **named gap** (bind-peel cons-carry ×4, Seq→cons ladders ×6, Seq→String folds ×3, Seq(enum) effect batches ×4, lib scan/parse unrolls ×3, min-of-positions nests ×1) | V18 / §3.1 | — | 19 | — |
| Value-selection / case-code ternary chains (incl. pass `phase` chains ×3, `card_items`, `RtIdxOf/RtSortOf` slot-unrolled pins) | §3.4 / V9 (+V12) | — | 32 | — |
| Unbounded carried String registries in scripts/passes — **tolerated-tracked, no operator ruling** | V15-class | — | 3 | — |
| Raw `LibCall` where `Build*` sugar exists (`__mem` read/write_long) | §2.8 | — | 2 | — |
| Index-in-interface (`recval_slot`) | §3.2 / V6 | — | 1 | — |
| Anemic type (`EqPlanSlot`) | V7 | — | 1 | — |
| Inline duplication of the `FtiNameEntry` pad-31 row encoding (`bcast_dup`; also inside `pskip_g*`, `callable_names` rows) | §3.5/§6.4 | — | 1 | — |
| Missing `-- MODULE` contract triple | §6.1 / V16 | — | — | 8 |
| Stale contract headers (names that no longer exist) | V16 | — | — | 4 |
| Size budgets (fsm >150, claim >25, file >350, entry-point mixing) | §6.2–6.3 / V17 | — | — | 16 |
| Comment violations (code-in-prose, narration, ruling-superseded rationale) | §3.7 / V13 | — | — | 11 |
| Naming (letter-code prefix families incl. pass `tk_*`, opaque abbreviations, Int-as-flag) | §3.6 / V5 / V14 | — | — | 5 |
| No-op sentinel effects (`time(0)`/`getpid` fillers, pass `eff_nop`) — grouped | judgment | — | — | 1 |
| Missing `Build*` sugar claims (free/calloc/tuple_sort/set_sort/empty_set/set_add) — grouped | §2.8 | — | — | 1 |
| God-record width / string-packed fields (`RecTypeEntry`, reduced since v1) | V8 | — | — | 1 |
| Int case codes where enums would carry names (repo-wide anchor; now incl. pass `phase` codes) | §2.6 | — | — | 1 |
| **Totals** | | **4** | **59** | **48** |

### RESOLVED since v1 (verified at this commit)

| v1 finding | Resolution |
|---|---|
| **S1 record half** — `rec0/1/2` + `acc0..acc5` (all 6 v1 BLOCKERs) | `recs ∈ Seq(RecTypeEntry)` + `accs ∈ Seq(Int)` (merge `b46f373`); driver_ir.ev:89 confirms |
| **W0** duplicate `setvar_slot` | Two distinctly-named keyed projections (`setvar_cur_slot` / `setvar_slot`), driver_setvar.ev:50–55 |
| matchpin `fold_tester1/2` chains (2 of 6 v1 WARNs) | Keyed-projection pin families + guarded floor pins, driver_matchpin.ev:268–279 |
| matchpin `fold_acc1/acc2/def_acc` chains (3 of 6) | Restructured to single conditionals (blessed §3.4); residual `mp_acc0` floor chain remains (WARN below) |
| symlookup `lookup_handle` floor chain | Guarded-pin family + `enum_values` pin pair, driver_symlookup.ev:43–47 |
| calllower `call1/2/3_items` chains | Guarded-pin families with covering ¬-defaults (`f1b8af4`); residual `card_items` count chain remains |
| exprdecomp `matches_tester` chain | Guarded-pin family, driver_exprdecomp.ev:81–86 |
| driver_lex `frac_pow` chain | Guarded-pin family, driver_lex.ev:122–131 |
| driver_enum `field_sort0/1/2` ×3 unroll + `field_ref*` duals | `field_slots ∈ Seq(EnumFieldSlot)` with element-∀ writes (:190–210); residual per-element type-name chain remains (WARN) |
| driver_broadcast `bcast_nf/bcast_fnames` slot chains | Direct variable indexing `recs[_bcast_slot]` (:36–37); module now the **claim-headers pilot** (headered fsm, :29) |
| driver_recval `recval_fnames/ftypes/nf/ctor` slot chains | Direct `recs[recval_slot]` indexing (:18–21); `recval_slot` V6 remains |
| Stale headers: driver_matchpin (`uev_*`/`mp_arm_*`), driver_enum (`evt_*`/`uev_*`) | Refreshed to current names |
| The `_t_` transient-namespace class | Operator ruling `3417a78`: `_` = carry namespace, full stop; renamed `tk_*` |

**Re-graded since v1 (severity change, not resolution):** the
`bind_n0..n5`/`bind_h0..h5` peel + its three consumers — v1 BLOCKER →
**WARN-with-named-gap (cons-list carry)** under the V18 rule: `binds`
must remain a `C2Binds` cons list because it travels inside `CFCons`
frame payloads (Seq-inside-enum-payload is not covered surface), so the
blessed registry conversion is genuinely blocked. §1.5 still applies:
the durable fix is the lowering/gap closure, never acceptance.

## The four systemic findings (read these first)

**S1(v2) — V18 BLOCKERs: numbered families whose Seq surface is already
proven in-repo.** Four sites spell a collection as suffixed scalars
while the *same codebase* (often the same file) demonstrates the
blessed form:

- `driver_record.ev:229–264` — `recdecl_ty0..5` + `recdecl_sort0..5`:
  twelve numbered transients filled by claim-call instantiation per
  literal index. driver_recval.ev:22–26 proves the exact blessed shape:
  `recval_fields ∈ Seq(RecField)`, `∀ k ∈ {0..5} : RtRecName(s ↦ …,
  i ↦ k, name ↦ recval_fields[k].name)` + `RtSortOf` per element.
  CARRIED (v1 graded WARN-unroll; V18 upgrades).
- `driver_enum.ev:74–79, 312–323` — `enum_h_field0..5`: a carried Int
  latch family with step-keyed guards. driver_record.ev:170/200 proves
  the blessed carried form: `recdecl_h_fields ∈ Seq(Int)`, `#≤6`,
  `∀ k : recdecl_h_fields[k] = (… ∧ _recdecl_field_i = k ? cap_int :
  _recdecl_h_fields[k])`. NEW.
- `driver_claimidx.ev:70–74, 112–116` — `vf_t0..t4`: a carried String
  family with cursor-keyed writes. driver_posbind.ev:172/195
  (`param_names ∈ Seq(PbStr)`) and driver_record.ev:87/159
  (`rec_params ∈ Seq(RcParam)`) prove carried String-field-record Seqs
  with identical ∀-cursor writes. NEW. (The `vfc_f1..f5` cons ladder
  consuming them stays WARN — Seq→cons fold gap.)
- `driver_buildeff.ev:59–93` — `enum_fieldsym0..5` (six claim
  instantiations differing only in `idx ↦ 0..5`) selected by the
  `enum_fld_sym_eff` chain, plus `enum_fld_istab`/`enum_fld_tabpos`
  chains over `field_slots[0..5]` literal indices. Blessed surface for
  both halves is in-repo: slot values are expressions
  (driver_record.ev:300 `addr ↦ … + (8 * recdecl_eff_fi)`) — one
  `VariantFieldSymStep(… idx ↦ enum_fld_k …)` replaces all six + the
  chain; and variable indexing (`recs[rec_slot]`, driver_record.ev:91)
  gives `field_slots[enum_fld_k].istab` directly. NEW.

**S2(v2) — the case-code chain class persists (32 WARNs, was 38).**
Same diagnosis as v1: discriminants are Int/String codes
(`parse_mode`, `zstep`, `enum_act/step`, `recdecl_st`, `_pratt_kind`,
tag tables, `phase` in all three pass programs) dispatched by
`d = k1 ? v1 : d = k2 ? v2 : …` chains or transition tables. The v1
reference rewrite stands: `match` over an enum / literal-Int patterns,
or a registry + keyed projection when the table is data. **The
direction of travel is right**: eight v1 chain sites converted to
guarded-pin families since v1 (see RESOLVED) — the pin-family form
(`(k = "x") ⇒ (out = v)` + covering ¬-default) is the §2.5 surface and
is the template for the remainder. scripts/passes adds three new
`phase` transition chains to this class.

**S3(v2) — scripts/passes: unbounded carried String registries
(tolerated-tracked).** The pass programs carry unbounded Strings as
state: source/registry accumulators (`ins_reg`, `inj_reg`, `cur_line`,
`body`, `claim_bare`) and packed-string registries with bespoke bracket
alphabets probed by `index_of` (`fsm_set`, `bare_reg`, `bind_reg`,
`slot_reg`, `ment_reg`, `carry_reg`, `work_list`, `acc_a/b`,
`hdr_pend`, `hdr_slots`). Under §1.2 these are not finite state
machines — unbounded data belongs on the tape — and the packed-string
registry is the encoding compiler2 keeps only as an FTI wire format.
**Per operator instruction this class is WARN, annotated
tolerated-tracked (no operator ruling yet)** — one row per file below.
The eventual ruling should choose between: FTI-buffer residency (the
compiler2 pattern) or a bounded-registry redesign.

**S4(v2) — the lib scan-chain unroll class (autocarry_lib).** Five scan
claims (`AcWsSkip`, `AcWordEnd`, `AcTrimEnd`, `AcWsBack`, `AcWordBack`)
are 16–64-deep hand-unrolled character scans; `AcParseInt` is a
7-arm digit-place fold; and `AcBodyScan3` is the worst single surface
in the tree — `AcBodyProbe`'s body **macro-expanded ~12× into two
single-line expressions thousands of characters long**, where three
sequential `AcBodyProbe` instantiations with intermediate locals are
available blessed surface (claims declare body locals — `RtFieldAcc`
does; instantiation chains are §2.2 composition). No blessed
bounded-scan/iteration surface exists for the 64-deep unrolls — named
gap (bounded scan/fold lowering, the W7 class); the `AcBodyScan3`
fusion, by contrast, has a real preferred alternative today.

---

## Per-file verdicts (worst first)

### scripts/passes/autocarry_lib.ev — 0 BLOCKER / 3 WARN / 2 NOTE — NEW (file new to review surface)

| line | severity | rule | status | finding |
|---|---|---|---|---|
| 350–352 | WARN (strong) | §3.5/§2.2 (judgment) | NEW | `AcBodyScan3` — `AcBodyProbe` macro-expanded ~12× into two ~4,000-char single-line expressions. Rewrite: three sequential `AcBodyProbe(… o0 ↦ p0, f0 ↦ f0 …)` instantiations with intermediate body locals — available surface today. |
| 11–194, 260–344 | WARN | §3.1 / V18-gap | NEW | `AcWsSkip`/`AcWordEnd`/`AcTrimEnd`/`AcWsBack`/`AcWordBack` — 16/48/64-deep unrolled char scans. Named gap: no bounded-scan/fold surface; the durable fix is a scan lowering (W7 class). Until it lands these are tolerated; do not re-spell. |
| 246–254 | WARN | V9/V18-gap | NEW | `AcParseInt` — 7-arm digit-count dispatch, each arm a place-value fold. Same named gap (bounded fold). |
| 6–9 | NOTE | — | NEW | The MAINTAINS measured-cost note (64-deep = 0.19 s, dated) is an allowed class-2 trap — recorded as correctly NOT flagged. |
| 78–143 etc. | NOTE | V17 | NEW | 65-line single-expression claims (budget ~25); the file is one module with a proper contract header (V16 satisfied — all four pass files have the triple). |

### scripts/passes/autocarry_analyze.ev — 0 BLOCKER / 3 WARN / 4 NOTE — NEW

| line | severity | rule | status | finding |
|---|---|---|---|---|
| 37–49, 271–334 | WARN | V15-class, **tolerated-tracked (no operator ruling)** | NEW | Unbounded carried Strings: `line`, `code`, `body` (accumulates each claim's whole body), `claim_bare`, `hdr_pend`, `hdr_slots`, `acc_a` — packed-string registries probed by `index_of` over the `⟨⟩⦂` alphabet. §1.2: unbounded data belongs on the tape. Listed WARN per the operator note; the ruling is the burndown item. |
| 255–264 | WARN | V9 (S2) | NEW | `phase` transition chain over codes 1/2/3/4/5/13/14 — a phase enum + `match` is the surface (the codes are even named in the header comment :25–26). |
| 76–108, 153–164 | WARN | §3.1 (judgment) | NEW | `tk_hdr_name` — a 33-line nested ternary computing min-of-four `index_of` positions by hand; `tk_base` likewise min-of-two. The file's own pattern is the fix: a lib claim (`AcNameEnd`-shaped, like `AcWordEnd`) owns the scan; the inline nest is unreadable and unreviewable. |
| 27–29 | NOTE | V13 | NEW | The `tk_`-naming rationale comment ("an un-carried local costs measurably less per tick") is superseded by the 2026-06-10 operator ruling (`3417a78`: intermediates are ordinary variables; carrying them is noise). Reconcile or delete — a rationale that contradicts a ruling misleads the next reader. |
| throughout | NOTE | V5/V14 | NEW | The `tk_` letter-code prefix family (~60 names) + opaque scratch names (`tk_h_b`, `tk_ce_gt`, `cur_a/cur_b`, `acc_a`) — hand-namespacing pending headers; 2–3-word names exist for most. |
| 33–365 | NOTE | V17 | NEW | fsm body ~330 lines (>150); file 365 (>~350). |
| 1–29 | NOTE | — | NEW | Recorded clean: the record wire-format table in the MODULE header is an exemplary class-3 cross-file fact; the contract triple is present. |

### scripts/passes/autocarry_fix.ev — 0 BLOCKER / 2 WARN / 2 NOTE — NEW

| line | severity | rule | status | finding |
|---|---|---|---|---|
| 25–37, 136–188 | WARN | V15-class, **tolerated-tracked (no operator ruling)** | NEW | Unbounded carried String registries: `fsm_set`, `bare_reg`, `bind_reg`, `slot_reg`, `ment_reg`, `carry_reg`, `work_list`, `ins_out`, `inj_out`, `acc_a/b` — the whole fixpoint state is packed strings over `⟨⟩❲❳⦂❨❩❰❱` probed by `index_of`/`str_contains`. Same annotation as analyze. |
| 128–135 | WARN | V9 (S2) | NEW | `phase` transition chain over codes 1/6/7/8/9/10/11 — phase enum + `match`. |
| 34–37, 54–126 | NOTE | V5/V14 | NEW | `cur_a`/`cur_b`/`acc_a`/`acc_b` multiplexed across four phases with phase-dependent meanings — opaque; per-walk names (`drain_cursor`, `insert_cursor`…) would read. Counted in the naming row. |
| 23–200 | NOTE | V17 | NEW | fsm body ~180 lines (>150). |

### scripts/passes/autocarry_apply.ev — 0 BLOCKER / 2 WARN / 1 NOTE — NEW

| line | severity | rule | status | finding |
|---|---|---|---|---|
| 23–35, 98–132 | WARN | V15-class, **tolerated-tracked (no operator ruling)** | NEW | Unbounded carried Strings: `ins_reg`/`inj_reg` (whole registries as two carried lines), `cur_line`, `ins_txt`/`inj_txt`, `splice_add`. Same annotation. |
| 91–96 | WARN | V9 (S2) | NEW | `phase` chain over codes 1/2/3/4 — phase enum + `match`. |
| 14–16 | NOTE | — | NEW | Recorded clean: the dated effects-leaf measured trap is allowed class 2; contract triple present; the registry consumption via carried cursors (`ins_no`/`ins_cur`) is honest sequential-stream reading, not flagged. |

### compiler2/driver_record.ev — 1 BLOCKER / 5 WARN / 1 NOTE

| line | severity | rule | status | finding |
|---|---|---|---|---|
| 229–264 | BLOCKER | V18/V3 (S1v2) | CARRIED (upgraded from v1 WARN) | `recdecl_ty0..5` + `recdecl_sort0..5` numbered claim-call families. Rewrite: `recdecl_tys ∈ Seq(RecField)`-shaped Seq + `∀ k ∈ {0..5} : RtRecName(s ↦ rec_cur_ftypes, i ↦ k, name ↦ …[k])` + `RtSortOf` per element — the exact driver_recval.ev:22–26 shape. |
| 270–293 | WARN | V18-gap (Seq(enum) elements) | CARRIED | `recdecl_write_fn0..5`/`fs0..5` Effect families + the driver.ev batch literal. Blocked on Seq(enum)-element construction; named gap. |
| 34–59, 70–73 | WARN | V12/V9 | CARRIED (restructured) | `RtIdxOf`/`RtSortOf` converted from chains to guarded-pin families (`ff09791`) but still slot-unrolled over `recs[0]/[1]/[2]`; `RtFieldAcc` :70–73 remains a 3-test chain. The element form (`∀ r ∈ recs : ((r.name = nm ∧ r.sort > 0) ⇒ …` + ¬∃ default) is the §2.5 surface. `RtIdxOf` returning a slot index is V6-adjacent (consumed by `recval_slot` — see W9). |
| 176–195, 306–313 | WARN | V9 (S2) | CARRIED | `recdecl_st_now` transition table + `recdecl_step_eff` dispatch over `recdecl_eff_st` codes — RD-step enum + `match`. |
| 125–143 | WARN | V18-gap (Seq→String fold) | CARRIED | `field_row_p0..p2` family + `field_rows_names/types` count-dispatched prefix folds over `_rec_param_n`. Named gap (bounded Seq→String fold); `rec_params` is already the Seq. |
| 75–313 | NOTE | V17 | CARRIED | fsm body ~239 lines (>150); raw `Z3_mk_tuple_sort`/`Z3_mk_set_sort` LibCalls (:294/:304) in the grouped §2.8 NOTE. The `recs` registry + ∀-cursor writes + the gap-documenting comment :79–81 are exemplary. |

### compiler2/driver_buildeff.ev — 1 BLOCKER / 2 WARN / 2 NOTE

| line | severity | rule | status | finding |
|---|---|---|---|---|
| 59–93 | BLOCKER | V18/V3 (S1v2) | NEW | `enum_fieldsym0..5` ×6 claim instantiations + `enum_fld_sym_eff`/`enum_fld_istab`/`enum_fld_tabpos` selection chains. Rewrite: one `VariantFieldSymStep(… idx ↦ enum_fld_k …)` (slot-value expressions are proven surface, driver_record.ev:300) and direct `field_slots[enum_fld_k].istab/.tabpos` (variable indexing proven, `recs[rec_slot]`). Also folds `enum_fsym0..5_now` (:49–54) once driver_enum's `enum_h_field*` converts (S1v2 row 2). |
| 195–216 | WARN | V9 (S2) | CARRIED | `enum_step_eff` act×step nested case-code dispatch — act/step enums + `match` (same fix as driver_enum's tables, which produce the codes). |
| 94–192 | WARN | V18-gap (Seq(enum) elements) | CARRIED | `enum_write_fn0..5`/`fs0..5`, `enum_read_acc0..5`, `acctab_w0..5` Effect families consumed as count-dispatched batch literals in driver.ev:949–978. Named gap. |
| 8–9, 219–227 | NOTE | V14 | CARRIED | `enum_wbatchf/fx/2/3u/acc/tw` — opaque abbreviations; words exist (`enum_write_batch_fields`…). |
| 23–321 | NOTE | V17 | CARRIED | fsm body ~300 lines (>150). The arena-layout table + the bisected-2026-06-10 ctor-array note are exemplary class-3/class-2 comments. Raw `calloc` LibCalls in the grouped §2.8 NOTE. |

### compiler2/driver_enum.ev — 1 BLOCKER / 3 WARN / 1 NOTE

| line | severity | rule | status | finding |
|---|---|---|---|---|
| 74–79, 312–323 | BLOCKER | V18/V3 (S1v2) | NEW | `enum_h_field0..5` carried latch family. Rewrite: `enum_h_fields ∈ Seq(Int)`, `#≤6`, `∀ k : enum_h_fields[k] = (is_first_tick ? 0 : (_enum_act = 1 ∧ _enum_step = 2 + k) ? cap_int : _enum_h_fields[k])` — the proven recdecl_h_fields shape (driver_record.ev:200). |
| 213–242 | WARN | V9 (S2) | CARRIED | `enum_act_now`/`enum_step_now` transition tables over act/step codes — the header comment :40–57 already names every state; the enum exists in prose. |
| 130–137 | WARN | V9 (S2) | CARRIED | `floor_decl_now`/`floor_name_now` chains over `_enum_src` codes — floor-source enum + `match`. |
| 200 | WARN | V9 (S2) | CARRIED (reduced) | `field_slots[k].sort` element-∀ write whose RHS is a 10-key type-name→sort chain. The ∀-write is blessed; the chain is the residual of the resolved `field_sort0/1/2` unroll — floor-sort registry + keyed projection. |
| 29–433 | NOTE | V17 | CARRIED | fsm body ~400 lines (>150); file 433 (>~350). CLEAN highlights: `field_slots` element-∀ writes (:190–198), `user_variants`/`enum_values` ∀-cursor + keyed `_e` writes (:426–433) — the §2.5 surfaces done right; header PRODUCES refreshed (v1 stale-header row RESOLVED). |

### compiler2/driver_claimidx.ev — 1 BLOCKER / 2 WARN / 2 NOTE

| line | severity | rule | status | finding |
|---|---|---|---|---|
| 70–74, 112–116 | BLOCKER | V18/V3 (S1v2) | NEW | `vf_t0..t4` carried String family with cursor-keyed writes. Rewrite: `vf_tys ∈ Seq(RcParam)`-shaped Seq, `#≤5`, `∀ k : vf_tys[k].pname = (… (vfield_more ∧ _vf_n = k) ? vfield_ty : _vf_tys[k].pname)` — the proven param_names/rec_params shape. |
| 117–127, 226–249 | WARN | V18-gap (Seq→cons folds) | CARRIED + NEW | `vfc_f1..f5` EVFieldList ladder (NEW row), `conj_items1..4` + `chain_lvl0..4` ladders with `efflit_branch_fold`/`chain_fold_items` count dispatch (CARRIED). Named gap (bounded Seq→cons program fold); do not re-spell. |
| 149, 168–254 | NOTE | V14 | CARRIED | `efflit_libcall ∈ Int` used as a 0/1 flag — `efflit_in_libcall ∈ Bool` reads as a predicate. |
| 28–281 | NOTE | V17 | CARRIED | fsm body ~250 lines (>150). `FtiNamedAppend` composition + the `claimidx_buf.count < 2048` bound-with-rationale are exemplary. |

### compiler2/driver.ev — 0 BLOCKER / 4 WARN / 5 NOTE

| line | severity | rule | status | finding |
|---|---|---|---|---|
| 932–1039 | WARN | V9 (S2) | CARRIED | The effects schedule: one ~70-arm chain over `zstep`/`plan_kind`/`emit_step`/`fetch` codes. §2.8's guarded-writer / phase-composed `++` surface exists; tolerated debt, but the chain is the lowered artifact's shape. |
| 949–978, 1021–1025 | WARN | V18-gap (Seq(enum) elements) | NEW (shape grew with multi-enum grammar) | `enum_field_n`-dispatched effect-batch literals (`enum_write_fn*`, `enum_read_acc*`, `acctab_w*`, `recdecl_write_*` batches) — count-dispatch selecting among ⟨…⟩ literals of numbered Effect families. Named gap. |
| 446–460, 559–579, 895–927 | WARN | V9 (S2) | CARRIED | `stack_pops`/`capture_pend`/`eff_step` select over `it_*` recognizers all derived from the `work_head` enum — `match work_head` is the direct surface. |
| 797–811, 852–855, 862–868, 883–887 | WARN | V9 (S2) | CARRIED | `eff_mkconst` sort table (incl. literal slot codes 10/11/12, 20/21/22 — V6-adjacent), `sel_idx_handle`, `real_denom` (still duplicating driver_lex's `frac_pow`, which is now a pin family — converge on one claim), `len_lit_handle`. |
| 1–144 | NOTE | V13 | CARRIED | Header still has Evident-shaped code in prose (`` `_name ∈ Type` `` :104, `` `cond ⇒ effects = ⟨…⟩` `` :135) — the lint false-positive class. |
| 1–187 | NOTE | V16 | CARRIED | No `-- MODULE driver_main` CONSUMES/PRODUCES/MAINTAINS triple; the scope essay is not the §6.1 format. |
| 187–1039 | NOTE | §6.2/V17 | CARRIED | Entry point mixes wiring (30 `..` lifts — legitimate wide-context driver) with field-level logic (`work_items` ~45-arm dispatcher, `consume_n`, `capture_pend`, the schedule). File 1,039 lines; fsm ~850. |
| 221, 224 | NOTE | §2.6 | CARRIED | Repo-wide Int-case-code anchor (`phase`, `parse_mode`, `zstep`, `_pratt_kind`…) — now also the pass `phase` codes. One systemic NOTE. |
| 873–877, 893 | NOTE | §2.8 / sentinel | CARRIED | Raw `Z3_mk_empty_set`/`Z3_mk_set_add`; `eff_filler` getpid — grouped NOTEs. |

### compiler2/driver_posbind.ev — 0 BLOCKER / 4 WARN / 2 NOTE

| line | severity | rule | status | finding |
|---|---|---|---|---|
| 222–269 | WARN | V18-gap (Seq→cons folds) + §3.5 | NEW | `pskip_g1..g5`/`pskip_decl_items` — the claim-headers seam ladder: 5-deep numbered cons fold whose every guard re-inlines (a) the pad-31 row encoding (`"|" ++ substr(_param_names[k].s ++ "␣"*31, 0, 31)`) — `FtiNameEntry`'s job — and (b) the 6-term `bind_n*` disjunction. Named gap for the fold; the two re-inlinings are fixable today (compose `FtiNameEntry`; one `name_bound`-style shared projection). |
| 155–157 | WARN | V18-gap (cons-list carry) | CARRIED (re-graded from v1 BLOCKER) | `elem_is_bound` disjunction over `bind_n0..n5` — bind-peel consumer; see the S-section re-grade. |
| 216–221 | WARN | V9 (S2) | NEW | `pskip_sort` — duplicates driver_classify's `line_sort` type-name→sort-code table verbatim minus one row. One shared claim (or the sort-code enum) covers both. |
| 341–358 | WARN | V9 (S2) | CARRIED | `pratt_done_items`/`pratt_done_consume` dispatch over `_pratt_kind` codes — kind enum + `match`. |
| 1–27 | NOTE | V16 | CARRIED | Header CONSUMES still names `ilb_n*` and `d_h_t*` — stale. |
| 30–358 | NOTE | V17 | CARRIED | fsm body ~330 lines (>150). NOT flagged: `bindzip_h*`, `tup_h*`, `tup_binds*` — argc/stack-depth arithmetic where position IS the subject (§3.1); `param_names`/`param_types` ∀-cursor writes are the blessed form. |

### compiler2/driver_compose.ev — 0 BLOCKER / 3 WARN / 1 NOTE

| line | severity | rule | status | finding |
|---|---|---|---|---|
| 58–108 | WARN | V18-gap (cons-list carry) | CARRIED (re-graded from v1 BLOCKER) | The `bind_n0..n5`/`bind_h0..h5`/`bind_tail0..4` peel — declaration site. `binds` rides inside `CFCons` payloads, so the Seq registry is blocked; named gap. §1.5: close the gap (Seq-in-enum-payload or a frame redesign), then convert peel + consumers together (W-B2 below). |
| 178–207 | WARN | V18-gap (Seq→cons folds) | CARRIED | `type_pin_g1..g5` cascade — same decl+pin program per slot, hand-unrolled over `slot_names` elements. Named gap. |
| 230–241 | WARN | V18-gap (Seq→String fold) | NEW | `hdr_join_now` — count-dispatched pipe-joined fold over `_param_names[0..5]` (the claim-headers join-set). Same fold gap as `callable_names`/`qset`. |
| 39–338 | NOTE | V17 | CARRIED | fsm body ~300 lines (>150). NOT flagged: `slot_h0..h5`, `binds_new`, `callw_pop_stack` — stack-depth/slot-order arithmetic, position IS the subject (§3.1, v1 ruling held). |

### compiler2/driver_window.ev — 0 BLOCKER / 4 WARN / 1 NOTE

| line | severity | rule | status | finding |
|---|---|---|---|---|
| 34–86 | WARN | V9 (S2) | CARRIED | `FtiTok` 47-arm tag→Token chain — `match tag` with literal Int patterns is the surface; the tag table itself is a legitimate class-3 wire fact. |
| 293–309 | WARN | V9 (S2) | CARRIED | `win_need` over 14 `parse_mode` codes — mode enum + `match`. |
| 101–245, 319–334 | WARN | V18-gap (Seq(enum) elements) | CARRIED | `lat_tag/pay0..7`, `res_int0..15`, `res_str0..7`, `is_str0..7`, `copy_eff0..7`, `dec_tok0..7`, `read_tag/pay0..7` — wire-position families (honest subjects) hand-unrolled 8/16-fold. Every chain in the family routes through enum-typed elements (`last_results[k]` matches, Token/Effect outputs) — the named Seq(enum) gap; `∀ k ∈ {0..7} : FtiTok(…)` over bounded Seqs once it closes. |
| 319–334, 193–200 | WARN | §2.8 | CARRIED | 16 raw `__mem read_long` + 8 conditional `__cstr copy`/getpid where `BuildMemReadLong` exists and is used in sibling files. |
| 88–335 | NOTE | V17 | CARRIED | fsm body ~250 lines (>150). `tok0..7`/`win_rest*` peels are honest lookahead positions — not flagged. |

### compiler2/driver_classify.ev — 0 BLOCKER / 2 WARN / 1 NOTE

| line | severity | rule | status | finding |
|---|---|---|---|---|
| 183–185 | WARN | V18-gap (cons-list carry) | CARRIED (re-graded from v1 BLOCKER) | `name_bound` disjunction over `bind_n0..n5` — bind-peel consumer. |
| 62–68 | WARN | V9 (S2) | CARRIED | `line_sort` type-name→sort-code chain (now duplicated by posbind's `pskip_sort`). |
| 31–226 | NOTE | V17 | CARRIED | fsm body ~195 lines (>150). The header-join `name_outer` gate (:188–190) and the enter-signal priority guards are clean (distinct-event guards, not V9 — v1 appendix (d) ruling held). |

### compiler2/lex_fti.ev — 0 BLOCKER / 3 WARN / 2 NOTE

| line | severity | rule | status | finding |
|---|---|---|---|---|
| 67–108 | WARN | V9 (S2) | CARRIED | `LexCharTag` — 33-arm char→tag chain + 33-term disjunction. `match`-on-literals or a registry. |
| 111–125 | WARN | V9 (S2) | CARRIED | `LexKeywordTag` — 14-arm keyword→tag chain. |
| 144–182 | WARN | V9 (S2) | CARRIED | `kind` selection + `tag0`/`count_n`/`last_n`/`prev_n` dispatch over `kind` codes — a LexKind enum + `match`. |
| 1–62 | NOTE | V16 | CARRIED | No `-- MODULE` contract triple (the prose covers the material; the format is absent). The encoding/host-contract tables are allowed class-3 wire facts. |
| 66–183 | NOTE | V17 | CARRIED | `LexCharTag` 43 lines, `LexFtiPlan` 57 lines (budget ~25). |

### compiler2/driver_recval.ev — 0 BLOCKER / 3 WARN / 0 NOTE

| line | severity | rule | status | finding |
|---|---|---|---|---|
| header, 18–21 | WARN | V6 | CARRIED | `recval_slot ∈ Int` ("the registry slot index 0/1/2") in the module interface + the `C2RecVal(String, Int)` payload — index-in-interface; the key (`recs[k].name`) is the domain identity. (The v1 slot *chains* here are RESOLVED — now direct `recs[recval_slot]` reads.) |
| 27–60, 64–87 | WARN | V18-gap (Seq→cons folds) | CARRIED | `recval_seg0..5` + `recval_items` arity dispatch + `recval_dtail5..1` — per-field cons-program fold, hand-unrolled. Named gap. The `recval_fields` ∀-instantiation block (:22–26) is the blessed S1v2 reference shape. |
| 61–63 | WARN | V9 (S2) | CARRIED | `recval_sortcodes` ∀-write whose RHS is a type-name→code chain — sort-code table (third copy: classify, posbind, here). |

### compiler2/driver_matchpin.ev — 0 BLOCKER / 2 WARN / 1 NOTE

| line | severity | rule | status | finding |
|---|---|---|---|---|
| 142–144 | WARN | V9 | CARRIED (reduced from 6 rows) | `mp_acc0` — IntResult/StringResult floor keys → else user registry value. The residual of the v1 fold-chain class; same fix (floor handles join the registry). `fold_tester1/2` are now exemplary pin families — RESOLVED. |
| 287–289 | WARN | V9 | CARRIED | `match_dtail` over `fold_arm_n` codes — arm-count enum + `match`. |
| 24–318 | NOTE | V17 | CARRIED | fsm body ~295 lines (>150). Header refreshed (v1 stale-header row RESOLVED). Hold chains, capture-or-carry views, and the new pin families are calibration-pinned clean. |

### compiler2/driver_expr.ev — 0 BLOCKER / 2 WARN / 2 NOTE

| line | severity | rule | status | finding |
|---|---|---|---|---|
| 20–52, 55–84, 98–121 | WARN | V9 (S2) | CARRIED | `C2TokOp`/`C2AtomE`/`C2Prec` — chains over `matches`-derived booleans from one discriminant; a single `match t`/`match o` says the same without 14 intermediates. |
| 427–433 | WARN | V9 (S2) | CARRIED | `ps_red_e`/`ps_red_base` select over `ps_top` (an enum) — `match ps_top`. (`ps_call_e` argc selection honest-positional — not flagged.) |
| 171–467 | NOTE | V17 | CARRIED | `C2PrattStep` ~297-line pure claim (budget ~25); file 468 lines (>350). |
| 21–52, 99–121, 177–264 | NOTE | V5/V14 | CARRIED | `c2to_`/`c2a_`/`pc_`/`ps_` prefix families — claim-scoped, partially excused; retire as headers land. The oracle-precedence-divergence comment (:93–97) is an allowed measured fact. |

### compiler2/driver_symlookup.ev — 0 BLOCKER / 1 WARN / 1 NOTE

| line | severity | rule | status | finding |
|---|---|---|---|---|
| 25–34 | WARN | V18-gap (cons-list carry) | CARRIED (re-graded from v1 BLOCKER) | `bound_found` disjunction + `bound_handle` selection over `bind_n0..n5` — the principal bind-peel consumer. |
| 1–18 | NOTE | V16 | CARRIED | Header still names `ilb_n*/ilb_h*` and `d_lk_pfx_*` — stale. The `lookup_handle` pin family (:43–47) is now exemplary — v1 WARN RESOLVED. |

### compiler2/driver_calllower.ev — 0 BLOCKER / 1 WARN / 1 NOTE

| line | severity | rule | status | finding |
|---|---|---|---|---|
| 67–76 | WARN | V9 (S2) | CARRIED (reduced) | `card_items` over `card_count` codes — the residual chain; `call1/2/3_items` are now guarded-pin families (RESOLVED, `f1b8af4`). |
| 1–18 | NOTE | V16 | CARRIED | Header PRODUCES still says `d_sl_*/d_card_*` — stale. The `set_vars` keyed projections (:54–62) are exemplary. |

### compiler2/driver_pratt.ev — 0 BLOCKER / 1 WARN / 1 NOTE

| line | severity | rule | status | finding |
|---|---|---|---|---|
| 34–47 | WARN | V18-gap (Seq→String fold) + §3.5 | CARRIED | `callable_names` — nine builtin rows + `variant_names` + three guarded `recs[k]` rows concatenated by hand, re-inlining the pad-31 encoding. Elements are the subject (order immaterial) — the dishonest side of §3.1; fold lowering or registry probe is the fix. |
| 136–137 | NOTE | V13 | CARRIED | Evident code-in-prose (`` `offset_pos ∈ IVec2 = <expr>` ``). The mskim walk and hold chains are clean. |

### compiler2/driver_setvar.ev — 0 BLOCKER / 1 WARN / 1 NOTE

| line | severity | rule | status | finding |
|---|---|---|---|---|
| 116–130 | WARN | V18-gap (Seq→cons folds) | CARRIED | `qset_seg2/seg1/qset_items` — per-element cons fold over `qset_names[0..2]` with count guards. Named gap. |
| 1–18 | NOTE | V16 | CARRIED | Header PRODUCES still says `stv_n*/stv_k*/stv_e*/stv_c*` — stale. `set_vars` + `setvar_cur_name` remain the model registry; the v1 duplicate-`setvar_slot` defect is RESOLVED (:50–55, two named outputs). |

### compiler2/driver_lex.ev — 0 BLOCKER / 1 WARN / 1 NOTE

| line | severity | rule | status | finding |
|---|---|---|---|---|
| 133–138, 216–221 | WARN | §2.8 | CARRIED | Raw `__mem write_long` ×8 where `BuildMemWriteLong` exists and is used by siblings. (`frac_pow` is now a pin family — v1 WARN RESOLVED.) |
| 21–240 | NOTE | V17 | CARRIED | fsm body ~220 lines (>150). The `tok_buf.count < 65534` bound + its measured rationale are exemplary. |

### compiler2/translate2_bool.ev — 0 BLOCKER / 2 WARN / 2 NOTE

| line | severity | rule | status | finding |
|---|---|---|---|---|
| 57–75 | WARN | V9 (S2) | CARRIED | `BoolCmpBuildZ3` eff chain over `is_*` booleans from the `Op` enum — `match op` applies verbatim. |
| 84–90 | WARN | V9 (S2) | CARRIED | `BoolNaryBuildZ3` — as above. |
| 1–36 | NOTE | V16 | CARRIED | No `-- MODULE` contract triple. |
| 75, 90 | NOTE | sentinel | CARRIED | `time(0)` no-op sentinels — grouped repo-wide NOTE (N7). |

### compiler2/translate2_ctor.ev — 0 BLOCKER / 2 WARN / 1 NOTE

| line | severity | rule | status | finding |
|---|---|---|---|---|
| 186–198 | WARN | V9 (S2) | CARRIED | `FieldSortSlot` — type-name keys → sort handles. |
| 260–267 | WARN | V9 (S2) | CARRIED | `EffectCtorArity` — name keys decomposed to `eca_*` booleans then chained. |
| 1–42 | NOTE | V16 | CARRIED | No contract triple. `VariantFieldCount`/`VariantFieldType` cons-peels are honest (callers wire `idx ↦ k`); the sort_refs width caveat is exemplary class 3. |

### compiler2/translate2_seq.ev — 0 BLOCKER / 1 WARN / 1 NOTE

| line | severity | rule | status | finding |
|---|---|---|---|---|
| 105–143 | WARN | V9 (S2) | CARRIED | `StrOpBuildZ3` — 9 String keys → LibCalls; no `match` for strings — shared op enum with the emitters is the durable fix. Tolerated debt. |
| 1–54 | NOTE | V16 | CARRIED | No contract triple; the legacy-SMT mapping table is an allowed class-3 wire fact. |

### compiler2/translate2_record.ev — 0 BLOCKER / 1 WARN / 1 NOTE

| line | severity | rule | status | finding |
|---|---|---|---|---|
| 41 | WARN | V7 | CARRIED | `type EqPlanSlot(addr, in_range, is_last, needs_and)` — bodyless; `is_last ⇒ in_range` is statable. |
| 1–37 | NOTE | V16 | CARRIED | No contract triple. The probe's `plan[0..2]` rows wire `idx ↦ 0/1/2` — honest-positional. |

### compiler2/translate2_match.ev — 0 BLOCKER / 0 WARN / 2 NOTE

| line | severity | rule | status | finding |
|---|---|---|---|---|
| 1–72 | NOTE | V16 | CARRIED | 72 of 101 lines are header prose without the contract triple. |
| 25–35 | NOTE | V13 | CARRIED | Evident `match` block quoted in prose — the flattened-source false-positive class (the SMT column is fine). Claims themselves are exemplary one-LibCall builders. |

### compiler2/driver_ir.ev — 0 BLOCKER / 0 WARN / 3 NOTE

| line | severity | rule | status | finding |
|---|---|---|---|---|
| 89–91 | NOTE | V8 | CARRIED (reduced) | `RecTypeEntry` now 9 fields with a real body (`0 ≤ n_fields ≤ 6`, `#accs ≤ 6`) — the v1 `acc0..acc5` BLOCKER is RESOLVED; the packed `fnames`/`ftypes` string-registries remain (retire with a `fields ∈ Seq(RecField)` once Seq-in-record lands); `sort > 0 ⇒ ctor > 0` still statable. The GAP comment (:85–88) is an exemplary class-3 note. |
| 1–8 | NOTE | V16 | CARRIED | `-- MODULE driver_ir` lacks the CONSUMES/PRODUCES/MAINTAINS triple. |
| 99–121 | NOTE | V13 | CARRIED | Membership-shaped code in prose (`` `enum_values ∈ Seq(EnumVariantVal)` `` etc.) — the lint false-positive class. `FtiBuffer`/`Z3SolverCtx`/`Z3Sorts`/`Z3Numerals` bodies remain the exemplary type-invariant models. |

### compiler2/driver_emit.ev — 0 BLOCKER / 0 WARN / 2 NOTE

| line | severity | rule | status | finding |
|---|---|---|---|---|
| 18–23 | NOTE | V13 | CARRIED | §-trail narration citing an external notes doc ("the §31 eff_out indirection", "§12", "§6.3") — keep the one-line decision, drop the trail. |
| 55–57 | NOTE | §2.8 | CARRIED | Raw `free` ×3 — no `BuildFree`; grouped sugar NOTE (N6). |

### compiler2/driver_broadcast.ev — 0 BLOCKER / 1 WARN / 1 NOTE

| line | severity | rule | status | finding |
|---|---|---|---|---|
| 58–60 | WARN | §3.5 | CARRIED | `bcast_dup` hand-inlines the pad-31 row encoding — compose `FtiNameEntry(name ↦ bcast_dname, entry ↦ …)` then probe. |
| 19–23 | NOTE | V13 | CARRIED | "INTERFACE WIDTH … JUSTIFIED per §6.4" review-status narration. **CLEAN highlight:** this is now the headered-fsm pilot (:29) — bare-mention composition against a 10-slot header; the v1 `bcast_nf/bcast_fnames` slot chains are RESOLVED (direct `recs[_bcast_slot]` indexing). |

### compiler2/driver_exprdecomp.ev — 0 BLOCKER / 0 WARN / 1 NOTE

| line | severity | rule | status | finding |
|---|---|---|---|---|
| 66–69 | NOTE | V13 | CARRIED | "the legacy dropped-Exit(3+4) class is impossible" — history narration; keep the collision-freedom fact. `matches_tester` and `ctor_decl` pin families are now exemplary (v1 WARN RESOLVED). |

### compiler2/driver_symtab.ev — 0 BLOCKER / 0 WARN / 1 NOTE

| line | severity | rule | status | finding |
|---|---|---|---|---|
| 20–198 | NOTE | V17 | CARRIED | fsm body ~180 lines (>150). `h_top..h_6th`/`h_tail1..6` stack peel = depth IS the interface (§3.1, v1 ruling held); the `it_*` recognizers are blessed `matches`; `recval_slot` decode counted under driver_recval's V6 row. |

### compiler2/driver_quant.ev / driver_group.ev / driver_litmem.ev — 0 / 0 / 1 NOTE each

| file:line | severity | rule | status | finding |
|---|---|---|---|---|
| driver_quant.ev:11–14 | NOTE | V13 | CARRIED | `` `∀\|∃ v ∈ {lo..hi} : body` ``, `` `#seq = k` `` in MAINTAINS prose. Otherwise CLEAN. |
| driver_group.ev:11–13 | NOTE | V13 | CARRIED | `` `claim main(a, b ∈ Int, ok ∈ Bool)` ``, `` `x, y, z ∈ Nat` `` in prose. Otherwise CLEAN (`group_done_mode`/`group_ty_exit_mode` are 2-test prioritized boolean guards, not V9). |
| driver_litmem.ev:12, 46–56 | NOTE | V13 | CARRIED | `` `s ∈ Seq(Int)` `` and `` `x ∈ {a,b,…}` `` quoted in prose — the specific lint false-positive example. Otherwise CLEAN. |

### CLEAN files

- **driver_guard.ev** — hold chains, `matches` recognizers, claim
  composition, correct module header. (The `≠ 0` at :53 is perf-scope —
  never flagged.) CARRIED CLEAN.
- **driver_zinit.ev** — the blessed latch-bank idiom end to end;
  record-typed carries with invariants in driver_ir's type bodies.
  CARRIED CLEAN.
- **driver_input.ev** — contract present; hold chains; `last_results`
  wire positions documented in-header. CARRIED CLEAN.

---

## Burndown (severity × blast radius; each box one work item)

**BLOCKER — the V18 proven-surface families (S1v2)**

- [ ] **B1.** driver_buildeff.ev:59–93 — replace `enum_fieldsym0..5` +
  `enum_fld_sym_eff/istab/tabpos` with one
  `VariantFieldSymStep(idx ↦ enum_fld_k)` call and direct
  `field_slots[enum_fld_k]` indexing. Smallest diff, kills 1 BLOCKER +
  ~35 lines; no gap involved.
- [ ] **B2.** driver_enum.ev:74–79/312–323 — `enum_h_field0..5` →
  `enum_h_fields ∈ Seq(Int)` (recdecl_h_fields shape); then fold
  driver_buildeff's `enum_fsym0..5_now` views.
- [ ] **B3.** driver_claimidx.ev:70–74/112–116 — `vf_t0..4` →
  carried `Seq(RcParam)`-shaped registry (param_names shape). The
  `vfc_f*` cons ladder stays (gap) but reads `vf_tys[k]`.
- [ ] **B4.** driver_record.ev:229–264 — `recdecl_ty/sort0..5` →
  Seq + ∀-instantiated `RtRecName`/`RtSortOf` (recval_fields shape).
  The Effect write batch stays numbered under the Seq(enum) gap.

**WARN — gap closures that retire whole classes (file the plan, never re-spell)**

- [ ] **W1.** Close the **cons-list carry / Seq-in-enum-payload** gap
  (or redesign frames) → retires the `bind_n0..n5` peel + 3 consumers
  (compose/symlookup/classify/posbind) — the re-graded v1 B2.
- [ ] **W2.** The **bounded Seq→cons / Seq→String fold** lowering →
  retires `type_pin_g*`, `hdr_join_now`, `pskip_g*`, `vfc_f*`,
  `conj/chain` ladders, `recval_seg/dtail`, `qset_seg*`,
  `callable_names`, `field_rows_*`, and autocarry_lib's scan/parse
  unrolls (S4). Largest WARN-class blast radius in the tree.
- [ ] **W3.** The **Seq(enum) elements** gap → retires the
  effect-batch families (buildeff write/read/acctab, record
  recdecl_write_*, window lat/res/dec/copy/read, driver.ev batch
  dispatch).
- [ ] **W4.** **Operator ruling for the scripts/passes
  unbounded-carried-String class** (S3): FTI-tape residency vs bounded
  registries. Until ruled: tolerated-tracked, 3 WARN rows.
- [ ] **W5.** Enums for the pervasive case codes (`phase` ×3 in passes,
  `parse_mode`, `enum_act/step/src`, `recdecl_st`, `_pratt_kind`,
  lex `kind`) → unlocks `match` rewrites for ~20 of the 32 V9 chains;
  continue the pin-family conversion pattern (8 sites done since v1)
  for the key-dispatch rest (`mp_acc0`, `card_items`, sort tables —
  and converge the three copies of the type-name→sort-code table:
  classify:62, posbind:216, recval:63, plus `real_denom`/`frac_pow`).
- [ ] **W6.** autocarry_lib.ev:350–352 — rewrite `AcBodyScan3` as three
  `AcBodyProbe` instantiations (available surface today; no gap).
- [ ] **W7.** autocarry_analyze.ev:76–108 — extract the min-of-positions
  scan into a lib claim (`AcNameEnd`); same for `tk_base`.
- [ ] **W8.** Raw `LibCall` → sugar: driver_lex write_long ×8,
  driver_window read_long ×16 / cstr-copy ×8.
- [ ] **W9.** driver_recval `recval_slot` (V6) — key `C2RecVal`/
  `C2RecDecl` on the type name once practical.
- [ ] **W10.** Pad-31 inlining → compose `FtiNameEntry`:
  driver_broadcast:58, driver_posbind pskip guards, driver_pratt
  callable_names.
- [ ] **W11.** translate2_record `EqPlanSlot` — state `is_last ⇒
  in_range` (V7).

**NOTE — organization and hygiene**

- [ ] **N1.** Add `-- MODULE` triples: driver.ev,
  translate2_{match,record,seq,bool,ctor}.ev, lex_fti.ev, driver_ir.ev.
- [ ] **N2.** Refresh stale headers: driver_symlookup (`ilb_*`,
  `d_lk_pfx_*`), driver_posbind (`ilb_n*`, `d_h_t*`), driver_setvar
  (`stv_*`), driver_calllower (`d_sl_*`, `d_card_*`). (matchpin + enum
  done since v1.)
- [ ] **N3.** Size budgets (V17, 16 rows): driver_main ~850,
  DriverEnum ~400, DriverPosBind ~330, autocarry_analyze ~330,
  DriverCompose ~300, DriverBuildEff ~300, DriverMatchPin ~295,
  C2PrattStep 297-line claim, DriverWindow ~250, DriverClaimIdx ~250,
  DriverRecord ~239, DriverLex ~220, DriverClassify ~195,
  DriverSymtab ~180, autocarry_fix ~180; files driver.ev 1,039 /
  driver_expr.ev 468 / driver_enum.ev 433 / autocarry_analyze.ev 365 /
  driver_posbind.ev 358. Decompose into carry-owning sub-fsms, not
  banners.
- [ ] **N4.** §6.2 driver.ev entry-point mixing — `work_items`
  dispatcher / `consume_n` / `capture_pend` are the extraction
  candidates (driver_emit's header documents the one deliberate
  exception).
- [ ] **N5.** Comment hygiene (V13): strip Evident-shaped code from
  prose (driver.ev header, driver_ir, translate2_match, driver_quant,
  driver_group, driver_litmem, driver_pratt); drop narration
  (driver_emit §-trail, driver_exprdecomp "legacy", driver_broadcast
  JUSTIFIED banner); **reconcile autocarry_analyze:27–29 with the
  `3417a78` ruling**.
- [ ] **N6.** Missing `Build*` sugar (§2.8): BuildFree, BuildCalloc,
  BuildZ3MkTupleSort/SetSort/EmptySet/SetAdd, a cstr-copy wrapper.
- [ ] **N7.** The no-op sentinel (`time(0)`/`getpid`/pass `eff_nop`) —
  needs a plan note (kernel `NoOp` effect or caller-side gating); no
  blessed surface exists today.
- [ ] **N8.** Naming: `efflit_libcall` Int-flag → Bool; `enum_wbatch*`
  → words; pass `tk_*`/`cur_a/b` families and `ps_/pc_/c2to_/c2a_`
  retire as headers land.
- [ ] **N9.** driver_ir `RecTypeEntry`: statable `sort > 0 ⇒ ctor > 0`;
  packed fnames/ftypes retire with a `fields ∈ Seq(RecField)` field
  when Seq-in-record lands.

---

## Honesty appendix (judgment calls not covered by calibration)

(a) **The V18 BLOCKER/WARN line** was drawn at "proven in-repo": a
family is BLOCKER only when the identical shape (carry pattern, write
guards, claim instantiation) demonstrably exists in compiler2 today;
everything else cites one of the four named gaps. Notably this
**re-graded the v1 bind-peel BLOCKERs to WARN** (cons-list carry gap:
`binds` rides inside `CFCons` payloads) — severity moved, the §1.5
obligation did not. (b) **Batch vs selected** families: a numbered
Effect family consumed as a variable-length `⟨…⟩` batch is gap-blocked
(Seq(enum) elements → WARN); a family consumed one-per-tick through a
selection chain has the slot-expression surface available → BLOCKER
(driver_buildeff). (c) The scripts/passes **packed-string registries**
were folded into the unbounded-carried-String rows rather than flagged
separately as V3 encodings, since the operator note covers the class as
one tracked debt. (d) Pass `phase` chains graded V9 like compiler2's
transition tables (v1 precedent), not blessed hold chains — the
discriminant is one carried code tested against successive constants.
(e) `AcBodyScan3` graded WARN, not BLOCKER: composition of
`AcBodyProbe` is available surface, but the fusion is valid grammar and
not a lowered artifact leaking back — strongest-possible WARN with a
today-fix. (f) Prioritized distinct-event boolean guards
(`pratt_enter_*`, classify enter signals, lex_fti `kind` *selection*)
remain NOT flagged per v1 appendix (d); single conditionals and
capture-or-carry views remain blessed throughout.
