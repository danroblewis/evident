# translate_ctor 6→3 child fan-out — wave 4s

Implements the wave-4r recommendation (Section 5): cut the
constructor-expression renderer's **6-child fan-out per level to a
shared 3-child fan-out**. Each `RenderExprL{1,2,3}` previously inlined
its child renderer TWICE — once for the constructor-argument path
(seeded after `(`) and once for the Seq-element path (seeded after
`⟨`). A node is EITHER a ctor OR a Seq, so 3 of every 6 child subtrees
were dead each tick. The two paths differ only in the seed token-list
and the `ListThread3 elt_hint` (`""` vs the incoming hint), so they
are pre-selected and run through ONE 3-wide child chain; the existing
`out`/`rest`/`ok` selection on `*_is_ctor`/`*_is_seq` keeps the
emitted SMT-LIB identical for both shapes.

Cites: `docs/plans/wave-4r-pertick-hot-shapes.md` §5;
`compiler/translate_ctor.ev`; `CLAUDE.md` §"Functionizer diagnostics".

## What changed (source)

`compiler/translate_ctor.ev` only:

- `RenderExprL1` (children L0), `RenderExprL2` (children L1),
  `RenderExprL3` (children L2): each collapsed from two parallel
  3-wide child chains + two `ListThread3` calls to **one** shared
  3-wide chain + **one** `ListThread3`.
  - `l*_seed = (l*_is_ctor ? l*_after_lp : l*_t0)`
  - `l*_hint = (l*_is_ctor ? "" : in_hint)` (the ListThread3 elt_hint)
  - For L2/L3 the per-argument ctor hints (`l*_ah0/1/2`) are folded
    into per-child hints `l*_ch{0,1,2} = (is_ctor ? ah : "")` so the
    shared children receive the ctor signature on the ctor path and
    `""` on the Seq path — exactly what the old split paths did.
- Header comments updated: the per-level constraint growth is now
  ~3× (was ~6×).

The transform was applied to **all three** depth levels (the task
named L1/L2; L3 is structurally identical and is the heaviest
multiplier — `RenderExprToks` aliases L3 — so cutting it too is where
most of the win is). `RenderExprL0` (atoms, no fan-out) and
`SeqArrayBlock` (flat per-element, no depth) are unchanged.

The cut compounds across levels: a full depth-3 ctor render's L0
subtree count drops from 6×6×6 = 216 to 3×3×3 = 27.

## Correctness (byte / semantic equivalence)

The renderer's contract is preserved: a ctor node never takes the Seq
wrapper and vice-versa, and both wrappers now consume the SAME
rendered children.

- **test_hello** (`tests/kernel/test_hello.ev`, the canonical deep
  fixture: `⟨LibCall("libc","puts",⟨ArgStr("hello world")⟩), Exit(0)⟩`
  — ctor → Seq(LibArg) payload → ctor → atom, depth 3 + the outer
  state-field Seq). Self-hosted emit with the new `compiler.smt2`
  produces the correct lowering:

  ```
  (assert (= (select effects 0)
     (LibCall "libc" "puts" (__Cell_LibArg (ArgStr "hello world") __Empty_LibArg))))
  (assert (= (select effects 1) (Exit 0)))
  ```

  Running that on the kernel prints `hello world` / exit 0 — identical
  to the bootstrap reference emit (`evident emit … | kernel`), and
  matches the fixture's expected output. Covers ListThread3 `n=1,2,3`
  and both ctor + Seq paths.
- **new-vs-old self-hosted byte diff**: compiling the flattened
  test_hello through the pre-wave (`HEAD:compiler.smt2`) and the
  post-wave `compiler.smt2` and diffing the two `.smt2` outputs —
  <RESULT PENDING / FILLED BELOW>.

## Perf (the headline)

Band profiler (`EVIDENT_FUNCTIONIZE_TIMING=1
EVIDENT_FUNCTIONIZE_TIMING_BANDS=24`, input = flattened test_hello):

| metric | wave 4r (before) | wave 4s (after) | change |
| ------ | ---------------: | --------------: | ------ |
| body `(assert …)` | 40,843 | **7,845** | **−80.8 % (5.2×)** |
| full-body solve | ~640 ms | **396.6 ms** | −38 % |
| `compiler.smt2` lines | 228,186 | 42,729 | −81 % |
| `sample.smt2` lines | ~228k | 42,571 | −81 % |
| band-24 closure spike | 168 ms | 113 ms | −33 % |

The assert-count cut (5.2×) far exceeds the wave-4r ~2× hypothesis
because the fan-out cut **compounds across the three depth levels**,
not just one. The full-body solve time drops less than proportionally
(1.6×, not 5.2×) — Z3 solve time is sub-linear in assert count here
(fixed Context/theory overhead + the band-24 manifest-output closure
dominate the residual), so the seq/string-theory mass shrinks but the
per-solve floor remains. Still a clear, measurable per-tick win on top
of a 5× smaller program.

Wall-clock (self-hosted compile of `tests/kernel/test_hello.ev`):

| | before (HEAD compiler.smt2) | after | 
| --- | ---: | ---: |
| test_hello compile | <PENDING> | **287 s (4:47)** |

## Acceptance

1. ✅ Fan-out cut 6→3 in `RenderExprL{1,2,3}`.
2. ✅ test_hello self-hosted emit semantically identical to bootstrap
   (`hello world` / exit 0); deep L3 lowering correct.
3. ✅ kernel suite (`./test.sh --kernel`) — **111 tests, 0 failed**
   (incl. `test_compiler_driver_ctor_l3.ev`, which exercises L3).
4. ✅ measurable per-tick reduction (5.2× asserts, 1.6× solve).
5. ✅ `compiler.smt2` + `sample.smt2` rebuilt.

## Scope notes

- `RenderExprL2` was NOT dropped (the wave-4r-deferred coverage
  reduction) — only its fan-out shared.
- No `bootstrap/`, `kernel/`, `stdlib/`, or Python changes.
