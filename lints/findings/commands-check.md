# Findings: runtime/src/commands/check.rs

Reviewed against `lints/rules/` as of baf8078.

## Violations of existing rules

None.

- AP-001 (no library-specific in language-core): `commands/*` is
  out of scope per the rule (the CLI surface "may wire any
  layer"). In any case the file mentions no C library, no
  platform path, no SDL/GL/Audio token. Clean.
- AP-002, AP-003, AP-006, AP-007, AP-008: `examples/` scope only;
  not applicable.
- AP-004: `tests/conformance/**.py` only; not applicable.
- AP-005: `runtime/tests/**.rs` only; not applicable. The file
  contains no test code.

## Per-file-invariant check

The shared `cmd_*` brief (runtime-invariants.md, group 6) calls
for: exactly one `pub fn cmd_<name>(args: &[String]) -> ExitCode`;
parse args → load via `common::load_runtime` → call into runtime
API → format → return; no Z3 expression construction; no Solver;
no manual model decoding; no `crate::*` reaches; printing only via
`common.rs` formatters where shared; ~100-line soft cap.

All hold:

- Single public entry `pub fn cmd_check(args: &[String]) ->
  ExitCode` at line 23. One private helper
  (`has_generic_seq_param`).
- 54 lines total — well under the 100-line soft cap.
- Only `evident_runtime::*` and `super::common`/`super::desugar`
  imports. No `use crate::*`, no `z3::*`, no
  `evident_runtime::translate::*` reach-through. The
  `evident_runtime::ast::BodyItem` import is the public AST
  re-export that the runtime facade publishes, used only for a
  pattern match on a returned `SchemaDecl`.
- No `Solver`, no `z3::ast::*`, no `Model::eval`. Solving is
  delegated to `rt.query(name, &empty)` and the resulting
  `QueryResult.satisfied` is read; no manual model decode.
- All output is via direct `println!`/`eprintln!` of fixed lines
  (`SAT    {name}`, `UNSAT  {name}`, `SKIP   {name} ...`,
  `ERROR  {name}: {e}`). This is per-command custom output, which
  the brief explicitly allows ("per-command custom output is fine
  in the per-command file"). The shared `print_query_result` /
  `format_value` formatters wouldn't fit the per-schema tabular
  shape `check` produces.
- Calls `super::desugar::auto_apply_desugar(&mut rt, &files)`
  (line 37) — same self-hosted pre-pass `cmd_query` and
  `cmd_sample` run. This matches the dual-role design of
  `desugar.rs` ("library used by other parts of the CLI") in
  group 6's invariants.

## Candidate new rules

None worth promoting. Two observations that did NOT clear the
proposing bar:

**Observation 1 (review-only).** `cmd_check` does not run the
inference pre-pass (`super::infer_types::auto_apply_inferences`)
that `cmd_query` and `cmd_sample` run, even though it does run
`auto_apply_desugar`. This may be intentional (check should report
on the program AS WRITTEN, without mutating it via inference) or
may be an oversight. Not a pattern — it's a single-site
behavioral question; review-only, no rule.

**Observation 2 (review-only).** The "skip generic-Seq-param
helpers" gate (`has_generic_seq_param`) is `cmd_check`-specific
filtering logic. If a sibling command (a future `cmd_check_all`,
or `cmd_test`'s discovery) needs the same gate, the shape of
"identify claims that can't be standalone-evaluated" should be
factored. Today only one caller needs it; keeping it local is
correct per the brief ("if helpers are only used by a single
`cmd_*` file, they live in that file"). Not a rule, just a note
for future factoring if it recurs.

## Clean

The file is clean against all 8 active rules and against its
runtime-invariants brief. No findings to fix.
