# Findings: runtime/src/commands/common.rs

Reviewed against `lints/rules/` as of HEAD (commit 188c682).

The file is 198 lines. Imports verified clean: only `std::*` and
`evident_runtime::{EvidentRuntime, QueryResult, Value}` (the public
facade). No `crate::*` or runtime-internal reaches. No `static mut` /
`OnceCell` / module-level mutable state — `Flags::default()`
constructs fresh state on every `parse_flags` call.

Cross-file usage map (helpers used by which `cmd_*`):

  | Helper                  | Used by                              |
  |-------------------------|--------------------------------------|
  | `usage`                 | `main.rs` (binary entry)             |
  | `split_files_and_flags` | `check.rs`, `query.rs`, `sample.rs`  |
  | `Flags` / `parse_flags` | `query.rs`, `sample.rs`              |
  | `infer_value`           | (only `common::parse_flags` itself)  |
  | `load_runtime`          | `check.rs`, `query.rs`, `sample.rs`  |
  | `print_query_result`    | `query.rs` ONLY                      |
  | `format_value`          | `query.rs`, `sample.rs`              |
  | `value_as_json`         | `sample.rs` ONLY                     |
  | `json_str`              | (only `common::value_as_json` itself)|

## Violations of existing rules

None. AP-001 through AP-008 don't apply to `commands/*` (per AP-001's
explicit scope: "Do NOT apply to: `runtime/src/commands/*`"). The
remaining active rules concern examples / tests / conformance, not
the CLI surface.

## Candidate new rules

### Suggested AP-009: single-use-helper-in-common

**Pattern observed at runtime/src/commands/common.rs:99 and :152:**
> ```rust
> pub fn print_query_result(r: &QueryResult, json: bool) -> ExitCode { … }   // used only by query.rs
> pub fn value_as_json(v: &Value) -> String { … }                            // used only by sample.rs
> ```

`print_query_result` (lines 99–125) is imported only by
`query.rs:10`. `value_as_json` (lines 152–182) is imported only by
`sample.rs:10`. The per-file invariant for `commands/common.rs`
explicitly forbids this: *"if helpers are only used by a single
`cmd_*` file, they live in that file, not here"*
(`lints/runtime-invariants.md` line ~789).

The drift mechanism is generic to any "common helpers" file in any
multi-file CLI: a helper is added in `common.rs` because it's
*expected* to be shared, then only one caller actually adopts it,
and `common.rs` slowly accumulates command-specific code masquerading
as shared. Once that happens, refactoring the lone caller (e.g.
changing `query`'s output format) requires editing `common.rs` and
risks affecting other commands that don't actually share the code.

**Why it might be bad:** The invariant document already names this
pattern as a violation; it has just not been mechanized as a rule.
Same family as the layering rules — drift erodes the file-purpose
contract.

**Suggested fix:** Move `print_query_result` into `query.rs`. Move
`value_as_json` into `sample.rs`. (Once `value_as_json` moves,
`json_str` — currently only an internal helper of `value_as_json` —
should move with it; `infer_value` similarly stays where its sole
caller `parse_flags` is.) After the move, the helpers in
`common.rs` are exactly the multi-caller set.

**Detection idea:** AST-based, not grep. A `runtime/tests/lints.rs`
test that:
  1. Reads `runtime/src/commands/common.rs`, collects every `pub fn`
     / `pub struct` name.
  2. For each name, greps `runtime/src/commands/cmd_*.rs` (and
     `main.rs`) for `common::<name>` or `<name>` mentioned in a
     `use super::common::{…}` list.
  3. Fails for any pub item appearing in zero or one caller files
     (zero = unused dead code; one = single-use-helper-in-common).

Alternatively, a shell check: for each `pub fn`/`pub struct` in
`common.rs`, count how many sibling `cmd_*.rs` files import it; flag
if count == 1.

### Suggested AP-010: review-only — over-broad pub on file-private helper

**Pattern observed at runtime/src/commands/common.rs:82 and :184:**
> ```rust
> pub fn infer_value(v: &str) -> Value { … }
> pub fn json_str(s: &str) -> String { … }
> ```

Both are declared `pub` but used only by other functions in the
same `common.rs` file (`infer_value` only by `parse_flags`,
`json_str` only by `value_as_json`). Could be private (`fn`) without
losing any caller.

**Why it might be bad:** Misleading visibility — readers parsing the
public surface of `common.rs` see four formatters/parsers when only
two are actually exported. Encourages future drift (a sibling file
might start calling `infer_value` directly because the visibility
suggests it's intended).

**Suggested fix:** Drop `pub` from both. The change is local and
mechanical.

**Detection idea:** Review-only. A general "find unused-pub" check
across the crate would fire on too many legitimate cases (test
fixtures, library exports used by external embedders, etc.) to be
mechanical without a per-file allowlist. Listed here as a candidate
the invariants doc could call out for `commands/common.rs`
specifically — alongside the single-use-helper rule above — but not
worth a standalone AP entry.

## Clean

The file is otherwise clean against the invariants:
  - imports are tight (`std` + `evident_runtime` facade only)
  - no `crate::*` internal reach
  - no module-level mutable state
  - all helpers operate on owned/borrowed data passed in by callers
  - the "must NEVER do" list for this file is satisfied except for
    the single-use-helper drift noted above
