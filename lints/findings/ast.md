# Findings: runtime/src/ast.rs

Reviewed against `lints/rules/` as of baf8078.

## Violations of existing rules

None outside the documented exemptions.

Rule applicability:
- **AP-001** (no-library-specific-in-language-core): ast.rs is in
  scope. Grepped for `SDL_`, `Sdl[A-Z]`, `\bGl[A-Z]`, `Glsl`,
  `Audio[A-Z]`, `\.dylib`, `\.framework/`, `/opt/homebrew/lib/`,
  `/usr/lib/lib`. All non-comment hits are on the lines documented in
  `lints/exemptions/AP-001.txt`:
  - Line 345 (`/// SDL_Vertex layout — 20 bytes…`) — exempted ("doc comment about SDL_Vertex").
  - Line 351 (`pub struct SdlVertex {`) — exempted.
  - Line 383 (`/// Pack N \`SDL_Vertex\` structs…`) — exempted ("SDL-specific buffer doc").
  - Line 384 (`/// \`SDL_RenderGeometry\`…`) — exempted.
  - Line 388 (`SdlVertexBuf(Vec<SdlVertex>)`) — exempted.

  Additional doc-comment mentions of SDL types appear at lines 346,
  347, 348, 379, 380. AP-001's own rule text states that "Comment-only
  lines (starting with `//`, `///`, or `//!`) are exempt." All five
  are `///` doc-comment lines that elaborate on the exempted struct/
  variant they belong to (SDL_FPoint / SDL_Color / SDL_Vertex* /
  SDL_Rect / SDL_Point), so they are not standalone violations under
  the rule as written. No NEW library names appear (no GL, Audio,
  Cocoa, etc.).
- **AP-002, AP-003, AP-006, AP-007, AP-008**: scoped to `examples/*.ev`
  — not applicable.
- **AP-004**: scoped to `tests/conformance/**.py` — not applicable.
- **AP-005**: scoped to `runtime/tests/**.rs` — not applicable.

## Invariant compliance (per `lints/runtime-invariants.md`)

The brief for `ast.rs` says: "Pure data definitions — no behavior,
no I/O, no references to Z3 or anything else… Zero `use crate::*`
imports — leaf module… Never contain logic beyond trivial derives."

Verified:
- Zero `use` statements of any kind. No `crate::*`, no `z3`, no
  external crate. Pure leaf.
- Zero `impl` blocks. Zero `fn` definitions. Behavior is exclusively
  trait derives (`Debug, Clone, PartialEq, Eq, Default`).
- No I/O references (no `std::io`, no `print!`, no `File`).
- No references to Z3, the solver, the scheduler, or FFI machinery
  beyond the variant *names* in `Effect` and `EffectFfiArg`, which
  are AST shapes mirroring `stdlib/runtime.ev` (cross-language data
  contract, not behavior).

## Candidate new rules

None that clear the bar for inclusion.

Two observations recorded but NOT promoted to rules:

**Observation 1 (review-only): specialized FFI-arg variants
motivated by single-library needs.** `EffectFfiArg::I32Buf` (lines
377–382) and `EffectFfiArg::IntOut` (lines 389–395) have generic
names but doc comments that justify their existence by enumerating
specific C-API call sites (`SDL_Rect`, `SDL_Point`,
`glGenVertexArrays`, `glGetShaderiv`). The variant names themselves
are generic (so AP-001 is not triggered), but the proliferation of
specialized buffer-shape variants is the same accretion pattern that
produced `SdlVertexBuf`. A formal rule capping how many specialized
buffer-shape primitives may live in `EffectFfiArg` is too vague to
mechanize; the right long-term answer (per the AP-001 exemption
note) is a generic `ArgByteBuf(Vec<u8>)` plus a stdlib packing layer.
Calling it out for human review only.

**Observation 2 (review-only): `Effect::SpawnFsm` doc comment is
~25 lines of example program (lines 299–323).** Doc comments embedding
multi-line worked examples in a pure-data file blur the AST role
into "AST + protocol documentation." The `SpawnFsm` cross-file
contract belongs in `docs/design/fsm-spawning.md` (which the comment
already references); a one-line summary plus the doc link would be
sufficient here. Not anti-pattern enough to mechanize as a rule —
verbose docstrings in data files are a judgment call, not a recurring
shortcut — but worth noting that this file's invariant ("pure data,
no behavior") is in spirit, not just in code, slightly strained by
this docstring's protocol exposition.

## Clean

The file is clean against all 8 active rules (with the two SDL
struct/variant declarations and their immediate doc lines documented
in `lints/exemptions/AP-001.txt`) and against its `runtime-invariants.md`
brief. No new findings to fix.
