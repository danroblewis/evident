# Findings: runtime/src/translate/preprocess.rs

Reviewed against `lints/rules/` as of HEAD (baf8078).

## Violations of existing rules

None. AP-001 through AP-008 don't apply: this file is in the
language-core role but contains no library-specific tokens
(no SDL/GL/Audio identifiers, no dylib paths, no C-symbol
strings); it's neither an example, conformance test, nor a
Rust integration test.

## Violations of per-file invariants (`runtime-invariants.md`)

The invariants doc says preprocess.rs:
  * Pre-translation passes only — input AST + Value map → refined AST + Value map
  * Must NOT build Z3 expressions (that's exprs.rs)
  * Must NOT assert constraints
  * Must NOT use a Solver

### Invariant breach: builds Z3 expressions in `apply_seq_lengths`
At `runtime/src/translate/preprocess.rs:315-340`:
> ```
> pub(super) fn apply_seq_lengths<'ctx>(
>     env: &mut HashMap<String, Var<'ctx>>,
>     seq_lengths: &HashMap<String, i64>,
>     ctx: &'ctx Context,
> ) {
>     for (name, n) in seq_lengths {
>         let Some(var) = env.get(name) else { continue };
>         let new_len = Int::from_i64(ctx, *n);
> ```

`Int::from_i64(ctx, *n)` constructs a Z3 AST node, and the
function rewrites `Var::SeqVar`/`Var::DatatypeSeqVar` entries
in env to substitute that Z3 expression in place of the
symbolic `len`. This is Z3-expression construction inside a
file whose invariant says "Must NOT build Z3 expressions
(that's exprs.rs)." It also takes `ctx: &'ctx Context` as a
parameter, which is the giveaway — preprocess passes are
supposed to be `AST + Value map → refined AST + Value map`,
no Z3 context required.

Note: `apply_pinned_ints` (lines 295-304) is fine — it only
swaps `Var` enum variants, no Z3 construction.

### Invariant breach: `literal_range` builds and simplifies Z3 expressions
At `runtime/src/translate/preprocess.rs:353-366`:
> ```
> pub(super) fn literal_range<'ctx>(
>     e: &Expr,
>     ctx: &'ctx Context,
>     env: &HashMap<String, Var<'ctx>>,
> ) -> Option<(i64, i64)> {
>     if let Expr::Range(lo, hi) = e {
>         let lo_z3 = translate_int(lo, ctx, env)?;
>         let hi_z3 = translate_int(hi, ctx, env)?;
>         let lo_v = lo_z3.simplify().as_i64()?;
>         let hi_v = hi_z3.simplify().as_i64()?;
> ```

This calls `translate_int` (returns a Z3 `Int` AST), then
`.simplify()` (a Z3 operation). Two breaches in one
function: Z3-expression construction and use of Z3 machinery
in a pre-translation pass.

### Cycle (preprocess ↔ exprs) confirmed from this end
At `runtime/src/translate/preprocess.rs:13`:
> ```
> use super::exprs::translate_int;
> ```

And `exprs.rs:13`:
> ```
> use super::preprocess::{env_clone, literal_range};
> ```

The preprocess → exprs edge (using `translate_int` for
literal folding inside `literal_range`) and the exprs →
preprocess edge (importing `env_clone` and `literal_range`
for quantifier unrolling) close a module-level cycle. The
invariants forbid the cycle from the exprs side, recommending
the helpers move to `types.rs`. From preprocess.rs's side,
the dependency on `exprs::translate_int` would be eliminated
in lockstep — `literal_range` (which is the only consumer of
`translate_int` here) would itself migrate to `types.rs`,
along with `env_clone`. After that move, preprocess.rs would
no longer need to import from `exprs`, and the file would be
back to "AST + Value map → refined AST + Value map" with no
Z3-construction surface.

## Candidate new rules

### Suggested AP-009: no-z3-context-parameter-in-preprocess
**Pattern observed at `runtime/src/translate/preprocess.rs:315-318` and `353-357`:**
> ```
> pub(super) fn apply_seq_lengths<'ctx>(
>     env: &mut HashMap<String, Var<'ctx>>,
>     seq_lengths: &HashMap<String, i64>,
>     ctx: &'ctx Context,
> )
> ...
> pub(super) fn literal_range<'ctx>(
>     e: &Expr,
>     ctx: &'ctx Context,
>     env: &HashMap<String, Var<'ctx>>,
> )
> ```

**Why it might be bad:** A function taking `ctx: &'ctx
Context` from the `z3` crate inside a "pre-translation"
module is a structural giveaway that the function is
constructing Z3 ASTs — that's the only purpose of holding the
Z3 context. The invariants doc explicitly forbids Z3
expression construction inside `preprocess.rs`. A grep for
the parameter type catches the breach mechanically without
needing to walk the function body.

**Suggested fix:** Either the function moves out of
`preprocess.rs` (into `exprs.rs` or the recommended
`types.rs` migration target), or it loses its `ctx`
parameter and operates purely on `Var` enum substitution +
`HashMap<String, i64>` data.

**Detection idea:** grep — `runtime/src/translate/preprocess.rs`
for any `fn .*ctx: *&.*Context` or `Int::from_i64\(ctx`,
`Bool::.*\(ctx`, etc. Lints scope is restricted to that one
file to avoid false-positives in legitimate Z3-using files.

### Suggested AP-010: no-cross-module-cycles-within-translate
**Pattern observed at `runtime/src/translate/preprocess.rs:13` ↔ `runtime/src/translate/exprs.rs:13`:**
> ```
> // preprocess.rs:
> use super::exprs::translate_int;
>
> // exprs.rs:
> use super::preprocess::{env_clone, literal_range};
> ```

**Why it might be bad:** The translate pipeline has a
documented layering (`types` → `datatypes`/`declare`/`preprocess`
→ `exprs` → `inline` → `eval`). A back-edge inside this DAG
silently muddles which pass owns which concern. The
invariants doc for `exprs.rs` already calls out this specific
cycle; AP-010 makes the rule mechanical so similar cycles
between, say, `inline.rs` and `declare.rs` get caught
automatically rather than relying on a reviewer remembering
the layering.

**Suggested fix:** Lower-layer helpers used by both sides
move down to `types.rs` (the leaf) so neither side has to
import from the other.

**Detection idea:** ast/grep — for each file in
`runtime/src/translate/*.rs`, build a `use super::X` edge set
and run a cycle check. Could be a small Rust integration test
under `runtime/tests/lints.rs` that walks the directory and
parses the `use` lines. Review-only is acceptable as a
fallback if mechanizing turns out to be brittle.

(Both candidates clear the bar — observable in syntax,
specific fix, likely to recur. Per the agent prompt, I'm
cataloging them here only; rule-file creation and
`checks.sh` wiring is for a follow-up.)
