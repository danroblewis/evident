# Findings: runtime/src/commands/effect_run.rs

Reviewed against `lints/rules/` as of HEAD (main @ 188c682).

## Violations of existing rules

None. All eight active rules (AP-001..AP-008) target language-core,
examples, conformance, or rust-tests scopes; none apply to
`runtime/src/commands/*`.

## Layer-crossing review (the specific question)

The brief asks whether `use evident_runtime::{EvidentRuntime,
effect_loop};` is a valid layer crossing or scope creep.

It is **valid**. The runtime invariants (`runtime-invariants.md`)
explicitly say of `commands/effect_run.rs`: "this is the only command
that reaches the effect_loop." `lib.rs` exposes `pub mod
effect_loop;` precisely to serve this command, and `runtime.rs`'s
invariant forbids the runtime facade from owning the scheduler.
Therefore `commands/effect_run.rs` is the *only* legitimate consumer
of `evident_runtime::effect_loop`. The two-symbol surface used here
(`effect_loop::run`, `effect_loop::LoopOpts`) matches the
"thin glue around `effect_loop::run`" carve-out spelled out in the
group-6 invariant.

No reach into translate, ffi, fti, or event_sources internals — the
command stays within the sanctioned facade plus the one extra module
it's chartered to use. Size 73 lines, well under the ~100-line soft
cap.

## Candidate new rules

### Suggested AP-009: stdlib-bootstrap-paths-belong-in-common (review-only)

**Pattern observed at `commands/effect_run.rs:17`, `commands/lint.rs:23-24`,
`commands/desugar.rs:41-42`, `commands/infer_types.rs:27-31`:**
> ```rust
> const STDLIB_RUNTIME: &str = "stdlib/runtime.ev";
> ```
> (and similar `STDLIB_AST`, pass-file path consts in four files)

**Why it might be bad:** Multiple `cmd_*` files hardcode literal
relative paths to stdlib files. If the stdlib layout moves (e.g.
to `share/evident/stdlib/`, or to an embedded resource), every
command file needs editing. There's already a precedent for
shared command concerns living in `common.rs` (the invariant calls
out `load_runtime`, flag parsing, formatters). Stdlib bootstrap
path resolution arguably belongs there too.

**Suggested fix:** Add a `common::stdlib_path(name: &str) -> PathBuf`
or a small `common::STDLIB` const with a centralised root, and have
each command derive its file path from that.

**Detection idea:** grep for `"stdlib/.*\.ev"` in
`runtime/src/commands/*.rs` excluding `common.rs`; flag if more than
one file declares the path. Review-only — too easy to false-positive
on doc comments.

**Bar check:** Pattern is concrete and observable, fix is specific,
likely to recur as more self-hosted passes appear. Borderline on
overlap with the group-6 invariant ("if helpers are only used by a
single `cmd_*` file, they live in that file"). I'm leaving this as
review-only because today each path *is* used by only one command —
so the invariant arguably tolerates the duplication. Promote to a
real rule once two commands need the same path.

### Suggested AP-010: cmd-must-not-print-internal-mechanism-names (review-only)

**Pattern observed at `commands/effect_run.rs:63`:**
> ```rust
> eprintln!("effect-run: did not halt cleanly after {} steps", r.steps);
> ```

**Why it might be bad:** "did not halt cleanly" leaks scheduler
internals (`halted_clean: bool` is an `effect_loop` private-ish
status field) into user-facing diagnostics. A user who hasn't read
the scheduler design doesn't know what "halt cleanly" means. The
companion message at line 21 (`"effect-run: need a program path"`)
is cleaner — domain language, no internals.

**Why it might NOT be bad:** This is a single line in one command,
and "did not halt cleanly after N steps" is at least more
informative than a bare exit code. Pure UX nit, not structural.

**Suggested fix:** Rephrase in user terms: e.g., `"effect-run:
program ran {steps} steps without finishing (--max-steps {max})"`.

**Detection idea:** None mechanizable. Review-only and weak; not
proposing as a real rule. Listing only because the brief asks for
shortcuts and quick fixes.

## Clean

Aside from the two review-only candidates above (one borderline,
one UX-only), this file is clean. The layer crossing the brief
asked about is sanctioned by the runtime invariants. The file's
shape — argv parse, runtime construction, single
`effect_loop::run` call, exit-code formatting — matches the
group-6 skeleton exactly, and stays well within the 100-line cap.
