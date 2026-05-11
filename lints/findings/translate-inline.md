# Findings: runtime/src/translate/inline.rs

Reviewed against `lints/rules/` as of 53fa1fe.

## Status of prior findings

### RESOLVED — `spawnable_only` literal at inline.rs:507 (was AP-001-spirit)

The prior wave flagged the file's direct check of the literal string
`"spawnable_only"` — a scheduler-side body marker — as a violation of
the file's invariant ("must NOT know about Effects, scheduler, or any
I/O"). That violation is **resolved**.

What changed:

1. `runtime/src/ast.rs:19` declares `pub const BODY_MARKERS: &[&str] =
   &["spawnable_only"]` with a doc comment establishing it as an
   AST-level concept ("Bare-identifier body items recognized as
   runtime metadata rather than translatable constraints"). The doc
   explicitly states the invariant going forward: "The translator MUST
   NOT reference any specific marker by literal string; scheduler /
   runtime layers MAY reference specific entries by looking them up
   against this list."
2. `runtime/src/translate/inline.rs:509` now reads:
   ```
   if crate::ast::BODY_MARKERS.contains(&s.as_str()) { continue; }
   ```
   No literal scheduler-marker name appears anywhere in this file.
3. The accompanying comment at lines 503–507 is updated to describe
   the new design ("Recognized runtime markers (declared in
   `crate::ast::BODY_MARKERS`) are bare identifiers that carry
   metadata for some other runtime layer — they have no Bool
   translation. Skip silently so they don't trip the dropped-
   constraint diagnostic.") No mention of the scheduler, no mention
   of `spawnable_only`. The comment correctly describes the generic
   shape, not a specific consumer.

**Does inline.rs still "know about" the scheduler?** No. It knows
that *some* names are AST-level markers without Bool translation;
who consumes them and what they mean is opaque to it. If a second
marker is added (parser-level pragma, future runtime-layer hint,
type-checker hint, etc.), `BODY_MARKERS` grows by one entry and
inline.rs is unchanged. The abstraction is at the right layer.

**Is the abstraction sufficient?** Yes, with one mild observation
(below): `BODY_MARKERS` is currently a flat string list with no
per-marker metadata about which layer consumes it. If a future marker
needs different translator behavior than "skip silently" (e.g. "skip
but emit a warning if it appears outside its expected enclosing
shape"), the list-of-strings form will need to grow into a struct.
But for the current contract — "these names are not constraints,
ignore them" — a `&[&str]` is the right shape.

The companion side (effect_loop.rs:222–230) still references the
literal `"spawnable_only"` string directly rather than going through
`BODY_MARKERS`. That's acceptable per the AST doc-comment ("scheduler
/ runtime layers MAY reference specific entries by looking them up
against this list"), but reads as "may look up", not "must look up
by name only" — effect_loop.rs uses a hardcoded literal `s ==
"spawnable_only"`, which means a typo on the AST side wouldn't
fire a compile error. A small follow-up would be to expose a named
const (`pub const SPAWNABLE_ONLY: &str = "spawnable_only"`) that
both files reference. Out of scope for this file's review.

### UNCHANGED — AP-009 candidate: 4 std::process::exit calls

Prior finding listed lines 280, 298, 345, 526. Current state, lines
280, 298, 345, 528 (one call moved by 2 lines from a comment shift).
Same four sites, same shapes:

- `runtime/src/translate/inline.rs:280` — positional pin on unknown type
- `runtime/src/translate/inline.rs:298` — positional pin arg-count overflow
- `runtime/src/translate/inline.rs:345` — pin-translation failure
  (under EVIDENT_LENIENT=0)
- `runtime/src/translate/inline.rs:528` — constraint-translation failure
  (under EVIDENT_LENIENT=0)

The candidate has not been promoted to a rule; it remains a candidate
in this findings file. No fix yet.

### UNCHANGED — AP-010 candidate: env-var reads in hot paths

Prior finding listed lines 65–70 (`max_inline_depth()`), 162
(`EVIDENT_INLINE_TRACE`), and 320 / 512 (`EVIDENT_LENIENT`). Current
state matches: 66 (max_inline_depth body), 162, 320, 514. Same four
sites, same shapes — no `OnceLock` / `LazyLock` introduced. Candidate
stands as before.

### UNCHANGED — AP-011 candidate: 3 claim-inline arms drifted

Prior finding listed three arms (positional `Constraint(Expr::Call)`,
guarded `Implies`, explicit `ClaimCall`) running near-identical
env-clone + isolate-helper-locals + per-call-fresh + recurse
sequences with documented divergence (only positional arm calls
`isolate_helper_locals` initially; only ClaimCall arm has
`force_fresh`; only guarded arm has the explanatory comment).

Current state: the positional arm AND the guarded arm now both call
`isolate_helper_locals` (lines 416 and 487), so the prior "only
positional" divergence on isolate is fixed. But the third divergence
remains: `force_fresh` at lines 580–598 still lives only in the
explicit `ClaimCall` arm; positional and guarded arms have no
equivalent recursive-shadowing logic. Three sites still have
`call_id = next_call_id()` + per-Membership `declare_var_named` loops
with subtly different behavior:

- positional Call arm (428–437): loops, skips if `slot_set.contains`
  or `inner.contains_key`, declares fresh.
- guarded Implies arm (488–495): loops, skips only if
  `inner.contains_key` (no `slot_set` because guarded form has no
  positional args — but `isolate_helper_locals` has already been run,
  so the post-isolation env is the relevant input).
- explicit ClaimCall arm (573–599): loops, computes `force_fresh`,
  removes from inner before re-declaring on recursive frames.

The rule's underlying claim ("the arms are drifted, fix once = fix
everywhere only if they're a shared helper") is still true. Partial
convergence has happened; full convergence has not. Candidate stands.

## Per-file-invariant compliance check

From `lints/runtime-invariants.md` for `translate/inline.rs`:

- "Must NOT own the Solver (borrows)." — satisfied. Every entry
  point takes `solver: &Solver<'static>`.
- "Must NOT own registries (borrows)." — satisfied. `registry:
  &DatatypeRegistry`, `enums: Option<&EnumRegistry>` everywhere.
- "Must NOT decide what's a 'schema' vs 'claim' vs 'type'" —
  satisfied. One uniform `schemas: &HashMap<String, SchemaDecl>`.
  No keyword-based branching.
- "Must NOT know about Effects, scheduler, or any I/O" — **now
  satisfied**, via the `BODY_MARKERS` indirection. See "RESOLVED"
  above.

## Other observations (review-only, unchanged from prior wave)

- **Unused `HashSet` import** at line 15: `use std::collections::
  {HashMap, HashSet};` — `HashSet` is referenced only via
  fully-qualified `std::collections::HashSet` at lines 417 and 554.
  The unqualified import is dead. Trivial cleanup, not a rule.

- **Doc-comment claim mismatch** at lines 350–357: comment says
  "Bare-identifier-as-passthrough handling moved to the self-
  hosted desugar pass," but the next arm (line 369) does the
  positional-Call form of the same handling for
  `Constraint(Call(known_claim, args))`. Local clarity issue;
  not a rule.

- **`exit_frame` ordering bug risk** at lines 372 / 462 / 548: the
  `Some(claim) = schemas.get(name) else { exit_frame(...); continue
  }` pattern depends on `try_enter` having run first. Three sites; a
  reorder would silently desync `visited`. Cosmetic — would benefit
  from a guard struct that calls `exit_frame` on Drop.

## Clean against rule scope

- **AP-001 letter** (no `Sdl[A-Z]` / `gl[A-Z]` etc.): clean. No
  library-specific tokens in this file.
- **AP-002, AP-003, AP-006, AP-007, AP-008**: examples-scoped, not
  applicable to runtime/src/.
- **AP-004**: conformance-scoped, not applicable.
- **AP-005**: `runtime/tests/**.rs`-scoped, not applicable.

## Summary

The `spawnable_only` violation is fully resolved by the `BODY_MARKERS`
indirection in `runtime/src/ast.rs`. The translation layer now knows
only about a generic AST-level concept (named runtime markers); the
specific scheduler-side meaning has moved to the consumer
(effect_loop.rs). The doc-comment in inline.rs at lines 503–507 also
reflects the new design — no scheduler reference. Abstraction is
sufficient for the current single-marker case; would need a small
shape change if a future marker needs translator-side behavior beyond
"skip silently."

Three prior candidate findings (AP-009 process-exit, AP-010 env-var
in hot paths, AP-011 claim-inline arm drift) all stand essentially
unchanged. AP-011 has partial progress — `isolate_helper_locals` now
fires in two of three arms, where previously only one — but the
`force_fresh` recursive-shadowing logic still lives only in the
explicit `ClaimCall` arm.
