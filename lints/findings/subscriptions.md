# Findings: runtime/src/subscriptions.rs

Reviewed against `lints/rules/` as of baf8078.

## Violations of existing rules

None.

- AP-001 (no library-specific in language-core): `subscriptions.rs` is
  in scope. Grep for the rule's forbidden token classes
  (`SDL_|Sdl[A-Z]|\bGl[A-Z]|Glsl|Audio[A-Z]|\.dylib|\.framework/|/opt/homebrew/lib/|/usr/lib/lib`):
  zero matches. The doc comment at line 119 mentions "stdin plugin"
  generically. Clean.
- AP-002 / AP-003 / AP-006 / AP-007 / AP-008: scoped to `examples/`;
  not applicable.
- AP-004 / AP-005: scoped to `tests/conformance/` / `runtime/tests/`;
  not applicable.

## Per-file invariant check (`lints/runtime-invariants.md`)

- "Pure static analysis — AST → Set<String>." Holds. Two public
  functions: `world_access_sets(claim) -> AccessSets` and
  `body_references_identifier(claim, ident) -> bool`. Both pure.
- "Must NOT touch the Solver." Holds. Zero `z3::*` imports; no
  Solver / Context / Sort / Expr references.
- "Must NOT touch any translation state (no EnumRegistry, no Var
  bindings)." Holds. Zero `translate::*` imports.
- "Must NOT cause side effects." Holds. No file I/O, no
  println, no env access, no statics-with-state.
- "Must NOT resolve passthrough or ClaimCall recursively."
  Holds. `BodyItem::Passthrough(_) => {}` (line 51) is the
  no-op the invariant requires. `BodyItem::ClaimCall` (lines
  53-55) walks only the call's own mapping VALUES, not the
  called claim's body — exactly the opaque treatment specified.
- "Must NOT know about Effects, FTI, scheduling state, or any
  C library." Holds. No `Effect*`, `Fti*`, `EventSource`,
  `DispatchContext`, `Scheduler*`, `libffi`, `libloading` imports.
- "Dependencies: `ast` only." Holds.
  `use std::collections::HashSet;` (line 21) and
  `use crate::ast::{BodyItem, Expr, Mapping, MatchPattern, Pins, SchemaDecl};`
  (line 23) — those are the only two `use` lines outside the
  `#[cfg(test)]` block.

## Candidate new rules

### Suggested AP-009: no fake-use anchors to silence unused-import warnings
**Pattern observed at runtime/src/subscriptions.rs:104 and 109:**
```rust
let _ = MatchPattern::Wildcard; // anchor for future changes
…
// Mapping appears only inside Pins/ClaimCall, handled above.
let _ = std::any::type_name::<Mapping>();
```

**Why it might be bad:** `MatchPattern` and `Mapping` are imported at
line 23 but never substantively used in the module body — patterns are
not walked (the comment at lines 102-103 explains why), and `Mapping`
is only ever destructured via `m.value` field access through the
top-level `BodyItem::ClaimCall { mappings, .. }` arm, which doesn't
require the type name in scope. The two `let _ = …` statements exist
purely to suppress `unused_imports` warnings on imports that aren't
actually load-bearing in the current code. The comments call them
"anchors for future changes," but a non-load-bearing import is not an
"anchor" — it's an unused import dressed up to look intentional.
The right fix is to remove the import; if a future change needs the
type, add the import then. Tolerating this pattern teaches authors
that "the linter complained, so I'll add a `let _ =`" is the
acceptable response to dead code, instead of "delete the dead code."
The pattern is small and easy to copy; it would creep into other
files reviewed by other agents in exactly the same shape if not
named.

**Suggested fix:** Drop the `MatchPattern` and `Mapping` items from
the `use crate::ast::{…};` line. Delete the two `let _ = …` lines.
If a future edit needs the types, add the import in the same commit
that uses them.

**Detection idea:** grep for `let _ = .*::[A-Z][a-zA-Z]*;?\s*//` and
`let _ = std::any::type_name::<` in `runtime/src/**/*.rs`. Both
patterns are very specific to the "fake-use anchor" idiom and have
near-zero false-positive risk (legitimate `let _ = expr` discards a
value-returning expression; the suspicious form discards a unit
constant or a name's type-id).

### Suggested AP-010 (review-only): duplicated AST-walker pair when one mutates and the other queries
**Pattern observed at runtime/src/subscriptions.rs:**
The module has TWO walks over the AST: `walk_body / walk_pins /
walk_expr` (lines 47-110, mutating `&mut AccessSets`) and a
near-identical inner `walk / walk_pins / walk_expr` triplet inside
`body_references_identifier` (lines 122-170, returning `bool`). They
match the AST structure case-for-case, but each has to be kept in
sync independently as new `Expr` / `BodyItem` variants land.

**Why it might be bad:** When `Expr` grows a new variant
(historically: `Match`, `Matches`, `Ternary` were each added one at
a time), both walkers must be updated. Forgetting one silently
under-counts or under-detects. This is the textbook case for a
generic visitor with a `Visit` trait or a callback-based walker; the
two specializations would call `visit_expr(&expr, |e| { … per-call
side effect … })`.

**Why review-only:** Rust's lack of stable monomorphic visitor
patterns means a generic walker would either pay closure-call
overhead or require macro generation. The duplication is local
(one file, ~40 lines of mirror) and the AST is small enough that
review can catch divergence. Not worth a mechanical rule today, but
worth noting if a third walker shows up in this file or if the AST
gains another five variants.

**Suggested fix:** Defer until a third walker is needed. At that
point extract a single visitor into `ast.rs` or a new `ast_walk.rs`,
parameterized by either a closure or a small trait.

**Detection idea:** Review-only — too easy to over-flag legitimate
single-purpose walkers.

## Notes (clean otherwise)

The file is otherwise idiomatic for its role: small (172 non-test
lines), single-concern, leaf in the dependency graph, fully tested
(6 in-file unit tests covering the documented limitations and the
common cases). The one real anti-pattern is AP-009 above; AP-010
is a long-term watch item, not a present-day violation.
