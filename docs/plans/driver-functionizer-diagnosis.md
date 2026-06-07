# driver.smt2 functionizer refusal — diagnosis + fix plan

Date: 2026-06-07. Read-only investigation; no kernel/driver edits in
this session. Every claim below is traceable to a captured trace line
(raw snippets inline). Fix work is for a LATER operator-reviewed
kernel session.

## TL;DR

The driver is gated by **one** extract-time refusal class, not many:
Z3's `propagate-values` pass (run by the functionizer's own
`simplify_assertions` tactic chain) constant-folds 44 Bool
definitions into **bare unit literals** (`(assert v)` /
`(assert (not v))`), and `extract_program`'s collector has no case
for that shape. 9 of the 44 are manifest state fields, so
`build_body` fails and the whole program refuses with
`extract_program: an output had no covering assignment`. All 6,144
ticks of a small fixture compile then run on Z3 at ~46 ms/tick
(283 s total, 100.0% z3 / 0.0% func). An offline replica of the
collector shows that capturing unit literals covers **all 440
outputs**, leaves **zero** dependency cycles, and only 2 benign
residual predicates. The fix is ~10 lines in
`kernel/src/functionize/mod.rs::extract_program`.

## Method

- Kernel built from this tree (`cargo build --release`, kernel source
  identical to `main` — `git diff main -- kernel/` is empty).
- Driver emitted by the oracle from repo root:
  `evident-oracle emit compiler2/driver.ev driver_main -o $D/driver.smt2`
  (186,635 bytes; manifest carries **440 state fields** + `effects`).
- Fixture: `tests/conformance/features/017-nat-membership/source.ev`,
  flattened by `scripts/flatten-evident.sh` (255 lines / 11.8 KB).
- Runs (stdin = flattened path + `main`, per the wave-4o wire
  protocol) under combinations of `EVIDENT_FUNCTIONIZE_TRACE=1`,
  `EVIDENT_FUNCTIONIZE_STATS=verbose`, `EVIDENT_FUNCTIONIZE_WHY=1`,
  `EVIDENT_FUNCTIONIZE_DUMP=1`, `EVIDENT_PHASE_TRACE=1`.
- The 723 simplified+flattened body asserts (from `_DUMP=1`) were
  re-analyzed offline with a line-for-line replica of
  `extract_program`'s matchers (`try_record_guarded`,
  `split_not_eq_bool_both`, len/select pins, scalar `(= var expr)`,
  first-def-wins) to enumerate **all** uncovered outputs — the kernel
  itself stops at the first one.

## The refusal, verbatim

Load-time (extract-time, NOT verify-time — verify never ran):

```
[functionizer-why] uncovered output: d_fss_self
[functionizer-why]   has guarded?  false
[functionizer-why]   has scalar?   false
[functionizer-why]   seq_lengths?  None
[functionizer-why]   seq_elements? None
[fz] extract_program: an output had no covering assignment
[fz] not functionized — running Z3 path
[functionizer] load:
  body asserts: 723
  not functionized — fast path disabled; all 723 asserts run on Z3 each tick
  reason: extract_program: an output had no covering assignment
```

## Root cause — class 1 (the only gating class)

`d_fss_self` is defined in the emitted artifact as a perfectly
capturable scalar equality (driver.smt2 line 934):

```
(assert (= d_fss_self (= d_exit_fty "Effect")))
```

But the functionizer extracts from the **simplified** body
(`simplify_assertions` = `simplify` → `propagate-values`,
kernel/src/functionize/mod.rs:485). `d_exit_fty` is itself pinned to
a constant (`VariantFieldType` of the Exit variant's field 0 = "Int"),
so propagate-values folds the whole RHS and the assert becomes a bare
unit literal:

```
[fz/dump] flat[44] = (not d_fss_self)
```

`extract_program`'s collector (mod.rs:848–930) recognizes guarded
implications, `(not (= a b))` XOR shapes, `__len`/`select` pins, and
`(= var expr)` equalities — but a **bare Bool const** or `(not const)`
falls through to `raw.predicates`. `build_body` then finds no
covering assignment for the var, and if the var is a manifest output,
extraction refuses.

Source shape that produces it: any claim invoked with
**constant arguments** whose internal Bools constant-fold. Two
independent driver sites, 44 folded asserts total
(10 `v = true` / 34 `v = false`), of which **9 are state fields**:

1. `FieldSortSlot` (compiler2/translate2_ctor.ev:197,
   `is_self = (ty = enum_name)`) called with constant `ty` at
   compiler2/driver.ev:597–601 → `d_fss_self`.
2. `BoolCmpBuildZ3` (compiler2/translate2_bool.ev:84–92,
   `is_eq = (op matches OpEq)` … `ok = (is_eq ∨ …)`,
   `needs_not = is_ne`) called 4× with constant `op` pins
   (`d_op_geq = OpGeq`, `d_op_eq = OpEq`) at
   compiler2/driver.ev:1581–1614 → `d_nat_ok/d_nat_nn`,
   `d_pin_ok2/d_pin_nn`, `d_seleq_ok/d_seleq_nn`,
   `d_leneq_ok/d_leneq_nn` (+ 28 `BoolCmpBuildZ3__is_*__call{86,87,95,96}`
   intermediates and 7 `VariantFieldCount__vfc_*` /
   `VariantMkConstructorStep__vmc_nullary__call20` intermediates).

Complete uncovered-output list (offline replica over the dump; the
kernel's own run confirms the prefix — it covered ~80 outputs in
manifest order before dying on the first of these):

```
UNCOVERED OUTPUTS (9):
  d_fss_self  (flat[44]  = (not d_fss_self))
  d_leneq_nn  (flat[707] = (not d_leneq_nn))
  d_leneq_ok  (flat[706] = d_leneq_ok)
  d_nat_nn    (flat[671] = (not d_nat_nn))
  d_nat_ok    (flat[670] = d_nat_ok)
  d_pin_nn    (flat[681] = (not d_pin_nn))
  d_pin_ok2   (flat[680] = d_pin_ok2)
  d_seleq_nn  (flat[697] = (not d_seleq_nn))
  d_seleq_ok  (flat[696] = d_seleq_ok)
```

## What does NOT gate the driver (verified absent)

- **Seq state** (`state field is a Seq` refusal, mod.rs:1191): zero.
  All carried compound state (`wtoks:TokenList`, `witems:C2Items`,
  `d_pe:Expr`, `p_ops:PrOps`, …) is algebraic-datatype-sorted, which
  the fast path carries as `Sv::Datatype`.
- **Unsupported ops in step bodies**: an op-head census over all 723
  simplified asserts shows only eval-supported ops
  (kernel/src/functionize/eval.rs): `= ite and or not select + - * <= >=`,
  `str.substr/str.++/str.len/str.indexof` (SEQ_EXTRACT/CONCAT/LENGTH/INDEX),
  datatype constructors/accessors/recognizers (`(_ is X)`), numerals,
  strings. No `div`/`mod`, no `store`-heavy shapes, no const-arrays.
- **Dependency cycles**: with unit literals captured, topo over all
  720 step bodies orders 720/720 (offline replica; same dependency
  rule as `topo_order`, mod.rs:1015).
- **Match-heavy claims / guarded effects**: the simplifier's else-if
  `or`-chains all classified — the offline replica captures `effects`
  as the single guarded step and 719 scalar steps with **2** residual
  predicates, both trivially evaluable:

```
uncovered outputs after fix: 0 []
topo: 720/720 ordered; stuck (cycle) = 0
residual predicates after fix: 2
  flat[0] = (>= effects__len 0)
  flat[1] = (>= last_results__len 0)
step shapes: {'scalar': 719, 'guarded': 1}
```

## Measured per-tick breakdown

Full fixture-017 compile through the driver (exit 0, correct unit
emitted), `EVIDENT_FUNCTIONIZE_TRACE=1`:

```
ticks=6144 total z3=283.0s func=0.0s dispatch=0.1s
z3/tick: mean=46.06ms median=50.54ms min=19.79 max=6423.37
first 100 ticks: z3 mean=112.52ms func mean=0.0000 dispatch mean=0.0197
top5 z3 ticks: ['6423', '80', '57', '56', '56']
```

100% of tick time is Z3 solving; dispatch is noise (0.1 s of 283 s).
`EVIDENT_PHASE_TRACE=1` startup attribution (second run, evidentc
cache warm):

```
[phase +0.0s] body parsed: 722 asserts
[phase +0.0s] functionize done (functionized: false)
[phase +0.0s] tick 2
[phase +6.5s] tick 3          ← first content-bearing solve: 6.4 s
[phase +11.3s] tick 100       ← then ~50 ms/tick steady state
```

The 6.4 s outlier is the first solve after `ReadFile` lands the
11.8 KB source as a Z3 string constraint (`content_read`); it
disappears entirely once ticks stop being solves.

Reference: **compiler.smt2 compiling the same fixture, functionized**
(same kernel, same session):

```
[functionizer] 7852 total / 810 JIT / 6450 interp / 45 residual; 87475.3 ms total (80445.8 ms func / 0.0 ms z3)
compiler.smt2 ref: ticks=6771 func=80.4s z3=0.0s dispatch=0.0s mean func/tick=11.88ms
```

compiler.smt2 pays 11.9 ms/tick interpreting 7,852 steps. The driver
has **720** steps (~1/11 the size), so a functionized driver projects
to ~1–4 ms/tick — i.e. the 283 s fixture compile becomes ~10–25 s,
a **>10–25× wall-clock win**, and the gap census's "days" projection
for sample.ev-scale sources collapses proportionally (every one of
the 6,144+ ticks is gated by this single refusal: coverage win =
100% of ticks).

evidentc (wave 5d) caching is already live for the driver — observed
`[fz] evidentc cache HIT — skipped simplify+propagate-values` and a
450 KB `driver.smt2.evidentc` side-car — but a cache can only skip
simplify; the refusal is recomputed each run. Caching is **not** a
fix for this class.

## Ranked fix list (effort × speedup)

1. **Kernel: capture unit Bool literal pins in `extract_program`**
   (kernel/src/functionize/mod.rs, the assertion-classification loop
   at ~848–930, before the `raw.predicates.push(a)` fall-throughs).
   Add two cases, first-def-wins like the existing scalar branch:
   - `a` is an uninterpreted Bool const `v` → `raw.scalar.insert(v, true_ast)`
   - `a` is `(not v)` with `v` an uninterpreted const →
     `raw.scalar.insert(v, false_ast)`
   (`Z3_mk_true/false` inc-ref'd; same `contains_key` gate so a later
   real definition still lands in predicates and is enforced by the
   run-time predicate check at mod.rs:1452.)
   Effort: ~10 lines + a regression fixture (a claim invoked with a
   constant arg whose Bool output folds). Speedup: unlocks the entire
   fast path for driver.smt2 — est. >10–25× per compile, growing with
   source size. **This is the whole wall.**

2. **Re-probe verify after (1) lands** — extraction was the observed
   refusal; the verify gates (`tick-0/1 eval refused / mismatch`,
   mod.rs:1226–1244) never executed and stay unproven until (1) is
   in. Evidence says low risk (all body ops eval-supported, no
   cycles, predicates trivial), but the probe is one command:
   the WHY/TRACE run above, watching for `verify:` refuse lines.
   Effort: zero (diagnostics exist). Contingency if a mismatch shows:
   the `[fz/eval] unsupported op` trace names the exact op for a
   small eval.rs extension.

3. **Driver source reshape (alternative to 1, NOT recommended)** —
   avoid constant-arg invocations of `BoolCmpBuildZ3` (driver.ev:1584,
   1589, 1609, 1614) and `FieldSortSlot` (driver.ev:601), e.g. pin
   `d_nat_ok = true` / `d_nat_nn = false` literally in driver.ev and
   call a thinner builder. Works without touching Rust, but is
   whack-a-mole: ANY claim called with constants can fold (two
   independent sites already), and the driver will keep growing.
   Effort per site: small; total: unbounded. Only worth it if kernel
   freeze blocks (1).

4. **evidentc-style caching** — already functioning for driver.smt2
   (cache HIT observed); orthogonal to the refusal. No action.

## Raw artifacts

Captured under /tmp/tmp.uJi4eV4dCv/ (this machine, this session):
`driver.smt2` (oracle emit), `trace017.log` (6,144-tick full run),
`dump.log` (723-assert simplified dump + why lines), `phase017.log`
(PHASE_TRACE), `trace_ref.log` (compiler.smt2 reference),
`out017.smt2` (the driver's emitted unit, exit 0). Offline collector
replica: /tmp/fzan.py.
