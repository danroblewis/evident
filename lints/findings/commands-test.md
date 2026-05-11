# Findings: runtime/src/commands/test.rs

Reviewed against `lints/rules/` as of HEAD (188c682).

## Violations of existing rules

None. AP-001 through AP-008 either do not apply to `runtime/src/commands/*.rs`
(AP-002, AP-003, AP-006, AP-007, AP-008 are example-file rules; AP-004 is
conformance-only; AP-005 is `#[ignore]` in `runtime/tests/`) or do not match
this file's contents (AP-001 forbids library-specific identifiers in the
language-core role; `commands/` is explicitly excluded from AP-001's scope and
the file mentions no SDL/GL/library-specific symbols anyway).

## Notes against the per-file invariants

Re: the brief's flag that `commands/test.rs` reaches for
`evident_runtime::pretty` and `evident_runtime::translate::preprocess_api::collect_referenced_names`,
those are **not** layering violations:

- `pretty` is declared `pub mod pretty;` in `runtime/src/lib.rs:13` — first-class
  public surface, not an internal reach-in.
- `preprocess_api` is a deliberately narrow public namespace at
  `runtime/src/translate.rs:45`:
  `pub mod preprocess_api { pub use super::preprocess::collect_referenced_names; }`.
  The `_api` suffix and the single re-exported symbol make clear the boundary is
  intentional — the translator authors chose to expose this one helper to
  commands without widening the rest of `preprocess`.

So both imports are sanctioned by the public facade. They are deeper than the
top-level `EvidentRuntime` / `Value` / `QueryResult` re-exports in `lib.rs:20`,
but they are not internals being reached around. No violation.

The invariants doc's "Notably uses…" sentence reads as documentation of an
acceptable shape, not an alarm.

Other invariants — checked, all clean:

- **Must NOT build constraints itself.** No Z3 use, no `Solver`, no expression
  construction. Constraint work goes via `rt.query`, `rt.query_with_core`,
  `rt.load_file`, `rt.get_schema`, `rt.schema_names`.
- **Must NOT decode models manually.** The per-test results come back as
  `QueryResult` (`r.satisfied`, `r.bindings`); the file inspects
  `evident_runtime::Value` variants for display only (`display_value_compact`,
  `flatten_value`), not to extract semantics.
- **Must NOT skip / xfail tests silently.** Every discovered claim either
  produces a `Pass`, `Fail`, or `Error` `Outcome`; the only "skip" path in the
  file is the comment at line 208 noting trace tests were removed entirely (not
  hidden behind a skip). `referenced_names_in` falls through `_ => {}` for
  scaffolding body items, but that's per-item filtering, not test-level skipping
  — every test gets reported.
- **Must NOT carry state across runs.** `Opts` is parsed per call;
  `EvidentRuntime` is constructed fresh per file (line 152) and again fresh per
  failure rendering (lines 337, 368). No module-level mutable state.

## Candidate new rules

### Suggested AP-009: extern-c-isatty-or-libc-fns-belong-in-one-file (review-only)

**Pattern observed at `runtime/src/commands/test.rs:65-68`:**
> ```rust
> extern "C" {
>     #[link_name = "isatty"]
>     fn libc_isatty(fd: i32) -> i32;
> }
> ```

**Why it might be bad:** The file declares its own private `extern "C"`
binding to libc's `isatty` to decide whether to colorize output. Two issues:

1. There's already a real FFI machinery in `runtime/src/ffi.rs`. Bypassing it
   for one libc symbol means we now have two ways the runtime calls into libc:
   the libffi-mediated path that the `Effect::FFICall` family uses, and a
   per-need ad-hoc `extern "C"`. If a second `cmd_*` file ever needs to detect
   TTY, color depth, terminal width, or anything similar, the most likely
   outcome is another copy of this `extern` block.

2. `commands/` is the CLI surface; libc bindings are bridge-layer concerns. The
   per-file invariants for `commands/*` say each file uses
   `evident_runtime::*` + `super::common::*` + `std`. A naked `extern "C"`
   block sneaks past that boundary without going through the runtime facade.

**Suggested fix:** A small `tty.rs` (or method on `commands::common`) that
encapsulates `is_stdout_tty()` and exposes it to any color-using command.
Probably cleanest as `commands::common::stdout_supports_color()` that callers
import — the dependency on libc stays in one file.

**Detection idea:** grep for `extern "C"` blocks in
`runtime/src/commands/*.rs`. Expected count: 0. If non-zero, flag.

**Why review-only:** Single offender today, and the cleanest fix is small
enough that a one-line lint may overshoot. But if a second `cmd_*` adds its
own `extern`, this should escalate to a real rule (next available number is
AP-009).

### Suggested AP-010: redundant-load-on-failure-render (review-only)

**Pattern observed at `runtime/src/commands/test.rs:337-339, 368-376`:**
> ```rust
> let mut rt = EvidentRuntime::new();
> if rt.load_file(&run.file).is_err() { return; }
> let Some(schema) = rt.get_schema(&run.name) else { return };
> ```

**Why it might be bad:** Each failure rendering re-loads + re-parses the
source file (twice in the SAT-counterexample path: once at line 368, and
the unsat-core path also re-loads at line 337). The driver loop already loaded
the file successfully at line 152 to enumerate `sat_*`/`unsat_*` claims; that
runtime is dropped before failure rendering happens.

This isn't a correctness problem (the comment at line 366-367 explicitly notes
"Re-loading is cheap for a single file"), but it is structural state-loss: the
file knows it had a valid `EvidentRuntime` and a valid `SchemaDecl` at run
time, then deliberately throws them away and re-computes them for printing.
If load fails between query and render (e.g., user edits mid-run, line
371-372), the comment acknowledges the failure case and falls back to a raw
dump.

**Suggested fix:** Hold a `(SchemaDecl, …)` snapshot per `TestRun` so the
report function doesn't need a runtime at all. Failure rendering then becomes
purely a function over `TestRun + bindings + opts`.

**Why review-only:** Performance is acceptable today and the comments note
the choice was intentional. Promoting it would require evidence the redundant
load is causing real pain.

### Suggested AP-011: hand-rolled-ansi-helpers-instead-of-a-color-module (review-only)

**Pattern observed at `runtime/src/commands/test.rs:70-90, 514-569`:**
> ```rust
> const RESET: &str = "\x1b[0m";
> // …
> fn red(on: bool, t: &str) -> String { paint(on, RED, t) }
> // …
> fn highlight_constraint(text: &str, on: bool) -> String { … }
> ```

**Why it might be bad:** The file contains its own ANSI palette, its own
`paint()`/`red()`/`green()`/etc. wrappers, and a mini-tokenizer
(`highlight_constraint`) that walks a pretty-printed string to recolor
operators / strings / identifiers. None of this is test-runner-specific.
If a second `cmd_*` ever wants colored output (e.g., `cmd_query` highlighting
a counterexample, or `cmd_lint` highlighting code), the natural shortcut is
to copy this block.

`commands/common.rs` is the documented home for "shared helpers used by
multiple `cmd_*` files." Color helpers + a constraint highlighter belong
there if more than one command would use them; today they're stuck in
`test.rs`.

**Suggested fix:** Promote the ANSI constants, `paint()`, color wrappers,
and `highlight_constraint()` to `commands/common.rs` (or a new
`commands/color.rs` if `common.rs` is filling up). Update `test.rs` to
import them. Until a second consumer materializes this is YAGNI, hence
review-only.

**Detection idea:** grep for `\x1b\[` literals in `runtime/src/commands/*.rs`.
If they appear in more than one file with no shared `paint()` helper, flag.

**Why review-only:** Single user today; the pattern is "gets bad on the
second copy," not "is bad now."

## Clean

The file is functionally clean against the rulebook — no AP-001..008
violation. The candidates above are review-only observations about
structural shapes that haven't yet recurred but would be worth promoting
if a second `cmd_*` reproduces them.
