# Findings: runtime/src/translate/decode_ast.rs

Reviewed against `lints/rules/` as of HEAD (188c682).

## Violations of existing rules

None outside the documented AP-001 exemptions
(`runtime/src/translate/decode_ast.rs:623,643,651,660` in
`lints/exemptions/AP-001.txt`).

### AP-001 exempted hits (recorded for completeness; NOT new violations)

The grep pattern `Sdl[A-Z][a-zA-Z]` (case-sensitive, lowercase
`d`+`l`) fires on lines 623, 643, 651, 660 — all references to
`crate::ast::SdlVertex` / `EffectFfiArg::SdlVertexBuf`. All four
are present in `lints/exemptions/AP-001.txt` and the file otherwise
passes the rule.

There are additional SDL-token hits in this file that are NOT
caught by AP-001's grep but are arguably the same "language-core
knows about SDL" leak conceptually:

  - line 621: `"ArgSDLVertexBuf"` — the FFI variant string tag
  - line 644: `check_enum(v, "SDLVertex")?`
  - line 645: `if variant != "MkSDLVertex"`
  - line 647: `enum_name: "SDLVertex".into()`
  - line 661: `decode_list(v, "SDLVertexList", "SVNil", "SVCons", …)`

These contain `SDL` (uppercase) without an underscore, so they
miss the `SDL_` and `Sdl[A-Z]` (lowercase d/l) patterns. They
ride along with the exempted `decode_sdl_vertex[_list]` functions
and would be removed in the same refactor as the AP-001
exemptions, so I'm not proposing a new rule for them — flagging
here so the eventual fix knows to remove the string tags too.

## Cross-language contract check (decoder ↔ stdlib/ast.ev + stdlib/runtime.ev)

Confirmed: every `check_enum` / `decode_list` shape in this file
matches an enum decl in `stdlib/ast.ev` or `stdlib/runtime.ev`.

stdlib/ast.ev coverage:
  - `BinOp` (14 variants) — all 14 mapped in `decode_binop`.
  - `Keyword` (4 variants) — all 4 mapped.
  - `Pins` (3 variants) — all 3.
  - `BodyItem` (5 variants) — all 5.
  - `Expr` (21 variants) — all 21.
  - `MatchPattern` (2), `MatchBind` (2), `MatchArm` (1),
    `Mapping` (1), `EnumField` (1), `EnumVariant` (1),
    `EnumDecl` (1), `SchemaDecl` (1), `Program` (1) — all
    correct arities.
  - All 9 `*List` enums (`StringList`, `BodyItemList`,
    `ExprList`, `MappingList`, `BindList`, `MatchArmList`,
    `SchemaList`, `EnumDeclList`, `EnumVariantList`,
    `EnumFieldList`) have matching `Nil`/`Cons` variant names.

stdlib/runtime.ev coverage:
  - `FFIArg` (10 variants) — all 10.
  - `Effect` (18 variants) — all 18.
  - `Result` (7 variants) — all 7.
  - `SDLVertex(MkSDLVertex)` arity 8 — matches.
  - All `*List` enums (`StrList`, `IntList`, `SDLVertexList`,
    `ArgList`, `EffectList`, `ResultList`) — match.

One documented intentional drop: `decode_program` builds a
`Program` with `imports: Vec::new()` and `decode_schema_decl`
sets `param_count: 0` (lines 367, 527). Both are noted in
inline comments and reflect the fact that stdlib/ast.ev's
`MakeProgram` and `MakeSchemaDecl` shapes don't carry these
fields. Acceptable per the file's "reconstructs only" invariant
— the decoder fills the gap with default values rather than
inventing.

## Adherence to per-file invariants

  - "Inverse of encode_ast" — yes, structurally; every `decode_*`
    has a counterpart in `encode_ast.rs`.
  - "Must NOT invent AST nodes from scratch" — yes; every
    constructed node comes from a decoded `Value`.
  - "Must NOT know about Effects, FFI, or scheduler beyond AST
    variants" — partially. The file does decode `Effect`,
    `EffectResult`, `EffectFfiArg`, `SdlVertex` (lines 539–688)
    because those AST variants exist in `ast.rs`. The invariant
    permits this ("beyond AST variants"), so OK.
  - "Must fail fast on shape mismatch" — yes; `check_enum`,
    `need_arity`, and `DecodeError::UnknownVariant` all
    short-circuit.
  - "Cross-language contract: must change in lockstep with
    stdlib/ast.ev AND encode_ast.rs" — verified above; the
    decoder is currently in sync with both stdlib files.

## Candidate new rules

### Suggested AP-009: dead-code stub kept alive by a wrapper function

**Pattern observed at decode_ast.rs:531-535:**
> ```rust
> // `variant_name` is exposed for diagnostic use (e.g. error
> // messages on round-trip mismatches); silence unused-import
> // warning when the only callers are inside this file.
> #[allow(dead_code)]
> fn _use_variant_name(v: &Value) -> &str { variant_name(v) }
> ```

`variant_name` is declared on line 57 but never called anywhere
in the crate. The fix chosen is a no-op wrapper function
`_use_variant_name` plus an `#[allow(dead_code)]` on the wrapper
to suppress the warning. The comment claims `variant_name` is
"exposed for diagnostic use" but the function isn't `pub`, so
no other module can call it.

**Why it might be bad:** Two anti-patterns at once. (1) The
target function is dead code dressed up to look live; future
readers can't tell whether `variant_name` is part of the API
surface or just unreachable. (2) The wrapper-to-suppress-warning
trick costs a same-named symbol just for the lint; the direct
fix is `#[allow(dead_code)]` on `variant_name` itself, or
deleting both and the comment.

**Suggested fix:** Either delete `variant_name` and
`_use_variant_name` outright, or annotate `variant_name` directly
with `#[allow(dead_code)]` and a one-line comment naming the
intended caller. Don't keep both functions.

**Detection idea:** grep for `fn _use_[a-z_]+` in
`runtime/src/**/*.rs` — the underscore-prefixed `_use_X` pattern
is a smell signature for this construction. Could also be ASTed
by walking `runtime/tests/lints.rs` for functions whose body is
exactly one expression and whose name starts with `_use_`.
Review-only is fine — this is a one-off so far.

This is the only candidate I see in the file. Listing as
review-only; not creating a rule file or check function.

## Clean

Other than the already-documented AP-001 exemptions and the
candidate new rule above, the file is clean. The decoder is
mechanically faithful to both stdlib enum decl files, fails fast
on shape mismatches, and stays within its declared invariant.
