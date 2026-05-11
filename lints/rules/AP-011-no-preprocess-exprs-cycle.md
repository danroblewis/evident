# AP-011: no-preprocess-exprs-cycle

**Status:** active

**Pattern.** `runtime/src/translate/preprocess.rs` imports
`super::exprs` (or `crate::translate::exprs`), AND/OR
`runtime/src/translate/exprs.rs` imports `super::preprocess` (or
`crate::translate::preprocess`). Either direction creates the
cycle; both are forbidden.

**Why.** `preprocess` is an AST→AST stage; `exprs` is an
AST→Z3-expression stage. Historically they grew shared helpers
across the boundary — `preprocess` borrowed `translate_int` from
`exprs` for literal folding, and `exprs` borrowed env utilities
from `preprocess`. The mutual dependency made the layering opaque
and any change in one rippled into the other for no good reason.
Commit `092b62c` broke the cycle by promoting shared helpers down
to `types.rs`. This rule prevents the cycle from re-forming.

**Fix.** Helpers used by both `preprocess` and `exprs` belong in
`types.rs` (the shared data leaf). Code that depends on both
stages belongs in `inline.rs` or `eval.rs` (which sit above both).

**Detection.** grep

**Pattern (grep).** Two greps, one per file:
  - In `preprocess.rs`: `use (super::|crate::translate::)exprs`.
  - In `exprs.rs`: `use (super::|crate::translate::)preprocess`.
Either firing = fail.

**Scope.**
  - Apply to: exactly two files —
    `runtime/src/translate/preprocess.rs` and
    `runtime/src/translate/exprs.rs`.

**Exceptions.**
  - `#[cfg(test)]`-gated blocks (test code may import either side
    to exercise integration). Strips out via `strip_rs_test_modules`.
  - Comment-only lines.

**Examples.**
  - Pre-`092b62c`: `preprocess.rs` had
    `use super::exprs::translate_int;`
    and `exprs.rs` had
    `use super::preprocess::{env_clone, literal_range};`.
    Cycle. Post-fix: shared helpers live in `types.rs`; both
    files import from `types`, neither imports the other.
