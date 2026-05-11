# Findings: runtime/src/commands/query.rs

Reviewed against `lints/rules/` as of HEAD (188c682).

## Violations of existing rules

None. Active rules AP-001 through AP-008 don't apply to a CLI
subcommand file:

  - AP-001 (language-core leakage) — `commands/*` is explicitly
    out of scope.
  - AP-002 / AP-003 / AP-006 / AP-007 / AP-008 — `examples/*.ev`
    only.
  - AP-004 — `tests/conformance/**/*.py` only.
  - AP-005 — `runtime/tests/**/*.rs` only.

## Per-file invariant check

`runtime-invariants.md` lists this file under "simple `cmd_*`
files" — same shape as `check.rs`, one `pub fn cmd_query`, uses
`evident_runtime` + `super::common`, ~100 lines, no internal
reach.

  - One `pub fn cmd_query(args: &[String]) -> ExitCode`. ✓
  - One private helper `explain_unsat`. ✓
  - 86 lines including doc + blank lines, under the ~100-line
    soft cap. ✓
  - Imports: `std::collections::HashMap`,
    `std::process::ExitCode`, `evident_runtime::{EvidentRuntime,
    Value}`, `super::common::{...}`, plus
    `super::desugar::auto_apply_desugar` and
    `super::infer_types::auto_apply_inferences`. The two
    `super::desugar` / `super::infer_types` imports are sibling
    `cmd_*` files used as libraries — explicitly sanctioned by
    the per-file invariants for `infer_types.rs` /
    `desugar.rs` ("dual-role: a `pub fn cmd_<name>` AND a `pub
    fn auto_apply_*` for use as a library from sibling `cmd_*`
    files"). ✓
  - `evident_runtime::pretty::body_item` (line 80) is reached
    through the public facade re-export, not an internal path.
    ✓
  - Per-command custom output via direct `eprintln!` (the
    `explain_unsat` UNSAT dump) is allowed by the invariant
    ("per-command custom output is fine in the per-command
    file"). ✓
  - No state held across calls. ✓
  - Doesn't build Z3 expressions, run the Solver, or decode
    models manually — all routed through `rt.query(...)`. ✓

## Candidate new rules

### Suggested AP-009: shared-cmd-prologue-belongs-in-common

**Pattern observed at `runtime/src/commands/query.rs:13-47` and
mirrored at `runtime/src/commands/sample.rs:13-36`:**

> ```rust
> let strict = args.iter().any(|a| a == "--strict");
> let stripped: Vec<String> = args.iter()
>     .filter(|a| a.as_str() != "--strict")
>     .cloned().collect();
> let (files_and_schema, flag_args) = split_files_and_flags(&stripped);
> if files_and_schema.len() < 2 {
>     eprintln!("query: need <files…> <schema>");
>     return ExitCode::from(2);
> }
> let schema = files_and_schema.last().unwrap().clone();
> let files: Vec<String> = files_and_schema[..files_and_schema.len() - 1].to_vec();
> let flags = match parse_flags(&flag_args) {
>     Ok(f) => f,
>     Err(e) => { eprintln!("{e}"); return ExitCode::from(2); }
> };
> let mut rt = match load_runtime(&files) {
>     Ok(r) => r,
>     Err(e) => { eprintln!("{e}"); return ExitCode::from(1); }
> };
> if !strict {
>     super::desugar::auto_apply_desugar(&mut rt, &files);
>     super::infer_types::auto_apply_inferences(&mut rt, &files);
> }
> ```

The block (~25 lines) appears nearly verbatim in both `query.rs`
and `sample.rs`. It performs five concerns: detect-and-strip
`--strict`, split files-and-schema-from-flags, validate "at
least one file + schema", parse `Flags`, load runtime, and run
the desugar+infer pipeline conditionally. The `eprintln!`'s
sub-name ("query" vs "sample") is the only material difference.

**Why it might be bad:** Per the per-file invariants for
`commands/common.rs`: "Belong to one specific command — if
helpers are only used by a single `cmd_*` file, they live in
that file, not here." The converse: when a non-trivial helper
is duplicated across `cmd_*` files, it belongs in `common.rs`.
Right now both files independently encode the `--strict` ad-hoc
parse, the "last positional is schema" convention, and the
"desugar then infer" ordering. A future `--strict` semantic
change (e.g., adding `--no-desugar`) requires editing both
places in lockstep. Conversely, if a third subcommand grows the
same prologue (likely candidates: a future `evident solve`,
`explain`, `prove`), it'll start as a copy-paste of the third
copy.

**Suggested fix:** Promote a `common::load_with_pipeline(args:
&[String], cmd_name: &str) -> Result<(EvidentRuntime, String,
Flags), ExitCode>` helper to `common.rs`. The helper handles
`--strict` stripping, the `<files…> <schema>` split, the
length-check error message (parameterized by `cmd_name`), the
Flags parse, the runtime load, and the conditional
desugar+infer pipeline. Each `cmd_*` reduces to roughly:

```rust
let (mut rt, schema, flags) = match common::load_with_pipeline(args, "query") {
    Ok(t) => t,
    Err(code) => return code,
};
let r = match rt.query(&schema, &flags.given) { ... };
```

Alternative: put `--strict` into `Flags` (it IS a flag) so the
ad-hoc strip-before-parse goes away and `parse_flags` learns
one more arm. Then the prologue collapses naturally with the
existing `load_runtime` + `parse_flags` calls plus a single
`if !flags.strict { desugar + infer }` line.

**Detection idea:** Review-only, OR a structural lint that
walks `runtime/src/commands/cmd_*.rs` and flags any pair of
files whose first ~25 lines have a textual diff smaller than
some threshold (after replacing the cmd_name string). Easier
to do as a code-review concern than to grep for, since the
duplication isn't a single regex — it's a structural copy.

**Bar check:** Pattern is observable (two files share ~25 lines
of near-identical code). Fix is specific (one new helper in
`common.rs` OR one new `Flags` field). Pattern is likely to
recur (any new query-shaped subcommand will copy it). Doesn't
overlap with an existing rule. Clears the bar — but listing as
**review-only** because mechanizing the detection is awkward
and the fix is straightforward enough that a one-time refactor
plus reviewer attention is more valuable than a checked-in
script.

## Clean

The file does not violate any active AP rule, and it conforms
to the per-file invariants. The single candidate (AP-009) is a
DRY observation about duplication with `sample.rs`, not a
defect in `query.rs` per se — `query.rs` was the first of the
two and `sample.rs` evidently copied from it.
