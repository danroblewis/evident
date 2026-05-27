# Multi-FSM coordination via shared world

## Contract (1 paragraph)

Multiple FSMs coordinate through a single shared `World` record whose fields are partitioned by ownership: each field has exactly one writer FSM (or plugin) and arbitrarily many reader FSMs. The writer outputs `world_next.X = ...` in its solve result; the scheduler merges those values into a global `world_snapshot` (keyed `"world.X"`) after the writer's tick. Reader FSMs receive the updated snapshot on their next scheduled tick via a `fsm_view` built from `world_snapshot`. FSMs are subscription-driven: a reader FSM is woken when any field in its statically-inferred read-set appears in `pending_changes[j]`, which the scheduler populates whenever a writer changes a field that reader subscribes to. Writers run before readers within the same tick (declaration-order iteration, writers first in the FSM list), so a reader that ticks in the same step as its writer sees the writer's freshly-merged values. The previous tick's world value is accessed inside any FSM body as `_world.X` (desugared to a `_world.X` pin in `fsm_view` from the FSM's own `prev_values` map). The single-owner invariant is checked at load time in `check_single_owner` (`effect_loop/mod.rs`); violation is a hard error naming both conflicting writers and the shared field.

---

## World model: `type World`; one writer per field; `world.X` / `world_next.X`

A `type World` record declares the shared mutable state. Each field `X` has a type. Writer FSMs declare `world, world_next ∈ World` (legacy form) or `world ∈ World` with `world.X = ..._world.X + ...` (terse `_world`/`world` form). The scheduler keys world snapshot entries as `"world.X"` (and `"world.X.subfield"` for nested records).

**Snapshot merge** (`scheduler.rs` lines 325–348): after a writer FSM's Z3 solve returns, the scheduler iterates `r.bindings` looking for keys matching `"world_next."`. For each such key, the top-level field name (first segment) is checked against the FSM's write-set (`access_sets[idx].writes`); if present, the value is inserted into `world_snapshot` under the key `"world.{field}"`. If the value differs from the prior snapshot entry, the field name is added to `just_changed`, then propagated into `pending_changes[j]` for every other FSM whose read-set contains that field.

**`world.X` / `world_next.X` naming convention**: the scheduler stores snapshot values under `"world.X"` keys. When building `fsm_view` for a reader, it passes the full `world_snapshot` (which contains these keys), so `world.X` in a reader's body resolves directly. For `_world.X`, the per-FSM `prev_values` map (populated from `r.bindings` after each tick, lines 363–367) carries `"world.X"` → previous value; the `_var` time-shift logic in lines 244–268 mirrors `"world.X"` entries from `prev_values` into `fsm_view` as `"_world.X"`.

---

## Subscription waking: `pending_changes`, read-set match

Wake triggers for a non-bootstrap FSM (lines 203–217 of `scheduler.rs`):

```rust
let woken = had_effects_last[idx]    // (2) self-feedback: emitted ≥1 effect last tick
    || !pending_changes[idx].is_empty() // (3) world delta: a read-set field changed
    || state_changed_last[idx]          // (4) state self-feedback: transitioned last tick
    || external_event[idx];             // (5) async plugin event fired
```

On tick 0 every FSM is bootstrapped unconditionally (the `if step_count > 0` gate). On subsequent ticks, `pending_changes[idx]` accumulates field names that were written by another FSM (or plugin) and appear in FSM `idx`'s read-set. `pending_changes[idx]` is cleared when the FSM is scheduled (`pending_changes[idx].clear()` at line 215), so the wake signal is edge-triggered: a field value that stays constant across ticks does not re-wake a reader.

Read-set inference (`subscriptions.rs` + `stdlib/passes/subscriptions.ev`): `AccessSets.reads` contains field names `X` where `world.X` appears in the claim body (transitively, including passthroughs and subclaims). The transitive walk is done once at startup via `fsm::full_world_access` and cached as `initial_access`; spawned FSMs compute their own sets at spawn time.

---

## Single-owner rule: `check_single_owner` at load

`check_single_owner` (`effect_loop/mod.rs` lines 276–309) enforces "the relation world-field → writer is a partial function." It builds a `HashMap<&str, &str>` (field → first writer seen) by iterating all writer FSMs and plugins in declaration order, sorted by field name for deterministic error messages. The first field that gets a second owner triggers:

```
multi-FSM: writers `{prev}` and `{name}` both write to world fields {shared:?}. Each world field must have at most one writer (single-owner rule). Fix by either: (1) merging the two FSMs into one writer for that field, (2) splitting the field so each writer owns a distinct one, or (3) making one FSM a reader ...
```

Plugin writes (`plugin_writes: HashSet<String>`) are included in the check as synthetic writer entries named `"<plugin>:{field}"`, so a user FSM that also writes a plugin-owned field (e.g. `tick_count`) is rejected. The check runs after access-set computation but before the scheduler loop starts.

---

## Scheduling within a tick: writers before readers

FSMs are iterated in `fsms` slice order (`for (idx, fsm) in fsms.iter().enumerate()`). Writers (FSMs with `world_next` in their write-set, i.e. `fsm.is_writer()`) produce bindings that are merged into `world_snapshot` immediately within the same `for` loop pass (lines 326–348). Because there is no separate "writer phase / reader phase" separation — just a single iteration — the ordering guarantee is: **a reader FSM scheduled later in the slice sees the writer's merged values if and only if the writer appears earlier in the slice**. In `test_09_two_fsms.ev`, `producer` is declared before `consumer`, so consumer's `fsm_view` on the same tick contains producer's freshly-merged `world.n`.

Implication: declaration order matters for same-tick visibility. A reader declared before its writer in the same file will see the previous tick's world value on the tick the writer first writes, not the new one.

---

## Fixture candidates

1. **Writer single-tick output** (capturable as a sat_ claim — test_09 already has this):
   Pin `state = PTick(3)` on `producer`; assert `world_next.n = 3`. This is a pure single-FSM Z3 query; no cross-FSM coordination needed. See `claim sat_producer_writes_n` in `test_09_two_fsms.ev`.

2. **Reader single-tick output given world value** (capturable as a sat_ claim — test_09 already has this):
   Pin `world.n = 7`, `state = CWait` on `consumer`; assert `effects = ⟨IntToStr(7)⟩`. Again a pure single-FSM query. See `claim sat_consumer_emits_int_to_str_when_n_positive` in `test_09_two_fsms.ev`.

3. **Single-owner rejection** (capturable as a load-time error test):
   Load two FSMs both writing the same world field and assert that `run_with_ctx` returns `Err` containing `"single-owner rule"`. This exercises `check_single_owner` directly; no tick needed. Covered by `mod.rs` unit tests `single_owner_rejects_shared_field_naming_both_writers` and `single_owner_rejects_plugin_vs_fsm_writer`.

4. **Wake subscription** (partially capturable):
   After one writer tick that changes `world.n`, assert that the reader's `pending_changes` would be non-empty. This is only indirectly observable from outside: run a two-FSM program for 1 step and check that the reader FSM produced output on tick 1 (i.e. it was scheduled). Requires a full `run_with_ctx` call with output capture — not a single-claim sat_ query.

5. **`_world.X` previous-tick value** (capturable as a sat_ claim within a single FSM):
   Write an `fsm` with `world.n = _world.n + 1`, pin `is_first_tick = false`, pin `_world.n = 5`, assert `world.n = 6`. This is a single-tick Z3 query but requires the `_world` desugaring and `prev_values` pin infrastructure to be exercised at the query layer, not just at Z3 level.

### Contract TODOs (not capturable as single-tick sat_ fixtures)

- **Cross-FSM wake propagation**: the causal chain `writer writes X → pending_changes[reader] gets X → reader is scheduled` spans two FSMs and two ticks. No single-claim query captures it; a fixture needs a multi-step integration run.
- **Declaration-order same-tick visibility**: whether a reader sees a writer's value on the same tick depends on FSM order in the loaded schema slice, which is a scheduler-internal ordering property. Not expressible as a sat_ claim.
- **Bootstrap tick**: every FSM ticks on step 0 unconditionally; this is a scheduler invariant with no single-claim expression.
- **Plugin-owned field waking**: async event sources writing world fields trigger `pending_changes` via the plugin path (lines 182–193 of `scheduler.rs`); this requires a live event source thread and cannot be a static sat_ fixture.
