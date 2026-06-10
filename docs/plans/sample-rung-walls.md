# sample.ev self-host rung — walls hit and capacities widened

Date: 2026-06-10. Rung: compile `compiler/sample.ev` (flattened, 5,326
lines) through the compiler2-built stage1 driver
(`.goalpost/artifacts/compiler2-stage1.smt2`), then check behavioural
equivalence against the committed `sample.smt2` (53,061 lines) with
`scripts/verdict-equiv.sh` over `tests/lang_tests/*.ev`. The committed
artifact is NOT replaced by this rung.

Companion census: `docs/plans/sample-ev-gap-census.md` (corpus shape:
127 claims, 31 enums, 0 types).

## Capacity walls (widened in commit 1601d39, conformance 137/138 green)

compiler2's enum machinery was sized for single-digit test enums.
sample.ev carries **142 user variants across 31 enums**; the widest
enum (`Token`) has **57 variants** (census 2026-06-10). Three coupled
walls:

### 1. ctor-handle array clobber (driver_buildeff)

`enum_ctors_p` sat at `z_arena + 64`, giving **8 slots** before the
array ran into `enum_sortnames_p` at `+128`. Variant 9+ of any enum
overwrote that slot; the corrupted sort-name pointer made
`Z3_query_constructor` fail **EINVAL**. Bisected 2026-06-10: an enum
with 7 variants compiles, 10 fails. Fix: move the ctor array to
`z_arena + 512` with **128 slots** (+512..+1536; Token needs 57), and
widen the arena malloc **512 → 2048 bytes** to hold it.

### 2. cross-enum registry collision (driver_enum)

`user_variants` slots were allocated positionally by `_enum_vidx`,
which **resets on each `enum_go`** — so the 2nd enum's variant 0
overwrote the 1st enum's variant 0. Reads are name-keyed (variant
names are globally unique), so the registry must hold every user
variant of the whole program at once. Fix: a new **global allocation
cursor** `variant_alloc` (monotone, never resets) replaces
`_enum_vidx` as the write index. This is the "allocate by position,
everything else by key" registry rule (commit ebb7e48) applied across
enum boundaries.

### 3. registry capacities (driver_enum, driver_pratt)

- `user_variants`: **6 → 160** slots (142 needed + headroom).
- `enum_values`: cap **6 → 128**.
- driver_pratt's callable-name string spliced exactly
  `user_variants[0..5].name` — six hardcoded slots. Replaced by a new
  `variant_names` string registry in driver_enum that accumulates
  every variant call-name (31-char padded, `|`-separated) as variants
  are declared, which driver_pratt splices wholesale.

Unit-fixture follow-on: `tests/compiler2_units/driver_pratt/entry_kind.ev`
gained the `variant_names` carry declaration.

Gate evidence: conformance 137/138 (only the known
`123-subschema-shadowing-quantifier` failure), not bailed — the
widening costs nothing on the existing corpus.

## Compile-attempt log

| # | stage1 build | result |
|---|---|---|
| 1 | pre-widening (6-slot registries, +64 ctor array) | exited after ~750 s with **zero output lines**; stderr carried only the functionizer summary (`2114 total / 617 JIT / 1347 interp / 71 residual; 0.0 ms z3`). Diagnosed as wall 1 (ctor clobber → EINVAL). |
| 2 | post-widening (05:16 build, contains 2048 arena) | launched 05:15–05:16, `EVIDENT_TICK_LIMIT=2000000 timeout 3600`; outcome recorded below. |

## Attempt v2 outcome (2026-06-10 05:15–05:32): rc=7, ROOT-CAUSED below

**Correction (2026-06-10, later the same day): v2 did NOT exhaust the
tick budget.** A tick-limit halt exits **3** with `kernel: tick limit
(N) reached` on stderr (kernel/src/main.rs maps every `tick::run` Err
to 3); v2's stderr carried no such line and the exit code was **7** —
a *driver-emitted* `Exit(7)`, the TernaryBuildZ3 null-operand guard
(the only Exit(7) in compiler2). Both v1 (rc=7 @750s) and v2 (rc=7
@992s) died at the SAME guard at the same input position; the 750→992s
ratio matches the 2114→2976 widened step count, not a budget edge.

```
[functionizer] 2976 total / 894 JIT / 1932 interp / 71 residual;
992629.8 ms total (889154.1 ms func / 0.0 ms z3)
```

Two findings stand regardless: **0.0 ms z3 at compiler-scale input**
(the constraint architecture holds), and **~0.5 ms/tick interp cost**
(the wave-5c-adjacent throughput concern is real — it makes every
attempt slow — but it is not what killed the compile).

## Root cause of rc=7 (bisected via 9 probe fixtures, 2026-06-10)

The 0-handle reaching the ternary guard was the END of a silent causal
chain that starts in enum-decl DISPATCH. Three stacked grammar gaps,
all of which used to fail silently:

### Wall A — ONE user enum per program (the primary blocker)

`enter_enum_decl` (compiler2/driver.ev) requires `(¬_user_enum_done)`:
the SECOND and every later user `enum` decl falls through to
`enter_skip` and is silently dropped. Every variant of every dropped
enum resolves to 0 handles at use (ternary branch → Exit(7); pin RHS →
kernel null-AST guard, `Error: invalid argument`, rc=1). The singular
`user_enum_name` / `user_enum_sort` registers and driver_classify's
`line_ty_name = _user_enum_name` membership sort-code all assume one
user enum. sample.ev declares **31**. The conformance corpus (137/138
green) never exercises two user enums — which is why this survived.
Repro: `tests/seam/known-failing/repro_second_enum.ev`.

### Wall B — variant payload arity ≤ 2

The variant walker (compiler2/driver_claimidx.ev) has `variant_pay1` /
`variant_pay2` forms only; a 3-field payload spans past the 8-token
lookahead window. sample.ev: `FloatLit(Int,Int,Int)`,
`EBinOp(Op,Expr,Expr)`, `ECall3(...)`, `ETernary(...)`. Repro:
`tests/seam/known-failing/repro_payload_arity3.ev`.

### Wall C — payload types limited to Int/Bool/String/Real/self

`variant_ty0_ok/ty1_ok` whitelist (driver_claimidx.ev) admits the four
scalar types plus self-reference (`_user_enum_name`); `FieldSortSlot`
(translate2_ctor.ev) and the `field_sort*` fallthroughs (driver_enum.ev)
can produce sorts only for those plus the floor types. A cross-enum
payload (`EBinOp(Op, …)`, `MArm(MatchPattern, Expr)`, every cons-list
over a payload type) has no sort source — there is no per-enum sort
registry and no mutual-recursion (multi-sort `Z3_mk_datatypes`) build.
**33 of sample.ev's 157 variants** need Wall B and/or Wall C support
(census script in git history; `Nat` is also missing from the
whitelist).

### What landed now: silent → LOUD (commit pending, gates green)

A compiler must error nameably, not 992s later (the TernaryBuildZ3
precedent). Two guards:

- `variant_unsupported` (Wall B/C at the walker) → puts diagnostic +
  **Exit(8)**.
- `enum_decl_second` (Wall A at dispatch) → puts diagnostic +
  **Exit(9)**.

sample.ev now fails in the parse phase with the Wall-A message instead
of rc=7 after 16 minutes. Probe battery: arity-3 → 8, second-enum → 9,
all previously-supported shapes (incl. 60-variant single enum,
payload-ctor ternaries, repro_deep) still compile.

### The remaining rung plan (out of this session's scope)

Wall A/B/C are one wave: a user-enum registry (name → sort/ctor-list,
the same bounded-Seq registry pattern as `user_variants`), batched
multi-sort `Z3_mk_datatypes` for mutual recursion, an N-field variant
walk (multi-tick, not window-bound), and `FieldSortSlot` consulting the
registry. The gap census called this C1 and sized it L — that estimate
stands. The ~0.5 ms/tick interp throughput item is the *second* wall
behind it (a full-sample compile at current speed is ~20+ min even
once it compiles).

## v5 datum (2026-06-10 06:10): registry WIDTH is a throughput multiplier

The guard-demo run (default 100k tick limit, widened-registry stage1)
measured **6.7 ms/tick vs v2's 0.5 ms/tick — 13× slower per tick at the
same ~2978 step count**. The widening (user_variants 6→160 slots) deepens
the per-slot select chains inside each step; per-step interp cost scales
with chain depth. Consequence for the multi-enum grammar work: the
per-enum sort registry must NOT be a wide flat slot family — a tape-side
(FTI) registry or a narrow per-enum window is required. (The full-sample
Exit(9) demo remains undemonstrated — it now needs ~150k+ ticks at this
speed; the guard logic is probe-proven instead.)
