# AP-009: no-solver-assert-in-declare

**Status:** active

**Pattern.** `runtime/src/translate/declare.rs` contains a call to
`solver.assert(...)` or `solver.add(...)`. Declaration's job is to
allocate Z3 constants and *return* any post-declaration constraints
to the caller; the caller (`inline`) is the one that asserts on the
Solver.

**Why.** The translation pipeline has clean stage-of-pipeline
boundaries: `declare` allocates constants, `exprs` translates
expressions, `inline` orchestrates and asserts. When `declare` reaches
through to the Solver itself, those layers blur — the same module
becomes responsible for both binding names and stating their
constraints, and a future change to one half affects the other for
no good reason. Commit `db342c3` removed the last 5 `solver.assert`
sites from `declare.rs` after exactly that knot needed untangling.
This rule codifies the fix so it can't regress.

**Fix.** Return constraints (or arrange for them to be returned) and
let the caller assert. The conventional shape is for `declare` to
produce typed `Var` bindings; if a declaration has a side
constraint (e.g., a Seq's length needs pinning), `declare` records
it on the env or returns it, and `inline` asserts.

**Detection.** grep

**Pattern (grep).** `solver\.(assert|add)\b` in
`runtime/src/translate/declare.rs` (production code only;
`#[cfg(test)]`-gated blocks are exempt via `strip_rs_test_modules`).

**Scope.**
  - Apply to: `runtime/src/translate/declare.rs`.
  - Do NOT apply to other translate files; `inline.rs` and `eval.rs`
    legitimately assert on the Solver as part of their job.

**Exceptions.**
  - `#[cfg(test)]`-gated blocks within `declare.rs` (not currently
    used, but the same exemption used by AP-001).
  - Comment-only lines (a doc-comment showing what callers do is
    fine).

**Examples.**
  - The 5 assertion sites removed in `db342c3` — Seq-length pinning,
    record-equality pinning, and friends. Pre-fix, `declare.rs`
    knew about both binding allocation AND constraint shape; post-fix,
    only allocation.
