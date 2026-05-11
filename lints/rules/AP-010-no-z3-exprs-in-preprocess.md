# AP-010: no-z3-exprs-in-preprocess

**Status:** active

**Pattern.** `runtime/src/translate/preprocess.rs` constructs a Z3
AST expression — `z3::ast::Int::new_const(...)`, `Bool::new(...)`,
`Real::from_real(...)`, etc. Preprocess is an AST→AST rewrite stage;
Z3 expression construction is `exprs.rs`'s job.

**Why.** Preprocess exists to surface concrete integers (literal
folding, Seq-length pinning, quantifier-bound folding) so the
downstream translator produces smaller Z3 formulas. Its inputs and
outputs are both pure AST + a small `Value` map. The moment it
starts building Z3 expressions of its own, it's competing with
`exprs.rs` for the same job and the layering blurs — and historically
that's exactly what created the preprocess ↔ exprs cycle (broken in
this session). Codify so a future helper can't grow back into a
Z3-expression builder.

**Fix.** Keep the rewrite pure-AST. If a value needs evaluation, do
it on the Rust side (Rust ints / strings) and store the result in
the `Value` map. If a Z3 expression needs to be built downstream,
let `exprs.rs` build it during translation.

**Detection.** grep

**Pattern (grep).** `z3::ast::(Int|Bool|Real|String|Datatype)::(new_const|new|from_)`
in `runtime/src/translate/preprocess.rs` (production code only;
`#[cfg(test)]`-gated blocks are exempt).

**Scope.**
  - Apply to: `runtime/src/translate/preprocess.rs`.
  - Do NOT apply to other translate files; `exprs.rs`, `declare.rs`,
    `inline.rs`, `extract.rs`, `eval.rs` legitimately build Z3
    expressions as part of their job.

**Exceptions.**
  - `#[cfg(test)]`-gated blocks (test code may build a Z3 expression
    to exercise a helper, even if the helper itself shouldn't).
  - Comment-only lines (a doc-comment showing what `exprs.rs` does
    is fine).
  - Plain `use z3::ast::*` import lines that don't construct
    anything are NOT matched by the regex.

**Examples.**
  - The cycle break in `092b62c` and surrounding commits — shared
    helpers between preprocess and exprs were moved to `types.rs`
    so neither file needs to reach across. This rule prevents
    a regression where a shared helper grows back into preprocess
    and starts touching Z3 directly.
