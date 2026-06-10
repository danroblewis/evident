# Multi-enum grammar — unblocking the sample self-host rung

**Status:** LANDED (2026-06-10). Walls A/B/C retired; the
known-failing repros graduated into `tests/seam/` and conformance
fixtures 149–153 pin the feature (two-enums, cross-enum payload,
mutual-recursion pair, arity-3 match read-back, unsat dual).
Deviations from the sketch below: the build group is collected as a
prepend cons list during the variant walks (no second token pass);
acts 1 and 3 peel the group in the same order, so the global ctor
cursor needs no per-member base bookkeeping; match-pin arm patterns
walk multi-tick with one bind at any field index (accessors 1..5 on
an acc-table tape row per registry slot, field 0 via the registry);
a matches-pattern skim in DriverPratt consumes 3+-element patterns
past the window. Known residual gap (named, loud at use): ctor
APPLICATIONS in expressions still cap at 3 args (ECall3) — sample.ev's
4-field ECall3/ETernary variants will hit it at their call sites.

## Problem

The sample rung (compile `compiler/sample.ev` through the
compiler2-built stage1 driver) is blocked by three stacked grammar
gaps in compiler2, root-caused and made loud on 2026-06-10
(`docs/plans/sample-rung-walls.md`, commit `19acb52`):

- **Wall A — one user enum per program.** `enter_enum_decl`
  (compiler2/driver.ev:238) requires `(¬_user_enum_done)`; the second
  and every later user `enum` decl used to fall through to
  `enter_skip` and silently drop. Now: `enum_decl_second` → diagnostic
  + **Exit(9)**. Repro: `tests/seam/known-failing/repro_second_enum.ev`.
- **Wall B — variant payload arity ≤ 2.** The variant walker
  (compiler2/driver_claimidx.ev:78–84) has `variant_pay1`/`variant_pay2`
  forms only; a 3-field payload spans past the 8-token lookahead
  window. Now: `variant_unsupported` → diagnostic + **Exit(8)**.
  Repro: `tests/seam/known-failing/repro_payload_arity3.ev`.
- **Wall C — payload type whitelist.** `variant_ty0_ok/ty1_ok`
  (driver_claimidx.ev:74–76) admits Int/Bool/String/Real plus
  self-reference (`_user_enum_name`); `FieldSortSlot`
  (translate2_ctor.ev) and the `field_sort0/1/2` fallthroughs
  (driver_enum.ev:158–172) can produce sorts only for those plus the
  floor types. A cross-enum payload has no sort source. `Nat` is also
  missing from the whitelist.

These survived because the conformance corpus (137/138 green) never
exercises two user enums — the gap census and the v2 compile attempt
(rc=7 after 992 s at the TernaryBuildZ3 zero-handle guard) found them
the expensive way. The loud guards are the floor this plan builds on:
the feature work converts Exit(8)/Exit(9) sites into working paths,
and the known-failing repros graduate into passing fixtures.

## Requirement, quantified

From the census (`docs/plans/sample-ev-gap-census.md`, 2026-06-07) and
the walls doc (2026-06-10):

- sample.ev (flattened) declares **31 enums, 142 user variants**;
  the widest enum (`Token`) has **57 variants**.
- **33 of 157 variants** (user + floor) need Wall B and/or Wall C:
  `FloatLit(Int,Int,Int)`, `EBinOp(Op,Expr,Expr)`, `ECall3(...)`,
  `ETernary(...)`, `MArm(MatchPattern, Expr)`, and every cons-list
  over a payload enum type.
- Mutual recursion is **spec**, not an extra: CLAUDE.md's enum
  section pins `enum A = X(B) ; enum B = Y(A)` (forward refs +
  mutual recursion) as language. Any design that builds each enum's
  datatype in isolation cannot express a reference cycle.

## The design constraint that rules out the obvious encoding (v5 datum)

Measured 2026-06-10 (`sample-rung-walls.md`, "v5 datum", commit
`5f31b6f`): widening `user_variants` from 6 to 160 slots took per-tick
interp cost from **0.5 ms/tick to 6.7 ms/tick — 13× — at the same
~2978 step count**. The lowered per-slot select chains deepen with
registry WIDTH, and the functionizer interp evaluates every step
every tick regardless of program phase. Per-slot marginal cost from
that pair: roughly (6.7 − 0.5) / 154 ≈ **0.04 ms/tick per slot of
chain depth** (projection from the one measured pair, not a new
measurement).

Consequence: a per-enum sort registry written as another wide flat
carried slot family — 31 rows × (name, sort, ctor-list, variant
base/count) ≈ 155 more carried slots feeding keyed select chains —
projects to the same order of regression again, on top of the
existing 6.7 ms/tick. The registry must therefore be:

1. **tape-side** — rows in an FTI buffer reached through effects, so
   a lookup costs only on the tick that asks (the `sym_names` /
   `FtiNameEntry` fixed-width-32-row precedent, already the symbol
   table's encoding); or
2. **a narrow per-enum window** — only the enum currently being
   declared or queried occupies carried scalars; everything else
   lives on the tape.

These compose: the recommendation below uses both (tape rows as the
store, a one-row window as the working set).

## Design

### D1 — enum registry (retires Wall A)

A tape-side **enum table**: one fixed-width row per declared enum,
appended at `enum_done_user` time, carrying
`name → (sort handle, ctor-list handle, variant base index, variant
count)`. Probes go through the existing FTI name-entry pattern
(compose `FtiNameEntry`, `index_of` on the name column, then
`read_long` the handle cells) — the exact mechanics driver_symlookup
and driver_classify already use for symbols.

Dispatch changes (compiler2/driver.ev):

- `enter_enum_decl` drops `(¬_user_enum_done)`; `enum_decl_second`
  and Exit(9) are deleted.
- The singular `user_enum_name` / `user_enum_sort` registers become
  the **current-window** state: loaded when an enum decl enters the
  walk, written back to the tape row at finalize, and reloaded on
  query miss. They stop being "the one user enum" and become "the
  enum in hand".
- driver_classify's membership sort-code test
  (`line_ty_name = _user_enum_name`) becomes an enum-table probe:
  `line_ty_name` is enum-typed iff its padded key hits the enum
  table. Same `FtiNameEntry` + `index_of` shape as `name_dup`.

The cross-enum variant registry already generalized in commit
`1601d39`: `user_variants` (160 slots) is keyed by globally-unique
variant name with a monotone `variant_alloc` cursor, so variant
lookups need no per-enum anything. The enum table is only for
**enum-name** lookups: payload sort resolution (D3) and membership
classification.

### D2 — N-field variant walk (retires Wall B)

The single-window `variant_pay1`/`variant_pay2` recognizers become a
**multi-tick field loop**, the same move the ED machine already made
on the declare side (driver_enum.ev act 1 steps 2–4 walk fields one
micro-step at a time — read its module header: one variant per
(enum_act, enum_step) micro-step, one effect per tick, captures next
tick). The parse-side walker gets the matching loop: consume
`Ident [Comma]` per tick, accumulating into `field_slots`
(`Seq(EnumFieldSlot)`, today `#field_slots ≤ 3` — raise the bound to
the chosen arity cap). Arity cap: **6**, matching the existing
slot-call width (`slot_names` `#≤ 6`, ≤4 binds per call) — sample.ev
needs 3; the cap is a loud guard, not a silent window edge.

The 8-token window stops being the limiting resource because the
walker no longer needs the whole payload in view at once — exactly
how the body-line and slot-call walks already work.

### D3 — payload sort resolution via the enum table (retires Wall C)

`variant_ty0_ok`-style whitelists are replaced by a sort-resolution
order per field type name:

1. scalar floor: Int/Bool/String/Real (+ **Nat**, lowering to the Int
   sort with the `≥ 0` membership constraint the oracle applies);
2. self-reference: the enum currently in the window;
3. any name that hits the enum table (D1) → that row's sort handle;
4. any name in the **current mutual-recursion group** (D4) → a
   forward sort reference by group index;
5. otherwise → the existing `variant_unsupported` Exit(8), now
   meaning "genuinely unknown type", not "outside a whitelist".

### D4 — mutual-recursion datatype builds

Single-sort `Z3_mk_datatype`-per-enum cannot express `enum A = X(B);
enum B = Y(A)`. The Z3 C API's batched form — `Z3_mk_datatypes` over
N sort names + N constructor-list arrays, with cross-references made
by sort-reference index — exists for exactly this, and the ED
machine's act-2 finalize (sort sym → mk_constructor_list → write
batch → mk_datatypes → read sort) is already shaped as a batch of
size 1. The generalization:

- **Group rule:** consecutive top-level `enum` decls form one build
  group; the group is finalized (one `Z3_mk_datatypes` call, N sorts
  read back, N tape rows appended) when the first non-enum top-level
  decl arrives. Within a group, payload references to group members
  resolve as forward sort refs (D3 rule 4). Across groups, only
  already-built enums are referenceable — a forward reference to an
  enum declared *after* an intervening claim is a loud error. This
  matches how sample.ev is written (its enums cluster at the top of
  parser.ev/lexer.ev/kernel.ev) and gives CLAUDE.md's
  `enum A = X(B) ; enum B = Y(A)` adjacent-decl form its semantics.
- The ED machine runs once per **group** instead of once per enum:
  act 1 iterates variants of all group members (the walk state gains
  the member index), act 2 finalizes N sorts in one batch.
- Arena: the act-2 regions (`sort_names`, `clists`, `sorts_out`)
  become N-row arrays. The ctor array already moved to
  `z_arena + 512` with 128 slots and a 2048-byte arena (commit
  `1601d39`); the act-2 rows need a sizing pass of the same kind —
  31 names + 31 clists + 31 sorts do not fit the current +128/+136/
  +144 single-slot cells. This is the same class of capacity work as
  wall 1 of the walls doc; size it against sample.ev's worst group,
  not 1.

### D5 — conformance fixtures that land WITH the feature

The one-enum limit survived precisely because no conformance fixture
declares two user enums; the feature is not done until the corpus
pins it. Sketches (numbering on implementation):

```evident
-- two-user-enums-independent (sat; the Wall-A pin)
enum Light = Red | Green
enum Door = Open | Shut
claim main
    l ∈ Light = Green
    d ∈ Door = Shut
    ok ∈ Bool = ((l matches Green) ∧ (d matches Shut))
    effects ∈ Seq(Effect) = ⟨Exit(0)⟩
```

```evident
-- cross-enum-payload (sat; the Wall-C pin — second enum's payload
-- is the first enum)
enum Op = Plus | Minus
enum Node = Leaf(Int) | Branch(Op, Int)
claim main
    n ∈ Node = Branch(Plus, 3)
    v ∈ Int = match n
        Leaf(x) ⇒ x
        Branch(_, y) ⇒ y
    effects ∈ Seq(Effect) = ⟨Exit(v - 3)⟩
```

```evident
-- mutual-recursion-pair (sat; the D4 pin, straight from CLAUDE.md's
-- enum spec)
enum A = AEnd | AWrap(B)
enum B = BEnd | BWrap(A)
claim main
    x ∈ A = AWrap(BWrap(AEnd))
    ok ∈ Bool = (x matches AWrap(_))
    effects ∈ Seq(Effect) = ⟨Exit(0)⟩
```

Plus an arity-3 fixture (the `repro_payload_arity3.ev` shape with a
`match` read-back) and an unsat dual for the cross-enum fixture
(wrong variant pinned). The known-failing repros graduate: their
`-- expect:` headers already record the target
(`exit = 0 once the multi-enum registry lands`); move them from
`tests/seam/known-failing/` into the seam suite when green.

### Gates

- Conformance 137/138 + the new fixtures (the behavior gate; ~4 min,
  15-min bail cap).
- `scripts/functionization-gate.sh` after every step that touches
  carried state.
- The v5 methodology as a perf regression check: per-tick ms on the
  guard-demo run must not multiply again. Today's widened baseline is
  6.7 ms/tick; the budget for this work is "no worse", with the
  interp-throughput item (wave-5c adjacent) tracked separately as the
  multiplier on everything.

## Sequencing — behind claim headers

The claim-headers work (approved 2026-06-10, oracle commit `292c7ef`
on the oracle branch, pin bump in flight) and this plan touch the
same machinery: the claimidx registry (headers add an interface list;
this plan rewrites the variant walker), driver_classify (headers
restrict `name_outer` to header names; this plan replaces the
membership sort-code test), and compose/dispatch. Two agents in those
files at once is a merge knot; and headers change what the enum
machine's consumers may see of each other, which D1's window state
should be designed against rather than before.

Order: **claim headers land first; this plan rebases on them.** The
loud guards (Exit 8/9) keep the sample rung honestly red in the
meantime — nothing silently depends on the gap.

The second wall behind this one, named here so the plan doesn't
over-promise: even with all three walls down, a full sample.ev
compile at the current ~0.5 ms/tick interp throughput is ~20+ min
(walls doc estimate), and 6.7 ms/tick on the widened registries makes
the full-program Exit(9) demo need ~150k+ ticks. Grammar is the gate;
throughput is the multiplier; this plan removes the gate only.
