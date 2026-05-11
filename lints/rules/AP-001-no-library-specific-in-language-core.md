# AP-001: no-library-specific-in-language-core

**Status:** active

**Pattern.** A file in the **language-core role** —
`runtime/src/{ast,lexer,parser,pretty,subscriptions}.rs` and
`runtime/src/translate/*.rs` — contains identifiers, type
declarations, or `#[repr(C)]` structs that mirror a specific C
library's data model. Examples of forbidden tokens: `Sdl[A-Z]`,
`SDL_`, `\bGl[A-Z]`, `Glsl`, `Audio[A-Z]`, dlopen-able paths
(`.dylib`, `.framework/`, `/opt/homebrew/lib/`).

**Why.** The language-core role's job is to define what an
Evident program IS and how to translate it. It must compile and
work if every C library binding were deleted. When library
specifics leak in (the canonical example: `pub struct SdlVertex`
landed in `runtime/src/ast.rs`, `EffectFfiArg::SdlVertexBuf` in
the same role's `ffi.rs`, `decode_sdl_vertex` in
`translate/decode_ast.rs`), four files in the language-core role
end up knowing about SDL. Removing or replacing SDL becomes a
multi-file refactor of the language definition.

**Fix.** Library-specific code goes in the bridge role
(`runtime/src/event_sources/<library>.rs`, currently in the
single 1390-line `event_sources.rs` pending split) and in the
stdlib wrapper role (`packages/sdl/`, etc.). If the language-core
role needs a hook (e.g. a generic typed buffer to support
`SDL_RenderGeometry`), add a generic primitive
(`ArgByteBuf(Vec<u8>)`), not a library-specific variant.

**Detection.** grep

**Pattern (grep).** Forbidden token classes (case-sensitive):
`SDL_`, `Sdl[A-Z][a-zA-Z]`, `\bGl[A-Z]`, `Glsl`, `Audio[A-Z]`,
`\.dylib`, `\.framework/`, `/opt/homebrew/lib/`,
`/usr/lib/lib`. Comment-only lines (starting with `//`,
`///`, or `//!`) are exempt.

**Scope.**
  - Apply to: `runtime/src/ast.rs`, `runtime/src/lexer.rs`,
    `runtime/src/parser.rs`, `runtime/src/pretty.rs`,
    `runtime/src/subscriptions.rs`, `runtime/src/translate/*.rs`,
    `runtime/src/runtime.rs`, `runtime/src/effect_loop.rs`,
    `runtime/src/effect_dispatch.rs`, `runtime/src/ffi.rs`.
  - Do NOT apply to: `runtime/src/event_sources*` (the bridge
    role), `runtime/src/fti.rs` (the registry that maps Evident
    type names → bridge install fns; mentions library names
    intentionally), `runtime/src/commands/*` (the CLI surface
    may wire any layer).

**Exceptions.**
  - Tokens in line comments / doc comments don't count
    (a doc-comment example mentioning SDL is fine).
  - String literals naming a generic FFI thing
    ("libffi", "dlopen") are NOT library-specific; only real
    C libraries count.
  - Code gated by a `#[cfg(...)]` (or `#[cfg_attr(...)]`)
    that includes `test` as a predicate is exempt — tests
    legitimately reference real libraries (libc to exercise
    the FFI primitive, etc.). Covers the common forms
    `#[cfg(test)]`, `#[cfg(any(test, feature = "..."))]`,
    `#[cfg(all(test, target_os = "macos"))]`,
    `#[cfg_attr(test, ...)]`. The rule applies only to
    production code.

**Examples.**
  - `runtime/src/ast.rs` lines ~345-355 today: `pub struct
    SdlVertex { pub pos: [f32; 2], pub color: [u8; 4], pub tex:
    [f32; 2] }`. Documented as a known violation in
    `examples/COUNTEREXAMPLES.md` and `docs/design/code-standards.md`.
  - `runtime/src/ast.rs` line ~388: `EffectFfiArg::SdlVertexBuf`
    variant.
  - `runtime/src/ffi.rs` line ~66 + ~325-365: corresponding
    handling.
  - `runtime/src/translate/decode_ast.rs` lines 621-661:
    `decode_sdl_vertex`, `decode_sdl_vertex_list`,
    `"ArgSDLVertexBuf"` arm.
