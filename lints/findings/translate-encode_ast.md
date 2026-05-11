# Findings: runtime/src/translate/encode_ast.rs

Reviewed against `lints/rules/` as of baf8078.

## Violations of existing rules

None.

- AP-001 (no library-specific in language-core): `translate/*.rs` is in scope.
  Grep for `SDL_`, `Sdl[A-Z]`, `Gl[A-Z]`, `Glsl`, `Audio[A-Z]`, `.dylib`,
  `.framework/`, `/opt/homebrew/lib/` returns no hits in this file. Clean.
- AP-002 / AP-003 / AP-006 / AP-007 / AP-008: scoped to `examples/`; not
  applicable.
- AP-004 / AP-005: scoped to tests; not applicable.

## Per-file invariant check (`lints/runtime-invariants.md`)

The brief says the file (a) encodes Rust `Program` → Z3 Datatype matching
`stdlib/ast.ev`, (b) must not be the AST source of truth, (c) must not build
constraints or run a Solver, (d) must move in lockstep with `stdlib/ast.ev`
and `decode_ast.rs`.

- **Not the AST source of truth.** Holds. All AST types come from
  `crate::ast::*` (line 26); no AST shape declared here.
- **No constraint building, no Solver.** Holds. `grep -E
  'Solver|::assert|push\(\)|pop\(\)|check\(\)'` is empty. The file
  *constructs* `Bool` `_eq` assertions in `encode_body_items_into_seq`
  (lines 569-577) but pushes them onto a `Vec` for the caller to assert —
  no `Solver::assert` call inside this module. That matches the documented
  contract (caller asserts).
- **Cross-language contract with `stdlib/ast.ev`.** Mostly holds. See
  drift notes below.

## Cross-language drift vs `stdlib/ast.ev`

Walked every enum in `stdlib/ast.ev` against its `apply(...)` call sites
in `encode_ast.rs`.

**Variant-name + arity coverage (clean):**

  - `BinOp` — 14 variants, all 14 mapped (lines 130-145).
  - `Keyword` — 4 variants, all 4 mapped (lines 154-158).
  - `Pins` — `PNone` / `PNamed(MappingList)` / `PPositional(ExprList)`,
    all 3 mapped (lines 178-187).
  - `Mapping` — `MakeMapping(String, Expr)` matches encoder (line 169).
  - `EnumField` / `EnumVariant` / `EnumDecl` — single-variant `Make*`
    constructors, arity matches.
  - `BodyItem` — 5 variants (BIMembership, BIPassthrough, BIClaimCall,
    BIConstraint, BISubclaim) all covered (lines 347-369).
  - `Expr` — 20 variants in `stdlib/ast.ev` (EIdentifier, EInt, EReal,
    EBool, EStr, ESetLit, ESeqLit, ERange, EInExpr, EForall, EExists,
    ECall, ECardinality, EIndex, EField, EBinary, ENot, ETernary,
    EMatch, EMatches), all 20 covered.
  - `MatchArm`, `MatchArmList`, `MatchPattern`, `MatchBind`, `BindList` —
    constructor names and arities all match.
  - All `*List` enums (`StringList`/SLNil/SLCons, `ExprList`/EL*,
    `MappingList`/ML*, `BodyItemList`/BIL*, `SchemaList`/SchL*,
    `EnumDeclList`/EDL*, `EnumVariantList`/EVL*, `EnumFieldList`/EFL*,
    `MatchArmList`/MAL*, `BindList`/BL*) — every name used in the
    encoder is present in `stdlib/ast.ev`.

**Real drifts (information loss the encoder hides):**

### Drift 1: `SchemaDecl::param_count` is silently dropped

Rust `ast::SchemaDecl` (`runtime/src/ast.rs:21-35`) carries
`param_count: usize` — the count of leading body items that came from
the first-line param list. `stdlib/ast.ev`'s `MakeSchemaDecl(Keyword,
String, BodyItemList)` (line 174) has no slot for this. The encoder
(`encode_schema_decl` at line 328) drops it.

A self-hosted pass that reads the encoded `SchemaDecl` cannot tell
which leading members are "interface" vs "helper-locals" — exactly the
distinction the Rust runtime uses to decide whether a recursive
`ClaimCall` re-binds with positional mapping or freshens helper consts.
A pass that wants to round-trip a `SchemaDecl` through
`encode_program` → desugar → `decode_program` will lose this field.

This is invisible at compile time (no error fires) and silent at
runtime (encoded program loads fine, just behaves differently).

### Drift 2: `EffectResult::Real` is silently coerced to `NoResult`

Lines 603-607:

> ```rust
> EffectResult::Real(_)     => {
>     // Real round-trips need real_from_f64 helper; defer
>     // until any actual program needs Real results.
>     apply(enums, "Result", "NoResult", &[])
> }
> ```

`stdlib/runtime.ev:160` declares `RealResult(Real)` as a real variant.
The Rust enum `EffectResult::Real(f64)` exists. The encoder has
`z3_real` available (line 106). But this arm coerces every Real
result to `NoResult` rather than emitting `RealResult(...)`. A pass
consuming a `last_results` Seq that contains a Real result will read
it back as `NoResult` — silent data loss.

The comment is honest but the behavior is a footgun: the only signal
to a future caller that Real results vanish is reading this file. A
runtime FSM that emits a `Time` or `ParseReal` effect and tries to
inspect the Real result in a self-hosted pass would silently see
`NoResult` and likely behave wrong.

### Drift 3: invariant doc undersells the cross-language coupling

The per-file brief says:

> Cross-language contract: any change to stdlib/ast.ev requires
> matching changes here AND in decode_ast.rs

…but the file *also* encodes types from `stdlib/runtime.ev` —
`Result` and `ResultList` (lines 581-630, under a section header that
explicitly names `stdlib/runtime.ev`). That means this file is
implicitly coupled to `stdlib/runtime.ev` too. A change to
`enum Result` or `enum ResultList` in that file would silently break
encoding here.

Not a code violation, but a brief that under-specifies the contract
makes future drift more likely. The brief should say: "any change to
**`stdlib/ast.ev` or to the `Result`/`ResultList` enums in
`stdlib/runtime.ev`** requires matching changes here and in
decode_ast.rs."

## Other notes

- **Dead `HashMap` imports.** Line 22 (`use std::collections::HashMap`)
  and line 551 (`use std::collections::HashMap as _Sentinel` with an
  `#[allow(unused_imports)]` shim) are both unused. The `_Sentinel`
  re-import is presumably intended to suppress a warning from the
  earlier import; the cleanup is to delete both. Style only —
  `cargo fmt`/`clippy` territory.
- **Recursive list encoders are quadratic for long lists.** `encode_*_list`
  helpers iterate-then-rev-prepend (lines 197-294) which is O(n)
  per list — fine. But `encode_match_arm_list` (line 479) and
  `encode_bind_list` (line 516) use head/tail recursion that builds a
  Rust call stack proportional to list length. For a `match` with
  pathological arm count or a constructor with many binds you'd blow
  the stack. Not something seen in practice today; flagging as a
  consistency note (the other list encoders use iteration, these two
  don't).

## Candidate new rules

### Suggested AP-009: encoded-AST round-trip parity

**Pattern observed at runtime/src/translate/encode_ast.rs:328 and :603:**
> `encode_schema_decl` drops `param_count`; `encode_effect_result` for
> `EffectResult::Real(_)` falls back to `NoResult`.

**Why it might be bad:** `encode_ast.rs` and `decode_ast.rs` are a
matched pair. If a Rust AST field has no slot in the corresponding
`stdlib/*.ev` enum (param_count) or if a Rust variant gets coerced to
a different stdlib variant on encode (Real → NoResult), then any
self-hosted pass that round-trips a program through the encoder, runs
desugar/inference in Evident, and decodes back will *silently* lose
fields or change values. The drift can hide for a long time because
no error fires — the encoded value is well-formed, just wrong.

**Suggested fix:** When a Rust AST field/variant has no
`stdlib/ast.ev` (or `stdlib/runtime.ev`) representation, the encoder
should either (a) error explicitly with `EncodeError::Unsupported`
naming the field, or (b) the stdlib enum should be extended to carry
the missing slot. Silent drop is the wrong default. A targeted fix
for the existing two cases:

  - Add `param_count: Int` to `MakeSchemaDecl` in `stdlib/ast.ev`
    (and update `decode_ast::decode_schema_decl` + every consumer);
    OR change `encode_schema_decl` to return `EncodeError::Unsupported`
    when `param_count > 0` so failures are loud.
  - Implement the Real → `RealResult` arm using the existing
    `z3_real` helper instead of falling back to `NoResult`.

**Detection idea:** Review-only — automated detection requires
cross-referencing every Rust AST field name against the corresponding
constructor's payload field count in `stdlib/*.ev`. Worth doing as a
one-shot audit task; not worth a recurring lint.

### Suggested AP-010: cross-language enum contract spans more than one stdlib file

**Pattern observed at runtime/src/translate/encode_ast.rs:581-630:**
> Section header "stdlib/runtime.ev: Effect / Result encoders" inside
> a file whose per-file brief says "Cross-language contract: any
> change to stdlib/ast.ev requires matching changes here AND in
> decode_ast.rs."

**Why it might be bad:** A future change to `stdlib/runtime.ev`'s
`Result`/`ResultList` enum would not trip the documented
"stdlib/ast.ev requires matching change" warning, because the
invariant doc names only one stdlib file. The encoder's coupling
surface is wider than the brief admits.

**Suggested fix:** Expand `lints/runtime-invariants.md` to list every
stdlib file each encoder/decoder is coupled to. For
`encode_ast.rs` and `decode_ast.rs`, that's both `stdlib/ast.ev` and
the `Result`/`ResultList` portion of `stdlib/runtime.ev`. As a
forcing function, add a `// CROSS-LANGUAGE CONTRACT:` comment block
at the top of each section in the encoder/decoder naming the stdlib
file + enums it mirrors.

**Detection idea:** Review-only. Could be partially mechanized by
grepping `apply(enums, "X", ...)` calls for enum names "X" not
declared in `stdlib/ast.ev`, then asserting they appear in some
other stdlib file the brief mentions.

## Clean

Against the active rulebook the file is clean (no AP-001..AP-008
violations). Against the per-file invariant brief, the file is mostly
clean — no Solver use, no AST authority, no library-specific code —
but two real cross-language drifts (`SchemaDecl::param_count`,
`EffectResult::Real`) and one under-specified coupling to
`stdlib/runtime.ev` suggest the proposed AP-009 / AP-010 rules above.
