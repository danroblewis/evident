# Findings: runtime/src/translate/declare.rs

Reviewed against `lints/rules/` as of baf8078.

## Violations of existing rules

None of the eight active AP-NNN rules apply to this file in the
violating sense:

- AP-001 (no library-specific in language-core): declare.rs is in
  scope. Scanned for `Sdl[A-Z]`, `SDL_`, `\bGl[A-Z]`, `Glsl`,
  `Audio[A-Z]`, `\.dylib`, `\.framework/`, `/opt/homebrew/lib/`,
  `/usr/lib/lib` — zero hits. Clean.
- AP-002, AP-003, AP-006, AP-007, AP-008: examples-only scope; not
  applicable.
- AP-004: conformance-only scope; not applicable.
- AP-005: applies to `runtime/tests/**.rs`; declare.rs has no
  in-file `#[cfg(test)]` module. Not applicable.

## Per-file-invariant check

The runtime-invariants brief for `translate/declare.rs` says it
**owns CLAIM_CALL_COUNTER for per-invocation Z3 name suffixes**,
and **must NEVER (a) assert constraints, (b) call into eval or
extract, (c) know what an Effect is**.

- (a) **Violation — asserts constraints.** Five `solver.assert(...)`
  call sites in this file:
  - `declare.rs:93` — `solver.assert(&v.ge(&Int::from_i64(ctx, 0)));`
    (Nat non-negativity)
  - `declare.rs:98` — `solver.assert(&v.gt(&Int::from_i64(ctx, 0)));`
    (Pos positivity)
  - `declare.rs:131` — `solver.assert(&len.ge(&Int::from_i64(ctx, 0)));`
    (primitive Seq length non-negativity)
  - `declare.rs:148` — same for user-type Seq length
  - `declare.rs:171` — same for enum-typed Seq length

  These are constraint assertions on the borrowed `Solver`. The
  brief says "declaration is its single concern." Today's
  implementation entangles the type-bound invariants of `Nat` /
  `Pos` / `Seq.len ≥ 0` with declaration. These could be moved
  into `inline.rs` (the file whose job IS to assert) — declare
  would return the bare `Var::IntVar` and a per-type "post-
  declaration assertion" callback that `inline` runs. Or inline
  could call a small helper `assert_type_invariants(env, solver,
  type_name)` after each `Membership`. As of now, `declare` both
  (a) accepts a `&Solver<'static>` parameter — visible in every
  function signature — and (b) uses it. The signature being there
  is the syntactic marker; the calls are the live violations.

- (b) **Clean — no call into eval or extract.** Imports list
  (lines 7–14) covers `std::collections::HashMap`,
  `std::sync::atomic`, `z3::ast`, `z3::{Context, Solver, Sort}`,
  `crate::ast::*`, `super::types`, `super::datatypes`. No
  `super::eval`, no `super::extract`. Clean.

- (c) **Clean — no Effect knowledge.** The file uses
  `BodyItem::Membership` only (line 220); no reference to
  `Effect`, `EffectList`, `EffectResult`, or any execution-side
  type. Clean.

- **CLAIM_CALL_COUNTER ownership confirmed.** Defined at
  `declare.rs:20`, exposed via `pub(super) fn next_call_id()` at
  line 22. Single owner; no other file in the crate also defines
  it. (Spot-checked — no other `static.*COUNTER.*AtomicU64` in
  `runtime/src/translate/`.) Clean.

## Candidate new rules

### Suggested AP-009: declaration-layer-must-not-assert
**Pattern observed at declare.rs:93, 98, 131, 148, 171:**
> `solver.assert(&v.ge(&Int::from_i64(ctx, 0)));`
>
> `solver.assert(&len.ge(&Int::from_i64(ctx, 0)));`

**Why it might be bad:** The translate pipeline has a layered
contract — `declare` allocates Z3 consts and registers them in
the env; `inline` walks BodyItems and asserts constraints on the
solver. Mixing them means a future reader can't tell which file
owns "what gets put on the solver." The Nat/Pos/Seq-length
invariants are a small, clean asymmetry today, but every "small
exception" is the seed for the kind of multi-file cleanup that
already bit the SdlVertex chain (see AP-001). The runtime-
invariants doc explicitly calls this out: declare's signature
takes `&Solver<'static>` purely for these invariants — that's a
documented smell.

**Suggested fix:** Either (1) hoist the invariant-asserts into
`inline.rs` after every `Membership`-handling call, removing the
`Solver` parameter from declare entirely; or (2) have declare
return the asserts as a `Vec<Bool<'static>>` for the caller to
post on the solver. Option 1 is cleaner because it keeps the
Solver parameter list out of the declaration layer's public
surface.

**Detection idea:** `grep -n 'solver\.assert' runtime/src/translate/declare.rs`
must return zero matches. (One-line grep check; cheap.)

**Note.** I'm proposing this as a candidate rather than promoting
straight to the rulebook because (a) it has only one file in
scope today and (b) the right fix is a refactor, not a small
edit, so it'd want to ship with the fix rather than as a
standalone lint. Recommend leaving as a candidate / review-only
note on this finding until someone refactors; if a similar
"declarations leaking into a sibling layer's job" pattern
appears in another file, promote then.

## Clean

The file is clean against all 8 active AP-NNN rules. The single
finding is a per-file invariant violation: `declare.rs` asserts
constraints (Nat/Pos non-negativity, Seq-length non-negativity)
in violation of its documented "declaration is its single
concern" rule. One candidate new rule proposed (AP-009,
review-only).
