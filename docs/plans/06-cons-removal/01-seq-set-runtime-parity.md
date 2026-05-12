# Phase 6.1: Seq + Set runtime parity at dispatch time

## Goal

Make `Seq(T)` and `Set(T)` values extractable from the Z3 model into
Rust collections so the rest of the runtime can consume them the way
it currently consumes Cons-walked `Vec<T>`. No FFI marshalling
changes yet — that's Phase 6.2. After this phase, a roundtrip
(declare → constrain → solve → extract → re-encode → solve) works
for `Seq(Int)`, `Seq(String)`, `Seq(Bool)`, `Set(Int)`,
`Set(String)`, `Set(Bool)`, plus `Seq(UserType)` (which already
works).

## Prereqs

- Plan: `docs/plans/06-cons-removal/README.md` — read it first.

## Current state (from `runtime/src/translate/extract.rs:87-125`,
`runtime/src/translate/types.rs:83-122`, `:220-222`,
`runtime/src/translate/eval.rs:1033`)

**Seq is mostly shipped.** `extract_seq` reads length + walks the
`Array(Int → T)` model. `extract_seq_composite` handles user-typed
elements. `Value::SeqInt / SeqBool / SeqStr / SeqComposite` exist.
Tests in `runtime/tests/basic.rs:340-472` cover the primitive
cases. `assert_seq_given` reverses for re-encoding.

**Set is documented as not-extracted.** `Var::SetVar`'s comment
(`types.rs:220-221`) says explicitly: *"Z3 sets are functions over
infinite domains, not finite containers. Model extraction returns
no binding for SetVars."* All four extract paths (`eval.rs:318,
438, 614, 1033`) skip `Var::SetVar`. `Value` has no `Set*` variant.

**Seq literal already lowers correctly.** `SeqLit(...)` against a
`Seq(T)`-typed LHS goes through `translate_seq_lit_eq`
(`exprs.rs:1061-1090`) and pins length + per-index elements.

**Set literal lowering is incomplete.** `x ∈ {a, b, c}` lowers to
OR-of-equalities (`exprs.rs:1362-1370`). `S = {a, b, c}` (Set
equality to literal) is not handled — falls through.

## What to build

### 1. `Value::SetInt / SetBool / SetStr` variants

Stored as sorted `Vec<T>` for deterministic ordering. The runtime
picks an order at extraction time; the program does not get to
depend on which order, which is what Set is for.

`runtime/src/translate/types.rs`:
```rust
pub enum Value {
    // ... existing variants ...
    SetInt(Vec<i64>),
    SetBool(Vec<bool>),
    SetStr(Vec<String>),
}
```

No `SetComposite` for v1 — `Set(UserType)` is rare and the
deterministic-order question is harder.

### 2. `extract_set` function

Set extraction needs a candidate list — Z3 sets are characteristic
functions over an infinite domain, so we can only check membership
for values we already know about. v1 strategy: when a SetVar is
pinned to a SetLit (`S = {a, b, c}`), record the candidates in a
per-evaluation map keyed by env name. At extract time, look up
the candidates and ask the model `model.eval(set.member(c))` for
each.

Generalizing to non-literal-pinned Sets is future work; v1
returns `None` for SetVars with no recorded candidates (extracted
as missing binding, same as today's no-op).

`runtime/src/translate/extract.rs`:
```rust
pub(super) fn extract_set<'ctx>(
    set: &Set<'ctx>,
    elem: SeqElem,
    candidates: &[Value],
    model: &Model<'ctx>,
    ctx: &'ctx Context,
) -> Option<Value> { /* check each candidate's membership */ }
```

### 3. SetLit-as-RHS lowering

`exprs.rs` needs a new path: when the LHS of `=` translates to a
SetVar and the RHS is a SetLit, lower to:
- Membership assertion for each element (`set.member(e_i)`)
- Recorded candidates in the eval context's `set_candidates` map

This adds one new entry-point parallel to `translate_seq_lit_eq`:
`translate_set_lit_eq(name, items, ctx, env, …)`.

### 4. Candidates tracking

A `RefCell<HashMap<String, Vec<Value>>>` on whatever context
threads through `translate_body_item` → `extract_binding`. Probably
attaches to the existing eval state. Populated by
`translate_set_lit_eq`; read by the extract path.

### 5. Eval hook-up

Replace the `Var::SetVar { .. } => {}` no-op in `eval.rs:1033`
(and the parallel skips at `:318, :438, :614`) with a call to
`extract_set` that consults the candidates map. If no candidates
recorded, fall through silently (the binding doesn't appear in
results — same observable behavior as today).

### 6. Roundtrip tests

In `runtime/tests/basic.rs`, mirror the existing Seq tests:

- `set_int_basic` — declare `Set(Int)`, pin to a literal, query,
  assert `Value::SetInt` with the right members (sorted).
- `set_string_basic` — same for String.
- `set_bool_basic` — same for Bool.
- `set_roundtrip_extract_then_pin` — extract, then re-pin via
  a parallel `assert_set_given` (the inverse direction).
- `set_no_candidates_returns_none` — declare a free SetVar,
  verify no `Value::Set*` binding appears (back-compat with
  today's behavior).

`assert_set_given` mirrors `assert_seq_given` — takes a SetVar +
`Value::Set*`, produces a Bool constraint asserting membership for
each value in the Vec.

## Files touched

- `runtime/src/translate/types.rs` — add `SetInt/SetBool/SetStr`
  to `Value`; possibly thread a `set_candidates` map through eval
  state.
- `runtime/src/translate/extract.rs` — add `extract_set` and
  `assert_set_given`.
- `runtime/src/translate/exprs.rs` — add `translate_set_lit_eq`
  path; integrate candidates tracking.
- `runtime/src/translate/eval.rs` — replace SetVar no-ops with
  extract_set calls.
- `runtime/tests/basic.rs` — add Set extraction tests.

No changes to:
- `stdlib/` — Cons enums stay in place until Phases 6.2-6.5.
- `runtime/src/ffi.rs` — FFI marshalling untouched until 6.2.
- `runtime/src/effect_dispatch.rs` — no consumer changes yet.
- Parser / lexer — `SetLit` already parses.

## Acceptance

- `cargo test --release set_` (or equivalent filter) passes for
  the new tests.
- `./test.sh` passes end-to-end (no regression).
- `grep -n 'Var::SetVar { .. } => {}' runtime/src/translate/` is
  empty — all SetVar no-ops have been replaced.
- A program that declares `S ∈ Set(Int); S = {1, 2, 3}` and
  queries `S` returns `Value::SetInt(vec![1, 2, 3])` in the
  result bindings.

## What's deferred to later

- **Z3 set enumeration without candidates** — sets populated
  dynamically (e.g., union of two sets where one came from
  another FSM) won't extract until we have a general enumeration
  path. This is research; deferred.
- **`Set(UserType)`** — same reason as `SetComposite` not being
  added: order question is harder. Deferred until a use case
  needs it.
- **FFI marshalling of Seq/Set values** — Phase 6.2.
- **Literal sugar retargeting (`⟨...⟩` → Seq)** — Phase 6.6.
