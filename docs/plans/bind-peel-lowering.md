# Bind-peel lowering — retiring the S1 bind half (`bind_n0..n5`)

**Status:** option (a) LANDED (2026-06-10). The remaining half of the
baseline report's S1 BLOCKER complex (`docs/critic-reports/compiler2-baseline.md`,
2026-06-10); the record half resolved in merge `b46f373`. Peel deleted
(bind-peel tokens 99→0), `C2Binds` deleted, consumers on the window
surfaces; staging columns are `Seq(Bind)` (compose `bind_stage`,
posbind `bindzip_binds`).

## Resolved before implementation (2026-06-10)

1. **The ≤4-vs-6 contract contradiction.** The compose MAINTAINS line
   ("≤4 binds per call") is stale: the slot-call grammar gates
   (`callw_slot`/`callw_pun` admit up to `_slot_count < 6` + the firing
   slot) and `binds_new` both carry **6**; positional `bindzip` caps at
   4 (`tup_binds_full` max = argc 3 + record pair = 4). The real
   per-call max is **6**; the contract line is corrected to ≤6.
2. **Tape bound: 12.** Measured on the conformance corpus
   (156 fixtures + the stdlib they import): max binds in one call = 3
   (mapped), 4 by positional grammar; **zero nested bind-carrying
   calls** (no fixture composes a binds-carrying claim from inside
   another) — measured peak live binds = 4. Contract-true worst case
   is 6×8 = 48, but reader chains pay per slot, so the tape is sized
   12 = grammar cap (6) × depth 2 = 3× the measured peak, with
   `bind_base ≤ bind_top ≤ 12` as a kernel-checked invariant: overflow
   is a loud UNSAT (exit 2), never a silent drop.
3. **Frames save BOTH cursors** (deviation from the single-Int sketch
   above). Bare/`..`/guard splices INHERIT the caller's bind window
   (the live code holds `binds = _binds` through `callw_bare_jump` /
   `guard_jump_fire`); a single saved base cannot restore both a fresh
   window (truncate to the popped window's base) and an inherited one
   (whose base IS the caller's). `CFCons` therefore carries
   `(ret, prefix, bind_base, bind_top, join_hdr, tail)` — pop restores
   both, uniformly for every frame kind. Still one wire change.
4. **Lowering extension 1 (guarded multi-append) is NOT needed.** Pop
   is a *truncation*, which no append form expresses — the tape is
   written registry-style instead (`∀ k ∈ {0..11} : binds[k].f = (… ∧
   _bind_top + j = k) ? staged[j] : _binds[k].f`, the blessed
   user_variants alloc pattern), with explicit `bind_base`/`bind_top`
   cursor recurrences. Extension 2 narrows to: the index-form
   keyed-projection PAIR accepts the two window conjuncts
   `(k ≥ LO) ∧ (k < HI)` before the key equality. The ∃ rule already
   substitutes the predicate wholesale — window existentials need no
   pass change.

## Problem

compiler2 carries the inline-frame bind table as a cons-list enum and
peels it into per-slot scalars for cross-module consumption
(compiler2/driver_compose.ev:48–98):

```evident
bind_n0 ∈ String = match _binds
    CBCons(n, _, _) ⇒ n
    CBNil ⇒ ""
bind_h0 ∈ Int = match _binds
    CBCons(_, h, _) ⇒ h
    CBNil ⇒ 0
bind_tail0 ∈ C2Binds = match _binds
    CBCons(_, _, r) ⇒ r
    CBNil ⇒ CBNil
-- … ×6 (bind_n0..n5 / bind_h0..h5 / bind_tail0..4)
```

Three consumers read the peel as membership disjunctions / selection
chains:

- driver_symlookup.ev:25–34 — `bound_found` (6-way ∨) +
  `bound_handle` (6-arm chain);
- driver_classify.ev:173–175 — `name_bound` (the classifier's
  no-redeclare rule);
- driver_posbind.ev:152–154 — `elem_is_bound`.

Per the purism rulebook (`docs/evident-purism.md` §3.6/V3): per-slot
scalar families never appear in source — they are transform output.
The baseline graded all of this BLOCKER and ruled that **the durable
fix is the lowering, not the peel** (§1.5: keep the surface, change
the lowering). This doc designs that fix.

## Constraints the design must satisfy

1. **The C2Frames/C2Binds wire format.** `enum C2Binds = CBNil |
   CBCons(String, Int, C2Binds)`; `enum C2Frames = CFNil |
   CFCons(Int, String, C2Binds, C2Frames)` (driver_ir.ev:154–160).
   `CFCons` carries the caller's whole bind list so `frame_pop`
   restores it (`binds = … frame_pop ? frame_binds …`,
   driver_compose.ev:213–218). Changing either enum is a wire-format
   change touching every matcher: driver_compose, driver_posbind
   (which also *builds* binds — `bindzip_binds`/`tup_binds` via
   `CBCons`, driver_posbind.ev:229–242), and driver_ir.
2. **Carry mechanics.** Cons-list enums carry natively (CLAUDE.md
   state-carry rules). A carried bounded `Seq(R)` carries via the
   pre-oracle lowering to flat scalars — proven by the record-half
   conversion (`recs ∈ Seq(RecTypeEntry)`, merge `b46f373`) and by
   the lowering's dynamic-index select chains (commit `a406494`).
   A carried Seq the *oracle* sees does not functionize
   (post-cutover-roadmap.md, measured 2026-06-09) — everything below
   stays behind `scripts/passes/lower-bounded-seq.sh`.
3. **Purism.** Per-slot scalars banned in source (V3); new grammar
   over cons lists is V1 territory (the tuple-bind precedent,
   `d1be22a` invented / `2b0efb2` reverted).
4. **The v5 width lesson** (`docs/plans/sample-rung-walls.md`,
   2026-06-10): lowered select-chain depth multiplies per-tick interp
   cost (user_variants 6→160 slots measured 0.5→6.7 ms/tick, 13×).
   Whatever replaces the peel must keep reader chains narrow.
   Mitigating measured fact: ITE/∧/∨ are lazy in the interp
   (passes-in-evident-walls.md, 2026-06-10), so a chain costs the
   distance actually scanned — but a *miss* (the common case for
   "is this name bound?") scans every guard, so the bound itself is
   still the cost.

## Options

### (a) Global bind tape + per-frame base cursor — RECOMMENDED

`type Bind(name ∈ String, handle ∈ Int)`; one bounded registry
`binds ∈ Seq(Bind)` (the **bind tape**) with a length cursor, where
the **current frame's binds are always the top segment**
`[bind_base, #binds)`:

- **push** (`frame_jump`): the frame saves the caller's `bind_base`
  (an Int) — `CFCons(ret, prefix, bind_base, tail)`; the callee's
  binds are appended above the caller's; `bind_base` moves to the
  old length.
- **pop** (`frame_pop`): restored length = the popped window's base
  (`#binds ← _bind_base`), restored base = the Int saved in the
  frame. No copying, no list payload in the frame.
- **reads** become the blessed §2.5 surfaces over the live window:

```evident
bound_found = (∃ i ∈ {0..#binds-1} : (i ≥ bind_base ∧ binds[i].name = lookup_name))
∀ i ∈ {0..#binds-1} : ((i ≥ bind_base ∧ binds[i].name = lookup_name) ⇒ (bound_handle = binds[i].handle))
(¬(∃ i ∈ {0..#binds-1} : (i ≥ bind_base ∧ binds[i].name = lookup_name))) ⇒ (bound_handle = 0)
```

  (Index form rather than element form because the window predicate
  is genuinely positional — the frame boundary IS a position, the
  §3.1 honest-positional class, same ruling as the `h_top/h_2nd`
  stack views.)

- **writes**: `binds_new`'s six-way cons construction
  (driver_compose.ev:142–155) and posbind's `bindzip_*` builders
  become guarded appends at the cursor.

Wire-format cost: one payload-type change in `CFCons`
(`C2Binds` → `Int`); `C2Binds`/`CBCons` deleted outright once
posbind's builders convert. Blast radius: driver_compose,
driver_posbind, driver_ir, plus the three consumer rewrites.

Width cost: tape bound = depth × binds-per-call. The module contract
says ≤4 binds per call at depth ≤8 (driver_compose.ev MAINTAINS),
but `binds_new` admits 6 slot binds — reconcile the contract first;
worst case bound 48, contract-true bound 32. Reader chains deepen
from 6 arms to the tape bound. Projecting from the v5 pair the way
the multi-enum plan does (~0.005 ms/tick per chain-arm across that
widening's readers): ~4 reader chains × ~40 extra arms ≈ on the
order of +1 ms/tick — a projection, not a measurement; the
acceptance check is `scripts/functionization-gate.sh` plus the
guard-demo ms/tick number, and the fallback is shrinking the tape
bound (depth 8 is generous — measure actual peak depth on
conformance first).

### (b) Auto-peel transform: surface keeps the cons list

A `lower-bounded-cons` rule: declare `binds ∈ C2Binds` with a static
bound and let the transform synthesize the `bind_n*/bind_h*` peel,
plus some quantifier surface for the consumers. Rejected:

- The consumers need a surface to write. `∃ b ∈ binds : …` over an
  enum cons list is **not in the blessed catalog** (§2.5's forms are
  Seq forms); inventing it and teaching a pass to lower it is
  exactly the V1 laundering pattern the rulebook's calibration
  incident pins (tuple-bind, reverted `2b0efb2`). It would need an
  operator ruling for grammar whose only purpose is to preserve an
  encoding we have a blessed alternative to.
- The transform must know `CBCons`'s payload shape (name at slot 0,
  handle at slot 1, tail at slot 2) — a per-enum special case wired
  into a text pass, where the Seq lowering is shape-generic over
  record fields.
- It leaves two representations of "a keyed registry" live in the
  tree (bounded Seq for recs/set_vars/user_variants, bounded cons
  for binds) with different lowering machinery each.

### (c) Frames AND binds as bounded Seqs with a depth cursor

Option (a) plus converting `frames` to `frames ∈ Seq(Frame)`,
`#frames ≤ 8`, `Frame(ret ∈ Int, prefix ∈ String, bind_base ∈ Int)`,
indexed by `frame_depth`. Deletes both wire enums. Rejected as the
first step, kept as the follow-on:

- The frame peels (`frame_ret`/`frame_prefix`/`frame_binds`,
  driver_compose.ev:100–110) are head-of-stack `match` reads — the
  baseline did NOT flag them; a stack's interface is depth order
  (honest-positional, the `h_top/h_2nd` ruling). There is no purity
  debt to repay on `frames`, so converting it buys uniformity, not
  adherence, at the price of a second wire change and dynamic-index
  chains where a head `match` is free.
- Once (a) lands, `CFCons` is `(Int, String, Int, C2Frames)` — all
  scalar payloads; converting later is mechanical if a reason
  appears.

## Recommendation

**Option (a).** It is the same move as the record half (`b46f373`):
the blessed bounded-Seq registry surface over the existing lowering,
with the one genuinely stack-shaped structure (`frames`) staying a
cons list because stacks are the honest use of one.

### Lowering extensions required (do these first)

1. **Guarded multi-append.** The lowering's append rule
   (`scripts/passes/lower-bounded-seq.sh` header, REWRITE RULES) is
   single-element per tick (`_xs ++ ⟨v⟩`). A slot call binds up to 6
   names in ONE tick (`callw_fire`), so the rule must extend to
   `xs = (… G ? _xs ++ ⟨e1⟩ ++ … ++ ⟨ek⟩ …)` — slot writes at
   `_xs_len … _xs_len+k-1`, `xs_len = _xs_len + k`, each element
   under its own count guard (`bindzip_plain`'s argc cases become
   per-element guards).
2. **Range-restricted ∃/∀-pin.** The existential and keyed-projection
   rules must accept the extra positional conjunct
   `i ≥ bind_base` (substituting `i` per slot as a literal so the
   conjunct constant-folds). Verify whether `a406494`'s
   dynamic-index work already covers the substitution; if not, it is
   one more substitution rule in the same family.

Both extensions get fixtures in `tests/compiler2_units/seq_lowering/`
+ `tests/seq/` before any compiler2 edit (the lowering's own
conformance, per the roadmap's parity rule).

### Migration sketch

1. Reconcile the binds-per-call contract (4 vs 6) and measure actual
   peak `frame_depth` × binds on the conformance corpus → pick the
   tape bound (unmeasured today; do not guess it into the type).
2. Land lowering extensions 1–2 with fixtures (no compiler2 change;
   flatten output on compiler2/driver.ev byte-identical since
   nothing uses the new rules yet).
3. driver_compose: add `Bind`, the tape + cursor + window writes;
   `CFCons` payload `C2Binds` → `Int` (the wire change, one commit);
   keep `bind_n*` temporarily DERIVED from the tape so consumers are
   untouched — gate on compose/symlookup/classify/posbind isolation
   units + functionization-gate.
4. Rewrite the three consumers to the window existential/projection;
   delete `bind_n0..n5`/`bind_h0..h5`/`bind_tail0..4`.
5. Convert posbind's `bindzip_*`/`tup_binds` builders to guarded
   appends; delete `C2Binds` from driver_ir.
6. Batch gate: full conformance (137/138 bar) + guard-demo ms/tick
   vs the 6.7 ms/tick baseline. Per-step commits so a bail bisects.
