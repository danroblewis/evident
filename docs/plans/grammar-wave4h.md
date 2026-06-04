# Grammar wave 4h — in-ctor Seq-value Cons encoding (Blocker 2c)

Closes the sole remaining `test_hello` gap left by wave 4g: a `Seq(T)`
VALUE in constructor-argument position rendered to the Z3 seq-theory
form `(seq.unit …)`, a sort mismatch against the `__SeqOf_T` accessor
type, so the kernel rejected it. See **`docs/plans/blocked-grammar-wave4g.md`**
for the precise blocker spec and bootstrap's element-type-driven
renderer (the porting target).

## The fix — thread the element type to the renderer

The in-expression renderer (`compiler/translate_ctor.ev`'s
`RenderExprL1/L2/L3` + `ListThread3`) was element-type-AGNOSTIC: it knew
its children's rendered text but not that a Seq's element type was
`LibArg`, so it could not name `__Cell_LibArg`/`__Empty_LibArg`. This
wave threads the expected element type in.

### Item 1 — variant-signature table (`sigtab`)

The driver (`compiler/compiler.ev`) already parses every enum's variant
fields. During the enum walk, for each payload field of type `Seq(T)` it
now appends one `<ctor>#<idx>#<T>;` entry to a program-global `sigtab`
string (reusing the `commit_is_seq`/`commit_elt` the wave-4g
field-commit machinery already computes; `_cur_name` is the variant,
`_fcount` the field index). For `test_hello`'s `Effect` this yields
`LibCall#2#LibArg;`. Program-global — NOT reset on `enter_enum`, so the
table is complete before the claim phase renders.

### Item 2 — element-type-aware Seq render

`ListThread3` gains an `elt_hint` slot. When non-empty it renders the
Cons form instead of seq-theory:

```
n=0 →  __Empty_<T>
n=1 →  (__Cell_<T> e1 __Empty_<T>)
n=2 →  (__Cell_<T> e1 (__Cell_<T> e2 __Empty_<T>))
n=3 →  (__Cell_<T> e1 (__Cell_<T> e2 (__Cell_<T> e3 __Empty_<T>)))
```

Empty `elt_hint` keeps the existing `(seq.unit …)`/`(seq.++ …)` form —
the in-expression Seq-literal default is unchanged. Empty `⟨⟩` with a
hint lowers to `__Empty_<T>`.

### Item 3 — thread the hint through the ctor walk

`RenderExprL2`/`L3` gain `sigtab` + `in_hint` inputs (`RenderExprL1`
gains only `in_hint` — its children are atoms, so it never looks up a
ctor arg). When a level renders a constructor it queries `sigtab` via a
new `EltHintLookup(sigtab, ctor, idx)` claim (string `index_of` on the
`<ctor>#<idx>#` key, value read to the next `;`) and passes each arg's
element-type hint down to that arg's child renderer. A Seq literal at a
hinted position then renders Cons; everywhere else stays seq-theory.
`SeqArrayBlock` threads `sigtab` to its element renders (so a Seq payload
INSIDE a state-field Seq element ctor renders Cons) while the OUTER
state-field Seq keeps its `(Array Int T)+__len` encoding.
`CtorMembershipStep` gains a `sigtab` input, fed `_sigtab` by the driver.

This is element-type-DRIVEN, not a `LibArg` hardcode (the wave-4g doc's
explicit warning): any `Seq(T≠LibArg)` payload renders `__Cell_T` from
the same table.

## Verification

- **Datatypes / values byte-identical to bootstrap `evident emit`**:
  - `(LibCall "libc" "puts" (__Cell_LibArg (ArgStr "hi") __Empty_LibArg))`
  - `(LibCall "libc" "puts" __Empty_LibArg)`  (empty payload)

  Two new isolation fixtures pin these directly (kernel-run, exit 0):
  `tests/kernel/test_compiler_driver_seq_value_cons.ev` (1-/2-element
  Cons, via both the direct `in_hint` and the `sigtab` ctor-lookup path)
  and `tests/kernel/test_compiler_driver_seq_value_empty.ev` (`__Empty_T`
  + the no-hint seq-theory fallback preserved).

- **`compiler.smt2` rebuilds via bootstrap** (226 984 lines).

- **Smoke test — `test_hello`** (the headline):
  `scripts/diff-vs-bootstrap.sh --semantic tests/kernel/test_hello.ev hello`
  → **exit 0**: `SEMANTIC MATCH — kernel stdout+exit identical`. The
  self-hosted compile of the full flattened `test_hello` (1436 ticks)
  now produces a `.smt2` the kernel runs to byte-identical observable
  behaviour as bootstrap's. This is the first full-corpus self-host
  smoke pass — the deletion-readiness signal. Only the structural
  Blockers 4 (L4 work-stack walker) and 5 (per-tick solve cost) remain,
  both kernel-side.

## Blocker chain status after this wave

- Blocker 1 (comment stripping): ✓ wave 4f.
- Blocker 2a (compound field-type parse): ✓ wave 4g.
- Blocker 2b (`__SeqOf_T` datatype): ✓ wave 4g.
- Blocker 2c (in-ctor Seq VALUE → `__Cell` Cons): ✓ **this wave**.
- Blocker 3 (parametrized-claim skip + claim selection): ✓ wave 4g.
- Blocker 4 (L4 work-stack walker) / Blocker 5 (per-tick solve cost):
  structural, kernel-side, out of scope (unchanged).
