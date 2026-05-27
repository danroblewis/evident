# given-pinned inputs

## Contract

`given: HashMap<String, Value>` is the mechanism by which a caller pre-assigns values to named variables before a Z3 solve. Each entry asserts `var = value` as a hard equality constraint in the solver's current push frame (via `push`/`assert`/`check`/`pop` — see `run_cached` in `translate/eval/cached.rs`). Variables present in `given` are treated as inputs; every other env variable that appears in the simplified constraint body and is not a constant (`PinnedInt`, `EnumValue`, `EnumCtor`) is an output and appears in `bindings`. A claim whose entire input surface is fully pinned and whose body is a total function over those inputs produces exactly one satisfying model — making the solve a deterministic transition: `given X → bindings Y` with no Z3 free choices.

## Value types a given can carry

| Value variant | How it is pinned |
|---|---|
| `Value::Int(i64)` | `solver.assert(v._eq(&Int::from_i64(ctx, n)))` — direct scalar equality |
| `Value::Bool(bool)` | `solver.assert(v._eq(&Bool::from_bool(ctx, b)))` — direct scalar equality |
| `Value::Real(f64)` | `solver.assert(v._eq(&real_from_f64(ctx, f)))` — rational equality (f64 projected from Z3 exact rational at extraction) |
| `Value::Str(String)` | `solver.assert(v._eq(&z3_string(ctx, s)))` — string literal equality; non-ASCII escaped at `from_str` sites |
| `Value::Enum { .. }` | Two paths: (1) fast path via `query_with_pins_and_given`: `value_enum_to_datatype` encodes to a Z3 Datatype then `ast._eq(&dt)` in a push frame (`scheduler_api.rs` lines 70–79); (2) slow path via `evaluate_with_extra_assertions`: same encode+assert, called from the outer `given` loop (`extra.rs` lines 155–162). |
| `Value::SeqInt` / `Value::SeqBool` / `Value::SeqStr` | `assert_seq_given` in `translate/extract.rs`: asserts length equality (`len._eq(n)`) and per-index element equalities (`arr.select(i)._eq(elem)`). |
| `Value::SeqEnum(Vec<Value>)` | `assert_seq_given` handles via `DatatypeSeqVar` + `value_enum_to_dyn_with_dt`: length pin + per-index Datatype equality conjunct. |
| `Value::SeqComposite` / `Value::Composite` | Not directly pinnable via `given` in v1; record-element seq vars use `DatatypeSeqVar` with non-empty `fields`, which is excluded from the parallel path and may fall through to the full slow solve. |
| `Value::SetInt` / `Value::SetBool` / `Value::SetStr` | `assert_set_given` in `translate/extract.rs`: builds a Z3 set literal from items and asserts set equality; also populates `candidates` for `#s` cardinality queries. |

`PinnedInt`: a special case where a variable's value is baked into the cached solver at build time (not via `given`). If a `given` supplies the same value it is a no-op; a conflicting value asserts `false` → UNSAT.

## given vs pins

`pins: &[(&str, z3::ast::Datatype<'static>)]` is the legacy fast-path for pinning enum-typed variables that the scheduler already holds as live Z3 AST handles (typically `state` and `last_results`). `given: HashMap<String, Value>` is the general value map, accepted at every call site.

They overlap: the scheduler supplies `state` in both `pins` (as a `z3::ast::Datatype`) and in `given` (as `Value::Enum`). The comment at `scheduler_api.rs` line 32 makes this explicit: "Datatype pin is redundant with `current_state_v` in `given`." On the JIT fast path the Datatype pins are skipped entirely (the comment says "JIT fires even with non-empty pins: Datatype pin is redundant with `current_state_v` in given"); on the slow cached path both are applied, with the `given` enum path going through `value_enum_to_datatype` re-encoding in a push frame (`scheduler_api.rs` lines 69–79) and the `pins` loop asserting the pre-encoded Datatype directly (lines 62–67).

## outputs

`bindings` in the returned `QueryResult` contains every env variable that is:

1. Present in the simplified constraint body (touched by at least one assertion — `collect_touched_names` filter, `query.rs` lines 505–516), AND
2. Not a member of `given` (line 510: `.filter(|(name, _)| !given.contains_key(name.as_str()))`), AND
3. Not a constant (`EnumValue`, `EnumCtor`, `PinnedInt` — line 511–514).

In practice: `bindings` = `given` union `{computed outputs}` because `execute_plan` re-inserts the given values at the end (line 766: `for (k, v) in given { out.insert(k.clone(), v.clone()); }`). So the caller can read `bindings` as the full solution without tracking which keys were inputs vs outputs.

## Determinism

A model is unique (deterministic) when the body constraints, taken together with the given equalities, admit exactly one satisfying assignment for every unconstrained variable. When any output variable is not fully determined by the constraints — either because the body has no equality defining it, or because the body only gives bounds but not a unique value — Z3 picks an arbitrary satisfying value. This is the source of nondeterminism in Evident programs.

Fixtures must avoid this by either:
- Pinning all inputs that the body's output computations depend on (fully saturating the constraint), OR
- Asserting only on outputs whose value is uniquely forced by the pinned inputs (i.e., only testing constrained outputs).

The value cache (`ClaimValueCache`, `query.rs`) memoizes `(given-values hash) → QueryResult`; collisions are verified by full equality. This means repeated calls with identical `given` return the same `bindings`, which provides determinism at the call-site level even for Z3-nondeterministic claims — but only because the first call's arbitrary Z3 choice is cached. A fixture must not rely on this caching behavior; it must pin fully.

## Fixture candidates

Every fixture in this test suite is fundamentally a `given X → bindings Y` operation. The `given` map defines the input surface; `bindings` (minus the re-inserted given keys) is the output surface.

**Minimal pin→solve examples:**

1. **Scalar counter tick** (`test_19_prev_tick.ev`, claim `sat_subsequent_tick_increments`): given `{is_first_tick: Bool(false), _count: Int(7), state: Enum(Counting)}` → bindings contains `count: Int(8)`. All three inputs are pinned; the output `count = _count + 1` is uniquely forced.

2. **Enum state transition** (`test_02_counter.ev`, claim `sat_count_emits_int_to_str`): given `{state: Enum(Count(3))}` → bindings contains `effects: SeqEnum([Enum(IntToStr(3))])`. The state is a payload enum pinned via `Value::Enum`; the effects list is the sole output, uniquely determined by the match expression.

3. **Boolean given forcing UNSAT** (negative fixture): given `{state: Enum(Format(3)), state_next: Enum(Count(4))}` with claim `unsat_count_increments` from `test_02_counter.ev` → `satisfied: false`. The transition constraints rule out `Count(4)` as the next state from `Format(3)`. Demonstrates that a fully-pinned inconsistent assignment fails the solve cleanly.
