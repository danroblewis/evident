# Tick — one transition

## Contract

A single FSM tick is a Z3 satisfiability query: given the FSM's previous state (pinned as a Z3 Datatype assertion), the previous tick's `last_results` (encoded as a `Seq(Result)` given), the current world snapshot (scalar givens keyed by `world.<field>`), and all `_name` time-shifted previous-tick bindings (scalar givens keyed by the underscore name), the solver finds an assignment satisfying the FSM's body constraints. From that assignment the scheduler extracts `state_next` (the new enum value, which becomes the pin on the next tick), the `effects` list (dispatched to I/O immediately after all FSMs in the tick have solved), and any other model bindings (stored as `prev_values` for next-tick `_name` pinning). A tick that returns UNSAT halts the scheduler with `halted_clean: false`; a tick returning SAT with effects containing `Exit(code)` halts cleanly after dispatching the current tick's full effect batch.

## Primitive: the entry point

**File:** `runtime/src/runtime/scheduler_api.rs`, line 20

```rust
pub fn query_with_pins_and_given(
    &self,
    claim_name: &str,
    pins: &[(&str, z3::ast::Datatype<'static>)],
    given: &HashMap<String, Value>,
) -> Result<QueryResult, RuntimeError>
```

- `claim_name`: the FSM's schema name (e.g. `"hello"`, `"counter"`).
- `pins`: exactly 0 or 1 entry — `[(state_var_name, current_state_as_Z3_Datatype)]`. Built by the scheduler at `scheduler.rs:220–223`. Empty when the FSM has no state var.
- `given`: the full `fsm_view` HashMap — world snapshot fields (`world.<field>`), FTI keys, `last_results_var` value, `_name` time-shift entries, `is_first_tick`, and the current state as `Value::Enum` (redundant with `pins` but needed by the JIT fast-path).
- Return: `QueryResult { satisfied: bool, bindings: HashMap<String, Value> }` — all free Z3 variables the solver assigned.

Three solve paths are tried in order: (1) Cranelift JIT compiled function (`try_functionize_z3`), (2) slow-path cached solver with push/assert/check/pop (`slow_path_cache`), (3) fresh `evaluate_with_extra_assertions`. All three paths honour the same pins and given.

## Inputs

Everything that conditions one tick, assembled into `fsm_view` at `scheduler.rs:226–274` before calling `query_with_pins_and_given`:

| Input | Key in `given` / `pins` | Source |
|---|---|---|
| Previous state (enum) | `pins[0] = (state_var, Z3Datatype)` + `given[state_var] = Value::Enum{…}` | `fsm_rt[idx].current_state` + `current_state_v` |
| Previous effect results | `given[last_results_var] = Value::Enum{…}` (a `Seq(Result)` as Cons-list) | `fsm_rt[idx].last_results` encoded via `effect_results_to_value` |
| World shared state | `given["world.<field>"] = Value::…` | `world_snapshot` HashMap |
| FTI resource keys | `given["<param>.<field>"] = Value::…` | FTI plugin writes; prefix-stripped into bare param keys |
| `_name` time-shift | `given["_name"] = prev_val` | `fsm_rt[idx].prev_values` from prior tick's bindings |
| `is_first_tick` | `given["is_first_tick"] = Value::Bool(prev_values.is_empty())` | Injected when any `_name` item exists in body |

## Outputs

Extracted from `r.bindings: HashMap<String, Value>` returned by `query_with_pins_and_given`:

| Output | Binding key | Fate |
|---|---|---|
| Next state (enum) | `fsm.state_next_var` (e.g. `"state_next"`) | `encode_state_value` → new `current_state` pin; value stored in `current_state_v` |
| Effects list | `fsm.effects_var` (e.g. `"effects"`) | Extracted by `collect_dispatchable_effects`, dispatched via `dispatch_all` after all FSMs solve |
| All other bindings | any key not starting with `_` | Stored in `fsm_rt[idx].prev_values` for next-tick `_name` pinning (`scheduler.rs:363–367`) |

## Invariants / edge cases

**Tick-0 state seeding (nullary-first-variant rule):** Before the loop starts, `seed_state` (scheduler.rs:39–62) looks up the FSM's state enum. If the first variant is nullary (arity 0), it is applied as the initial `current_state` Datatype and `current_state_v`. If the first variant carries a payload, seeding returns `(None, None)` and Z3 picks freely on tick 0 — which typically selects a minimal/Done variant, causing immediate halt. This is documented as a known footgun in test_02_counter.ev (the `Start` nullary workaround comment). Spawned FSMs with an Int-payload first variant use `seed_state_with_arg` (state.rs:14–35) to seed `FirstVariant(spawn_arg)`.

**FSM without a state var:** If `resolve_fsm` finds no `(state_var, state_next_var)` pair, `MainShape.state_var` is `None`, `pins` is empty (`scheduler.rs:220–223`), and no state update occurs. The FSM ticks on bootstrap and on any world/effect/external-event wake, but has no persistent state of its own.

**UNSAT handling:** If `r.satisfied` is false, the scheduler logs the FSM name and tick number, then returns `LoopResult { halted_clean: false, … }` immediately (`scheduler.rs:284–298`). No effects are dispatched for that tick.

**`Exit(code)` dispatch and graceful halt:** `dispatch_all` sets `ctx.exit_requested = Some(code)` when it sees `Effect::Exit`. The scheduler checks `ctx.exit_requested` after each full tick (all FSMs solved + all effects dispatched) at `scheduler.rs:443–456`. All of the current tick's effects run before the process exits.

**Halt by quiescence:** When no FSM was scheduled in a tick and no async event is pending, the scheduler returns `halted_clean: true` (`scheduler.rs:459–498`). The `Done`/`Halt` variant name is a legacy check only used for display; quiescence is the actual halt signal.

**World-writer ordering:** Writer FSMs solve first (they appear first in `all_fsms` output); their `world_next.<field>` bindings update `world_snapshot` before reader FSMs solve the same tick (`scheduler.rs:326–349`).

**`_name` on first tick:** `is_first_tick = true` when `fsm_rt[idx].prev_values` is empty (tick 0). The FSM body typically gates initialization with `(is_first_tick ? initial_value : _name + delta)`.

## Fixture candidates

1. **hello / Init → Done + exit effects** (simplest possible tick)
   - FSM: `hello` (`examples/test_01_hello.ev`)
   - Claim: `sat_init_advances_to_done` + `sat_init_emits_greeting_and_exit`
   - Pin: `state = Init`, `last_results = ⟨⟩`
   - Expected: `state_next = Done`, `effects = ⟨Println("hello from evident"), Exit(0)⟩`

2. **hello / Done → Done + empty effects** (absorbing-state tick; verifies quiescence)
   - FSM: `hello` (`examples/test_01_hello.ev`)
   - Derived from `unsat_done_returns_to_init`
   - Pin: `state = Done`, `last_results = ⟨⟩`
   - Expected: `state_next = Done`, `effects = ⟨⟩`

3. **counter / Start → Count(5)** (nullary-seed → payload-state in one tick)
   - FSM: `counter` (`examples/test_02_counter.ev`)
   - Claim: `sat_start_seeds_count_five`
   - Pin: `state = Start`, `last_results = ⟨⟩`
   - Expected: `state_next = Count(5)`, `effects = ⟨Println("starting count")⟩`

4. **counter / Count(3) → Format(3) + IntToStr effect** (payload-state in + effect carrying payload)
   - FSM: `counter` (`examples/test_02_counter.ev`)
   - Claim: `sat_count_emits_int_to_str`
   - Pin: `state = Count(3)`, `last_results = ⟨⟩`
   - Expected: `state_next = Format(3)`, `effects = ⟨IntToStr(3)⟩`

5. **exit_demo / Init → Done + Exit(42)** (non-zero exit code propagation)
   - FSM: `exit_demo` (`examples/test_08_exit_code.ev`)
   - Claim: `sat_init_exits_42`
   - Pin: `state = Init`, `last_results = ⟨⟩`
   - Expected: `state_next = Done`, `effects = ⟨Println("exiting with code 42"), Exit(42)⟩`

6. **counter / Format(1) → Done** (boundary: last-countdown step to Done)
   - FSM: `counter` (`examples/test_02_counter.ev`)
   - Claim: `sat_format_one_goes_done`
   - Pin: `state = Format(1)`, `last_results = ⟨StringResult("1")⟩` (any StringResult)
   - Expected: `state_next = Done`, `effects = ⟨Println("tick …")⟩`
