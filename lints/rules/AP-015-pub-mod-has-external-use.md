# AP-015: pub-mod-has-external-use

**Status:** active

**Pattern.** A `pub mod X;` declaration in `runtime/src/lib.rs`
exists, but no file under `runtime/tests/`, `runtime/benches/`, or
`runtime/src/commands/` references it via `evident_runtime::X`
(direct path or brace-import). The `pub` is unjustified — should be
`mod X;` (private).

**Why.** Pattern E from the agent dependency-graph sweep. The
public surface of `evident_runtime` is intentionally narrow — every
`pub mod` widens what callers can reach into and what we have to
preserve as compatible. Pre-`0429ea6`, several modules were
`pub mod` speculatively or as a leftover from refactors, with
nothing actually using them externally. We narrowed the surface
in that commit. This rule prevents speculative re-widening.

**Fix.** Either:
  1. Demote to `mod X;` (private). Most cases.
  2. Actually use `evident_runtime::X` somewhere external. If the
     re-export is justified by tests / benches / commands, just
     write the use site.

**Detection.** grep (cross-file: lib.rs's `pub mod` list ↔ external
consumer scan)

**Pattern (grep).** Extract every `pub mod (\w+);` from
`runtime/src/lib.rs`. For each name `<X>`:
  - Search `runtime/tests/`, `runtime/benches/`, and
    `runtime/src/commands/` for `evident_runtime::<X>` (direct path)
    or `evident_runtime::{...<X>...}` (brace-import).
  - If no consumer exists, fail.

**Scope.**
  - Pub-mod source: `runtime/src/lib.rs`.
  - External-consumer search: `runtime/tests/`, `runtime/benches/`,
    `runtime/src/commands/`.

**Exceptions.**
  - If `lib.rs` has a `pub use X::{...}` re-export elsewhere, the
    re-exported items are the actual external surface; the bare
    `pub mod` may still be unnecessary, but the rule does not flag
    it (the items are reachable). Today this is only `runtime`,
    which is `mod runtime;` (private) with `pub use runtime::{...}`
    — so the exception doesn't currently fire on any `pub mod`.

**Examples.**
  - Pre-`0429ea6`: several `pub mod` decls in `lib.rs` had no
    `evident_runtime::X` consumers. Demoted to `mod X;` after the
    sweep. The rule catches the next time someone `pub mod`s
    speculatively.
