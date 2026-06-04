# Grammar wave 4g — nested enum payload types + parametrized-claim skip

Works through the blocker chain in `docs/plans/blocked-grammar-wave4d.md`
(cited throughout). Wave 4f closed Blocker 1 (lexer comment stripping).
This wave closes the *mechanical* grammar gaps that remained — Blocker 2
(parse + datatype facets) and Blocker 3 — plus two adjacent gaps that
`test_hello` surfaced (newline-separated enum variants, `Result` dedup).
The single remaining `test_hello` gap is now isolated to ONE renderer
line (the in-ctor Seq-value cons encoding) — see
`docs/plans/blocked-grammar-wave4g.md`.

## What landed

### Item 1 — nested enum payload types (Blocker 2, parse + datatype facets)

`stdlib/kernel.ev`'s `Effect` carries `LibCall(String, String,
Seq(LibArg))` — a `Seq` over a user enum in payload position. Two facets:

**(a) Parse — compound field types.** The enum-parse sub-machine's
field reader (ephase 6) previously consumed a single `Ident` per field
and stalled on the `(` of a compound type (wave-4d Blocker 2). It is now
a self-looping ACCUMULATOR with bracket-depth tracking, reusing
`parser.ev::TypeTokenText` + `BracketDelta`: a field type accumulates
token-by-token (`Seq` `(` `LibArg` `)`) and is COMMITTED only on a Comma
(next field) or RParen (close ctor) seen at depth 0. `Map(Int, Int)`,
`Seq(Edge<Rect>)`, etc. all work by the same depth rule. ephase 7 is
retired; `e6` loops on itself until the depth-0 RParen.

**(b) Datatype — `__SeqOf_T` Cons helper.** A `Seq(T)` payload is not a
Z3 sort. Bootstrap
(`runtime/src/runtime/register_enums.rs::generate_internal_cons_helpers`)
lowers it to a Cons-shaped helper datatype and types the accessor with
it. **Ported exactly** (verified byte-identical to `evident emit` —
below):

```
(declare-datatypes ((__SeqOf_LibArg 0))
  (((__Empty_LibArg) (__Cell_LibArg (head LibArg) (tail __SeqOf_LibArg)))))
…  (LibCall__f2 __SeqOf_LibArg)  …
```

`compiler/translate.ev` gains `SeqEltType` (recognise `Seq(T)`, extract
`T` via `substr`), `FieldSortName` (map a field-type string to its
accessor sort — identity for primitives/plain enums, `__SeqOf_T` for a
Seq payload; wired into `VariantText`), and `SeqHelperBlock` (emit the
helper block). The driver captures the Seq element type during the enum
walk (`seqelt`) and prepends the helper block before the enum that
references it, matching bootstrap's stage ordering (LibArg → __SeqOf_LibArg
→ Effect).

### Item 2 — parametrized-claim skip + claim selection (Blocker 3)

The inlined `Build*` sugar claims carry first-line params
`(s ∈ String, eff ∈ Effect)` and bodies. The self-hosted compiler has no
claim-name argument (it reads the source file, not `emit foo.ev main`),
so it uses the corpus convention: **the entry-point claim has no
first-line params; the helpers all do.** A new DISPATCH predicate
`claim_is_param` peeks the token after the claim name; a parametrized
head routes to a new CLAIM-SKIP sub-machine (`pmode 3`) that drops the
claim's tokens one-per-tick to the next top-level keyword, emitting
nothing. A bare-head claim is translated as before. Because `out`/`fstr`
reset on each `enter_claim`, when several bare-head claims appear the LAST
one wins — the file's target claim (mirrors bootstrap emitting only the
`emit`-named claim).

### Adjacent gaps `test_hello` forced (not in the original wave brief)

- **Newline-separated enum variants.** The consolidated lexer emits no
  `Newline` token (wave 4f), so `test_hello`'s `Effect` (variants on
  separate lines, no `|`) had no separator. The enum machine now treats a
  bare `Ident` while a variant is pending (ephase 5/8) as BOTH a finalize
  of the current variant and the start of the next — newline-separated
  and `|`-separated variants both parse.
- **`Result` dedup.** The kernel always pins `Result`; the EMIT prelude
  already declares it (bootstrap dedups the same way). The enum machine
  now skips emitting a source `enum Result` block.

## Verification

Built `compiler.smt2` via bootstrap (200 764-line body). Driving the REAL
self-hosted compiler (`kernel + compiler.smt2`) on focused inputs:

**Datatypes byte-identical to bootstrap** (`diff` clean) for
`enum LibArg = ArgStr(String)` + newline-separated
`enum Effect = ReadLine / LibCall(String, String, Seq(LibArg)) / Exit(Int)`:

```
(declare-datatypes ((LibArg 0)) (((ArgStr (ArgStr__f0 String)))))
(declare-datatypes ((__SeqOf_LibArg 0)) (((__Empty_LibArg) (__Cell_LibArg (head LibArg) (tail __SeqOf_LibArg)))))
(declare-datatypes ((Effect 0)) (((ReadLine) (LibCall (LibCall__f0 String) (LibCall__f1 String) (LibCall__f2 __SeqOf_LibArg)) (Exit (Exit__f0 Int)))))
```

**Claim selection** verified: input with a parametrized `BuildExit(code ∈
Int, eff ∈ Effect)` helper before `claim main` — the helper emits NO
declares/asserts; only `main`'s effects are translated; the emitted `.smt2`
runs to exit 0.

Two canonical-driver fixtures pin these end-to-end:
`tests/kernel/test_compiler_driver_enum_seq_payload.ev` and
`tests/kernel/test_compiler_driver_claim_selection.ev`.

## The smoke test — `test_hello` (the headline)

`scripts/diff-vs-bootstrap.sh --semantic tests/kernel/test_hello.ev hello`
does **not** exit 0. But the self-hosted compile of the FULL flattened
`test_hello` (1436 ticks, ~14 min) now succeeds (exit 0) and produces a
`.smt2` that differs from bootstrap's in exactly ONE semantically-relevant
way. Self-hosted output:

```
(declare-datatypes ((LibArg 0)) (((ArgInt …) (ArgStr …) (ArgReal …))))
(declare-datatypes ((__SeqOf_LibArg 0)) (((__Empty_LibArg) (__Cell_LibArg (head LibArg) (tail __SeqOf_LibArg)))))
(declare-datatypes ((Effect 0)) (((ReadLine) (ReadFile …) (WriteFile …) (LibCall … (LibCall__f2 __SeqOf_LibArg)) (Exit …))))
…
(assert (= (select effects 0) (LibCall "libc" "puts" (seq.unit (ArgStr "hello world")))))   ← ONLY DIFFERENCE
```

vs bootstrap's `(__Cell_LibArg (ArgStr "hello world") __Empty_LibArg)`.

Everything else — all 3 datatypes (incl. `__SeqOf_LibArg`), `Result`
dedup, ALL 6 `Build*` claims skipped, the target `hello` claim selected,
newline-separated variants, the effects Array+len structure — matches.
The remaining differences in declaration ORDER and assert FORM (`(= n n)`
vs separate `(= effects__len n)`) are semantically equivalent and pass
under `--semantic` once the one renderer gap is closed.

**The remaining gap is Blocker 2's second facet:** the in-ctor `Seq(LibArg)`
VALUE `⟨ArgStr("hello world")⟩` lowers (via `translate_ctor.ev`'s
`seq_wrapped`) to the Z3 seq-theory form `(seq.unit …)`, but the field is
now typed `__SeqOf_LibArg`, so the kernel rejects the sort mismatch. See
`docs/plans/blocked-grammar-wave4g.md` for why this is a renderer DESIGN
gap (the renderer is element-type-agnostic and a correct fix needs the
ctor's field-type threaded in) rather than a mechanical one — it is its
own wave, and is the last thing between `test_hello` and a clean exit 0.

## Blocker chain status after this wave

- Blocker 1 (comment stripping): ✓ wave 4f.
- Blocker 2a (compound field-type parse): ✓ this wave.
- Blocker 2b (`__SeqOf_T` datatype): ✓ this wave (byte-identical).
- Blocker 2c (in-ctor Seq VALUE → `__Cell` cons): ✗ — the sole
  `test_hello` blocker now; `blocked-grammar-wave4g.md`.
- Blocker 3 (parametrized-claim skip + claim selection): ✓ this wave.
- Newline-separated variants / `Result` dedup: ✓ this wave (adjacent).
- Blocker 4 (L4 work-stack walker) / Blocker 5 (per-tick solve cost):
  structural, kernel-side, out of scope (unchanged).
