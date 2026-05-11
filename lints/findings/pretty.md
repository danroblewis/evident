# Findings: runtime/src/pretty.rs

Reviewed against `lints/rules/` as of baf8078.

## Violations of existing rules

None.

Rule applicability:
- **AP-001** (no-library-specific-in-language-core): pretty.rs is in scope.
  Grepped for `SDL_`, `Sdl[A-Z]`, `\bGl[A-Z]`, `Glsl`, `Audio[A-Z]`,
  `\.dylib`, `\.framework/`, `/opt/homebrew/lib/`, `/usr/lib/lib`. No
  matches. File contains only generic AST-printing logic.
- **AP-002, AP-003, AP-006, AP-007, AP-008**: scoped to `examples/*.ev`
  — not applicable.
- **AP-004**: scoped to `tests/conformance/**.py` — not applicable.
- **AP-005**: scoped to `runtime/tests/**.rs` — not applicable.

## Invariant compliance (per `lints/runtime-invariants.md`)

- "Lossy by design (not the inverse of the parser)": satisfied. Docstring
  at line 6 explicitly states "Not a precise round-trip pretty-printer".
  The Binary-operand paren wrap at lines 77-78 is intentionally
  conservative ("cheap, slightly noisy, never wrong").
- "Must not depend on Z3 or runtime": satisfied. The only `use` is
  `crate::ast::{BinOp, BodyItem, Expr, Mapping, MatchPattern}` (line 11).
  No `z3`, no `crate::translate::*`, no `crate::runtime::*`.
- "Must not grow into a serializer (no JSON, no spec emission)":
  satisfied. Output is `String` formatted with infix Unicode operators
  for human consumption only. No `serde`, no JSON, no structured
  emission.

## Candidate new rules

None that clear the bar for inclusion.

Observation worth recording but not promoting to a rule: the
`MatchPattern` rendering logic appears twice (lines 41-50 inside
`Expr::Matches` and lines 56-66 inside `Expr::Match` arms) as
near-identical inlined match blocks. Extracting a `fmt_pattern(pat:
&MatchPattern) -> String` helper would deduplicate. This is a
local refactor opportunity, not a recurring anti-pattern across the
codebase, so it does not warrant a rulebook entry.

## Clean

The file is clean against the active rulebook and all three invariants
documented for `pretty.rs`.
