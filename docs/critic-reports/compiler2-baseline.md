# Evident critic — compiler2/ baseline review

- **Date:** 2026-06-10
- **Commit reviewed:** `ce8d7b4119781ce85c9e4de2486f9181a47e3471`
- **Rulebook:** `docs/evident-purism.md` @ `ce8d7b4` (2026-06-10 — includes §3.6
  naming shapes, §6 organization/size, V16/V17);
  calibration `docs/evident-purism-calibration.md` @ `593c75a`.
- **Scope:** surface text of all 35 `compiler2/*.ev` files (6,784 lines).
  Pipeline acceptance, transforms, and performance are inadmissible/out of
  scope per the rulebook header.

## Verdict

`VIOLATIONS: 6 BLOCKER / 51 WARN / 46 NOTE` (103 findings)

No new-grammar constructions found — nothing `requires operator ruling`.
Zero V2 (silently-vacuous) findings: no capitalized `True`/`False`, no bare
Seq membership, no unwrapped `⇒`-consequent/boolean-`=` near-misses were
found; every `∀ e ∈ xs`/`∃ e ∈ xs` instance is the blessed keyed-projection
or keyed-write surface (calibration case 2 pins these as exemplary).

## Summary table

### By severity / violation class

| Class | Rule | BLOCKER | WARN | NOTE |
|---|---|---|---|---|
| Numbered-slot scalar families in source (`bind_n0..n5`, `rec0/1/2`, `acc0..acc5`) | §3.6 / V3 (V1 territory) | 6 | — | — |
| Value-selection / case-code ternary chains | §3.4 / V9 | — | 38 | — |
| Hand-unrolled bounded folds (no blessed fold surface) | §3.1 / V3-adjacent *(judgment)* | — | 6 | — |
| Raw `LibCall` where `Build*` sugar exists | §2.8 *(judgment)* | — | 2 | — |
| Index-in-interface (`recval_slot`, slot-coded sorts) | §3.2 / V6 | — | 1 | — |
| Duplicate competing definition (`setvar_slot`) | *(judgment)* | — | 1 | — |
| Inline duplication of the FtiNameEntry wire encoding | §3.5/§6.4 *(judgment)* | — | 1 | — |
| Anemic type (`EqPlanSlot`) | V7 *(judgment)* | — | 1 | — |
| Calibration-pinned matchpin chains (subset of V9 above) | V9 | — | (6) | — |
| Missing `-- MODULE` contract header | §6.1 / V16 | — | — | 8 |
| Stale contract header (names that no longer exist) | V16 *(judgment)* | — | — | 6 |
| Size budgets (fsm >150, claim >25, file >350, entry-point mixing) | §6.2–6.3 / V17 | — | — | 15 |
| Comment violations (Evident code-in-prose, narration) | §3.7 / V13 | — | — | 10 |
| Naming (letter-code prefix families, opaque abbreviations, Int-as-flag) | §3.6 / V5/V14 | — | — | 3 |
| No-op sentinel effects (`time`/`getpid` fillers) | *(judgment)* | — | — | 1 |
| Missing `Build*` sugar for repeated syscalls (free/calloc/tuple_sort/…) | §2.8 | — | — | 1 |
| God-record width (`RecTypeEntry`, 14 fields) | V8 | — | — | 1 |
| Int case codes where enums would carry names (repo-wide) | §2.6/§3.4 *(judgment)* | — | — | 1 |
| **Totals** | | **6** | **51** | **46** |

### Top 5 files by finding count

| File | BLOCKER | WARN | NOTE | Total |
|---|---|---|---|---|
| driver_matchpin.ev | 0 | 6 | 2 | 8 |
| driver.ev | 0 | 3 | 5 | 8 |
| driver_record.ev | 1 | 3 | 1 | 5 |
| driver_window.ev | 0 | 4 | 1 | 5 |
| lex_fti.ev / driver_calllower.ev (tie) | 0 | 3–4 | 1–2 | 5 |

## The two systemic findings (read these first)

**S1 — the numbered-slot registry complex (all 6 BLOCKERs).** Two
collections are stored/peeled as hand-written per-slot scalar families —
exactly the shape §3.6 bans from source ("per-slot scalars never appear in
source — they are transform output; their presence in a .ev file means
lowered artifact leaked back into surface"):

- the record-type registry `rec0`/`rec1`/`rec2` (declared driver_record.ev,
  with `acc0..acc5` numbered accessor *fields* in `RecTypeEntry`,
  driver_ir.ev), consumed via slot-selection chains in 6+ files;
- the inline-frame bind table peeled to `bind_n0..n5`/`bind_h0..h5`
  (declared driver_compose.ev), consumed by membership disjunctions in
  driver_symlookup/driver_classify/driver_posbind.

The blessed surface is a bounded Seq-of-records registry with keyed
projections — and **the same codebase proves it works**: `set_vars ∈
Seq(SetVar)` (driver_setvar), `user_variants ∈ Seq(UserEnumVariant)` and
`enum_values` (driver_enum), and `recdecl_h_fields ∈ Seq(Int)` (in
driver_record itself) all use the §2.5 registry read/write surfaces over
the same pre-oracle bounded-Seq lowering. Where the carried-registry gap
(roadmap note `f103caa`: "carried registries resist records/Seq under
frozen oracle") still blocks a conversion, the §1.5 rule applies: the fix
is the lowering, never acceptance of the scalar encoding. Reference
rewrite (used by all S1 rows):

```evident
recs ∈ Seq(RecTypeEntry)
#recs ≤ 3
∀ k ∈ {0..2} : recs[k].name = (is_first_tick ? "" : (rec_start ∧ _rec_count = k) ? decl_name : _recs[k].name)
-- keyed read replacing every rec0/rec1/rec2 selection chain:
∀ r ∈ recs : ((r.name = ty ∧ r.sort > 0) ⇒ (out_sort = r.sort))
(¬(∃ r ∈ recs : r.name = ty ∧ r.sort > 0)) ⇒ (out_sort = 0)
-- binds: type Bind(name ∈ String, handle ∈ Int); binds ∈ Seq(Bind); #binds ≤ 6
∀ b ∈ binds : ((b.name = lookup_name) ⇒ (bound_handle = b.handle))
bound_found = (∃ b ∈ binds : b.name = lookup_name)
-- accessors: accs ∈ Seq(Int), #accs ≤ 6 (recdecl_h_fields already does this)
```

Distinguished and NOT flagged *(judgment)*: `h_top/h_2nd…h_6th` (a stack's
interface IS depth order), `tok0..tok7` (fixed lookahead positions),
`arm1_/arm2_` in matchpin (two distinct event-written slots, pinned clean
by calibration case 2), `res_int0..15`/`lat_*` (wire positions of
`last_results` — the kernel API contract). The line: a *keyed* collection
sharded into numbered names is the leak; *positions that are the subject*
(§3.1) are honest.

**S2 — the case-code chain class (38 of 51 WARNs).** The compiler
represents nearly every discriminant as an Int/String code (`parse_mode`,
`pratt_kind`, `zstep`, `kind`, `enum_act/step`, tag tables, `call_name`)
and dispatches with `d = k1 ? v1 : d = k2 ? v2 : …` chains — the exact V9
shape the calibration pins as WARN (matchpin `fold_*` chains; `fold_arm_n`
case-code dispatch). The set-theoretic surfaces, where they exist today:
`match` over an enum (for `Op`/`Token`/`C2Item` discriminants — several
chains re-derive `is_*` booleans from `matches` and then chain over them,
where a single `match` says the same thing); `match` with literal Int
patterns (§2.6 documents `match n / 5 ⇒ …`) for tag/code tables; a
registry + keyed projection when the table is data. Where the entries are
floor handles not yet representable as a registry, the chain stands as
tolerated debt per §3.4 — fix the registry/lowering, do not re-spell the
chain. Reference rewrite (used by all V9 rows below):

```evident
-- instead of: eff = (is_eq ? LibCall(…mk_eq…) : is_lt ? LibCall(…mk_lt…) : …)
eff = match op
    OpEq  ⇒ LibCall("libz3", "Z3_mk_eq", ⟨ArgInt(ctx_h), ArgInt(l_h), ArgInt(r_h)⟩)
    OpLt  ⇒ LibCall("libz3", "Z3_mk_lt", ⟨ArgInt(ctx_h), ArgInt(l_h), ArgInt(r_h)⟩)
    …
```

Carried-write hold chains (`x = (is_first_tick ? init : event ? v : … :
_x)`) are everywhere in these files and are **not** findings — §3.4
blessed; calibration case 2 pins this. That includes the very long
`parse_mode`/`work_items`/`consume_n` writes in driver.ev (prioritized
event guards terminating in the hold), and every zinit/latch bank.

---

## Per-file verdicts (worst first)

### driver_record.ev — 1 BLOCKER / 3 WARN / 1 NOTE

| line | severity | rule | finding |
|---|---|---|---|
| 80–82 (+91–95, 163–174, 215–244) | BLOCKER | §3.6/V3 (S1) | `rec0/rec1/rec2` numbered-slot registry: 3 hand-numbered `RecTypeEntry` carries, 30 triplicated per-slot latch writes (`rec0.acc0 … rec2.acc5`), `rec_cur_*` selection chains. Same file proves the alternative (`recdecl_h_fields ∈ Seq(Int)` with a ∀-cursor write, :179/:209). Rewrite: S1 reference — `recs ∈ Seq(RecTypeEntry)`, `#recs ≤ 3`, ∀-cursor writes; the 30 latch lines collapse to 6. |
| 36–39, 46–54, 67–76 | WARN | V9 (S2) | `RtIdxOf`/`RtSortOf`/`RtFieldAcc` — keyed lookups over `e0/e1/e2` + type-name keys + `rfa_i` index codes spelled as chains. Collapse to keyed projections once S1 lands; `RtFieldAcc`'s inner `acc0..acc5` chain becomes `e.accs[rfa_i0]`. |
| 185–194, 335–342 | WARN | V9 (S2) | `recdecl_st_now` transition table and `recdecl_step_eff` over `recdecl_eff_st` codes — case-code dispatch; an RD-step enum + `match` is the surface. |
| 258–293, 299–322 | WARN | §3.1 *(judgment)* (unroll) | `recdecl_ty0..5`/`recdecl_sort0..5`/`recdecl_write_fn0..5`/`fs0..5` — six-fold hand-unrolled claim invocations and effect rows. Positions are wired (`i ↦ k`), so the honest §3.1 form is `∀ k ∈ {0..5} : RtRecName(s ↦ …, i ↦ k, name ↦ tys[k].name)` with Seq outputs (calibration case 1's own suggested rewrite shape). |
| 78–343 | NOTE | V17 | fsm body 265 lines (>150 ceiling); also raw `LibCall("libz3","Z3_mk_tuple_sort"/"Z3_mk_set_sort")` at :323/:333 — add `BuildZ3MkTupleSort`/`BuildZ3MkSetSort` per §2.8 (counted in the repo-wide §2.8 NOTE). |

### driver_compose.ev — 1 BLOCKER / 1 WARN / 1 NOTE

| line | severity | rule | finding |
|---|---|---|---|
| 48–98 | BLOCKER | §3.6/V3 (S1) | Declaration site of `bind_n0..n5`/`bind_h0..h5` (+ `bind_tail0..4`): the carried cons-list `binds` peeled into the banned per-slot scalar family for cross-module consumption. Rewrite: S1 reference (`binds ∈ Seq(Bind)`, `#binds ≤ 6`, keyed projection); the cons-list/Seq carry gap means the durable fix is the lowering (§1.5), not the peel. |
| 163–192 | WARN | §3.1 *(judgment)* (unroll) | `type_pin_g1..g5` cascade — same decl+pin emitted per slot, hand-unrolled over `slot_names` elements; element iteration is the subject, not position. No blessed Seq→C2Items fold surface exists yet — name the gap: the work-item-emission fold belongs in a lowering (bounded-Seq → cons program). |
| | | | Correctly NOT flagged *(judgment)*: `slot_h0..h5`, `binds_new`, `callw_pop_stack` — stack-depth/slot-order arithmetic where position IS the subject (§3.1 positional-parameter slots); all carried writes are blessed hold chains. |
| 30–289 | NOTE | V17 | fsm body 260 lines (>150). |

### driver.ev — 0 BLOCKER / 3 WARN / 5 NOTE

| line | severity | rule | finding |
|---|---|---|---|
| 903–978 | WARN | V9 (S2) | The effects schedule: one ~70-arm chain over `zstep`/`plan_kind`/`emit_step` case codes. §2.8's own preferred surface exists: multiple guarded writers (`zstep = 5 ⇒ effects = ⟨zinit_bsort⟩`) or phase-composed `effects = eff_zinit ++ eff_lex ++ eff_emit` *(judgment: the ternary encodes priority for free; converting requires exclusive guards — tolerated debt, but the chain is the lowered artifact's shape)*. |
| 866–898 | WARN | V9 (S2) | `eff_step` selects over `it_*` recognizers all derived from the `work_head` enum — `match work_head / C2Ite ⇒ eff_ite / …` is the direct surface. |
| 766–780, 821–824, 831–837, 842–844, 854–858, 386–397 | WARN | V9 (S2) | Numeric-code → value tables: `eff_mkconst` sort table over `decl_const_sort` codes (incl. `10+slot`/`20+slot` slot-coded sorts — also V6-adjacent), `sel_idx_handle`, `len_lit_handle`, `real_denom` (duplicates driver_lex's `frac_pow`), `empty_set_sort`, `seq_elt_tyname`/`mem_tyname`. Sort-code enum + `match`, or the S1 registry for the rec-slot arms. |
| 1–141 | NOTE | V13 | Header contains Evident-shaped code in prose (`` `_name ∈ Type` `` :101, `` `cond ⇒ effects = ⟨…⟩` `` :132, `0 < x < 5` :63) — the exact flattened-source false-positive class the comment rules ban. The SMT-shaped and tick-map material is fine (allowed classes). |
| 1–183 | NOTE | V16 *(judgment)* | No `-- MODULE driver_main` CONSUMES/PRODUCES/MAINTAINS contract; the 141-line scope essay is not the §6.1 format. |
| 184–978 | NOTE | §6.2/V17 | Entry point mixes wiring (30 `..` lifts — legitimate per the headers plan's wide-context-driver rule, not V11) with heavy field-level logic (`work_items` 47-arm dispatcher, `handle_stack`, `capture_pend`, `consume_n`, the effects schedule). File 978 lines (budget ~350); fsm body 795. The driver_main decomposition (tasks #27/#28) is the named precedent; driver_emit.ev's header records the deliberate exception for the effects funnel. |
| 221, 26, 207 | NOTE | §2.6 *(judgment)* | Repo-wide: `parse_mode`/`phase`/`pratt_kind`/`zstep`/`*_st` are Int case codes; every S2 chain over them would become `match` if these were enums. One systemic note, anchored here (the codes' owner). |
| 845–848, 864 | NOTE | §2.8 | Raw `LibCall("libz3","Z3_mk_empty_set"/"Z3_mk_set_add")`, `getpid` filler — counted in the repo-wide §2.8 / sentinel NOTEs. |

### driver_matchpin.ev — 0 BLOCKER / 6 WARN / 2 NOTE (calibration case 2, reconfirmed)

| line | severity | rule | finding |
|---|---|---|---|
| 190–192 | WARN | V9 | `fold_tester1` floor chain (`IntResult`/`StringResult` → `z_*_test`). Fold the floor entries into the `user_variants` registry the pin pair at :187–189 already reads; until a registry can hold floor handles, tolerated debt (§1.5). |
| 196–198 | WARN | V9 | `fold_tester2` — as above. |
| 202–204 | WARN | V9 | `fold_acc1` — as above. |
| 208–210 | WARN | V9 | `fold_acc2` — as above. |
| 214–216 | WARN | V9 | `fold_def_acc` — as above. |
| 221–223 | WARN | V9 | `match_dtail` over `fold_arm_n` case codes — arm-count enum + `match` (§2.6). |
| 1–20 | NOTE | V16 *(judgment)* | Header CONSUMES names `uev_n*/uev_t*/uev_a*` and PRODUCES `mp_arm_*` — neither exists in any body (renamed to `user_variants`/`arm_*`); the contract is stale. |
| 21–252 | NOTE | V17 | fsm body 232 lines (>150). Hold chains :110–175, single conditionals/capture-or-carry :177–186, pin pairs :187–213, and the SMT-shaped comment :39 are all correctly NOT flagged (calibration-pinned). |

### driver_window.ev — 0 BLOCKER / 4 WARN / 1 NOTE

| line | severity | rule | finding |
|---|---|---|---|
| 34–86 | WARN | V9 (S2) | `FtiTok` 47-arm tag→Token chain — the wire-tag decode table. `match tag` with literal Int patterns (§2.6 literal-pattern match) is the surface; the tag table itself is a legitimate class-3 wire fact. |
| 293–309 | WARN | V9 (S2) | `win_need` over 14 `parse_mode` codes — mode enum + `match`. |
| 101–200, 227–245, 319–334 | WARN | §3.1 *(judgment)* (unroll) | `lat_tag0..7`/`lat_pay0..7`, `res_int0..15`, `res_str0..7`, `is_str0..7`, `copy_eff0..7`, `dec_tok0..7`, `read_tag/pay0..7` — wire positions (honest subject, NOT S1) but hand-unrolled 8/16-fold; the §3.1 form is `∀ k ∈ {0..7} : FtiTok(tag ↦ lats[k].tag, …, t ↦ win[k])` over bounded Seqs, pending the lowering. |
| 319–334 | WARN | §2.8 *(judgment)* | 16 raw `LibCall("__mem","read_long",…)` where `BuildMemReadLong` exists and is used in sibling files (grouped with driver_lex's row as one class). |
| 88–335 | NOTE | V17 | fsm body 247 lines (>150). `tok0..tok7`/`win_rest*` peels are honest lookahead positions — not flagged. |

### driver_calllower.ev — 0 BLOCKER / 4 WARN / 2 NOTE

| line | severity | rule | finding |
|---|---|---|---|
| 77–87 | WARN | V9 (S2) | `call1_items` — `call_name` tested against successive builtin-name keys. The table is data: a builtin-lowering registry + keyed projection is the durable form; tolerated debt while item programs aren't registry-representable. |
| 101–128 | WARN | V9 (S2) | `call2_items` — 8-key chain, as above. |
| 129–145 | WARN | V9 (S2) | `call3_items` — as above. |
| 67–76 | WARN | V9 (S2) | `card_items` over `card_count` case codes. |
| 1–18 | NOTE | V16 *(judgment)* | Header PRODUCES `d_sl_*/d_card_*` — names that no longer exist (now `card_*`); stale contract. |
| 48–62 | — | — | CLEAN highlight: the `set_vars` existentials and keyed projections are exemplary §2.5 reads. |

### lex_fti.ev — 0 BLOCKER / 3 WARN / 2 NOTE

| line | severity | rule | finding |
|---|---|---|---|
| 67–108 | WARN | V9 (S2) | `LexCharTag` — 33-arm char→tag chain + 33-term `recognized` disjunction. The char/tag table is data (it IS the FTI wire format): `match`-on-literals or a `Seq(CharTag)` registry + keyed projection. |
| 111–125 | WARN | V9 (S2) | `LexKeywordTag` — 14-arm keyword→tag chain, as above. |
| 144–182 | WARN | V9 (S2) | `kind` selection plus `tag0`/`count_n`/`last_n`/`prev_n` dispatching on `kind` case codes (`kind = 4 ∨ kind = 5 ∨ kind = 6`) — a `LexKind` enum + `match` names the shapes the comment table at :44–50 documents in prose. |
| 1–62 | NOTE | V16 | No `-- MODULE` CONSUMES/PRODUCES/MAINTAINS header (the prose covers the material; the checked format is absent). The encoding/host-contract tables are allowed class-3 wire facts. |
| 66–183 | NOTE | V17 | `LexCharTag` 43 lines, `LexFtiPlan` 56 lines — past the ~25-line pure-claim budget. |

### driver_recval.ev — 0 BLOCKER / 4 WARN / 0 NOTE

| line | severity | rule | finding |
|---|---|---|---|
| 2–4 (header), 18–21 | WARN | V6 + V9 | `recval_slot ∈ Int` — "the registry slot index 0/1/2" in the module interface is the index-in-interface idiom; with S1, the interface becomes the record (or its key `recval_ty_name`), and the four `recval_fnames/ftypes/nf/ctor` slot chains become field reads. |
| 46–60 | WARN | V9 (S2) | `recval_items` arity chain over `recval_nf` codes 1..5. |
| 63 | WARN | V9 (S2) | `recval_sortcodes` ∀-write whose RHS is a type-name→code chain (`"Bool" ? 1 : "String" ? 3 : "Real" ? 6 : 0`) — sort-code table; the ∀-write itself is a blessed assignment. |
| 27–44, 64–87 | WARN | §3.1 *(judgment)* (unroll) | `recval_seg0..5` + `recval_dtail5..1` — per-field cons-program fold, hand-unrolled. **§3.1 ruling the task asked for:** the explicit `recval_fields[0..5]` reads land on the *honest* side — field position IS the subject (ctor-application order) — so the indices are not the violation; the six-fold copy-paste of the same construction is, pending a fold lowering. The `∀ k ∈ {0..5} : RtRecName(… i ↦ k …)` lines at :24–26 are already the blessed §3.1 form. |

### driver_symlookup.ev — 1 BLOCKER / 1 WARN / 1 NOTE

| line | severity | rule | finding |
|---|---|---|---|
| 25–34 | BLOCKER | §3.6/V3 (S1) | `bound_found` 6-way disjunction + `bound_handle` selection chain over `bind_n0..n5`/`bind_h0..h5`. Rewrite: S1 reference — `bound_found = (∃ b ∈ binds : b.name = lookup_name)` + keyed projection for the handle. |
| 45–47 | WARN | V9 (S2) | `lookup_handle` floor chain (`"true"`/`"false"` → `z_true`/`z_false`) — matchpin-class floor dispatch; tolerated debt until the floor handles join a registry. |
| 1–18 | NOTE | V16 *(judgment)* | Header names `ilb_n*/ilb_h*` and `d_lk_pfx_*` — neither exists in any body (now `bind_n*`, `lookup_pfx_*`); stale contract. The `enum_values` pin pair at :43–44 is exemplary. |

### driver_classify.ev — 1 BLOCKER / 1 WARN / 1 NOTE

| line | severity | rule | finding |
|---|---|---|---|
| 173–175 | BLOCKER | §3.6/V3 (S1) | `name_bound` disjunction over `bind_n0..n5`. Rewrite: S1 reference existential. |
| 53–59 | WARN | V9 (S2) | `line_sort` — type-name keys → sort codes. Sort-code enum (or the S1 registry for the user-enum arm) + `match`. |
| 28–215 | NOTE | V17 | fsm body 187 lines (>150). The wide CONSUMES is documented and justified in-header (§6.4) — accepted. Everything else here (recognizer conjunctions, `enter_*` priority guards, `FtiNameEntry` composition reuse) is clean. |

### driver_posbind.ev — 1 BLOCKER / 1 WARN / 2 NOTE

| line | severity | rule | finding |
|---|---|---|---|
| 152–154 | BLOCKER | §3.6/V3 (S1) | `elem_is_bound` disjunction over `bind_n0..n5`. Rewrite: S1 existential. (Contrast: :151's `∃ i ∈ {0..#enum_values-1} : enum_values[i].name = elem_name` is the documented blessed membership form.) |
| 265–282 | WARN | V9 (S2) | `pratt_done_items`/`pratt_done_consume` — 8-code dispatch over `_pratt_kind`. Kind enum + `match`. |
| 1–24 | NOTE | V16 *(judgment)* | Header CONSUMES `ilb_n*` and `d_h_t*` — stale names. |
| 27–283 | NOTE | V17 | fsm body 256 lines (>150). NOT flagged *(judgment)*: `bindzip_h0..h3`, `tup_h0/h1`, `tup_binds*`, `bindzip_plain/pop` — argc/stack-depth arithmetic in a module whose entire subject is positional binding (§3.1 positional-parameter slots); `param_names`/`param_types` use the blessed ∀-cursor allocation writes. |

### driver_ir.ev — 1 BLOCKER / 0 WARN / 3 NOTE

| line | severity | rule | finding |
|---|---|---|---|
| 83–86 | BLOCKER *(judgment)* | §3.6/V3 (S1) | `RecTypeEntry` carries `acc0..acc5` as six numbered fields — the per-slot shape as record fields. driver_record.ev:179 (`recdecl_h_fields ∈ Seq(Int)`, `#≤6`) proves the Seq-field surface in the consuming file. Rewrite: `accs ∈ Seq(Int)`, `#accs ≤ 6`. Judgment: fields not top-level consts, but the same lowered-artifact naming shape; graded with the catalog. |
| 83–86 | NOTE | V8 | 14-field `RecTypeEntry` (name + 2 packed strings + count + 4 handles + 6 accessors) — god-record width is the tell; the packed `fnames`/`ftypes` strings are themselves a string-encoded registry (gap workaround; retires with S1's `fields ∈ Seq(RecField)`). Body has one invariant; `sort > 0 ⇒ ctor > 0` is statable (V7-adjacent). |
| 1–8 | NOTE | V16 *(judgment)* | `-- MODULE driver_ir` header lacks the CONSUMES/PRODUCES/MAINTAINS triple (vocabulary module — the triple still documents who reads/writes which registries). |
| 94–96, 100–102, 107–108 | NOTE | V13 | Evident membership-shaped code in prose (`` `enum_values ∈ Seq(EnumVariantVal)` `` etc.) — the documented lint false-positive class. `FtiBuffer`/`Z3SolverCtx`/`Z3Sorts`/`Z3Numerals` bodies are the exemplary type-invariant models; the dated perf-trap comment is an allowed class-2 measured trap. |

### driver_enum.ev — 0 BLOCKER / 2 WARN / 2 NOTE

| line | severity | rule | finding |
|---|---|---|---|
| 113–122, 176–195, 304–306 | WARN | V9 (S2) | `enum_go_variants`/`enum_go_name` over `_enum_src` codes; the `enum_act_now`/`enum_step_now` transition tables; `result_acc_pend` key chain. Act/step/src enums + `match` (the prose table at :44–56 already names every state — the enum exists in the comment). |
| 158–172 | WARN | V9 (S2) | `field_sort0/1/2` — type-name keys (`"Real"`/`"LibArg"`/`"Seq(LibArg)"`) → floor sort handles, ×3 unrolled (the `field_ref0/1/2` scalar duals are acknowledged cross-file debt in the :28–29 comment). Floor-sort registry + keyed projection. |
| 1–22 | NOTE | V16 *(judgment)* | Header PRODUCES `evt_*`/`uev_*` — stale (now `enum_values`/`user_variants`). |
| 32–354 | NOTE | V17 | fsm body 322 lines (>150); file 354 (>~350). CLEAN highlights: `field_slots`/`enum_values`/`user_variants` are the §2.5 registry surfaces done right — ∀-cursor allocation writes, keyed `_e` updates, bounded carries. |

### driver_expr.ev — 0 BLOCKER / 2 WARN / 2 NOTE

| line | severity | rule | finding |
|---|---|---|---|
| 35–49, 77–83, 114–120 | WARN | V9 (S2) | `C2TokOp`/`C2AtomE`/`C2Prec` — chains over booleans individually derived by `matches` from one enum discriminant; a single `match t`/`match o` says the same thing without the 14 intermediate `c2to_*` names. |
| 427–433, 420–425 | WARN | V9 (S2) | `ps_red_e`/`ps_red_base` select over `ps_top` (the `PrOp` enum) — `match ps_top`. (`ps_call_e` argc selection is honest-positional — not flagged.) |
| 171–467 | NOTE | V17 | `C2PrattStep` is a 297-line pure claim (budget ~25) — split into subclaims (shift/reduce/close/bump already name themselves at :146–166); file 467 lines (>350). |
| 21–52, 99–112, 177–245 | NOTE | V5/V14 *(judgment)* | Letter-code prefix families on claim-local names (`c2to_`, `c2a_`, `pc_`, `ps_`) — claim-scoped, so short names are partially excused (§3.6), but these are systematic hand-namespacing, the symptom the claim-headers plan names. The :93–97 oracle-vs-CLAUDE.md precedence divergence comment is an allowed measured fact. |

### translate2_bool.ev — 0 BLOCKER / 2 WARN / 2 NOTE

| line | severity | rule | finding |
|---|---|---|---|
| 57–75 | WARN | V9 (S2) | `BoolCmpBuildZ3` eff chain over `is_*` booleans derived from the `Op` enum — the S2 reference rewrite (`match op`) applies verbatim here. |
| 84–90 | WARN | V9 (S2) | `BoolNaryBuildZ3` — as above. |
| 1–36 | NOTE | V16 | No `-- MODULE` contract triple (prose header is good material; format absent). |
| 75, 90 | NOTE | *(judgment)* | `LibCall("libc","time",⟨ArgInt(0)⟩)` as the ¬ok no-op sentinel — repo-wide idiom (also translate2_seq :67/:103/:143, translate2_record :51, driver_window getpid fillers, driver_buildeff :98, driver.ev :864). A junk syscall standing for "no effect" is surface that lies about intent; the honest fix is caller-side gating or a kernel-floor `NoOp` effect (file the plan note — no blessed surface exists today). One grouped NOTE. |

### translate2_ctor.ev — 0 BLOCKER / 2 WARN / 1 NOTE

| line | severity | rule | finding |
|---|---|---|---|
| 149–155 | WARN | V9 (S2) | `FieldSortSlot` — type-name keys → sort handles. |
| 217–224 | WARN | V9 (S2) | `EffectCtorArity` — name keys decomposed to `eca_*` booleans then chained; the Effect floor is an enum — `match` on a parsed variant, or a floor registry. |
| 1–42 | NOTE | V16 | No `-- MODULE` contract triple. (`VariantFieldType`'s `idx` chain is honest-positional — callers wire `idx ↦ k`; not flagged. The sort_refs width caveat is an exemplary class-3 wire fact.) |

### translate2_seq.ev — 0 BLOCKER / 1 WARN / 1 NOTE

| line | severity | rule | finding |
|---|---|---|---|
| 118–143 | WARN | V9 (S2) | `StrOpBuildZ3` — 9 `op_name` String keys → LibCalls. String keys have no `match`; the durable fix is an op enum shared with the `C2StrOp` emitters. Tolerated debt. |
| 1–54 | NOTE | V16 | No `-- MODULE` contract triple. The legacy-SMT mapping table is an allowed class-3 cross-file wire fact (SMT-shaped, per the calibration's comment ruling). |

### translate2_record.ev — 0 BLOCKER / 1 WARN / 1 NOTE

| line | severity | rule | finding |
|---|---|---|---|
| 41 | WARN | V7 *(judgment)* | `type EqPlanSlot(addr, in_range, is_last, needs_and)` — bodyless where a relationship is statable: `is_last ⇒ in_range` (given `idx ≥ 0`); an anemic result-tuple otherwise. |
| 1–37 | NOTE | V16 | No `-- MODULE` contract triple. Handle-provenance prose is allowed class 3. The probe's explicit `plan[0..2]` rows wire `idx ↦ 0/1/2` — honest-positional (§3.1), not flagged. |

### translate2_match.ev — 0 BLOCKER / 0 WARN / 2 NOTE

| line | severity | rule | finding |
|---|---|---|---|
| 1–70 | NOTE | V16 | 71 of 100 lines are header prose without the `-- MODULE` contract triple. |
| 25–35 | NOTE | V13 *(judgment)* | The semantics table quotes a full Evident `match` block in prose (`match e / C1(v) ⇒ rhs1 …`) — Evident-shaped, so the flattened-source false-positive risk applies (the SMT column is fine). The fold-contract pseudocode (`else_h := …`) is neither Evident- nor SMT-shaped — allowed class 3. Claims themselves are exemplary one-LibCall Build-shaped claims. |

### driver_setvar.ev — 0 BLOCKER / 2 WARN / 2 NOTE

| line | severity | rule | finding |
|---|---|---|---|
| 50–55 | WARN | *(judgment)* | `setvar_slot ∈ Int` is **declared twice with two competing keyed projections** (keyed on `_setvar_cur_name` at :51–52, then on `setvar_name` at :54–55). When the two keys name different registry rows with different `kidx`, the projections conflict (UNSAT); when equal, one pair is dead. Not covered by the calibration — flagged as a likely real defect: keep one declaration, two distinctly named outputs (`setvar_slot_cur`, `setvar_slot_line`). |
| 116–130 | WARN | §3.1 *(judgment)* (unroll) | `qset_seg2/seg1/qset_items` — per-element cons-program fold hand-unrolled over `qset_names[0..2]` with count guards; same fold-lowering gap as recval/compose. |
| 1–18 | NOTE | V16 *(judgment)* | Header PRODUCES `stv_n*/stv_k*/stv_e*/stv_c*` — stale (now `set_vars[k].name/kidx/elems/count`). |
| 26–160 | NOTE | V17 | fsm body 135 lines, no section banners (>80 unbannered). CLEAN highlights: `set_vars` is the model registry — bounded Seq, ∀-cursor allocation, keyed `_e.name` element writes (:74–75), `setvar_cur_name` key-valued cursor (§3.2 exactly as doctrine). |

### driver_lex.ev — 0 BLOCKER / 2 WARN / 1 NOTE

| line | severity | rule | finding |
|---|---|---|---|
| 130–135, 213–218 | WARN | §2.8 *(judgment)* | Raw `LibCall("__mem","write_long",…)` ×8 where `BuildMemWriteLong` exists and is the form used by driver_buildeff/driver_record. Rewrite: `BuildMemWriteLong(addr ↦ plan_addr_tag0, value ↦ plan_tag0_pp, eff ↦ write_tag0)`. |
| 122–128 | WARN | V9 (S2) | `frac_pow` — `_frac_digits` codes → powers of ten (duplicated as `real_denom` in driver.ev:831). A shared table claim or literal-pattern `match`. |
| 21–237 | NOTE | V17 | fsm body 217 lines (>150). The `tok_buf.count < 65534` bounds comment is an exemplary class-2/3 measured trap; all carried writes are blessed hold chains. |

### driver_claimidx.ev — 0 BLOCKER / 1 WARN / 2 NOTE

| line | severity | rule | finding |
|---|---|---|---|
| 186–209 | WARN | V9 (S2) | `efflit_branch_fold` over `_efflit_count` codes and `chain_fold_items` over `_chain_n` codes, atop the `conj_items1..4`/`chain_lvl0..4` numbered ladder — a bounded-depth fold unrolled then chain-dispatched; literal-pattern `match` (or the fold lowering) is the surface. |
| 108–109 | NOTE | V14 *(judgment)* | `efflit_libcall ∈ Int` used as a 0/1 flag — a Bool named/typed as a code; `efflit_in_libcall ∈ Bool` reads as a predicate (§3.6). |
| 27–240 | NOTE | V17 | fsm body 214 lines (>150). `FtiNamedAppend`/`FtiNameEntry` composition reuse is exemplary. |

### driver_buildeff.ev — 0 BLOCKER / 1 WARN / 2 NOTE

| line | severity | rule | finding |
|---|---|---|---|
| 100–123 | WARN | V9 (S2) | `enum_step_eff` — nested act×step case-code dispatch (the largest single chain in the tree). Act/step enums + `match` (same fix as driver_enum's tables, which produce these codes). |
| 8–9, 124–127 | NOTE | V14 *(judgment)* | `enum_wbatch5/wbatch2/wbatch3u` — opaque abbreviations (write-batch); 2–3-word names exist (`enum_write_batch_fields` …). |
| 21–220 | NOTE | V17 | fsm body 200 lines (>150); raw `calloc` LibCalls (:170–171, :220) belong to the repo-wide §2.8 missing-sugar NOTE (`BuildCalloc`). The Build* effect bank itself is exemplary composition. |

### driver_broadcast.ev — 0 BLOCKER / 2 WARN / 1 NOTE

| line | severity | rule | finding |
|---|---|---|---|
| 37–38 | WARN | V9 (S2/S1 consumer) | `bcast_nf`/`bcast_fnames` slot chains over `rec0/1/2` — collapse to field reads under S1. |
| 60–62 | WARN | §3.5 *(judgment)* | `bcast_dup` hand-inlines the fixed-width-32 `"|" ++ substr(… ++ pad31 …)` row encoding that `FtiNameEntry` exists to own — compose `FtiNameEntry(name ↦ bcast_dname, entry ↦ bcast_key)` then probe, as driver_classify does. |
| 16–20 | NOTE | V13 *(judgment)* | "INTERFACE WIDTH … JUSTIFIED per §6.4" — review-status narration citing an external doc's § numbers; the CONSUMES list already carries the load. (The hold chains and `RtRecName(i ↦ _bcast_cur)` positional wiring are clean.) |

### driver_exprdecomp.ev — 0 BLOCKER / 1 WARN / 1 NOTE

| line | severity | rule | finding |
|---|---|---|---|
| 84–86 | WARN | V9 (S2) | `matches_tester` floor chain (`IntResult`/`StringResult`) — matchpin class; same registry fix. |
| 66–69 | NOTE | V13 *(judgment)* | "the legacy dropped-Exit(3+4) class is impossible by construction" — history narration (git's job); the collision-freedom fact (builtins lowercase / variants capitalized) is the keepable half. The `ctor_decl`/`matches_tester_user` pin pairs are exemplary. |

### driver_pratt.ev — 0 BLOCKER / 1 WARN / 1 NOTE

| line | severity | rule | finding |
|---|---|---|---|
| 34–52 | WARN | §3.1 *(judgment)* (unroll — the task-named candidate) | `callable_names` — nine builtin rows plus six `user_variants[k].name ≠ "" ? …` arms plus three `rec*` arms concatenated by hand. **§3.1 ruling:** order in the pipe-joined probe string is immaterial — the *elements* are the subject, not the positions — so this lands on the **dishonest** side of the honest-positional exemptions: it is element iteration hand-unrolled, not an order-sensitive fold. No blessed Seq→String fold surface exists yet; the durable fix is a fold lowering (or moving the probe to a registry lookup). Also re-inlines the pad-31 row encoding (`FtiNameEntry`'s job). |
| 100–101 | NOTE | V13 | Evident code-in-prose (`` `offset_pos ∈ IVec2 = <expr>` ``). All the `pratt_*` carried writes are blessed hold chains; the `C2PrattStep` slot composition is the correct claim-call form. |

### driver_symtab.ev — 0 BLOCKER / 0 WARN / 1 NOTE

| line | severity | rule | finding |
|---|---|---|---|
| 20–197 | NOTE | V17 | fsm body 178 lines (>150). Everything else clean *(judgment recorded)*: `h_top/h_2nd…h_6th` + `h_tail1..6` are a stack peel where depth IS the interface (§3.1) — deliberately distinguished from the keyed `bind_n*` family (BLOCKER class); the 27 `it_*` recognizers are blessed `matches`; `FtiNamedAppend` composition and the fixed-width-32 lookup comment (class-3 wire fact) are exemplary. |

### driver_quant.ev — 0 BLOCKER / 0 WARN / 1 NOTE

| line | severity | rule | finding |
|---|---|---|---|
| 11–14 | NOTE | V13 | Evident code-in-prose in MAINTAINS (`` `∀|∃ v ∈ {lo..hi} : body` ``, `` `#seq = k` ``). Otherwise CLEAN: hold chains, match decompositions, and single conditionals throughout. |

### driver_group.ev — 0 BLOCKER / 0 WARN / 1 NOTE

| line | severity | rule | finding |
|---|---|---|---|
| 11–12 | NOTE | V13 | Evident code-in-prose (`` `claim main(a, b ∈ Int, ok ∈ Bool)` ``, `` `x, y, z ∈ Nat` ``). Otherwise CLEAN — `group_pending` cons-list walk, blessed hold chains; `group_done_mode`/`group_ty_exit_mode` are 2-test prioritized boolean guards, not single-discriminant key chains (not V9). |

### driver_litmem.ev — 0 BLOCKER / 0 WARN / 1 NOTE

| line | severity | rule | finding |
|---|---|---|---|
| 12, 46–47 | NOTE | V13 | `` `s ∈ Seq(Int)` `` quoted in prose — the *specific example* CLAUDE.md's comment rules cite as having caused real lint false positives. The oracle-degenerate-length fact itself is a keepable class-3 note; re-spell without the membership-shaped quote. Otherwise CLEAN. |

### driver_emit.ev — 0 BLOCKER / 0 WARN / 2 NOTE

| line | severity | rule | finding |
|---|---|---|---|
| 18–23 | NOTE | V13 *(judgment)* | Design-rationale narration citing unanchored § numbers ("the §31 eff_out indirection", "§12", "§6.3") of an external notes doc — unreviewable without that doc; keep the one-line decision ("effects writer stays in driver_main; extraction loses the isolation test"), drop the §-trail. |
| 55–57 | NOTE | §2.8 | Raw `LibCall("libc","free",…)` ×3 — no `BuildFree` exists; §2.8 doctrine says add it (grouped repo-wide NOTE: BuildFree, BuildCalloc, BuildCstrCopy…, BuildZ3MkTupleSort/SetSort/EmptySet/SetAdd). Hold chains and manifest/prelude text assembly are clean. |

### driver_guard.ev — CLEAN

Hold chains (`guard_handle`/`guard_depth`), `matches` recognizers, claim
composition, and a class-3-correct module header. No findings.

### driver_zinit.ev — CLEAN

The entire file is the blessed latch-bank idiom: capture-or-carry single
conditionals keyed off the `zstep` program counter, record-typed carries
(`z3ctx`, `z3sorts`, `z3nums`, `FtiBuffer`) with their invariants living in
driver_ir's type bodies. No findings.

### driver_input.ev — CLEAN

Module contract present; hold chains; `last_results[0/1]` reads are kernel
wire positions (class-3 contract documented in-header). No findings.

---

## Burndown (by severity, then blast radius)

Severity first; within severity, ordered by how many files/lines a fix
touches (blast radius). Each box is one work item.

**BLOCKER — the S1 numbered-slot complex**

- [ ] **B1.** Convert the record-type registry to `recs ∈ Seq(RecTypeEntry)` with `accs ∈ Seq(Int)` fields — declaration (driver_record.ev:80–244, driver_ir.ev:83–86) plus consumer chains in driver_recval/driver_broadcast/driver_pratt/driver_calllower/driver.ev. Gate: if the carried-registry oracle gap (roadmap `f103caa`) blocks it, extend the bounded-Seq lowering first (§1.5 — fix the lowering, keep the surface). Largest blast radius in the tree.
- [ ] **B2.** Replace the `bind_n0..n5`/`bind_h0..h5` peel (driver_compose.ev:48–98) with a `binds ∈ Seq(Bind)` registry + keyed projections; rewrite the three consumers (driver_symlookup.ev:25–34, driver_classify.ev:173–175, driver_posbind.ev:152–154). Pre-req: same lowering gate as B1 (cons-list → bounded Seq).

**WARN — likely real defect**

- [ ] **W0.** driver_setvar.ev:50–55 — resolve the duplicate `setvar_slot` declaration / competing keyed projections (rename to two outputs or delete the dead pair). Small, but a conflicting-definition risk.

**WARN — the S2 case-code chain class (do after the enums exist)**

- [ ] **W1.** Introduce enums for the pervasive Int codes (`parse_mode`, `pratt_kind`, `enum_act/step/src`, `lex kind`, `recdecl_st`) — unlocks `match` rewrites for ~20 chains across driver.ev, driver_enum, driver_buildeff, driver_record, driver_posbind, driver_window, lex_fti, driver_claimidx, driver_lex.
- [ ] **W2.** driver.ev:903–978 — restructure the effects schedule toward §2.8 guarded writers / phase-composed `++` (or extract per-phase schedule claims); driver.ev:866–898 `eff_step` → `match work_head`.
- [ ] **W3.** `match`-over-enum rewrites where the discriminant is already an enum: translate2_bool (BoolCmp/BoolNary), driver_expr (C2TokOp/C2AtomE/C2Prec/ps_red_e), driver_window FtiTok (literal-pattern match), lex_fti char/keyword tables.
- [ ] **W4.** Floor-handle dispatch chains → fold floor entries into the registries they shadow: driver_matchpin ×6, driver_exprdecomp matches_tester, driver_symlookup lookup_handle, driver_enum field_sort0/1/2, translate2_ctor FieldSortSlot/EffectCtorArity. Blocked on a registry that can hold floor handles — tracked debt per §3.4.
- [ ] **W5.** driver_calllower call1/2/3 + card_items builtin-name dispatch → builtin-lowering registry (or at minimum a shared op enum with translate2_seq's StrOpBuildZ3 keys).
- [ ] **W6.** Slot-selection chains that simply disappear under B1: driver_recval:18–21, driver_broadcast:37–38, driver_classify line_sort user-enum arm, driver.ev sort tables (:766–780, :842–844, :386–397).

**WARN — the hand-unrolled fold class (needs a fold lowering; file the plan)**

- [ ] **W7.** Specify the bounded-fold lowering (Seq → cons-program / Seq → String) that retires: driver_recval seg/dtail (:27–87), driver_setvar qset segs (:116–130), driver_compose type_pin_g* (:163–192), driver_pratt callable_names (:34–52), driver_window lat/res/read/copy families, driver_record recdecl_ty/sort/write families, driver_claimidx chain_lvl ladder. Until it exists these are tolerated; do NOT re-spell them.
- [ ] **W8.** Raw `LibCall` where sugar exists: driver_lex:130–135/213–218 and driver_window:319–334 → `BuildMemWriteLong`/`BuildMemReadLong`.
- [ ] **W9.** driver_recval `recval_slot` index-in-interface (V6) — key the interface on the type name/record once B1 lands.
- [ ] **W10.** driver_broadcast:60–62 — compose `FtiNameEntry` instead of inlining the pad-31 row encoding (also inside driver_pratt callable_names).
- [ ] **W11.** translate2_record `EqPlanSlot` — state `is_last ⇒ in_range` in the type body (V7).

**NOTE — organization and hygiene**

- [ ] **N1.** Add `-- MODULE` contract triples: translate2_{match,record,seq,bool,ctor}.ev, lex_fti.ev, driver_ir.ev, driver.ev.
- [ ] **N2.** Refresh stale headers (rename-map debt): driver_symlookup (`ilb_*`, `d_lk_pfx_*`), driver_matchpin (`uev_*`, `mp_arm_*`), driver_setvar (`stv_*`), driver_posbind (`ilb_n*`, `d_h_t*`), driver_calllower (`d_sl_*`, `d_card_*`), driver_enum (`evt_*`, `uev_*`).
- [ ] **N3.** Size budgets (V17): 13 fsm bodies >150 lines (driver_main 795, DriverEnum 322, DriverRecord 265, DriverCompose 260, DriverPosBind 256, DriverWindow 247, DriverMatchPin 232, DriverLex 217, DriverClaimIdx 214, DriverBuildEff 200, DriverClassify 187, DriverSymtab 178; DriverSetVar 135 unbannered); `C2PrattStep` 297-line claim; files driver.ev 978 / driver_expr.ev 467 / driver_enum.ev 354. The driver_main decomposition precedent (#27/#28) names the fix: extract carry-owning sub-fsms, not banners.
- [ ] **N4.** §6.2 entry-point mixing in driver.ev — the residual field-level logic (work_items dispatcher, consume_n, capture_pend) is the next extraction candidate set (driver_emit's header documents the one deliberate exception).
- [ ] **N5.** Comment hygiene (V13): strip Evident-shaped code from prose (driver_litmem:12,47; driver_group:11–12; driver_pratt:100–101; driver_quant:11–14; driver.ev header; translate2_match:25–35; driver_ir:94–108); drop narration (driver_emit:18–23 §-trail, driver_exprdecomp:66–69 "legacy", driver_broadcast:16–20 JUSTIFIED banner).
- [ ] **N6.** Add the missing `Build*` sugar claims (§2.8): BuildFree, BuildCalloc, BuildZ3MkTupleSort, BuildZ3MkSetSort, BuildZ3MkEmptySet, BuildZ3MkSetAdd, a __cstr copy wrapper.
- [ ] **N7.** Decide the no-op sentinel: a kernel-floor `NoOp`-shaped effect or caller-side gating, replacing `time(0)`/`getpid()` fillers (translate2_bool/seq/record, driver_window, driver_buildeff, driver.ev). No blessed surface exists today — needs a plan note, not invention.
- [ ] **N8.** Naming (V5/V14): `efflit_libcall ∈ Int` flag → Bool; `enum_wbatch*` → words; the `ps_/pc_/c2to_/c2a_` prefix families retire as claim headers land.
- [ ] **N9.** driver_ir `RecTypeEntry` width (V8) + statable `sort > 0 ⇒ ctor > 0` invariant — falls out of B1's `fields ∈ Seq(RecField)`.

---

*Honesty appendix.* Judgment calls not covered by the calibration are
marked `(judgment)` inline; the recurring ones: (a) stack-depth/argc/slot
selection ruled honest-positional (§3.1) vs. the keyed `bind_n*` family
ruled BLOCKER — the discriminator is whether the collection is consumed by
key or by position; (b) hand-unrolled bounded folds graded WARN, not V3
BLOCKER, because no blessed fold surface exists to revert *from* — the
skill's "name the lowering" rule applied; (c) `acc0..acc5` record fields
graded BLOCKER by §3.6's naming-shape letter despite being fields, on the
strength of the in-file Seq counter-example; (d) prioritized *boolean-guard*
selections (`group_done_mode`, lex `kind`, `pratt_enter_kind`) NOT flagged
as V9 — the §3.4 letter requires one discriminant against successive
keys/codes; (e) Evident-shaped quotes in comments flagged V13 only when
membership/match-shaped (the calibration's SMT-shaped allowance inverted).

---

## Burndown updates (2026-06-10, post-baseline)

- **S1 record half: RESOLVED** (merge b46f373). rec0/rec1/rec2 +
  acc0..acc5 → `recs ∈ Seq(RecTypeEntry)` with `accs ∈ Seq(Int)`;
  all 6 BLOCKERs cleared. Critic on the diff: 0 BLOCKER.
- **NEW (from that diff's critic review): ambient-`recs` implicit
  interface** — RtIdxOf/RtSortOf/RtFieldAcc read `recs` via names-match
  pass-down with no declared interface. Fix when claim headers land
  (task #36): declare `recs` in the claims' headers.
- **Duplicate `setvar_slot` definition: RESOLVED** (d047518) — was a
  rename collision; latent UNSAT; the only collision in the map.
- S1 bind-peel half (`bind_n0..n5`), the S2 chain classes (V9), the
  fold families (W7), and index-in-interface (V6) remain open.
