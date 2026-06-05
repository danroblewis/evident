# Task: translate_ctor 6→3 child fan-out (wave 4s)

## Why

Wave 4r profiled compiler.smt2 and found ~90% of the body is the
constructor-expression renderer (`compiler/translate_ctor.ev`)
inlined ~4,400 times. The dominant Z3 theory cost is sequence /
string (theory_seq, seq_decl_plugin, seq_rewriter) — driven by the
renderer's `++` concatenation chains and `ListThread3` seq
threading.

**The structural waste**: each `RenderExprL1` / `RenderExprL2`
inlines its child renderer TWICE — once for the constructor-argument
path (seeded from `l1_after_lp`) and once for the Seq-element path
(seeded from `l1_t0`). But a node is EITHER a ctor OR a Seq, so 3
of 6 subtrees are dead every tick.

The two paths differ only in their seed token-list and the
`ListThread3 elt_hint` (`""` vs `in_hint`). Pre-select them, run
ONE 3-wide child chain, keep the existing `out`/`rest`/`ok`
selection on `l1_is_ctor`/`l1_is_seq`.

**Hypothesized speedup: 1.8-2× per tick** (renderer is ~90% of
body; halving its fan-out approximately halves the body).

## Authorisation

Edit:
- `compiler/translate_ctor.ev` — the fan-out reduction.
- `compiler.smt2`, `sample.smt2` — rebuild after the source change.
- `tests/kernel/*.ev` — verification fixture.
- `docs/plans/wave-4s-...md` — wave doc with before/after numbers.

Forbidden: `bootstrap/`, `kernel/`, `stdlib/`, `tests/lang_tests/`,
`tests/conformance/`, Python.

## Required reading

1. `CLAUDE.md`.
2. `docs/plans/wave-4r-pertick-hot-shapes.md` — the profiling
   report including the exact recommendation (Section 5).
3. `compiler/translate_ctor.ev` — esp. `RenderExprL1` (lines
   213-306) and `RenderExprL2` (lines 313+).
4. `kernel/src/tick.rs` — to re-run the band profiler.

## Scope

### Item 1: implement the 6→3 fan-out cut

Per the wave-4r recommendation:

```evident
-- Pre-select the seed and hint based on whether this node is a
-- ctor or a Seq.
l1_seed ∈ TokenList = (l1_is_ctor ? l1_after_lp : l1_t0)
l1_hint ∈ String    = (l1_is_ctor ? "" : in_hint)

-- ONE 3-wide child renderer chain on the shared seed/hint
-- (instead of two parallel 3-wide chains).
RenderExprL0(l ↦ l1_seed, in_hint ↦ l1_hint, …)
RenderExprL0(l ↦ <residual after first L0>, …)
RenderExprL0(l ↦ <residual after second L0>, …)
ListThread3(o1 ↦ …, o2 ↦ …, o3 ↦ …, elt_hint ↦ l1_hint, …)
```

Apply analogous transformation to `RenderExprL2` (its 6-child
fan-out over `RenderExprL1`).

Keep the existing `out`/`rest`/`ok` selection logic on
`l1_is_ctor`/`l1_is_seq` so the OUTPUT shape stays identical for
both paths.

### Item 2: correctness verification (byte-identical)

```bash
scripts/build-compiler-smt2.sh

# A ctor + Seq mixed fixture
cat > /tmp/probe-cs.ev <<'EOF'
enum Result = Ok(Int) | Err(String)
claim main
    r ∈ Result = Ok(42)
    xs ∈ Seq(Int) = ⟨1, 2, 3⟩
    effects ∈ Seq(Effect) = ⟨Exit(0)⟩
EOF
scripts/diff-vs-bootstrap.sh --semantic /tmp/probe-cs.ev main
# Expected: exit 0 (byte-identical or semantically equivalent)

# Wave 4h's test_hello (the canonical full-corpus smoke)
scripts/diff-vs-bootstrap.sh --semantic tests/kernel/test_hello.ev hello
# Expected: exit 0
```

Also kernel suite green:

```bash
./test.sh --kernel
```

### Item 3: re-profile (the perf headline)

```bash
EVIDENT_FUNCTIONIZE_TIMING=1 EVIDENT_FUNCTIONIZE_TIMING_BANDS=24 \
    ./kernel/target/release/kernel ./compiler.smt2 < /tmp/probe-cs.ev 2>&1 \
    | grep band_profile
```

Compare:
- body assertion count: was 40,843; expect ~22k (≈47% reduction).
- full-body solve time: was ~640 ms; expect ~330 ms (≈48% reduction).
- per-tick mean: was ~820 ms; expect ~410 ms.

Also wall-clock on a representative input (e.g. test_enums_basic):
- Old: ~28 min for 19 claims
- Expected: ~15 min

### Item 4: rebuild + commit compiler.smt2 + sample.smt2

```bash
scripts/build-compiler-smt2.sh
scripts/build-sample-smt2.sh
```

Commit both rebuilt artifacts.

## Acceptance

1. translate_ctor.ev fan-out cut from 6→3.
2. Byte-identical (or semantic-equivalent) emit on the probe
   fixture AND test_hello.
3. All 111 kernel tests green.
4. Re-profile shows measurable reduction in body assertions and
   per-tick solve time (close to the 1.8-2× target).
5. Compiler.smt2 + sample.smt2 rebuilt + committed.

## Forbidden

- Editing `bootstrap/`, `kernel/`, `stdlib/`, `tests/lang_tests/`,
  `tests/conformance/`, Python.
- Dropping RenderExprL2 entirely (coverage reduction — separate
  wave; the wave-4r doc explicitly defers this).
- Skipping verification on test_hello.

## Known gotchas

- The 3-wide child chain still uses the per-level token-list
  threading. Don't accidentally short-circuit a path that the
  `out`/`rest` selection later picks up.
- The change must preserve the renderer's contract: same
  emitted SMT-LIB for both ctor and Seq inputs at every depth.
- compiler.smt2 must be rebuilt; sample.smt2 too (shares the
  renderer).

## Reporting back

- Branch (`agent-50-translate-ctor-fanout`).
- Items 1-4 status.
- The before/after profile numbers (the headline).
- Test count: should stay 111.
- Cite docs.

Be terse.
