# Findings: runtime/src/commands/infer_types.rs

Reviewed against `lints/rules/` as of baf8078.

## Violations of existing rules

None. The active rulebook (AP-001 through AP-008) explicitly
excludes `runtime/src/commands/*` from AP-001's scope, and the
remaining rules target `examples/`, `tests/conformance/`, or
`runtime/tests/` — none reach this file.

## Invariant deviations (per `lints/runtime-invariants.md`)

The invariants doc names two requirements specific to this
file's "dual role" + "no Rust-side inference logic / no
special-casing" charter. Two real deviations and two borderline
ones:

### Missing CLI entry — `cmd_infer_types` is not in this file

The invariant says: *"each exposes both a `pub fn cmd_<name>`
for the user-facing subcommand AND a `pub fn auto_apply_*` …
for use as a library."* This file's doc comment (lines 1-21)
also opens with `\`evident infer-types <file>\` — Stage 6
user-facing self-hosted inference.`

But there is no `cmd_infer_types` function defined in this file
(only `collect_inferences`, `auto_apply_inferences`,
`unambiguous_inferences`, `render_bindings`). And `main.rs`'s
dispatch table (lines 24-37) has match arms for `query`, `check`,
`sample`, `test`, `effect-run`, `lint` — but no `infer-types`
arm. The CLI subcommand documented at the head of the file
isn't actually wired up. Either the CLI verb was removed and
the doc comment is stale, or the verb was never finished. The
"dual role" invariant is half-met: only the library role
exists.

### Rule-name string-matching for type extraction (special-casing rule naming)

> ```rust
> let typ = if rule.contains("string") { "String" }
>           else if rule.contains("int") { "Int" }
>           else if rule.contains("bool") { "Bool" }
>           else { "?" };
> ```
> (`render_bindings`, lines 256-259)

The invariant says: *"Must NOT special-case any specific rule
— if a rule needs special handling, that's a sign the rule
should be expressed differently in its `.ev` file."*

Reading the type out of the rule's NAME (a substring search on
"string"/"int"/"bool") is exactly the kind of special-casing
that ties Rust to a specific `.ev`-side naming convention.
Adding a `has_real_assignment` rule to `stdlib/passes/iter_types.ev`
or a `propagate_real` to `propagation.ev` requires changing
this Rust function — and the failure mode of forgetting is
"silent fallback to '?'", not a compile error.

The rule should bind the type as an output (`target_type` or
similar) so the Rust side is uniform. The neighbor branch above
(lines 247-251) already does this for `has_membership_of_var`,
which is the right pattern.

### Hardcoded rule-name lists couple Rust to .ev file contents

> ```rust
> const PROGRAM_RULES: &[&str] = &[
>     "extract_first_membership",
>     "infer_string_from_membership_plus_assignment",
>     …
> ];
> const ITER_RULES: &[&str] = &[
>     "has_membership_of_var",
>     …
>     "propagate_string", "propagate_int", "propagate_bool",
> ];
> ```
> (lines 34-55)

Every rule name in `stdlib/passes/{literal_types,iter_types,
propagation}.ev` is duplicated here in a Rust constant. Adding
a new inference rule means editing two files, and there's no
way for the .ev side to declare "this is an inference rule the
orchestrator should run."

The split between PROGRAM_RULES and ITER_RULES also encodes a
structural fact (does the rule need the encoded Program
injection or just the body Seq?) that the orchestrator
hardcodes per-rule rather than reading from a metadata claim.

This is borderline rather than a hard violation — the invariant
allows the Rust side to "load the pass file, run a query
against the user's source, decode the resulting Program value
back to Rust AST." Knowing rule names is part of "running the
query." But the asymmetry between `iter_types.ev`'s rules,
which appear in BOTH PROGRAM_RULES and ITER_RULES depending on
their family, and `propagation.ev`'s rules, which only appear
in ITER_RULES, is encoding pass-shape decisions in Rust that
read more naturally as a `claim is_iter_rule(rule_name ∈ String)`
table on the .ev side.

### Structural policy embedded in Rust (lines 98-105)

> ```rust
> // PROGRAM_RULES pattern-match the whole Program shape — they
> // require `MakeProgram(SchLCons(_, SchLNil), …)` (exactly one
> // user schema). For multi-schema user programs … they're
> // structurally UNSAT and we'd just pay solver setup cost for
> // nothing. Skip them.
> let n_claims = rt.user_claim_count();
> if n_claims == 1 {
>     …
> }
> ```

The "PROGRAM_RULES only run when there's exactly one user
schema" policy is enforced in Rust as an optimization gate.
This is a small example of "Rust knows what a particular
pass-family expects," which is the slippery slope the invariant
warns about. The right answer is for the rule to be expressed
in `.ev` such that it's structurally cheap to attempt against
multi-schema programs (it'll just be UNSAT instantly), and for
the orchestrator to be uniform.

Borderline; flagged for review-only.

## Candidate new rules

### Suggested AP-009: dual-role-files-must-export-both-roles

**Pattern observed at `runtime/src/commands/infer_types.rs:1-21`
+ `runtime/src/main.rs:24-37`:**
> File doc-comment opens with `\`evident infer-types <file>\` —
> Stage 6 user-facing self-hosted inference.` but the file
> contains no `pub fn cmd_infer_types`, and `main.rs`'s
> dispatch table has no `"infer-types" =>` arm.

**Why it might be bad:** The invariants doc identifies a
specific class of files (currently `infer_types.rs` and
`desugar.rs`) whose defining property is the dual role: CLI
verb AND library helper. When one role disappears (or never
gets wired up) but the doc-comment / module name still implies
both, callers and reviewers can't tell whether the missing
half is a regression, a dead doc, or "we removed it
deliberately." The same issue would arise if a future dual-role
file kept its `cmd_` function but lost the `auto_apply_` one
that other commands depend on.

**Suggested fix:** Either (a) implement the missing role and
add the dispatch arm in `main.rs`, or (b) delete the doc-
comment claim about being a CLI verb and clarify in the doc
that this file is library-only. The invariants doc should also
either name the file as library-only or list both roles as
mandatory.

**Detection idea:** For each file under `runtime/src/commands/`,
check (a) whether it defines `pub fn cmd_<basename>` and (b)
whether `runtime/src/main.rs`'s match block contains a
`"<basename-with-underscores-as-dashes>" => commands::<basename>::cmd_<basename>`
arm. If exactly one of (a) or (b) is true, fail. (If both are
false the file is library-only — currently undocumented as a
category but not necessarily wrong.) Could be a small Rust
test in `runtime/tests/lints.rs` that walks the `commands/`
directory and parses both files.

If accepted, this would be `AP-009` (next available after
AP-008).

### Suggested AP-010: rule-name-string-matching-in-pass-orchestrators

**Pattern observed at `runtime/src/commands/infer_types.rs:256-260`:**
> ```rust
> let typ = if rule.contains("string") { "String" }
>           else if rule.contains("int") { "Int" }
>           else if rule.contains("bool") { "Bool" }
>           else { "?" };
> ```

**Why it might be bad:** This pattern derives semantic data
(the Z3 sort name) from substring matches on `.ev` rule names.
It silently couples the Rust orchestrator to a naming
convention in the pass files, with no compile-time check that
the convention holds. Adding `has_real_assignment` to
`stdlib/passes/iter_types.ev` produces silent `"?"` results
instead of `"Real"`. The invariant for `infer_types.rs` /
`desugar.rs` says explicitly: *"Must NOT special-case any
specific rule — if a rule needs special handling, that's a
sign the rule should be expressed differently in its `.ev`
file."* This is exactly that special-casing.

The general form of the anti-pattern is: *the orchestrator
inspects a rule's name (or any other surface property) to
decide what data to extract from it.* The right answer is for
the rule to bind the relevant data as a named output that
the orchestrator reads uniformly across rules.

**Suggested fix:** Add a `target_type ∈ String` binding to
each rule in `stdlib/passes/iter_types.ev` and
`stdlib/passes/propagation.ev` that doesn't already produce
one (matching the shape `has_membership_of_var` already uses).
Then `render_bindings` reads `target_type` uniformly and the
Rust side stops caring about rule names.

**Detection idea:** grep for `rule.contains(` or `rule_name.contains(`
or `name.starts_with(` patterns in any file under
`runtime/src/commands/{infer_types,desugar}.rs`. Review-only
for now — narrow grep, low false-positive rate, but only two
files in scope.

If accepted, this would be `AP-010`.

## Clean

Aside from the invariant deviations and candidate rules above,
the file is well-organized: clear section comments, narrow
public surface (three `pub fn`s + one `pub struct`), no
panics, no `unwrap` on Z3 results, no internal-module reaches
beyond the published `evident_runtime::{EvidentRuntime, Value}`
facade. Imports are minimal and at the top of the file.
