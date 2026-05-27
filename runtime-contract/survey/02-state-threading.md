# State threading across ticks

## Contract

Each FSM tick is a pure constraint query. State is not mutated in place; it is
threaded as explicit pinned inputs. After every solved tick the scheduler reads
`state_next` out of the model bindings and encodes it to a Z3 Datatype via
`encode_state_value` (`effect_loop/state.rs`); on the next tick that Datatype is
supplied as a hard equality pin on the variable named by `fsm.state_var` (the
`pins` vec in `run_scheduler`). Similarly, all non-underscore, non-`is_first_tick`
bindings from the solved model are stored in `FsmRt::prev_values`; at the start
of the following tick each `_name` body membership is satisfied by inserting
`prev_values["name"]` — and the per-field flattened mirror for record-valued vars
— into the `fsm_view` given map passed to the solver. On tick 0 `prev_values` is
empty, so no `_name` pin is added and `is_first_tick` is pinned to `true`.


## Two mechanisms

### (a) Enum `state` / `state_next` pair

At load time, `inject_fsm_params` (self-hosted in `stdlib/passes/inject.ev` as
`fsm_params_build`) injects `state_next ∈ T` into the body if `state_next` is
referenced but not yet declared.  At run time:

1. **Tick 0 seed.** If the enum's first variant is nullary, the scheduler calls
   `seed_state` (`run_scheduler` lines 39-62) and stores the result as
   `FsmRt::current_state` (Z3 Datatype) and `FsmRt::current_state_v` (Value).
   If the first variant has a single Int payload (for spawned FSMs), `seed_state_with_arg`
   is called instead. Without this, Z3 would freely pick any variant — often a
   terminal one — on tick 0.

2. **Pin at query time.** Each tick, if `fsm.state_var` and `current_state` are
   `Some`, one `(&state_var_name, current_state_dt)` entry is added to the `pins`
   vec (`run_scheduler` line 220-223). This becomes a hard Z3 equality.

3. **Capture.** After the query, `state_next_val` is extracted from
   `r.bindings[state_next_var]` (lines 300-305). `encode_state_value` converts it
   back to a `z3::ast::Datatype` for next tick's pin, and the decoded `Value` is
   stored in `current_state_v` for halt-detection comparison (lines 351-360).

`encode_state_value` handles nullary variants (`ctor.apply(&[])`) and payload
variants by converting each `Value` field to a `z3::ast::Dynamic` and calling
`ctor.apply(&refs)`. Nested enum payloads recurse.

### (b) `_var` time-shift and `is_first_tick`

At load time, `inject_prev_tick_decls` (self-hosted as `prev_tick_build` in
`inject.ev`) injects `_var ∈ T` and `is_first_tick ∈ Bool` memberships for every
`_name` reference found in the body.  The Rust shim (`inject.rs`) decides which
(`_name`, type) pairs to inject; `prev_tick_build` constructs the `BIMembership`
nodes and conditionally prepends `is_first_tick ∈ Bool`.

At run time (`run_scheduler` lines 242-268):

```
let is_first = fsm_rt[idx].prev_values.is_empty();
for item in &claim.body {
    if let Membership { name, .. } = item, name starts with '_' {
        stripped = name.strip_prefix('_')
        if let Some(prev) = prev_values.get(stripped) {
            fsm_view.insert(name, prev.clone())      // scalar pin
        }
        // Record flattening: mirror stripped.<field> → _name.<field>
        for (k, v) in &prev_values {
            if k starts_with "stripped." {
                field = k.strip_prefix("stripped.")
                fsm_view.insert(format!("{name}.{field}"), v.clone())
            }
        }
    }
}
if sees_underscore {
    fsm_view.insert("is_first_tick", Value::Bool(is_first))
}
```

`is_first_tick` is `true` when `prev_values` is empty — which is only the case
on tick 0, because after every tick all non-`_`-prefixed, non-`is_first_tick`
bindings are written into `prev_values` (lines 362-366).


## Records: per-field mirror loop

When `_pos` is referenced and `pos` was bound in the previous tick, the scheduler
not only inserts `_pos → prev_values["pos"]` (if present as a scalar) but also
iterates all keys in `prev_values` that start with `"pos."` and mirrors them as
`"_pos.<field>"` (lines 254-258 of `run_scheduler`). This means `_pos.x` and
`_pos.y` are individually pinnable in static tests (as demonstrated in
`test_22_prev_record.ev` `sat_independent_field_pins`). The loop is:

```rust
let prefix = format!("{stripped}.");
for (k, v) in &fsm_rt[idx].prev_values {
    if let Some(field) = k.strip_prefix(&prefix) {
        fsm_view.insert(format!("{name}.{field}"), v.clone());
    }
}
```


## Terse vs explicit form: `unify_state_syntax`

`unify_state_syntax` (`runtime/src/runtime/desugar.rs` line 138) rewrites the
terse single-param form to the explicit pair the scheduler and `run`/`halts_within`
machinery consume. Conditions for rewriting a candidate `X ∈ T` param:

- Schema is declared `fsm` (not `claim`/`type`) and not `external`.
- `X` is at a param-count position (index < `param_count`).
- `X` is not `world` / `world_next` (owned by `unify_world_syntax`).
- No explicit `X_next` already declared.
- For primitive types (`Int`/`Bool`/`Real`/`String`): also requires `halt ∈ Bool`
  to be present; without `halt`, a bare primitive param is treated as a plain
  self-feedback variable, not paired.
- The body must actually reference `_X` (or `_X.field`); candidate vars with no
  underscore reference are skipped.

The one-pass rewrite:
- `_X` → `X` (read previous tick's value)
- `_X.rest` → `X.rest` (record field read)
- `X` → `X_next` (write this tick's value)
- `X.rest` → `X_next.rest` (record field write)

Then `X_next ∈ T` is injected at `param_count` (first non-param slot). The
explicit `X, X_next ∈ T` pair still loads unchanged (back-compat; `inert_on_explicit_pair`
unit test at line 372 of `desugar.rs`).

`halt` reads the **input tick** (`_count`) in the rewritten form. Example from
the CLAUDE.md canonical terse FSM:

```evident
fsm decrement(count ∈ Int, halt ∈ Bool)
    count = _count - 1
    halt  = (_count ≤ 0)
```

After `unify_state_syntax`: `_count → count` (input), `count → count_next`
(output), `count_next ∈ Int` injected. So `halt = (count ≤ 0)` reads the
**input** tick's `count`, not the output `count_next`.


## Invariants

**Tick 0 (`is_first_tick = true`)**

- `FsmRt::prev_values` is empty → no `_name` pins are added to `fsm_view`.
- `is_first_tick` is inserted as `Value::Bool(true)` into `fsm_view` whenever
  any `_name` membership exists in the body.
- The enum state seed is pinned via `pins` (hard equality on `state`), defaulting
  to the first nullary variant. If no state enum: no pin.

**Subsequent ticks (`is_first_tick = false`)**

- `prev_values` is non-empty → `_name` pins are inserted for every key found.
- `is_first_tick` is `false`.

**Bindings captured for next tick (lines 362-366)**

```rust
for (k, v) in r.bindings.iter() {
    if k.starts_with('_') { continue; }   // skip _name vars themselves
    if k == "is_first_tick" { continue; } // never carry forward
    fsm_rt[idx].prev_values.insert(k.clone(), v.clone());
}
```

All other solved bindings — `count`, `pos.x`, `pos.y`, `state`, `state_next`, etc.
— are stored. The underscore skip prevents double-indirection (`__count` would
arise otherwise). `is_first_tick` is always recomputed from `prev_values.is_empty()`.


## Fixture candidates

1. **test_19 / `sat_first_tick_count_is_zero`** — Pin `is_first_tick = true`,
   `state = Counting`, invoke `counter`. Assert `count = 0`. Exercises tick-0
   path: `(is_first_tick ? 0 : _count + 1)` takes the left branch; `_count` pin
   is absent and irrelevant.

2. **test_19 / `sat_subsequent_tick_increments`** — Pin `is_first_tick = false`,
   `_count = 7`, `state = Counting`, invoke `counter`. Assert `count = 8`.
   Exercises the `_var → prev_values` pin path: `_count` is supplied as 7, the
   ternary takes the right branch, `count = 7 + 1 = 8`.

3. **test_22 / `sat_independent_field_pins`** — Pin `is_first_tick = false`,
   `_pos.x = 7`, `_pos.y = 11`, invoke `walker`. Assert `pos.x = 8 ∧ pos.y = 13`.
   Exercises the per-field record mirror loop: `_pos.x` and `_pos.y` are
   individually pinned from `prev_values["pos.x"]` and `prev_values["pos.y"]`,
   independently of the record-level `_pos` key.

4. **test_02 / `sat_start_seeds_count_five`** — Pin `state = Start`, invoke
   `counter`. Assert `state_next = Count(5)`. Exercises the explicit enum
   `state` / `state_next` pair without `_var` at all: state is pinned via the
   `pins` vec, `state_next` is solved, `encode_state_value` converts
   `Count(5)` (payload variant) to a Z3 Datatype for next tick.

5. **test_02 / `sat_format_one_goes_done`** — Pin `state = Format(1)`, invoke
   `counter`. Assert `state_next = Done`. Exercises the `n ≤ 1` branch of the
   match: demonstrates that payload-variant pins (`Format(Int)`) round-trip
   correctly through `encode_state_value` → pin → solved model.
