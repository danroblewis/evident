# Findings: runtime/src/translate/extract.rs

Reviewed against `lints/rules/` as of baf8078.

## Violations of existing rules

None.

- AP-001 (no library-specific in language-core): translate/extract.rs is in
  scope. Scan for `Sdl[A-Z]`, `SDL_`, `\bGl[A-Z]`, `Glsl`, `Audio[A-Z]`,
  `\.dylib`, `\.framework/`, `/opt/homebrew/lib/`, `/usr/lib/lib` — zero
  hits. The file mentions no C library by name and contains no platform
  paths. Clean.
- AP-002, AP-003, AP-006, AP-007, AP-008: examples-only scope; not
  applicable.
- AP-004: conformance-only scope; not applicable.
- AP-005: applies to `runtime/tests/**.rs`. The in-file
  `#[cfg(test)] mod unescape_tests` carries no `#[ignore]` annotations.
  Clean.

## Per-file-invariant check

The file's invariants from `runtime-invariants.md` are:
  1. Reads model values back out of a satisfied Solver — leaf-level.
  2. Must NOT build new constraints.
  3. Must NOT declare new vars.
  4. Must NOT recurse into claim bodies.
  5. No `ast` import — operates on `Var`/`Value`, not raw AST.

Status against each:

  1. **Holds** for `extract_seq`, `extract_composite_value`,
     `extract_seq_composite`. All read `model.eval(...)` against an
     already-satisfied Solver and return `Value` shapes. `unescape_z3_string`
     is a pure string utility supporting that read path.

  2. **Documented exception, but worth flagging.** `assert_seq_given`
     (lines 212–266) and its private helper `composite_value_to_dyn`
     (lines 284–318) build Z3 `Bool` formulas (`Bool::and(ctx, &refs)`)
     and Datatype `Dynamic` constructor applications. They do NOT assert
     onto a Solver — they return the formula and let the caller assert.
     The brief in `runtime-invariants.md` explicitly lists `assert_seq_given`
     as part of this file's purpose ("the inverse direction: pinning a Seq
     variable to a `Value::Seq*` shape from a caller-supplied `given` map"),
     so this is recognized as an exception. Returning constraint values
     for the caller to assert is materially different from owning the
     Solver, and respects the boundary.

  3. **Holds.** No `Const::new_const`, `FreshConst`, or `*::new_const`
     calls anywhere. Builds Z3 *literals* via `Int::from_i64`,
     `Bool::from_bool`, `Z3Str::from_str` (these are constants, not
     declared variables in the SMT sense). Clean.

  4. **Holds.** No reference to `BodyItem`, `SchemaDecl`, `Passthrough`,
     or `ClaimCall` anywhere; no recursion structure beyond
     `extract_composite_value` walking nested record fields and
     `composite_value_to_dyn` walking the same nested record fields in
     reverse.

  5. **Holds at the source level**, with one transitive caveat. The
     file has no `crate::ast::*` import (only `std::collections::HashMap`,
     `z3::*`, and `super::types::{FieldKind, SeqElem, Value, Var}`).
     The `z3::ast::*` imports are Z3's `ast` module, unrelated to Evident's
     AST. The transitive caveat: `super::types::Var` (in
     `translate/types.rs`) imports `crate::ast::EnumVariant`, so the
     `Var::EnumVar { ast: &Datatype, .. }` variant carries Z3 AST refs
     and the `EnumRegistry` imported elsewhere references AST enum
     variant data. extract.rs itself touches none of that AST shape —
     it pattern-matches `FieldKind::Primitive` / `FieldKind::Nested` and
     primitive type-name strings ("Int", "Bool", "String", "Nat", "Pos").
     The invariant holds at the file boundary.

## Candidate new rules

### Suggested AP-NNN: file-purpose-vs-implementation drift

**Pattern observed at runtime/src/translate/extract.rs (whole file) +
runtime/src/translate/eval.rs:235–309, 367–410.**

The file's purpose statement in `runtime-invariants.md` says:

> One function per `Var` kind (Int, Bool, Real, Str, Handle, Enum,
> record, Seq…) mapping the Z3 binding to a `Value`.

In reality, extract.rs only has **composite/seq** extractors
(`extract_seq`, `extract_composite_value`, `extract_seq_composite`).
The per-`Var`-kind extraction for `Var::IntVar`, `BoolVar`, `RealVar`,
`StrVar`, `EnumVar` is inlined directly into eval.rs's
`sample_cached_inner` (lines 235–309) and `run_cached`'s extract phase
(lines 367–410), as `match var { Var::IntVar(i) => model.eval(i, ...) ... }`
arms. The same eight-arm match appears twice in eval.rs, neither call
delegates to extract.rs.

**Why it might be bad:** Two costs. First, the brief and the code
disagree, so a future reader looking for "where is the Int extractor"
won't find it where the brief says it lives. Second, the per-kind
match logic is duplicated between two call sites in eval.rs —
inviting drift if one call site adds (e.g.) a new Var variant and
the other doesn't. extract.rs already has the right home for these
helpers; it just hasn't been used.

**Suggested fix:** Either (a) add `extract_int`, `extract_bool`,
`extract_real`, `extract_str`, `extract_enum_value` (the latter
already lives in eval.rs and is called via `extract_enum_value(ast,
enum_name, dt, &model, enums)` — moving it would line up with the
brief), and have eval.rs's two extraction sites call them; or (b)
update `runtime-invariants.md` to describe the actual division
("scalar extraction is inlined in eval.rs; extract.rs owns composite
+ seq extraction + the seq-given inverse"). Pick one.

**Detection idea:** review-only. Mechanizing "the file's brief
matches its actual contents" is hard. The brief is prose and the
code is structure; equating the two requires human judgment.

### Suggested AP-NNN: returns-Bool-formula-but-named-assert

**Pattern observed at runtime/src/translate/extract.rs:212.**

> `pub(super) fn assert_seq_given<'ctx>(...) -> Option<Bool<'ctx>>`

The function name uses the verb `assert` — which in SMT context means
"add this formula to the Solver." But this function never touches a
Solver; it returns a `Bool` formula for the caller to assert (or do
anything else with). Callers that read the name might assume it has
performed the assertion and skip the assert step.

**Why it might be bad:** Functions named `assert_*` that return a
formula instead of asserting it onto a Solver invert the convention
the rest of `translate/eval.rs` follows (where `solver.assert(...)`
is the actual side-effect). Concretely, two `Solver::assert` sites in
eval.rs DO call this function (search "assert_seq_given" in the
codebase) and pair it with their own `solver.assert(...)`, which is
correct but only because callers already know the function returns a
formula. A new caller could plausibly write
`assert_seq_given(var, value, ctx);` (discarding the result) and
silently get no constraint.

**Suggested fix:** Rename to `seq_given_eq` or `build_seq_given_eq`
to signal "builds a Bool formula, does not assert." Or have the
function take a `&Solver` and assert internally — but that would
force the file to depend on the Solver, weakening the invariant
that extract.rs is a pure read/build module.

**Detection idea:** review-only or a custom AST lint —
`fn assert_*` whose return type is `Option<Bool<...>>` /
`Bool<...>` / `Option<bool>` and whose body contains no
`solver.assert(`. Probably review-only is good enough; this is
narrow.

Neither candidate clears the bar for promotion to a written rule
(the first is one-off prose-vs-code drift; the second is one
function in one file with a well-understood call site). Recording
both as review-only.

## Clean

The file is clean against all 8 active rules and against the
hard invariants in its runtime-invariants brief (no AST import, no
Solver, no var declarations, no claim-body recursion, no library-
specific tokens). Two review-only observations recorded above for
human consideration; no rules added.
