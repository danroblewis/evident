# Findings: runtime/src/translate/datatypes.rs

Reviewed against `lints/rules/` as of baf8078.

## Violations of existing rules

None.

- AP-001 (no library-specific in language-core): `translate/*.rs` is
  in scope. Grep for `Sdl[A-Z]`, `SDL_`, `\bGl[A-Z]`, `Glsl`,
  `Audio[A-Z]`, `\.dylib`, `\.framework/`, `/opt/homebrew/lib/`,
  `/usr/lib/lib` returns zero hits. Doc comments mention `SDLRect`
  and `SDLOutput` as examples (lines 3-4, 22-23) but these are in
  `///` doc comments which the rule explicitly exempts; they are
  also not matches for the case-sensitive grep patterns
  (`SDLRect` lacks the trailing `_` of `SDL_` and lacks the lower-
  case 'd' of `Sdl[A-Z]`). Clean.
- AP-002, AP-003, AP-006, AP-007, AP-008: examples-only scope; not
  applicable.
- AP-004: conformance-only scope; not applicable.
- AP-005: `runtime/tests/**` scope; not applicable. File contains
  no in-file `#[cfg(test)]` block either.

## Per-file-invariant check

The brief's three "must NEVER" clauses for `translate/datatypes.rs`
all hold:

- **Builds Z3 SORTS only — never expressions.** Z3 imports are
  `Context, DatatypeAccessor, DatatypeBuilder, DatatypeSort, Sort`
  (line 7). Sort constructions used: `Sort::int(ctx)`,
  `Sort::bool(ctx)`, `Sort::string(ctx)`, `nested_dt.sort.clone()`,
  and the final `DatatypeBuilder::new(...).variant(...).finish()`.
  No `z3::ast::*` import; no `Int::new_const` / `Bool::*` /
  `String::*` / quantifier construction anywhere.
- **Never asserts constraints, never calls the Solver.** No
  `Solver` import or use. No `assert(...)` / `add(...)` /
  `assert_and_track(...)` calls. The file is read-only with respect
  to constraints — comment at lines 105-108 even calls out that
  type-body invariants are intentionally not asserted on Seq
  elements in v1.
- **Never owns the DatatypeRegistry — borrows it.** Signature on
  line 39: `registry: &DatatypeRegistry`. Reads via
  `registry.borrow()` (line 42), writes via `registry.borrow_mut()`
  (line 124). Borrow only.

Other invariants from the brief that hold:

- Caches results so siblings sharing a nested type get one Z3 sort
  (line 42-44 cache hit; line 124 cache insert).
- Dependencies match the brief: `types` (for `DatatypeRegistry`,
  `FieldKind`) and `ast` only — no other crate-internal imports.
- `Box::leak` of the per-type `DatatypeSort` is consistent with the
  runtime's already-leaked `Context` (documented at lines 31-34).

## Candidate new rules

None worth promoting. Two observations that did NOT clear the
proposing bar:

**Observation 1 (review-only).** The fall-through arm at lines
94-102 reports unsupported field types via `eprintln!` and returns
`None`. Callers see only `None` — no way to distinguish "schema not
found" (line 45) from "field type unsupported" (line 101) from
"no fields" (line 111) from a successful inner build that itself
returned None (line 81). The `eprintln!` channel is the only signal,
which couples user-visible diagnostics to stderr ordering. Pattern
recurs in this file (3 distinct warn-and-None sites). A
`Result<(...), DatatypeBuildError>` would be cleaner. Style/refactor,
not an anti-pattern that recurs across files in a flagged way; not
promoting.

**Observation 2 (review-only).** `Box::leak` for every per-type
`DatatypeSort` is intentional and documented, but the recursive
descent means a malformed schema cycle would currently produce
unbounded recursion before stack overflow rather than a clean
"already in progress" detection. (No cycle guard exists — neither a
visit set nor a "currently building" sentinel in the registry.)
This is genuinely a latent bug rather than a style issue, but it's
a concrete one-file fix, not a recurring anti-pattern across the
codebase; not a rule. Worth noting to whoever fixes it.

## Clean

The file is clean against all 8 active rules and against its
runtime-invariants brief. No findings to fix.
