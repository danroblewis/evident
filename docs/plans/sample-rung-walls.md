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

## Attempt v2 outcome (2026-06-10 05:15–05:32): TICK-BUDGET WALL, not a crash

`EVIDENT_TICK_LIMIT=2000000 kernel compiler2-stage1.smt2 < sample_flat.ev`
(5,326 flattened lines) exhausted the full 2M-tick budget in ~992s and
exited with empty output (never reached emit). The decisive line:

```
[functionizer] 2976 total / 894 JIT / 1932 interp / 71 residual;
992629.8 ms total (889154.1 ms func / 0.0 ms z3)
```

- **0.0 ms z3 over 2M ticks** — the driver stayed fully functionized at
  compiler-scale input. The constraint architecture is NOT the wall.
- **~0.50 ms/tick, 90% in `func`** — the per-step interpreter (1932 of
  2976 steps interp, not JIT) is the entire cost. This is the SAME wall
  the autocarry-in-Evident port measured (docs/plans/passes-in-evident-walls.md:
  ~0.5–0.7µs per step per tick): convergent evidence from two independent
  workloads.
- Budget arithmetic: at ~375 ticks/input-line observed on fixtures, 5,326
  lines wants ~2M ticks just for the walk — the budget was at the knife
  edge. But raising the tick limit alone buys at best linear headroom at
  ~0.5 ms/tick (≥30 min compiles); the real fix is per-tick cost.

**Conclusion: the sample rung and the passes-in-Evident wiring share one
blocker — functionizer step-interp throughput (JIT coverage for the
string/accessor shapes that today fall to interp, or a kernel string-step
JIT). That is wave-5c-adjacent kernel work and is now the single highest-
leverage item on the self-hosting path.**
