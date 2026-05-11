# AP-013: no-package-paths-in-language-core

**Status:** active

**Pattern.** A file in the language-core role contains a string
literal of the form `"stdlib/<name>.ev"` or `"packages/<name>.ev"`.
Such paths are library bindings — they belong in `runtime/src/fti.rs`
(the registry that maps Evident type names to bridge installers) or in
a dedicated `SHIMMED_STDLIB_PATHS` const, never sprinkled through the
language core.

**Why.** The language core's job is to define what an Evident
program IS and how to translate / execute it. It must work if
every stdlib `.ev` file and every `packages/<library>/*.ev` were
deleted. When a hardcoded `"packages/sdl.ev"` shows up in
`runtime.rs` (the canonical historical example was `"stdlib/sdl.ev"`,
removed in `bf81ee6`), the language core takes on knowledge of
which library shims exist — exactly the coupling AP-001 forbids
for Rust types, but in string form. Codify so the cleanup can't
regress as the package layout evolves.

**Fix.** Move library path literals to the registry layer:
`runtime/src/fti.rs` for type-name → install-fn entries, or a
dedicated `SHIMMED_STDLIB_PATHS` const that the registry layer reads.
The language core should reference library paths only via a
caller-supplied path or a registry lookup, never as a baked-in
string.

**Detection.** grep

**Pattern (grep).** `"(stdlib|packages)/[^"]*\.ev"` in any language-core
file (same set as AP-001, production code only). Comment-only lines
exempt.

**Scope.**
  - Apply to: `runtime/src/ast.rs`, `runtime/src/lexer.rs`,
    `runtime/src/parser.rs`, `runtime/src/pretty.rs`,
    `runtime/src/subscriptions.rs`, `runtime/src/runtime.rs`,
    `runtime/src/effect_loop.rs`, `runtime/src/effect_dispatch.rs`,
    `runtime/src/ffi.rs`, `runtime/src/translate/*.rs`. Same
    file list as AP-001.
  - Do NOT apply to: `runtime/src/fti.rs` (registry — knows about
    stdlib / packages by design), `runtime/src/event_sources/*` (bridge
    role), `runtime/src/commands/*` (CLI may load any path the user
    supplies), `runtime/src/main.rs`, `runtime/src/lib.rs`.

**Exceptions.**
  - Comment-only lines (a doc-comment showing what library paths
    look like is fine — covered by the `:[[:space:]]*//` filter).
  - `#[cfg(test)]`-gated test code is exempt.
  - The home for the `SHIMMED_STDLIB_PATHS` const (post-`bf81ee6`,
    moved to `fti.rs`) legitimately holds these literals — it's not
    in the language-core file list.

**Examples.**
  - Pre-`bf81ee6`: `runtime/src/runtime.rs` had a hardcoded
    `"stdlib/sdl.ev"` import path baked into a code path that
    auto-loaded SDL when an SDL type was declared. Post-fix:
    auto-loaded shim paths live in a dedicated const outside the
    language-core file list, and `runtime.rs` consults the registry.
  - The same rule applies to the post-restructure `"packages/sdl.ev"`
    and any future `"packages/<library>.ev"` — they belong in the
    registry, not the language core.
