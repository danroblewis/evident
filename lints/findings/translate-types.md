# Findings: runtime/src/translate/types.rs

Reviewed against `lints/rules/` as of baf8078.

## Violations of existing rules

None.

- AP-001 (no library-specific in language-core): `translate/*.rs` is in scope.
  The only library-shaped tokens in the file (`SDLRect`, `SDLOutput`, `Color` at
  lines 29-30 and 163) all sit inside `///` doc comments, which the rule
  explicitly exempts. No `#[repr(C)]` structs, no dlopen paths, no
  library-specific identifiers in code.
- AP-002 / AP-003 / AP-006 / AP-007 / AP-008: scoped to `examples/` or
  `tests/conformance/`; not applicable.
- AP-004 / AP-005: scoped to test files; not applicable.

## Per-file invariant check (`lints/runtime-invariants.md`)

- "Pure data + trivial constructors only — no translation LOGIC." Holds. The
  file declares `EvalResult`, `Value`, `FieldKind`, `Var`, `EnumRegistry`,
  `DatatypeRegistry`, `CachedSchema`, plus a few accessor methods (`as_bool`,
  `as_str`, `as_real`, `as_seq`, `as_set`, `as_datatype_seq`, `FieldKind::name`)
  that are pure pattern matches, plus `EnumRegistry::new` / `Default`.
- "No Z3 expression construction, no Solver use, no constraint assertion."
  Holds. No calls to `.assert`, `.check`, `.push`, `.pop`, `.add`, `_eq`,
  `simplify`, etc. `CachedSchema` *stores* a `Solver<'ctx>` field, but storing
  an opaque handle is not USE; the assertion stack is built by `inline.rs` and
  driven by `eval.rs`.
- "Leaf within translate/ (no super::* imports)." Holds. Imports are `std`,
  `z3`, and one path-qualified reference to `crate::ast::EnumVariant`
  (line 51) — exactly what the invariant doc allows.
- "Must not know about Effects, scheduler, FFI." Holds. No `Effect`, `Fsm`,
  `LibCall`, `FfiArg`, or scheduler types appear.

## Candidate new rules

None. Nothing in this file looks like a recurring shortcut or layering
violation that would generalize into a new rule.

## Clean

The file is clean against the active rulebook and against its
`runtime-invariants.md` brief.
