# Ternary-chain census + refactor plan (compiler2/)

**Status:** analysis (2026-06-12). Prep for the "make the code more
set-theoretic, less ternary" refactor. READ-ONLY survey — no `.ev`
edits were made producing this doc. Line numbers are against this
worktree's `compiler2/*.ev` (the source the sibling agent and the
metric both read here); the `v9_selection_chains` count is **71** in
this tree (0 exempted via `docs/purism-exemptions.md`).

**What this counts.** Exactly the `v9_selection_chains` metric
(`.goalpost/measures/purism_v9_chains.sh`): a statement (joined across
lines by paren balance) with **≥2 literal-equality ternary tests**
(`x = <int> ?` or `x = "<key>" ?` in condition position), **excluding**
the blessed carried-write *hold chain* (final else-arm is a carry
`: _name)`). This census enumerates all 71.

**The ruling that frames it** (`docs/evident-purism.md` §3.4, §1.4):
≥2-test **value-selection** chains are WARN; the durable fix is a
lowering or registry change, not acceptance of the chain. The hold
chain is already excluded by the metric, so every row here is in-scope
by construction — *but* some rows are genuine single-discriminant case
codes whose honest surface is an **enum + `match`**, not a pin family
(per the guarded-pin plan's "out of scope" section), and a sizeable
fraction are genuine **arithmetic folds / positional selects** where the
position IS the subject. Those are marked **not-a-smell** or
**needs-design**. Not every ternary is a smell.

## The blessed replacement surfaces (the catalog)

Every proposal names one of these, all from CLAUDE.md /
`docs/evident-purism.md` §2 / `docs/plans/guarded-pin-family-lowering.md`:

| Tag | Surface | Lowering that keeps it functionizer-safe |
| --- | ------- | ---------------------------------------- |
| **KP-pair** | Keyed-projection pin pair over a registry: `∀ e ∈ xs : ((e.key = k) ⇒ (out = e.val))` + `(¬(∃ e ∈ xs : e.key = k)) ⇒ (out = def)` | `scripts/passes/lower-bounded-seq.sh` PAIR rule → covered select chain (**exists today**) |
| **PIN-family** | Scalar guarded-pin family: `g1 ⇒ (out = v1)` … `gN ⇒ (out = vN)` + `(¬(g1∨…∨gN)) ⇒ (out = d)` | `guarded-pin-family-lowering.md` (**proposed**; extends the PAIR pass) |
| **PIN-mixed** | Floor-literals + registry scan on one output (matchpin shape) | same proposed pass; disjointness from builtin/user name collision-freedom |
| **enum+match** | A case-code discriminant becomes an `enum`; the chain becomes `match` | native `match` (exists); needs the enum to be introduced first |
| **range-∀ / index** | `xs[i]` direct index, or `∀ k ∈ {0..N} : xs[k] = …` where position IS the subject | native; only where the index is genuinely the meaning |

**Cross-cutting dependency (Milestone 1).** PIN-family and PIN-mixed
both depend on the **guarded-pin-family lowering**
(`docs/plans/guarded-pin-family-lowering.md`), which is *proposed, not
landed*. Until that pass exists, rewriting a scalar chain to a pin
family leaves bare implications that fall off the functionizer (the
measured 19s→300s cliff, CLAUDE.md covered-output trap). **Milestone 1
is landing that pass** (its own fixtures, per that doc). The KP-pair
lowering already exists, so chains replaceable by a *pure* KP-pair (a
registry already in scope, or one extracted) are unblocked today.

## Per-change verification gate

Every rewrite below is a semantics-preserving refactor over a **single
claim's interface**, so the per-change gate is **[gate]**:

```
scripts/model-diff.sh <before.ev> <after.ev> <claim> [--inputs …] [--observe <lhs>]
```

`model-diff` suffixes per-side everything that is not a shared input and
asserts the **observed output diverges**; `unsat` ⇒ the solution spaces
agree over the interface ⇒ EQUIVALENT. For a single-output chain
`--observe <lhs>` is the chain's LHS; the guard discriminant(s) and any
`_x` carry are inputs (carries are inputs by default). The claim under
test is the enclosing `claim`/`fsm` name. A `DIFFER` verdict with a
witness means the rewrite changed a coverage edge — almost always an
under/over-coverage of the default arm. A chain's final `: else` arm is
*total* (covers everything not matched); a PIN-family default must
reproduce exactly that complement (`¬(g1∨…∨gN)`). If the hand-written
default disjunction is wrong, model-diff witnesses the diverging input.

---

## Census table (all 71)

Columns: **loc** = file:line of the statement start · **LHS** = output
defined · **dispatches on** = the guard discriminant · **operation** ·
**proposed surface** · **coverage risk**.

### Lexer tag tables — lexer.ev / lex_fti.ev (a clean Group-D cluster)

| loc | LHS | dispatches on | operation | proposed | coverage risk |
| --- | --- | ------------- | --------- | -------- | ------------- |
| lexer.ev:127 | `n` (DigitToInt) | `c` = "0".."9" | char→int table | **KP-pair** over a `digits ∈ Seq(DigitEntry)` registry (`char ↦ value`), default `-1`; OR a literal-`match` once chars get an enum (none today → **needs-design** for the enum, KP-pair otherwise) | low — `: -1` default total |
| lexer.ev:142 | `t` (MaybeKeyword) | `s` = keyword strings | keyword recognizer | **KP-pair** over a `keywords ∈ Seq(KwEntry)` (`name ↦ token`), default `Ident(s)` | medium — default `Ident(s)` is data-dependent (legal: an expression default) |
| lexer.ev:161 | `t` (SingleCharTok) | `c` = punctuation | char→token table | **KP-pair** over a punctuation registry; the paired `recognized` Bool is the `∃` of the same scan | medium — keep `recognized` consistent with the table |
| lexer.ev:228 | `out` (EscapeChar) | `c` = "n"/"t"/'"'/"\\" | escape table | **PIN-family** (4 disjoint string guards + passthrough default `c`) | low |
| lexer.ev:252 | `w` (IndentWidth) | `c` = " "/"\t" | width table | **PIN-family** (2 guards + `0` default) | low |
| lex_fti.ev:67 | `tag` (LexCharTag) | `c` = operator chars (33 tests) | char→tag table | **KP-pair** over an operator registry (`char ↦ tag`); the `recognized` Bool (next line) is the `∃` of the same scan — rewrite both so the table is one source | medium — table + membership predicate must stay in lockstep |
| lex_fti.ev:111 | `tag` (LexKeywordTag) | `s` = keyword strings (14 tests) | keyword→tag table | **KP-pair** over a keyword registry (dedup with lexer.ev:142's table) | low — default tag |

### translate2_ctor.ev

| loc | LHS | dispatches on | operation | proposed | coverage risk |
| --- | --- | ------------- | --------- | -------- | ------------- |
| translate2_ctor.ev:136 | `pick` | `idx` = 0..4, else `r5_pick` | selects the idx-th tail of a cons-list `EVFieldList` (hand-unrolled walk) | **needs-design (V18 named gap)**: honest surface is element access on a bounded Seq, but the data is a cons `enum` — the Seq(enum)-element lowering gap. Blocked on that gap, NOT a pin family | low |
| translate2_ctor.ev:192 | `sort_slot` (FieldSortSlot) | `typename` floor types, guarded by `is_self` | floor-type → sort-handle table | **PIN-family** (`is_self ? 0` is the first guard; 4 disjoint typename guards; default 0) | low — total |

### matchpin.ev

| loc | LHS | dispatches on | operation | proposed | coverage risk |
| --- | --- | ------------- | --------- | -------- | ------------- |
| matchpin.ev:142 | `acc0` | `_arm_ctor` = "IntResult"/"StringResult", else `acc0_user` (a KP-pair base) | keyed projection w/ floor literals | **PIN-mixed** (fold the `acc0_user` pair in — the calibration instance, `guarded-pin-family-lowering.md` ex.2) | low — default is the `¬∃` registry-miss; floor names disjoint from user variants |
| matchpin.ev:287 | `dtail` | `arm_n` = 0/1 | C2Items cons-depth select | **needs-design** (2-way over a small-int count — arguably a `match` on an arm-count enum; very low value) | low |

### driver_classify.ev / driver_recval.ev / driver_posbind.ev / driver_enum.ev — the typename→sortcode/sort-handle cluster (Group B)

| loc | LHS | dispatches on | operation | proposed | coverage risk |
| --- | --- | ------------- | --------- | -------- | ------------- |
| classify.ev:58 | `line.sort` | `line.ty_name` floor types, else `line.enum_pos ≥ 0 ? …` | typename→sortcode + enum-registry probe | **PIN-mixed** (floor literals + the `enum_pos` probe as the tail member) | low — `: 0` default total |
| driver.ev:422 | `mem_tyname` | `line.sort` = 1/3/4/5/6/30+ | sortcode→typename (the **inverse** of classify:58) | **PIN-family** (disjoint int guards); dedup target | medium — two arms compound (`sort=5 ∧ kind_slot≥0`) but disjoint; default `"Int"` total |
| recval.ev:50 | `sortcodes[k]` (range-∀) | `literal_fields[k].typename` floor types | per-element sortcode table | **PIN-family inside range-∀** (the `∀ k` stays; body is a pin family on `sortcodes[k]`) | low — `: 0` default |
| posbind.ev:214 | `pskip_sort` | `param_ty_name` floor types + `pskip_enum_pos≥0` | sortcode table again | **PIN-mixed** | low |
| enum.ev:196 | `e.sort` (range-∀) | `e.typename` floor types | typename→**sort-handle** (the handle twin) | **PIN-family inside range-∀** | low — `: 0` default |
| driver.ev:857 (`uild`) | `mkconst` `sort_h` arg | `new_const_sort` = 1/3/4/5/6/7/10/11/30+ | sortcode→**sort-handle** table (inline in `BuildZ3MkConst(...)`) | **PIN-family** (disjoint int guards; the metric labels the LHS `uild` from `Build`) | medium — `≥30` range arm; total via `recs[...]` tail |

> **Cluster note (Group B — the highest-leverage move).** classify:58,
> driver.ev:422, recval:50, posbind:214 are copies of the
> **typename→sortcode** table; enum:196 and driver.ev:857 are its
> **sort-handle** twin. Extract ONE `sort_codes ∈ Seq(SortEntry)`
> registry (`typename ↦ code`, plus a `code ↦ handle` companion) and
> make every site a KP-pair (or PIN-mixed where a floor short-circuit is
> needed). Retires ~6 chains + a documented dedup debt. The non-driver
> sites (classify, recval, posbind, enum) are unblocked **today** (KP-pair
> lowering exists); driver.ev:422/857 are deferred (sibling-owned).

### Power-of-ten table

| loc | LHS | dispatches on | operation | proposed | coverage risk |
| --- | --- | ------------- | --------- | -------- | ------------- |
| driver.ev:921 | `denom` | `digits` = 1..6 | int→power-of-ten string | **PIN-family** OR `match digits` — the un-converted twin of `driver_lex.ev`'s already-PIN-family `frac_pow` (`guarded-pin-family-lowering.md` ex.3) | low — `: "1"` default |

### Floor-handle literal picks

| loc | LHS | dispatches on | operation | proposed | coverage risk |
| --- | --- | ------------- | --------- | -------- | ------------- |
| driver.ev:911 | `chosen_position_handle` | `select_position` = 0/1/2 | int→`z3nums.*` literal pick | **PIN-family** OR **not-a-smell** (tiny positional literal pick) | low |
| driver.ev:942 | `lit_handle` | `lit` = 1/2/3/4 | int→`z3nums.*` literal pick | **PIN-family** | low |

### Case-code state machines — the enum+match deficit (Group C, needs-design)

| loc | LHS | dispatches on | operation | proposed | coverage risk |
| --- | --- | ------------- | --------- | -------- | ------------- |
| enum.ev:209 | `action_now` | `_decl_action` = 1/2/3 × `_decl_step` | act/step transition | **enum+match** on a `DeclAction`×`DeclStep` machine | high — nested FSM transition; needs-design |
| enum.ev:216 | `step_now` | `_decl_action`,`_decl_step` = 0..13 (26 tests) | step transition (largest case-code chain) | **enum+match** (same machine) | high — needs-design; consumed by a hold-chain carry |
| buildeff.ev:130 | `enum_step_eff` | `action_now`,`step_now` (16 tests) | act/step→effect dispatch | **enum+match** (effect projection of enum:216's state) | high — needs-design |
| record.ev:174 | `st_now` | `_recdecl_st` = 0..6 + guards | recdecl step transition | **enum+match** on a `RecDeclState` enum (W1 baseline) | high — FSM transition; needs-design |
| record.ev:184 | `fi_now` | `_recdecl_st` | field-index transition | **enum+match** (same `RecDeclState`) | medium — needs-design |
| record.ev:249 | `recdecl_step_eff` | `emit_state` = 0/1/3/4/5/6/7 | emit-state→effect dispatch | **enum+match** on an `EmitState` enum | medium — needs-design; default total |
| window.ev:202 | `need` | `_parse_mode` = 0..13 (14 tests) | parse-mode→token-need table | **enum+match** on a `ParseMode` enum (the guarded-pin plan's named non-example) | high — nested guards; needs-design |
| window.ev:35 | `t` | token-shape (52 tests) | token classification | **enum+match** over a Token enum IF not already a `match` (likely a hand-spelled tag dispatch — verify) | medium — needs source confirm |
| driver.ev:482 | `pops` | item-kind `is_*_item` bools + `_item_step` | item-kind→pop-count | **enum+match** on the item-kind union (the `is_*_item` bools ARE a case code spelled as predicates) | high — needs-design; large |
| driver.ev:595 | `capture_pend` | item-kind bools + `_item_step` | item-kind→capture-count | **enum+match** (same union) | high — needs-design |
| driver.ev:961 | `step_eff` | item-kind bools + `_item_step` (8+ tests) | item-kind→build-effect dispatch (the central VM step) | **enum+match** (same union; the core interpreter step) | high — needs-design |

### Parser-kind dispatch (posbind)

| loc | LHS | dispatches on | operation | proposed | coverage risk |
| --- | --- | ------------- | --------- | -------- | ------------- |
| posbind.ev:350 | `done_items` | `_parser.kind` = 1/3/4/6/7/8/9/10 | parser-kind→item-list | **enum+match** on a `ParserKind` enum | medium — many arms; default total |
| posbind.ev:364 | `done_consume` | `_parser.kind` = 3/6 | parser-kind→consume-count | **enum+match** (same `ParserKind`) OR PIN-family | low |

### Source-union projections (enum.ev)

| loc | LHS | dispatches on | operation | proposed | coverage risk |
| --- | --- | ------------- | --------- | -------- | ------------- |
| enum.ev:126 | `decl_now` | `_source` = 0/1/2 | source→floor-decl table | **PIN-family** (3 disjoint int guards + default `result_decl`); cleaner: `enum Source` + match | low |
| enum.ev:130 | `name_now` | `_source` (mirror of :126) | source→name table | **enum+match** (dedup with :126 — both project one `_source` union) | low |
| enum.ev:196 | (sortcode cluster — see Group B) | | | | |

### Positional / order-sensitive / fold — not-a-smell (position IS the subject)

| loc | LHS | dispatches on | operation | proposed | coverage risk |
| --- | --- | ------------- | --------- | -------- | ------------- |
| compose.ev:110 | `handles[0]` | `_slot_count` = 1..5 | depth→stack-handle positional pick | **not-a-smell** (positional; a `stack.nth(k)` accessor is a *separate* design needing an operator ruling) | medium |
| compose.ev:113 | `handles[1]` | `_slot_count` | same | not-a-smell | medium |
| compose.ev:115 | `handles[2]` | `_slot_count` | same | not-a-smell | medium |
| compose.ev:117 | `handles[3]` | `_slot_count` | same | not-a-smell | medium |
| compose.ev:120 | `pop_stack` | `_slot_count` = 1..6 | depth→cons-tail truncation | **not-a-smell** (positional cons truncation) | low |
| compose.ev:159 | `hdr_join_now` | `_param_count` = 0..5 | count→header-string concat fold | **not-a-smell** (arithmetic/string fold over a count; a range-∀ accumulate is the only alt — needs-design) | low |
| posbind.ev:258 | `bindzip_h0` | `_recv`, `_tuple.arity` = 1/2/3 | arity→handle positional pick | not-a-smell | low |
| posbind.ev:261 | `bindzip_h1` | `_recv`, `_tuple.arity` | same | not-a-smell | low |
| posbind.ev:264 | `bindzip_h2` | `_recv`, `_tuple.arity` | same | not-a-smell | low |
| posbind.ev:268 | `param_ty` | `_tuple.pos` = 0/1/2 | positional `_types[pos].s` pick | **QUICK WIN (range-∀/index)**: replace chain with `_types[_tuple.pos].s` (one index, no chain, no pass) | low — guard `_tuple.pos ≤ 3` in-bounds |
| posbind.ev:279 | `param_name` | `_tuple.pos` | positional `_param_names[pos].s` | **QUICK WIN**: `_param_names[_tuple.pos].s` | low — same in-bounds guard |
| posbind.ev:300 | `bindzip_count` | `_tuple.arity` | arity→count arithmetic | **not-a-smell** (arithmetic fold) | low |
| posbind.ev:305 | `bindzip_binds[0].name` | `_has_tuple`,arity,pos | positional bind assembly (range-∀ pin) | **not-a-smell** (order-sensitive bind tape) | medium — covering write per slot |
| posbind.ev:310 | `bindzip_binds[0].handle` | same | same | not-a-smell | medium |
| posbind.ev:315 | `bindzip_binds[1].name` | same | same | not-a-smell | medium |
| posbind.ev:318 | `bindzip_binds[1].handle` | same | same | not-a-smell | medium |
| posbind.ev:321 | `bindzip_binds[2].name` | same | same | not-a-smell | medium |
| posbind.ev:324 | `bindzip_binds[2].handle` | same | same | not-a-smell | medium |
| posbind.ev:333 | `bindzip_pop` | `_has_tuple`,`_tuple.arity` | positional cons-tail pick | not-a-smell | low |
| driver.ev:356 | `ctor_items` | `_emit_cursor` = 0/1 (inline) | positional arg pick `arg0/arg1/arg2` | **not-a-smell** (positional/step-indexed) | low |
| driver.ev:687 | `window_after` | `consume_n` = 0..6 | positional TokenList truncation | **not-a-smell** (positional walk; a `drop(seq, n)` accessor is a separate design) | low |
| driver.ev:838 | `primary_handle` | `strop_arity` = 3/2 | arity→handle positional pick | not-a-smell | low |
| driver.ev:839 | `secondary_handle` | `strop_arity` | same | not-a-smell | low |
| driver.ev:899 | `app_arg` | `_item_step` = 0..4 | step→handle positional pick (step IS subject) | not-a-smell | low |
| calllower.ev:67 | `cardinality_items` | `cardinality` = 1/2 | cardinality→record-value assembly | **not-a-smell** (order-sensitive cons assembly) | low |
| claimidx.ev:223 | `branch_fold` | `_efflit.count` | count→conjunction-fold depth | **not-a-smell** (arithmetic fold over a count) | low |
| claimidx.ev:238 | `fold_items` | `_chain_n` = 1/2/3/≥4 | depth→pre-built level pick (`chain_lvl0..4`) | **not-a-smell** (positional depth select) | low |
| expr.ev:427 | `call_e` | `call_arg_count` = 1/2/3 | arity→ECall1/2/3 ctor pick | **not-a-smell** (arity-dispatched constructor, 3 arms) | low |
| expr.ev:430 | `call_base` | `call_arg_count` | same | not-a-smell | low |
| record.ev:132 | `rows_names` | `_param_n` = 0/1/2 | count→string concat fold | **not-a-smell** (string fold over a count) | low |
| record.ev:137 | `rows_types` | `_param_n` | same fold | not-a-smell | low |
| recval.ev:33 | `literal_items` | `nfields` = 1..5 | count→record-literal cons assembly | **not-a-smell** (positional cons assembly) | low |
| driver.ev:799 | `phase` | `_phase` = 0/2 | phase transition | **not-a-smell** (2-test phase machine, near-hold, low value) | low |

### Event-ordered carried writes / single-writer schedule — not-a-smell (order IS the semantics)

| loc | LHS | dispatches on | operation | proposed | coverage risk |
| --- | --- | ------------- | --------- | -------- | ------------- |
| driver.ev:512 | `handle_stack` | event guards (`enter_claim`/`is_call_fire`/…) | carried stack transition (prioritized events) | **not-a-smell** (FSM transition; default is a non-`_x` expr so the metric counts it, but order is load-bearing) | n/a |
| driver.ev:533 | `work_items` | event guards | carried work-list transition | **not-a-smell** (same) | n/a |
| driver.ev:998 | `effects` | phase/condition guards (62 tests) | the top-level effects schedule (whole tick-dispatch) | **not-a-smell / blessed-adjacent** (single-writer `effects`, prioritized events — §3.4 hold-family; arms are distinct conditions, not key tests on one discriminant). Optional separate structural refactor: `effects = log ++ work ++ exit` per CLAUDE.md | n/a — single-writer, order-load-bearing |

---

## Prioritized worklist

### Milestone 1 (prerequisite, not a chain) — land the guarded-pin-family lowering

`docs/plans/guarded-pin-family-lowering.md` is **proposed, not landed**.
Every PIN-family / PIN-mixed rewrite is blocked on it. The KP-pair
lowering already exists, so pure-registry KP-pair rewrites are unblocked.
**Do Milestone 1 first**; gate on its own fixtures +
`scripts/functionization-gate.sh`.

### Group A — Quick wins (mostly unblocked / trivial) — ~0.5 day

1. **posbind.ev:268 `param_ty`, posbind.ev:279 `param_name`** — direct
   positional indices spelled as chains; replace with
   `_types[_tuple.pos].s` / `_param_names[_tuple.pos].s` (**no pass
   needed**, unblocked now). [gate] `model-diff DriverPosBind --observe
   param_ty` (resp. `param_name`); inputs `_tuple`, `_types`/`_param_names`.
   Risk: guard `_tuple.pos ≤ 3` (model-diff witnesses an OOB input).
2. **lexer.ev:228 `out` (EscapeChar), lexer.ev:252 `w` (IndentWidth),
   translate2_ctor.ev:192 `sort_slot`** — tiny disjoint PIN-families
   (blocked on Milestone 1, then trivial). [gate] `model-diff <claim>
   --observe <lhs>`.

### Group B — Cluster: the typename→sortcode/sort-handle registry — ~1.5 days, **highest leverage**

Extract ONE `sort_codes ∈ Seq(SortEntry)` registry (`typename ↦ code`,
plus a `code ↦ handle` companion) and make every site a **KP-pair**
(unblocked) or PIN-mixed. Sites: classify.ev:58, recval.ev:50,
posbind.ev:214, enum.ev:196 (**unblocked today** — non-driver), plus
driver.ev:422 and driver.ev:857 (**deferred**, sibling-owned). Retires
~6 chains + a documented dedup debt. [gate] per-site `model-diff
<claim> --observe <lhs>`; the registry keys must be unique (model-diff
witnesses a collision as DIFFER); each site's default (`: 0`/`: "Int"`)
must become the KP-pair `¬∃` default.

### Group C — Cluster: case-code state machines (enum+match) — ~3–5 days, needs-design

The `_decl_action`/`_decl_step`, `_recdecl_st`, `_parse_mode`,
`emit_state`, `_parser.kind`, and `is_*_item` item-kind discriminants
are case codes that want enums (baseline W1–W3; explicitly **out of
scope for the guarded-pin lowering**). Chains: enum:209/216,
buildeff:130, record:174/184/249, window:202/35, posbind:350/364,
driver.ev:482/595/961. Each needs a typed-state enum introduced first,
then the transition becomes `match`. **High effort, high design** (FSM
extraction; per-enum operator buy-in). Do AFTER A/B. [gate] `model-diff`
over each fsm's carried-state interface (`--observe st_now`/`step_now`/
`need`/…), discriminant + carry as inputs.

### Group D — Lexer/tag tables + small projections — ~1 day

lexer.ev:142/161, lex_fti.ev:67/111 (char/keyword tag tables → KP-pair
over shared registries, pairing the `recognized` Bool with the `∃` of
the table), lexer.ev:127 (digit table), enum.ev:126/130 (`_source`
union → PIN-family / `enum Source`+match), matchpin.ev:142 (PIN-mixed),
driver.ev:921 `denom` (PIN-family twin of `frac_pow`),
driver.ev:911/942 floor-handle picks (PIN-family). Self-contained;
two lexer tables dedup across files. [gate] model-diff per claim;
observe the `tag`+`recognized` pair together.

### Deferred — sibling-agent-owned files (BLOCKED until that work settles)

`driver.ev` and `translate2_bool.ev` are owned by a sibling agent. **Do
not touch.** Counted chains:

- **driver.ev (16 chains):** :356, :422, :482, :512, :533, :595, :687,
  :799, :838, :839, :857, :899, :911, :921, :942, :961, :998. Several
  are Group-B members (:422, :857 sortcode cluster) and Group-C members
  (:482, :595, :961 item-kind machine) — so those clusters are **partly
  blocked on the sibling agent**: the registry extraction (B) and the
  item-kind enum (C) land in driver.ev. Coordinate before starting B/C
  on the driver side.
- **translate2_bool.ev:** 0 counted chains in the current source —
  nothing deferred there, but off-limits regardless.

### Not-a-smell (leave as-is) — genuine folds / positional / event-ordered

Do NOT rewrite (purism §3.1 positional exception, §3.4 hold-family,
arithmetic folds):

- **Single-writer / event-ordered carried writes** (order is the
  semantics): driver.ev:998 `effects`, :512 `handle_stack`, :533
  `work_items`, :799 `phase`.
- **Positional stack/handle/step selects** (position is the subject):
  compose:110/113/115/117/120/159, posbind:258/261/264/283-adjacent/
  300/305/310/315/318/321/324/333, driver.ev:356/687/838/839/899/911/
  942, claimidx:238, expr:427/430, calllower:67, recval:33.
- **Arithmetic/concat folds over a count**: posbind:300, record:132/137,
  claimidx:223, compose:159.
- **2-arm / V18-gap**: matchpin:287 `dtail`, translate2_ctor:136 `pick`
  (Seq(enum)-element gap — needs the cons-element lowering, not a chain
  rewrite).

A few "positional" rows could become a `stack.nth(k)` / `drop(seq, n)`
accessor — a **separate design** (new blessed builtin requiring an
operator ruling, `docs/evident-purism.md` §5), out of scope here.

---

## Summary counts

- **Total censused:** 71 (matches the metric in this worktree).
- **Quick wins (Group A):** ~5 (posbind:268/279 need no pass; lexer:228/
  252 + ctor:192 trivial post-Milestone-1).
- **Cluster — sortcode/sort-handle (Group B):** ~6 chains across 6 sites;
  4 unblocked today (classify:58, recval:50, posbind:214, enum:196),
  2 deferred (driver.ev:422/857). Highest single-refactor leverage.
- **Cluster — case-code state machines (Group C, enum+match,
  needs-design):** ~13 (enum:209/216, buildeff:130, record:174/184/249,
  window:202/35, posbind:350/364, driver.ev:482/595/961).
- **Cluster — lexer/tag + small projections (Group D):** ~10 (lexer:127/
  142/161, lex_fti:67/111, enum:126/130, matchpin:142, driver:921/911/942).
- **Deferred (sibling-owned driver.ev):** 16 (several overlap B/C, so
  those clusters are partly blocked); translate2_bool.ev: 0.
- **Not-a-smell (leave):** ~28 (positional selects, event-ordered carried
  writes, arithmetic folds, 2-arm/V18-gap).

Buckets overlap (a driver.ev sortcode chain is both Group-B and
deferred). Headline: **~21 chains are honest value-selection smells with
a blessed surface** (A/B/D), **~13 are enum+match deficits** (C, the big
design lift), and **~28 are not smells at all**. Roughly 40% of the
count is the metric over-counting genuine folds/positional selects —
expected, since the metric is a cheap text heuristic.

## Top 3 highest-leverage refactors to start (once Milestone 1 lands)

1. **Group B — the typename→sortcode/sort-handle registry.** One
   `sort_codes` registry retires ~6 chains across duplicated sites and a
   long-standing dedup debt. KP-pair lowering already exists, so the
   non-driver.ev sites (classify:58, recval:50, posbind:214, enum:196)
   are unblocked **today** — start here, independent of the sibling agent.
2. **Group A quick wins — posbind:268/279 direct indices.** Zero-pass,
   zero-risk (`xs[pos]`), immediate metric drop; ideal first commit to
   calibrate the `model-diff` gate end-to-end on the smallest change.
3. **Group D — the lexer/lex_fti tag tables as a KP-pair cluster.** Four
   char/keyword tables (two dedup-able across the files) become KP-pairs
   over shared tag registries, and the `recognized` Bool stops being a
   parallel hand-written disjunction (it becomes the table's `∃`).
   Self-contained, mid-size, no sibling-agent coupling.

Group C (the case-code state machines) is the largest burndown but a
**design project** (FSM-to-enum extraction, per-enum operator buy-in) —
schedule it last, after the mechanical clusters prove the model-diff gate.
