# Findings: runtime/src/ast.rs

Reviewed against `lints/rules/` as of HEAD (post-BODY_MARKERS extraction).

## Change under review

A new `pub const BODY_MARKERS: &[&str] = &["spawnable_only"]` was added
near the top of `ast.rs` (line 19, after the file's docstring). This was
extracted to fix an inline.rs invariant violation: previously
`inline.rs:507` referenced the literal string `"spawnable_only"`, which
made the translator know about a scheduler-side marker. Both
`inline.rs:509` and `effect_loop.rs:222-230` now reference this const
(and the adjacent docstring).

## Assessment of the change

**The extraction does NOT violate ast.rs's invariant.** The
runtime-invariants brief says ast.rs is "pure data definitions, no
behavior, no I/O, no logic beyond trivial derives." A `pub const` of
strings is unambiguously data — it has no behavior, no I/O, no
control flow, and is a sibling shape to the existing variant enums
(both enumerate "names that mean something at the AST level"). It is
the same kind of artifact as `Keyword::{Schema, Claim, Type, Subclaim}`
or `BinOp::{And, Or, Implies, …}` — a fixed vocabulary the AST layer
publishes for downstream layers to recognize.

**Concern about scope creep is real but does not apply here.** A
"metadata grab-bag" file would be one where ast.rs accumulated lookups
keyed on AST shapes ("is this constraint a pure read?", "what's the
arity of this Effect?"), and the docstring explicitly forbids that
expansion: it tells future-you to add an entry "ONLY when the meaning
of a bare-identifier body item is established at the AST level — i.e.
when it's a language convention, not a one-off scheduler hook." That
gating clause is the right fence: BODY_MARKERS is the registry of
language-level reserved bare-identifier names, analogous to a reserved-
keyword list. It is data about Evident syntax, not metadata about
runtime behavior.

**The inline.rs invariant violation is cleanly addressed.** The
translator no longer references `"spawnable_only"` by literal; it
queries `crate::ast::BODY_MARKERS` and skips any matching identifier.
This is exactly the right shape — inline.rs no longer encodes
knowledge of a specific scheduler hook; it encodes the general rule
"identifiers in this AST-level reserved list are not translatable
constraints."

**One small drift to flag.** The docstring (lines 6-18) says "the
constraint translator skips it (it has no Bool meaning) and runtime
layers that care about the marker (currently the multi-FSM scheduler)
inspect the body for it directly." This describes the contract
correctly, but `effect_loop.rs:230` still hardcodes the literal
`"spawnable_only"` string in its `matches!` guard rather than checking
membership against `BODY_MARKERS`. Inline.rs got the const-reference
treatment; the scheduler is half-converted (the comment at 222-223
says the right thing — "one of `crate::ast::BODY_MARKERS`" — but the
code below it doesn't use the const). Not an ast.rs violation, but if
the goal of the extraction is "no layer hardcodes the literal string,"
effect_loop.rs:230 is still doing that. Worth fixing in the same pass.

## Violations of existing rules

None outside the documented exemptions.

Rule applicability:
- **AP-001** (no-library-specific-in-language-core): ast.rs is in
  scope. Grepped for `SDL_`, `Sdl[A-Z]`, `\bGl[A-Z]`, `Glsl`,
  `Audio[A-Z]`, `\.dylib`, `\.framework/`, `/opt/homebrew/lib/`,
  `/usr/lib/lib`. All non-comment hits are on lines documented in
  `lints/exemptions/AP-001.txt`:
  - Line 366 (`pub struct SdlVertex {`) — exempted.
  - Line 403 (`SdlVertexBuf(Vec<SdlVertex>)`) — exempted.

  Doc-comment mentions of SDL types appear at lines 360, 361, 362,
  363, 398. AP-001's rule text states that "Comment-only lines
  (starting with `//`, `///`, or `//!`) are exempt." All five are
  `///` doc-comment lines elaborating on the exempted struct/variant,
  so they are not standalone violations under the rule as written.

  Note on line numbers: the BODY_MARKERS const shifted everything in
  the file by +15 lines vs the prior review. The exemption file's
  pinned line numbers (366, 403) happen to still match the current
  positions of the offending tokens — verified by grep — so no
  exemption update is required.

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
- Zero `impl` blocks. Zero `fn` definitions.
- One `pub const` (BODY_MARKERS) — pure data, no logic.
- Behavior is exclusively trait derives (`Debug, Clone, PartialEq,
  Eq, Default`) plus `#[repr(C)]` on the (exempted) `SdlVertex`.
- No I/O references (no `std::io`, no `print!`, no `File`).
- No references to Z3, the solver, the scheduler, or FFI machinery
  beyond the variant *names* in `Effect` and `EffectFfiArg`, which
  are AST shapes mirroring `stdlib/runtime.ev` (cross-language data
  contract, not behavior).

## Candidate new rules

None that clear the bar for inclusion.

Three observations recorded but NOT promoted to rules:

**Observation 1 (review-only): specialized FFI-arg variants
motivated by single-library needs.** `EffectFfiArg::I32Buf` (lines
392-397) and `EffectFfiArg::IntOut` (lines 404+) have generic names
but doc comments that justify their existence by enumerating specific
C-API call sites (`SDL_Rect`, `SDL_Point`, `glGenVertexArrays`,
`glGetShaderiv`). The variant names themselves are generic (so
AP-001 is not triggered), but the proliferation of specialized
buffer-shape variants is the same accretion pattern that produced
`SdlVertexBuf`. The right long-term answer (per the AP-001 exemption
note) is a generic `ArgByteBuf(Vec<u8>)` plus a stdlib packing layer.
Calling it out for human review only.

**Observation 2 (review-only): `Effect::SpawnFsm` doc comment is
~25 lines of example program** (now lines 314-338 after the line
shift). Doc comments embedding multi-line worked examples in a
pure-data file blur the AST role into "AST + protocol documentation."
The cross-file contract belongs in `docs/design/fsm-spawning.md`
(which the comment already references); a one-line summary plus the
doc link would suffice. Not anti-pattern enough to mechanize as a
rule.

**Observation 3 (review-only, NEW with this change):
half-converted literal-vs-const reference for BODY_MARKERS.** The
extraction at `ast.rs:19` is intended to be the single source of
truth for bare-identifier marker names. `inline.rs:509` correctly
calls `crate::ast::BODY_MARKERS.contains(...)`. But
`effect_loop.rs:230` still pattern-matches the literal
`"spawnable_only"` directly inside a `matches!` guard, with only a
comment at 222-223 mentioning the const. If the marker name ever
changes (or another marker is added with the same scheduler
treatment), inline.rs will Just Work and effect_loop.rs will
silently drift. Worth a follow-up to make effect_loop.rs use the
const too — but this is an effect_loop.rs / consistency concern, not
an ast.rs concern. Not a candidate for a new mechanizable rule
because the "always reference the const, never the literal" pattern
is too narrow to grep for usefully.

## Clean

The file is clean against all 8 active rules (with the two SDL
struct/variant declarations and their doc lines documented in
`lints/exemptions/AP-001.txt`) and against its `runtime-invariants.md`
brief. The BODY_MARKERS extraction is appropriate for ast.rs's
role and does not introduce a new violation here. The remaining
literal-string reference in effect_loop.rs:230 is a follow-up for
that file, not a regression in ast.rs.
