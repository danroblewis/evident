# JIT codegen gaps — audit + status

The Cranelift JIT (`runtime/src/functionize/cranelift.rs`) compiles an
extracted `Z3Program` to native code. When it can't emit code for some
Z3 expression shape it returns `None`, and the component falls through
to a full Z3 solve (`runtime/src/runtime/query.rs:compile_one_component`
routes a `None` to `ComponentOutcome::Slow`). Falling back is always
*correct* — just slower — so a JIT gap is a performance bug, not a
correctness one.

This doc enumerates every JIT bail the session-T audit found, with the
Z3 shape, where in `cranelift.rs` it bails, a minimal repro, and status.

The pipeline matters when reading "where it bails":

```
Evident AST → translate/ → Z3 ASTs → z3_eval::extract_program → Z3Program
            → functionize/cranelift.rs::compile_program → native code | None
```

A bail is in **codegen** (cranelift returns `None` / miscompiles) or in
**extraction** (`z3_eval` produces a shape — e.g. a `Guarded` step — or
topo-bails). Codegen bails are fixable inside `cranelift.rs`; extraction
bails need `translate/` or `z3_eval` changes.

> **Other functionizer strategies.** Cranelift is the default, but the
> `Functionizer` trait is strategy-agnostic. For the GLSL fragment-shader
> backend (a GPU transpile of the same `Z3Program`, opt-in, macOS-only)
> and *its* scope limits, see [`glsl-functionizer.md`](glsl-functionizer.md).

Tracing: `EVIDENT_JIT_TRACE=1` prints `[jit] bail: …`. `EVIDENT_FUNCTIONIZE_STATS=1`
prints the per-claim `comp=C/N`. `EVIDENT_JIT_CALL_TRACE=1` prints each
compiled call's result + helper calls.

---

## Closed by this session

### 1. Integer division / modulo as a top-level Scalar value — FIXED

* **Shape**: `(div x y)`, `(mod x y)` (Z3 `IDIV`/`DIV`/`MOD`/`REM`) as
  the *outermost* decl of a `Z3Step::Scalar` expr, or as an ITE branch
  (which writes via `emit_write_value`).
* **Where it bailed**: `emit_write_value`'s arithmetic arm
  (`cranelift.rs`, the `DeclKind::ADD | SUB | MUL | UMINUS` match) did
  **not** list `IDIV/DIV/MOD/REM`, so a top-level div/mod hit `_ => None`.
  `emit_compute_i64` already emitted `sdiv`/`srem` for div/mod *operands*,
  so `q = x/2 + 1` compiled but `q = x/2` did not.
* **Category**: codegen — unhandled DeclKind in one of two parallel
  dispatchers.
* **Repro**: `q ∈ Int = x / 2` → `[jit] bail: Scalar q = (div x 2)`.
* **Fix**: add `IDIV | DIV | MOD | REM` to that arm (delegates to
  `emit_compute_i64` + `set_int`, like the other arithmetic ops).
* **Test**: `runtime/tests/jit_gap_div_mod.rs` (div top-level, div in an
  ITE branch, mod top-level — each asserts `compile_program` succeeds and
  the call returns the right value).

### 2. Seq-bodied `Guarded` steps (`match state ⇒ ⟨…⟩` effects) — FIXED

* **Shape**: a `Z3Step::Guarded` whose branch bodies are `GuardedBody::Seq`
  — the `effects = match state ⇒ ⟨Println(…), …⟩` shape. z3_eval extracts
  `match`/implication into guarded `(or (not P) Q)` branches.
* **Where it bailed**: `compile_program` refused **any** program
  containing a `Guarded` step outright (an early `return None` in Phase 2),
  because the existing Guarded codegen wrote a sentinel `Int(0)` on the
  "no branch matched" fallthrough — a wrong value that would silently
  propagate. This single refusal hit **24 of 27 demos** (every FSM whose
  effects vary per tick) and was the real reason test_29 only reached
  `comp=1/4`. (NOT the "deep nested ITE chains" the old test_29 docstring
  blamed — those compile fine; the audit confirmed no chain step bails.)
* **Category**: extraction-shaped refusal (the step is `Guarded`), but the
  fix is pure codegen.
* **Fix**: compile Seq-bodied Guarded steps via the existing branch-chain
  codegen, plus a **runtime bail flag**. `compile_program` now takes a
  4th ABI param `*mut i64`; the no-branch-matched fallthrough block stores
  `1` into it, and `JitProgram::call` returns `None` when set — so the
  caller (`execute_plan`) falls through to the slow Z3 solve, exactly the
  None-style bailout the slow path always gave. For an exhaustive match
  the fallthrough is dead code and the flag stays 0.
* **Test**: `runtime/tests/jit_gap_guarded_seq.rs` (match-on-enum picks
  the right Seq branch). End-to-end: test_29 `comp 1/4 → 3/4`,
  steady/tick `0.28ms → 0.01ms`.

### 3. String concatenation `(str.++ a b …)` — FIXED

* **Shape**: `DeclKind::SEQ_CONCAT` as a Scalar value — e.g.
  `Println("count = " ++ s)` payloads, `world_next.trail = "." ++ world.trail`.
* **Where it bailed**: `emit_write_value` had no `SEQ_CONCAT` arm →
  `_ => None`.
* **Category**: codegen — unhandled DeclKind.
* **Repro**: `out ∈ String = "hi " ++ name` →
  `[jit] bail: Scalar … (str.++ …)`.
* **Fix**: build each operand into a temp slot (operands reach this via
  the existing String-literal short-circuit / UNINTERPRETED clone-from-env)
  and call the already-present `ev_str_concat(out, args_ptr, len)` helper —
  mirrors the multifield-ctor codegen path.
* **Test**: `runtime/tests/jit_gap_str_concat.rs`. End-to-end: test_25
  `comp 8/12 → 10/12`.

### 4. `#seq` of an unpinned Seq — silent length-0 miscompile — FIXED (refuse)

* **Shape**: `#last_results > 0` etc. — `Expr::Cardinality` on a Seq whose
  length isn't statically pinned. translate lowers `#seq` to a separate
  Z3 const `<seq>__len` (`translate/declare.rs`), an UNINTERPRETED input.
* **Where it miscompiled**: the runtime supplies the Seq *value* in
  `given` but never the `__len` symbol, so `JitProgram::call` packed it as
  the `Int(0)` sentinel — the JIT computed length 0, e.g.
  `has_result = #last_results > 0 → false` (test_19 printed `count = ?`
  on every tick instead of `0,1,2`). This was a latent bug *unmasked* by
  fix #2 (the component used to refuse on its Guarded effects and never
  reached codegen). `emit_write_value` already refused `__len` (its
  UNINTERPRETED-with-`__arr`/`__len` path), but `emit_compute_i64`'s
  `DT_ACCESSOR` arm silently stripped the suffix and read a missing field.
* **Category**: codegen — a value the ABI can't supply, read as a wrong
  sentinel.
* **Fix**: in `compile_program`, bail the whole program if any collected
  input name ends in `__len` (these are unpinned-Seq length symbols; a
  *pinned*-length Seq folds `#seq` to a numeral and never reaches here).
  → component routes to the correct slow solve.
* **Repro**: `has_result ∈ Bool = (#last_results > 0)` with `last_results`
  unpinned → `[jit] bail: input last_results__len is a Seq-length symbol`.
* **Possible future upgrade** (deferred, see below): derive `<seq>__len`
  from the paired Seq value via a new `ev_seq_len` helper instead of
  bailing.

---

## Deferred — known fix path

### D1. `#seq` length, computed from the paired Seq value

The conservative fix #4 *refuses* components needing `<seq>__len`. We
could instead compile it: add `ev_seq_len(slot) -> i64` (a one-liner
matching on the `Seq*` Value variants) and, in `compile_program`'s env
setup, for each `X__len` input whose paired `X` is also available, emit
`ev_seq_len(X_slot)` into a fresh Int slot and bind `X__len` to it.
**Risk**: the pairing is only sound when `X` is genuinely the same Seq in
`given`; needs care when `X` is itself an output or absent. Affects
test_19, test_22. Medium effort, contained to `cranelift.rs` +
`value_builders.rs`.

### D2. String equality `(= s1 s2)` in a Bool / scalar-`match` guard

* **Shape**: `(= "" host.name)`, `(or (= world.stdin_line "quit") …)`.
* **Where**: `emit_compute_i64`'s comparison arm reduces both operands to
  i64 via `emit_compute_i64`; a String operand can't be reduced → `None`.
  Surfaces as the whole enclosing Scalar/ITE bailing.
* **Repros**: test_12 (`(ite (= "" host.name) HWait HShow)`), test_14
  (`(and fresh (or (= world.stdin_line "quit") …))`).
* **Fix path**: add a `ev_str_eq(a_slot, b_slot) -> i64` helper and a
  String-equality branch in `emit_compute_i64`'s `EQ` arm (detect String
  operand sorts, build both into temp slots, call the helper). Mirrors the
  existing nullary-enum-equality special case. Low–medium effort, contained
  to `cranelift.rs` + `value_builders.rs`. Would close test_14 fully and
  one of test_12's components.

### D3. Scalar-bodied `Guarded` steps (`match` → scalar)

* **Shape**: a `Z3Step::Guarded` whose branch bodies are
  `GuardedBody::Scalar` — e.g. `first_str = match last_results[1] {
  StringResult(s) ⇒ s | _ ⇒ "?" }` producing a String/Int.
* **Where**: `compile_program` now explicitly refuses Guarded steps with
  any scalar body (the `[jit] bail: Guarded … (scalar body …)` arm). The
  branch-chain codegen *exists* and would run, but the payload-extraction
  path (variant recognizer + accessor on a `(select seq i)` element)
  miscomputed for some shapes during the session — so it's gated off until
  it can be made correct and the bail-flag semantics verified for scalar
  outputs.
* **Fix path**: audit `emit_write_value`'s `DT_ACCESSOR` / `SELECT` /
  `is_variant` interaction for Seq-of-enum elements, add a focused test
  per shape, then drop the scalar-body refusal. Medium effort, contained
  to `cranelift.rs`. (Note: the common case where the scrutinee is given
  concretely simplifies to a plain Scalar ITE and already compiles — the
  refusal only bites genuinely-symbolic match-to-scalar.)

---

## Intractable here — needs a runtime change outside `functionize/`

### I1. test_29's last component (tick-0 bootstrap chains)

test_29 sits at `comp=3/4`, not 4/4. The missing one is the chains
component **in the tick-0 analysis**: at tick 0 the prev-tick value
`_tick` is absent, so `compile_one_component`'s `unsafe_free` check
(`runtime/src/runtime/query.rs`) routes it to the scoped slow solve
rather than baking a free model value. In steady state the same component
compiles. This is a `runtime/` policy decision, explicitly out of scope
for this session (the table in `SESSION.md` forbids editing
`runtime/src/runtime/*`). It costs only the one-shot tick-0 solve, not
steady-state throughput.

### I2. SDL float-vertex lists + LibCall-as-scalar (test_17)

* **Shapes**: `(PfCons (PfF32 320.0) …)` packed-float vertex buffers,
  `(LibCall "…libSDL2.dylib" …)` as a Scalar value, `Guarded effects seq
  elem` where an element is one of those.
* These are FFI/packed-buffer value kinds with no native-`Value`
  representation the JIT builds today; they belong to the FFI bridge, not
  the arithmetic JIT. Documented as out of scope. See
  `examples/COUNTEREXAMPLES.md` for the SDL-render-via-dispatch limits.

### I3. Mario's un-JIT'd components

Already analyzed in a prior session: not codegen-shape gaps but
`translate/` + `query.rs` issues (topo cycle from a content-free
crosslink, intermediate-global libcalls filed as GLOBAL assertions).
See `examples/COUNTEREXAMPLES.md` #12.

---

## Summary table

| # | Gap | Where | Category | Status |
|---|-----|-------|----------|--------|
| 1 | top-level `div`/`mod` | `emit_write_value` arith arm | codegen | **fixed** |
| 2 | Seq-bodied `Guarded` (effects) | `compile_program` refusal | codegen (+bail flag) | **fixed** |
| 3 | `str.++` concat | `emit_write_value` | codegen | **fixed** |
| 4 | `#seq` of unpinned Seq | `compile_program` input check | codegen | **fixed (refuse)** |
| D1 | `#seq` computed from value | `compile_program` env setup | codegen | deferred |
| D2 | String equality in guard | `emit_compute_i64` EQ arm | codegen | deferred |
| D3 | scalar-bodied `Guarded` | `compile_program` refusal | codegen | deferred |
| I1 | test_29 tick-0 chains | `runtime/query.rs` unsafe_free | runtime | out of scope |
| I2 | SDL float lists / LibCall | FFI bridge | runtime | out of scope |
| I3 | Mario components | `translate/` + `query.rs` | runtime | out of scope |

**Net effect on the example set** (`comp=C/N`, JIT default):
test_29 `1/4 → 3/4`, test_25 `8/12 → 10/12`, test_20 `0/2 → 1/2`,
test_01–10 + 16 + 24 at 100%. test_19/22 now correctly *refuse* (were a
silent miscompile risk once fix #2 unmasked them).
