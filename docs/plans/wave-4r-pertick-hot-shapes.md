# compiler.smt2 per-tick hot-shape profiling — wave 4r

**Diagnostic-only.** No `compiler/`, `stdlib/`, or `bootstrap/` source
changed. The only code edit is opt-in kernel instrumentation
(`kernel/src/tick.rs`, behind `EVIDENT_FUNCTIONIZE_TIMING`), off by
default. Deliverable = this doc + the recommendation for wave 4s.

Headline: **There is no single hot SHAPE. ~90 % of the 40 843-assertion
per-tick body is one thing — the manually-unrolled, 6×-per-level
fan-out constructor-expression renderer in `compiler/translate_ctor.ev`
(`RenderExprL0/L1/L2` + `AtomTokRenderC` + `ListThread3`), inlined at
~4 400 call-sites. Marginal solve cost is FLAT across the body (~15–30
ms / 1 700-assert band) because every band is the same renderer
repeated. The dominant per-tick COST, however, is not arith (52 % by
count) — a `sample` of Z3 shows the SEQUENCE/STRING theory
(`seq_decl_plugin`, `seq_rewriter`, `theory_seq`, `seq_concat`)
dominating actual solve time, driven by the renderer's `++`
string-concat chains and `ListThread3` seq-threading. The recommended
wave-4s change cuts the renderer's 6→3 child fan-out per level
(correctness-preserving), which ~halves both the body size and the
seq/string-theory mass. Hypothesized per-tick speedup: ~1.8–2×.**

Cites: `CLAUDE.md` §"Functionizer diagnostics"; wave 4e
`docs/plans/grammar-wave4e-perf-diagnostic.md` (this is its next-level
extension); `kernel/src/tick.rs` (`run_inner`, new `band_profile`);
`compiler/translate_ctor.ev`; memory
`project_functionizer_macro_finder_extracted`,
`project_fti_honesty_audit_result`.

---

## Section 1 — Setup

| item              | value |
| ----------------- | ----- |
| `compiler.smt2`   | 228 186 lines (this worktree's build; larger than wave 4e's 200 348) |
| body `(assert …)` | **40 843** (after pre-loop `simplify`) |
| manifest state-fields | ~205 (+ `effects`) |
| functionized      | **0 / 40 843** — refuse reason: `verify: tick-0 eval refused (unsupported op)` |

The compiler reads stdin line 1 = a flat source `.ev` path, line 2 =
optional target claim (`compiler/compiler.ev:64`). All runs below pipe
`"<path>\n\n"` into `./kernel/target/release/kernel compiler.smt2`.

---

## Section 2 — Instrumentation (Item 1)

Added `band_profile()` in `kernel/src/tick.rs`, gated by
`EVIDENT_FUNCTIONIZE_TIMING=1` (off by default — `./test.sh`
unaffected). Tunables: `EVIDENT_FUNCTIONIZE_TIMING_BANDS` (default 24),
`EVIDENT_FUNCTIONIZE_TIMING_REPS` (default 1, min-over-reps cuts noise).

Z3 exposes no per-assertion solve cost, so the profiler uses a
**cumulative-prefix band sweep**: assert the first *k* bands of the body
+ a fixed tick-0 pin set, time the solve; `marginal[k] = solve(k) −
solve(k−1)` attributes incremental cost to band *k*. Every prefix is SAT
(dropping body assertions only relaxes a SAT problem), so each prefix
solve is real and comparable. Each band also reports a shape histogram
(string / datatype / array / arith, by a substring scan of the rendered
assertion) and the output **var-names** each band defines (`(= VAR …)`),
which map bands → `compiler.ev` claims.

A second, zero-edit instrument — macOS `sample` of the kernel mid-solve
— attributes cost to Z3's internal theories (Section 4).

The instrument's signal is **input-independent**: the body is static
(same 40 843 assertions every tick, every input), so the band map is the
same for the trivial and lang-test inputs. The inputs differ only in
tick COUNT (Section 3).

---

## Section 3 — Representative inputs (Item 2)

Per-tick cost is flat (confirms wave 4e); inputs differ in tick count.

| input | lines | ticks | wall-clock | mean ms/tick (Z3) | functionized |
| ----- | ----- | ----- | ---------- | ----------------- | ------------ |
| trivial `claim foo / x ∈ Int = 5` | 2 | **37** | 30.2 s | **~820 ms** (tick 0 = 1139 ms, steady ≈ 760 ms) | 0 / 40 844 |
| `tests/lang_tests/test_enums_basic.ev` (enums + ternary + record + match; 2 916 chars) | 140 | **174** in 150 s cap (did not finish; ~3 800 est. total) | ≥150 s (extrapolates to **~51 min**) | **813.8 ms** (tick 0 = 1133 ms) | 0 |

The lang-test input was the required non-trivial driver. It does not
finish within a practical cap (the 140-line fixture extrapolates to tens
of minutes at ~760 ms/tick — matching wave 4e's "intractable" call), so
its row reports ticks-in-150 s + mean per-tick + extrapolation. The band
map (Section 4) applies to it unchanged.

---

## Section 4 — Analysis: the top 5 (Item 3)

### 4a. Band sweep (24 bands × 2 reps, full body)

```
band  1/24 [     0..  1702]  marginal  16.9 ms | str 225 dt  604 ari  870 | path_read src_path input target  (← state-carry pins)
band  2..23 [ 1702.. 39146]  marginal 13–37 ms each (flat) | each ≈ str 200 / dt 615 / ari 880 | RenderExprL0/L1 + AtomTokRenderC + ListThread3 __callNNNN
band 24/24 [ 39146.. 40843]  marginal 168.2 ms | the manifest-output ++ assembly closing the constraint graph
full-body solve: ~640 ms   shape totals: str 4784 / dt 14769 / arr 4 / arith 21286
```

Reading it:

- **Band 1 (~17 ms)** carries the ~205-field state-carry pins
  (`path_read`, `src_path`, `input`, `target`, …). The 200-field carry
  is **NOT** the bottleneck — refutes that hypothesis, confirms wave 4e.
- **Bands 2–23 are homogeneous and flat** (~15–30 ms each). Every band's
  var-names are `RenderExprL0__…__callNNNN`, `RenderExprL1__…`,
  `AtomTokRenderC__…`, `ListThread3__…` — the same renderer, inlined
  over and over. The `__callNNNN` site-id climbs to **12 784** by the
  last band.
- **Band 24 (168 ms spike)** is the final string-concat that ties
  everything to the manifest outputs (`out`/`smtlib` assembly), not a
  distinct shape.

### 4b. Z3 theory attribution (`sample`, 6 s mid-solve)

```
seq_decl_plugin 965   seq_rewriter 340   seq_util 161   theory_seq 70
seq_concat 23   seq_value_proc 7   str_units 11   str_itos 2     ← SEQ/STRING ≈ 1580
arith 123                                                        ← arith
datatype 54                                                      ← datatype
```

**The count view misleads.** Arith is 52 % of assertions but the actual
Z3 solve time is dominated by the **sequence/string theory** — the
`++`-concatenation chains every `RenderExprL*` builds and the seq
threading in `ListThread3`. This is the real per-tick lever.

### 4c. Inline-fan-out counts (`__callNNNN` distinct sites in `compiler.smt2`)

| sub-claim (`compiler/translate_ctor.ev`) | inline sites | role | theory |
| ---------------------------------------- | -----------: | ---- | ------ |
| `AtomTokRenderC` (line 67)               | **1 813** | atom token → text (`str_from_int` for IntLit) | string |
| `RenderExprL0` (line 186)                | **1 512** | depth-0 expr render; calls `AtomTokRenderC` + `ListThread3` | string/seq |
| `ListThread3` (line 129)                 | **602**   | thread ≤3 child renders into a seq/arg-list | **seq theory** |
| `RenderExprL1` (line 213)                | **252**   | depth-1; inlines **6× `RenderExprL0`** (3 ctor-arg + 3 seq-elt) | multiplier |
| `RenderExprL2` (line 313)                | (fewer)   | depth-2; inlines **6× `RenderExprL1`** | multiplier |

### The top 5 shapes by total time contribution

1. **`ListThread3` seq-threading (×602)** — the dominant *theory* cost.
   Each inlines Z3 seq-theory ops to splice ≤3 rendered children into an
   arg-list / seq literal. The `sample` puts `theory_seq` + `seq_*`
   first. `translate_ctor.ev:129`.
2. **`RenderExprL0` `++` string-concat (×1512)** — each produces
   `out = "(" ++ name ++ args ++ ")"`-style concat chains hammering the
   seq/string theory. `translate_ctor.ev:186`.
3. **`AtomTokRenderC` (×1813)** — atom→text incl. `str_from_int`
   (a documented functionizer-refuse shape,
   `project_fti_honesty_audit_result`). String theory. `translate_ctor.ev:67`.
4. **The 6×-per-level fan-out multiplier (`RenderExprL1`→6×L0,
   `RenderExprL2`→6×L1)** — this is *why* (1)–(3) are inlined thousands
   of times. One depth-2 ctor render expands to 6×6 = 36 `RenderExprL0`
   subtrees. `translate_ctor.ev:213,313`.
5. **Manifest-output `++` assembly** (band-24 +168 ms) — the final
   `out`/`smtlib` string concatenation closing the graph.

All five are the same machinery; (4) is the size lever, (1)–(3) the
per-tick theory cost it multiplies.

---

## Section 5 — Recommendation for wave 4s (Item 4)

> **Change `compiler/translate_ctor.ev` `RenderExprL1` (lines ~243–306)
> and `RenderExprL2` (analogous) from a 6-child fan-out to a 3-child
> shared fan-out.** Today each level inlines its child renderer **twice**
> — once for the constructor-argument path (seeded from `l1_after_lp`,
> the tokens past `Ident (`) and once for the Seq-element path (seeded
> from `l1_t0`, the tokens past `⟨`) — 6 child renders per level, and a
> node is **either** a ctor **or** a Seq, so 3 of the 6 subtrees are dead
> every tick. The two paths differ ONLY in the seed token-list and the
> `ListThread3 elt_hint` (`""` vs `in_hint`). Pre-select them:
> `l1_seed = (l1_is_ctor ? l1_after_lp : l1_t0)`,
> `l1_hint = (l1_is_ctor ? "" : in_hint)`, run **one** 3-wide child chain
> + **one** `ListThread3` on `l1_seed`/`l1_hint`, then keep the existing
> `out`/`rest`/`ok` selects on `l1_is_ctor`/`l1_is_seq`. This cuts each
> level's child fan-out 6→3 (a depth-2 render's L0 subtrees: 36→9).
>
> **Hypothesized speedup: ~1.8–2× per tick** — the renderer is ~90 % of
> the body, so halving its fan-out takes the body from ~40.8 k → ~22 k
> assertions and the full-body solve ~640 → ~330 ms; the cut lands
> directly on the seq/string-theory mass the `sample` flagged as
> dominant.
>
> **Verify by:** (1) re-run `EVIDENT_FUNCTIONIZE_TIMING=1
> EVIDENT_FUNCTIONIZE_TIMING_BANDS=24` and confirm body-assert count and
> `full-body solve` ms both drop ~2×; (2) compile a ctor+Seq fixture
> (e.g. `enum Result = Ok(Int) | Err(String)` + a `Seq` literal) under
> both the old and new `compiler.smt2` and confirm **byte-identical**
> `.smt2` output (correctness preserved — a ctor node never takes the
> Seq wrapper and vice-versa, and both wrappers consume the SAME rendered
> children).

**Why this over the alternatives:**

- *Functionize the body* — ceiling < 1.5× (wave 4e: 72.5 % of asserts
  are String/datatype/seq shapes the JIT refuses; `str_from_int` /
  `last_results` decode are documented refuses). Dead end at the kernel
  level.
- *Cut tick count (wave 4f bulk-skip)* — orthogonal and already
  recommended; it attacks tick COUNT, this attacks per-tick COST. Both
  compose multiplicatively.
- *Drop the `RenderExprL2` depth entirely* — would remove the 6×L1
  fan-out outright (bigger win) but is a **coverage reduction** (depth-2
  ctor nesting stops compiling). Hold as a follow-up only if a corpus
  scan shows depth-2 ctor expressions are unused.
- *Move string assembly out of Z3 seq theory* (build `++` results in the
  kernel/effects instead of the solver) — the largest theoretical win
  per the `sample`, but it's a kernel/representation change, out of scope
  for a `compiler/*.ev` wave.

The 6→3 sharing is the highest-confidence, correctness-preserving,
in-`compiler.ev`, profiler-verifiable change.

---

## Section 6 — Open questions

1. **Does `RenderExprL2` dominate `RenderExprL1`?** The 6×6 fan-out means
   L2 subtrees are 6× heavier than L1. If most real translate sites only
   reach L1, the L2 sharing is where the bulk of the win is. A
   per-call-site depth histogram (grep `__callNNNN` provenance back to
   the `RenderExprL{0,1,2}` that emitted it) would split the ~22 k saved
   asserts between the two levels. Not needed to act, but it would tell
   wave 4s which level to do first.
2. **Band-24's 168 ms spike** is the manifest-output closure. Worth a
   one-off finer band sweep over `[39146..40843]` to see whether it's the
   `smtlib` concat or the `out` concat — a possible second, independent
   lever (precompute the static prelude string outside the solved body).
3. **Tick-0 is 1139 ms vs steady ~760 ms.** The +380 ms is one-shot
   (first solve builds Z3's internal structures). Irrelevant to the
   steady-state lever but noted so wave 4s doesn't chase it.
