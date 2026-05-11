# Findings: runtime/src/translate/inline.rs

Reviewed against `lints/rules/` as of baf8078.

## Violations of existing rules

### AP-001 at runtime/src/translate/inline.rs:507
> ```
> if let crate::ast::Expr::Identifier(s) = e {
>     if s == "spawnable_only" { continue; }
> }
> ```

Not strictly a token-grep AP-001 hit (`spawnable_only` doesn't match
`Sdl[A-Z]` etc.), but it is the same family of violation against the
file's runtime-invariant brief: "Must NOT know about Effects, the
scheduler, or any I/O." `spawnable_only` is a scheduler-side marker
consumed in `runtime/src/effect_loop.rs:222–229` (the only other
mention in the runtime). Hard-coding the literal in the translate
walker means the translation pass has been taught one specific
scheduler-side feature flag. If a second such marker appears the
list grows here too — exactly the leak `lints/runtime-invariants.md`
calls out for this file.

Listed under "violations" because it directly contradicts the
explicit invariant ("Know about Effects, the scheduler, or any
I/O" forbidden) even though no existing AP-NNN rule names it.

## Per-file-invariant compliance check

Other invariants from `lints/runtime-invariants.md` for this file:

- "Must NOT own the Solver (borrows)." — satisfied. Every entry
  point takes `solver: &Solver<'static>`; no Solver construction
  in this file.
- "Must NOT own registries (borrows)." — satisfied. `registry:
  &DatatypeRegistry`, `enums: Option<&EnumRegistry>` everywhere.
- "Must NOT decide what's a 'schema' vs 'claim' vs 'type'" —
  satisfied. The walker uses one `schemas: &HashMap<String,
  SchemaDecl>` and treats all entries uniformly. No `if keyword ==
  "claim"` branches.
- "Must NOT know about Effects, scheduler, or any I/O" —
  violated by the `spawnable_only` arm at line 507 (above).

## Candidate new rules

### Suggested AP-009: no-process-exit-in-library-layers

**Pattern observed at runtime/src/translate/inline.rs:280, 298, 345,
526:**
> ```
> std::process::exit(1);
> ```

Four hard exits inside the translation walker. They fire on
positional-pin shape errors (lines 280, 298), pin-translation
failures (345), and constraint-translation failures (526).

**Why it might be bad.** `runtime/src/` is a library crate
(`evident_runtime`) consumed by `commands/*` (CLI), `runtime/tests/`,
and external embedders. A library calling `std::process::exit`
terminates the host process from arbitrary depth, defeats unit /
integration test isolation (no caller can recover from a malformed
fixture), and bypasses every `ExitCode` chain the CLI commands set
up. The runtime-invariants doc establishes the `commands/*` ↔
library split explicitly: commands return `ExitCode`, the library
returns results. Inside the library these errors should propagate
as `Err` / `Result` (or at minimum panic, which a test harness can
catch) rather than calling `exit`. Survey of the rest of
`runtime/src/`: `effect_dispatch.rs` is the only other file using
`process::exit`; every other translate sibling uses `eprintln!`
warnings. inline.rs's behavior is anomalous within its own
sub-package.

**Suggested fix.** Return `Result<_, TranslateError>` from
`inline_body_items*`, surface to the runtime facade, let the CLI
layer translate to `ExitCode::from(1)`. As a smaller-scope first
step: replace `process::exit(1)` with `panic!(...)` so test
runners can `catch_unwind`.

**Detection idea.** grep
`std::process::exit\|process::exit\(` with scope = `runtime/src/`
**excluding** `runtime/src/commands/**` and `runtime/src/main.rs`.
Likely 6 hits today (4 in inline.rs + 2 in effect_dispatch.rs);
both files would need fixing.

### Suggested AP-010: env-var-read-in-hot-path

**Pattern observed at runtime/src/translate/inline.rs:65–70 (called
from line 81):**
> ```rust
> fn max_inline_depth() -> usize {
>     std::env::var("EVIDENT_MAX_INLINE_DEPTH").ok()...
> }
> // …
> fn try_enter(visited: &mut HashMap<String, usize>, name: &str) -> Option<usize> {
>     let max = max_inline_depth();      // env::var on every call
>     ...
> }
> ```

`try_enter` runs once per claim invocation entered (positional
Call, guarded ⇒, Passthrough, ClaimCall — four callers). It calls
`max_inline_depth()` which does an `std::env::var` syscall every
time. For a recursive transpiler claim that fires hundreds of
times per query, that's hundreds of env lookups per query.

The same pattern appears at lines 162 (`EVIDENT_INLINE_TRACE`),
320 and 512 (`EVIDENT_LENIENT`) — all read fresh each time the
arm is taken. `EVIDENT_LENIENT` reading per-failure is acceptable
(failure is rare); `EVIDENT_MAX_INLINE_DEPTH` reading per-claim-
call is not.

**Why it might be bad.** Two issues. (a) Performance: env::var is
a process-global lock + string clone; it's the kind of per-call
overhead that disappears in a profile but adds up under recursive
inlining. (b) Semantics: re-reading the env var per call means a
caller that mutates the env mid-run would see the new value at
unpredictable points. Env knobs should be snapshotted at runtime
construction (or at most lazily once per process via `OnceLock`).

**Suggested fix.** Lift `max_inline_depth()` to a `OnceLock<usize>`
(or `LazyLock` once stabilized), so the env read happens once and
the cap is fixed for the process. Apply the same fix to
`EVIDENT_INLINE_TRACE`. `EVIDENT_LENIENT` may stay per-call (rare
path) but consistency-wise should also be `OnceLock`.

**Detection idea.** grep `std::env::var\("EVIDENT_` inside
function bodies that are called from within recursive walkers.
Hard to fully mechanize ("hot path"); review-only is OK, but a
weaker mechanizable form is "any `env::var` call inside a
non-`new`/`init`/`build` function within `runtime/src/translate/`
or `runtime/src/runtime.rs`." Today: ~3 hot-path hits (this file).

### Suggested AP-011: duplicated-claim-inline-prelude

**Pattern observed at runtime/src/translate/inline.rs:399–442,
465–501, 543–603:**

Three arms (positional `Constraint(Expr::Call)`, guarded
`Implies`, and explicit `ClaimCall`) all run the same pre-inline
sequence: clone env, isolate helper-locals (or skip), insert slot
mappings, walk the claim body and `declare_var_named` each
unmapped Membership with a per-call-suffixed Z3 name, then
recurse. The three implementations have drifted: only the
positional arm calls `isolate_helper_locals`; only the ClaimCall
arm has the `force_fresh` recursive-shadowing logic; only the
guarded arm has the explanatory comment about why slots aren't
cherry-picked from outer.

**Why it might be bad.** When the recursive-frame correctness
fixes happened (the "cnd/thn/els collapse" comment at lines
105–109, the `force_fresh` block at 580–598), they had to land in
multiple arms but didn't all get them. Today the positional and
guarded arms are missing the `force_fresh` shadowing, so a
recursive helper invoked positionally could still suffer the
collapse the ClaimCall arm guards against. Deduping into a
shared `inline_claim_call(env, claim, mappings, depth, guard,
tracker, ...)` helper would mean one site to fix and three
callers that look identical. The drift IS the reason for the
rule.

**Why this might NOT clear the bar.** The arms differ in pattern
matching (Call vs. Implies vs. ClaimCall) and in mapping
construction (positional zip vs. guard composition vs. explicit
mappings field). The shared payload is the env-clone + isolate +
declare-fresh + recurse sequence — a 30-line helper. This is
"refactor opportunity," which the agent prompt explicitly says
doesn't qualify ("This file is too complex" doesn't count). The
observable structural symptom would be "≥3 sites in one file
each calling `declare_var_named` in a per-Membership loop after
`env.clone()`" — mechanizable but specific to one file.

**Suggested fix.** Extract `inline_claim_call_helper` taking
mappings + depth + guard, run it from all three arms. While
extracting, audit which features (force_fresh, isolate_helper_
locals, slot_set tracking) every site needs and apply uniformly.

**Detection idea.** Review-only. The grep "≥N copies of
`declare_var_named.*call_id.*format!`" would catch this file but
is unlikely to recur usefully elsewhere.

## Other observations (review-only, not promoted)

- **Unused `HashSet` import** at line 15: `use std::collections::
  {HashMap, HashSet};` — `HashSet` is referenced only via
  fully-qualified `std::collections::HashSet` at lines 417 and
  552. The import is dead. Trivial cleanup, not a rule.

- **Doc-comment claim mismatch** at lines 350–357: comment says
  "Bare-identifier-as-passthrough handling moved to the self-
  hosted desugar pass," but the very next arm (line 369) does
  exactly that handling for `Constraint(Call(name, args))` — i.e.
  the desugar pass handled the `Identifier`-only case but the
  Rust side still has to recognize `Call(known_claim, args)` as a
  positional claim invocation. The comment as written suggests
  no special-cased claim handling remains; in fact a major arm
  follows. Local clarity issue; not a rule.

- **`exit_frame` ordering bug risk** at lines 372 / 462 / 549:
  the `Some(claim) = schemas.get(name) else { exit_frame(visited,
  name); continue }` pattern relies on `try_enter` having run
  first. Three sites; one type-check change to the lookup order
  would silently desync `visited`. Cosmetic — would benefit from
  a guard struct that calls `exit_frame` on Drop — but the
  current code is correct.

## Clean against rule scope

- **AP-002, AP-003, AP-006, AP-007, AP-008**: examples-scoped, not
  applicable.
- **AP-004**: conformance-scoped, not applicable.
- **AP-005**: `runtime/tests/**.rs`-scoped, not applicable to
  `runtime/src/translate/inline.rs`.

## Summary

One existing-rule-family violation (AP-001 spirit, not letter:
`spawnable_only` knowledge). Three candidate new rules — process-
exit in library layers (concrete enough to mechanize, 4 sites in
this file + 2 in `effect_dispatch.rs`), env-var-read in hot path
(concrete and mechanizable), duplicated-claim-inline-prelude
(real correctness drift but barely mechanizable, leaning review-
only). All three are listed under "Candidate new rules" without
being added to `lints/rules/` or `checks.sh` per the
"propose-don't-promote-yet" workflow.
