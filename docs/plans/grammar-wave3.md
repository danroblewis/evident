# compiler.ev grammar coverage — wave 3 (self-hosting core)

Status: **landed.** Extends the self-hosted compiler from wave 2's scalar
bodies & flow primitives (docs/plans/grammar-wave2.md) to the **structural
self-hosting core** the survey (docs/plans/grammar-coverage-survey.md §3
"Wave 3") flagged as the shapes `compiler/compiler.ev`'s own source uses
that nothing earlier handles: enums, `match`, `matches`, and `Seq`.

Every shape is lexed character-by-character from a real source string and
parsed from the actually-lexed token payloads — nothing is hardcoded as
tokens or AST.

## What landed (all four wave-3 items)

### Item 1 — `enum` declarations (incl. payload variants)

`enum Box = Pair(Int, Int) | Empty` →
`(declare-datatypes ((Box 0)) (((Pair (Pair__f0 Int) (Pair__f1 Int)) (Empty))))`

A top-level form. The driver lexes → reverses → walks the forward token
stream ONE TOKEN PER TICK with an enum-parse state machine (`ephase`
1..8) that builds each variant (nullary or payload) and renders it via
`compiler/translate.ev`'s `VariantText` (the C1 pass), accumulating the
variant-group string; the final block is assembled by `EnumDeclSmtlib`.
Payload-field rendering (`(Ctor (Ctor__f0 T) …)`) is bounded at ≤3 fields
— the widest payload arity in the corpus.
Fixture: `tests/kernel/test_compiler_driver_enum.ev`.

### Item 2 — `match` expression + patterns

```
n ∈ Int = match e
    Ok(v) ⇒ v
    _ ⇒ 0
```
→ `(ite ((_ is Ok) e) v 0)`

The arms are parsed (bounded: one ctor arm + one wildcard arm — the
dominant `match last_results[0]` shape in compiler.ev) into a
`MatchArmList`, and the nested ITE is produced by walking that list with
`compiler/translate_match.ev`'s `MatchTranslateStep` work-stack, one pop
per tick. Mirrors translate_match.ev's contract exactly: the wildcard arm
is the innermost `else`, and binds render as their NAME.
Fixture: `tests/kernel/test_compiler_driver_match.ev`.

### Item 3 — `e matches Ctor(_)` recognizer

`is_ok ∈ Bool = e matches Ok(_)` → `((_ is Ok) e)`

The sibling of match-arm lowering. **Integrated directly into the shared
`compiler/parse_body.ev` MembershipStep** (the parse component
`compiler/compiler.ev` already imports), so the canonical disk-reading
driver handles it for free — verified by emitting compiler.ev on a
matches source file.
Fixture: `tests/kernel/test_compiler_driver_matches.ev`.

### Item 4 — `Seq(T)` + `⟨…⟩` literal + `++` + `#`

```
xs ∈ Seq(Int) = ⟨1, 2, 3⟩
ys ∈ Seq(Int) = xs ++ ⟨4⟩
n  ∈ Int      = #xs
```
→
```
(declare-fun xs () (Seq Int))
(assert (= xs (seq.++ (seq.unit 1) (seq.unit 2) (seq.unit 3))))
(assert (= ys (seq.++ xs (seq.unit 4))))
(assert (= n (seq.len xs)))
```

New `compiler/parse_body_seq.ev` `SeqMembershipStep` handles the compound
`Seq(T)` type (which shifts the `=` position past the scalar
MembershipStep's assumptions), the ⟨…⟩ literal (≤3 elements), `++` concat
(`Ident ++ ⟨d⟩`), and `#` cardinality, composing `translate_seq.ev`'s
`RenderSeqUnit` / `SeqLenSmtlib`. Kept as a SEPARATE claim so the scalar
MembershipStep — and the byte-identical wave-1/2 fixtures depending on it
— is untouched. Targets mirror `translate_seq.ev`'s Z3 sequence-theory
form (`(Seq Int)` / `seq.++` / `seq.unit` / `seq.len`), NOT bootstrap's
`(Array Int Int)`+`__len` encoding: the driver only PRINTS the .smt2, so
the per-pass translator's form is the spec.
Fixture: `tests/kernel/test_compiler_driver_seq.ev`.

## New AST / lexer (compiler/parser.ev + lexer.ev)

Additive only — no existing variant changed (variant names are globally
unique, per the wave-2 finding):

- `parser.ev` `EnumVariantDecl`: added `EVDeclP(String, EVFieldList)` for
  payload variants (kept `EVDecl(String)` for nullary). New `EVField`
  (`EVFType(String)` / `EVFNone`) + `EVFieldList`.
- `lexer.ev`: added the `Hash` token (`#`) for cardinality, with
  `SingleCharTok` + `recognized` arms.

## Translator extensions

- `compiler/translate.ev` `VariantText`: now renders `EVDeclP` payload
  variants (`(Ok (Ok__f0 Int))`), bounded ≤3 fields. `EVDecl`/`EVNoVariant`
  paths unchanged (test_translate_datatype.ev / test_parser_enum.ev stay
  green; their non-exhaustive matches keep the last arm as `else`).
- `compiler/parse_body.ev` `MembershipStep`: added t9/l10 peels and the
  `<atom> matches Ctor(_)` → `((_ is Ctor) <atom>)` branch (additive — only
  fires when t5 is `KwMatches`, so every wave-1/2 shape is byte-identical).

## Integration status (honest)

- **`matches`** is fully integrated into the canonical disk-reading
  `compiler/compiler.ev` (via the shared MembershipStep).
- **`enum` / `match` / `Seq`** are proven end-to-end by dedicated driver
  fixtures that inline the SAME pipeline `compiler.ev` uses (consolidated
  lexer → reverse → walk → emit) — the same way the wave-2 fixtures are the
  driver, parameterised by a hardcoded input instead of `ReadFile`. Full
  unification into the single monolithic compiler.ev FSM is deferred: the
  `enum` top-level form needs a head-token dispatch that branches phase-2
  away from the membership walk, and `match`/`Seq` need a per-membership
  sub-walk (variable arms / elements) the single-tick MembershipStep
  cannot host — doing either in-place risks the 81 byte-identical
  wave-1/2/MVP fixtures for no new capability (the capability is already
  proven). That restructure is the natural wave-3.5 / wave-4 step.

## Footgun reconfirmed

Distinct composition SITES are α-renamed independently, so SeqMembershipStep
composes `RenderSeqUnit` three times on different element exprs safely; all
its scratch is `sq_`-prefixed (cf. wave-2's MembershipStep `ms_` prefix and
`[[project_claim_composition_leaks_body_locals]]`).

## Verification

- `./test.sh`: **all phases passed**.
- Kernel tests: **85 (was 81), 0 failed**, green under default /
  `EVIDENT_FUNCTIONIZE=0` / `EVIDENT_FUNCTIONIZE_JIT=1`.
- Wave-1 + wave-2 + MVP fixtures emit byte-identical (the runner checks
  each fixture's `-- expect:` headers, which are the prior outputs).
- Smoke test green: `scripts/flatten-evident.sh compiler/compiler.ev`
  (2281 lines) → `bootstrap emit` → /tmp/orig.smt2 (1452 lines), exit 0.

## No frozen files touched

No `bootstrap/`, no `kernel/`, no `stdlib/`, no Python. Diff is
`compiler/*.ev` + four new `tests/kernel/*.ev` + this doc.

## Out of scope (wave 4+)

Bind→accessor substitution in `match` (`Ok(v) ⇒ v` → `(Ok__f0 e)`, which
bootstrap does but translate_match.ev does not); exhaustive-ctor `match`
without a trailing wildcard; N>3 enum payload fields / seq literal elements
(needs the per-field / per-element work-stack walk); quantifiers, generics,
records, subclaims, `..`/positional composition; the full unification of
enum/match/Seq into the single disk-reading driver.
