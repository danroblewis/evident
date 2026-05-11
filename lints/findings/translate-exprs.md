# Findings: runtime/src/translate/exprs.rs

Reviewed against `lints/rules/` as of baf8078.

## Violations of existing rules

### AP-001 at runtime/src/translate/exprs.rs (file scan)

> (no hits)

`translate/*.rs` is in AP-001 scope. Scanned for `Sdl[A-Z]`, `SDL_`,
`\bGl[A-Z]`, `Glsl`, `Audio[A-Z]`, `\.dylib`, `\.framework/`,
`/opt/homebrew/lib/`, `/usr/lib/lib` — zero hits. Clean against
AP-001.

### AP-002 / AP-003 / AP-004 / AP-005 / AP-006 / AP-007 / AP-008
Out of scope (examples / conformance / Rust test files). Not applicable.

## Per-file-invariant violations

### Cycle: exprs ↔ preprocess (forbidden by the per-file invariant)

> exprs.rs:13 — `use super::preprocess::{env_clone, literal_range};`
> preprocess.rs:13 — `use super::exprs::translate_int;`

The runtime-invariants brief for `translate/exprs.rs` is explicit:
"Must NOT depend on `preprocess` — `preprocess` already depends on
`exprs` (via `translate_int` for literal folding), and a back-edge
would create a cycle. Helpers shared between the two (env utilities,
literal-range queries) belong in `types` so both can borrow without
forming a loop."

The current state has the back-edge wired in. `exprs.rs` reaches
into `preprocess::env_clone` (called at lines 1383, 1455, 1482,
1496, 1510, 1784) and `preprocess::literal_range` (called at line
1480). Both helpers are defined `pub(super)` in
`preprocess.rs:353` (`literal_range`) and `preprocess.rs:370`
(`env_clone`). Neither helper has any preprocess-specific
dependency — `env_clone` is a HashMap clone over `Var`, and
`literal_range` is a small `Expr` matcher returning a `(i64, i64)`
range. They're pure utilities living in the wrong module.

Fix per the invariant: move both helpers into
`runtime/src/translate/types.rs`. Update imports in both `exprs.rs`
and `preprocess.rs` to read them from `super::types`. The cycle
disappears; `preprocess → exprs → types` becomes a clean DAG.

## Candidate new rules

### Suggested AP-009: no-translate-pipeline-back-edges

**Pattern observed at exprs.rs:13 and preprocess.rs:13:**
> `use super::preprocess::{env_clone, literal_range};`
> `use super::exprs::translate_int;`

**Why it might be bad:** The `translate/` sub-pipeline has a
documented layering: `types` → {`datatypes`, `declare`,
`preprocess`} → `exprs` → `inline` → `eval`. A back-edge between
two of these (e.g. `exprs → preprocess`) makes both files
mutually reachable, so adding a helper to either side risks
pulling in the other's transitive cone. Today's specific
violation is `exprs ↔ preprocess`; tomorrow could be
`inline → exprs → preprocess → inline`. The invariant doc
already names the right discipline ("helpers belong in `types`")
but nothing mechanical enforces it.

**Suggested fix:** Any helper that needs to be visible to two
sibling translate modules belongs in the lowest one they share.
Today that means `types.rs` (the leaf). If a helper truly needs
to live higher (because it imports things `types` shouldn't see)
the right answer is to refactor the helper, not create a back-edge.

**Detection idea:** grep — for each ordered pair (A, B) in the
documented layering, fail if `runtime/src/translate/A.rs` contains
`use super::B::`. The forward direction (`preprocess → exprs`,
`inline → exprs`, `eval → *`) is allowed; the backward direction
isn't. A small static table in `lints/checks.sh` listing
permitted edges captures it.

**Bar check:** Observable in concrete syntax (a `use super::X` in
file Y when X is downstream of Y); fix is specific (move helper
to `types`); pattern is likely to recur (any time a translate
file grows a helper that also looks useful upstream). Doesn't
overlap with AP-001 (which is about library-specific code).
Worth promoting if the human reviewer agrees the layering
contract is mature enough to mechanize.

### Suggested rule (review-only): exprs.rs has visible substructure not yet sectioned

**Pattern observed across exprs.rs (1863 lines, ~30 free functions):**

The file groups along clear concern lines but only one `// ──`
section header exists (line 1723, before `translate_match_arms`).
The other groups blend into one another:

  1. Thread-local guard machinery (lines 15-91): `EnumRegistryGuard`,
     `with_active_enums`, target-enum hint helpers.
  2. Mapping / var resolution (93-210): `resolve_mapping`,
     `expr_as_var`.
  3. Enum AST resolution + Cons chains (212-345):
     `resolve_enum_ast`, `build_cons_chain`.
  4. Seq-field path resolution (347-434): `resolve_seq_field`.
  5. Per-sort scalar translators (436-630): `translate_str`,
     `translate_int`, `translate_real`, `real_from_f64`.
  6. Record-op broadcast (632-988, ~360 lines): `lift_record_op`,
     `lhs_record_leaves`, `schema_leaf_paths`,
     `enumerate_nested_leaves`, `substitute_record_refs`,
     `is_field_of_index_record`, `is_seq_element_record`,
     `collect_record_refs`. This is the largest single concern
     in the file and is itself a candidate for promotion to its
     own module (`translate/record_lift.rs` or similar) if it
     grows further.
  7. Seq-literal / seq-index assignment / composite dynamics
     (990-1232): `translate_cons_chain_eq`, `translate_seq_lit_eq`,
     `build_composite_dynamic`, `translate_seq_index_assign`,
     `bind_composite_fields`.
  8. `translate_bool` itself (1235-1721, ~485 lines), which
     contains the InExpr handling, the entire ∀/∃ unrolling
     (coindexed/edges/integer-range/seq-element forms), and the
     equality / comparison dispatch over scalar / enum / record
     paths.
  9. `match` expression translator (1723-1863) — the one section
     that's already marked.

**Why it might be bad:** The `translate/eval.rs` invariant
explicitly mandates `// ──` section headers and ordered
sub-concerns ("If a single section grows large enough that it
needs its own internal helpers + multiple public entries,
that's the signal to split it into its own file under
`translate/eval/`"). The same discipline naturally applies to
`exprs.rs`, which is now the largest file in `runtime/src/`
(1863 lines vs. eval's smaller body) and has at least three
groups (record-op broadcast at #6, seq-literal handling at #7,
the `translate_bool` body at #8) that each look like a
standalone module: a single concern, a small public surface
(one or two `pub(super)` entries), and a private helper cluster.

**Suggested fix:** Either add `// ──` headers to mark the nine
sections above (matching eval.rs's discipline) OR — more
durably — split sections #6 and #7 into their own files
(`translate/record_lift.rs`, `translate/seq_lit.rs`). The
`translate_bool` ∀/∃ unrolling inside section #8 is also a
plausible split (`translate/quantifiers.rs`). After splitting,
exprs.rs reduces to the per-sort scalar translators + the
match-arm + thread-local plumbing they share, which IS the
file's stated purpose.

**Detection idea:** Review-only — can't be mechanized without an
arbitrary line-count threshold. Promote if the file grows by
another 500 lines without sectioning, or if a third translate
file appears (record_lift, seq_lit) that takes a substantial
chunk out of this one.

## Clean

The file is clean against all 8 active rules. The single per-file-
invariant violation (the `exprs ↔ preprocess` cycle) is documented
and is exactly the case the runtime-invariants doc warned about.
The substructure observation is review-only — a candidate worth
flagging now and revisiting if the file grows or if eval.rs's
sectioning discipline gets formalized as a rule that applies to
every >500-line translate file.
