# Findings: runtime/src/commands/lint.rs

Reviewed against `lints/rules/` (AP-001 through AP-008) at HEAD (commit 188c682).

## Violations of existing rules

None. The file is in the `commands/*` role; AP-001's scope explicitly
excludes `runtime/src/commands/*`. AP-002 / AP-003 apply to `examples/*.ev`.
AP-004 applies to `tests/conformance/`. AP-005 applies to
`runtime/tests/**.rs`. AP-006 / AP-007 / AP-008 apply to `examples/`.
No rule in the rulebook targets this file's role + content.

## Candidate new rules

### Suggested AP-009: stdlib-pass-paths-shared-in-common
**Pattern observed at `runtime/src/commands/lint.rs:23-24`,
`runtime/src/commands/desugar.rs:41-42`,
`runtime/src/commands/infer_types.rs:27-31`:**
> ```rust
> // lint.rs:
> const STDLIB_AST: &str = "stdlib/ast.ev";
> const LINT_DUPS:  &str = "stdlib/passes/lint_duplicate_decls.ev";
>
> // desugar.rs:
> const STDLIB_AST:           &str = "stdlib/ast.ev";
> const DESUGAR_PASSTHROUGH:  &str = "stdlib/passes/desugar_passthrough.ev";
>
> // infer_types.rs:
> const STDLIB_AST:    &str = "stdlib/ast.ev";
> ```

**Why it might be bad:** Three sibling `cmd_*` files each redeclare
`STDLIB_AST = "stdlib/ast.ev"` as a private const, and each builds the
same "load STDLIB_AST → load pass → mark_system_loads_complete → load
user file" sequence inline. Per the per-file invariant for
`commands/common.rs`: "Shared helpers used by multiple `cmd_*` files …
runtime construction (`load_runtime` reads the file list and returns a
loaded `EvidentRuntime`)." The AST-pass-runtime construction shape IS
shared by multiple `cmd_*` files but isn't in `common.rs`. If
`stdlib/ast.ev` ever moves or is renamed, three files change
independently with no compiler-enforced link between them. Same for the
"load AST + this pass + user file + mark system" sequence — three
hand-rolled copies that can drift.

**Suggested fix:** Promote the constant `STDLIB_AST` and a
`load_pass_runtime(pass_paths: &[&str], user_paths: &[&str]) ->
Result<EvidentRuntime, String>` helper into `commands/common.rs`. Each
self-hosted-pass `cmd_*` (lint, desugar, infer_types) calls it with
its own pass list. The per-pass constants (`LINT_DUPS`,
`DESUGAR_PASSTHROUGH`, `LITERAL_TYPES`, …) stay in their owning file
because they're per-command, but the AST-load + system-mark + user-load
sequence becomes one call.

**Detection idea:** grep — count distinct files in
`runtime/src/commands/` containing `"stdlib/ast.ev"` as a `const &str`
literal; fail if >1.

### Suggested AP-010 (review-only): cwd-relative-stdlib-paths-in-runtime
**Pattern observed at `runtime/src/commands/lint.rs:23-24` (and
mirrored in `desugar.rs`, `infer_types.rs`):**
> ```rust
> const STDLIB_AST: &str = "stdlib/ast.ev";
> const LINT_DUPS:  &str = "stdlib/passes/lint_duplicate_decls.ev";
> ...
> rt.load_file(Path::new(STDLIB_AST))
> ```

**Why it might be bad:** The path is resolved relative to the process's
current working directory at invocation time. Running `evident lint
foo.ev` from any directory other than the repo root fails with "load
stdlib/ast.ev: …" — not a packaging-grade behavior for a CLI. This is
a runtime concern (UX of the binary), not a per-file rulebook issue,
and likely warrants a single fix (resolve via env var / installed
prefix / locate-from-binary) rather than per-call-site rule
enforcement.

**Suggested fix:** Resolve stdlib via `EVIDENT_STDLIB_DIR` env var or
locate-relative-to-binary, in one helper. Same fix lives anywhere
stdlib paths are loaded.

**Detection idea:** Review-only — picking a "wrong" cwd-relative path
from a "right" one needs context the linter doesn't have.

### Suggested AP-011 (review-only): repeated-value-string-extract-pattern
**Pattern observed at `runtime/src/commands/lint.rs:71-78`:**
> ```rust
> let dup_var = r.bindings.get("dup_var")
>     .and_then(|v| if let Value::Str(s) = v { Some(s.clone()) } else { None })
>     .unwrap_or_default();
> let type_a = r.bindings.get("type_a")
>     .and_then(|v| if let Value::Str(s) = v { Some(s.clone()) } else { None })
>     .unwrap_or_default();
> let type_b = r.bindings.get("type_b")
>     .and_then(|v| if let Value::Str(s) = v { Some(s.clone()) } else { None })
>     .unwrap_or_default();
> ```

**Why it might be bad:** The same `bindings.get(K).and_then(|v| if let
Value::Str(s) = v { … } else { None })` shape is open-coded three
times in a row, and similar patterns appear in `desugar.rs` and
`infer_types.rs` for extracting `Value::Str` / `Value::Int` from a
result. A small helper (`r.get_str("dup_var")` returning `Option<&str>`)
in `common.rs` (or a method on `QueryResult` in the public facade)
would shrink three lines to one and make per-rule extraction obvious.

**Suggested fix:** Add `pub fn get_str(&self, k: &str) -> Option<&str>`
and `get_int` to `QueryResult` (in `evident_runtime`), or as free
helpers in `commands/common.rs`.

**Detection idea:** grep — `if let Value::Str(.*) = v` repeated >2x in
one file. Marginal — might catch legitimate single-use shapes too.
Review-only.

## Clean against existing rules

The file is in role-correct shape: one `pub fn cmd_lint`, ~95 lines
(within the soft 100-line cap), no `crate::*` reach-ins (only
`evident_runtime::{EvidentRuntime, Value}` and `super::desugar`), no
library-specific identifiers, no FFI primitives, no `#[ignore]` or
xfail. The cross-file contract noted in the per-file invariant
(self-hosted pass file existence — `stdlib/ast.ev` and
`stdlib/passes/lint_duplicate_decls.ev`) is satisfied.
