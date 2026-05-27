# Halt semantics

## Contract (1 paragraph)

A multi-FSM program running under `evident effect-run` halts in exactly one of
four ways. (a) **All-FSMs-halted**: at the top of every tick the scheduler
checks `fsm_rt.iter().all(|f| f.halted)`; when true it returns immediately with
`halted_clean: true` (scheduler.rs:129–142). (b) **Implicit no-progress halt**:
after solving and dispatching all scheduled FSMs, if `scheduled_this_tick` is
all-false and `pending_world_writes` is empty, the scheduler blocks on the async
event channel if one exists; if no event arrives (or no sources are installed)
it returns with `halted_clean: true` (scheduler.rs:459–498). (c)
**`Effect::Exit(code)` graceful halt**: `Effect::Exit` is dispatched like any
other effect — it sets `ctx.exit_requested = Some(n)` but does NOT interrupt
the current tick (effect_dispatch.rs:194–200). After all effects for that tick
are dispatched the scheduler checks `ctx.exit_requested.is_some()` and returns
`halted_clean: true, exit_code: Some(n)` (scheduler.rs:443–456). All
co-scheduled FSMs' cleanup effects therefore run before the process exits. (d)
**`max_steps` ceiling**: `while step_count < opts.max_steps` exhausts with
`halted_clean: false` (scheduler.rs:507–512). UNSAT on any single FSM is also
an early-exit with `halted_clean: false` (scheduler.rs:284–298).

## The three halt paths

### (a) All FSMs halted — `halted_clean: true`

At the top of every iteration (before any solving), the scheduler checks
whether every FSM in `fsm_rt` has its `halted` flag set. When true it returns
immediately:

```
// scheduler.rs:129–141
if fsm_rt.iter().all(|f| f.halted) {
    return Ok(LoopResult {
        steps: step_count,
        final_state: ...,
        halted_clean: true,
        exit_code: ctx.exit_requested,
    });
}
```

This path fires when every `fsm` schema has been individually marked halted by
the state-machine machinery (e.g. absorbing Done state that emits nothing,
making the FSM un-schedulable forever and eventually marked halted by the
scheduler's wake logic).

### (b) No FSM scheduled this tick — implicit halt, `halted_clean: true`

After the solve-and-dispatch pass, the scheduler checks whether any FSM was
actually scheduled:

```
// scheduler.rs:459–498
if scheduled_this_tick.iter().all(|s| !s) && pending_world_writes.is_empty() {
    if let Some(rx) = event_rx {
        match rx.recv() {
            Ok(SchedulerEvent::Tick { name }) => { /* wake matching FSMs; continue */ }
            Ok(SchedulerEvent::Closed { .. }) | Err(_) => { /* fall through to halt */ }
        }
    }
    return Ok(LoopResult { ..., halted_clean: true, exit_code: ctx.exit_requested });
}
```

There is no `Done` or `Halt` name convention, no fixpoint heuristic. The
program halts because nothing happened — no FSM had a trigger (world delta,
self-feedback, state change, or external event) to be scheduled. This is the
"implicit halt" described in the CLAUDE.md "Halt is implicit" section.

### (c) `Effect::Exit(code)` — graceful, `halted_clean: true`

`Effect::Exit` is handled in `effect_dispatch.rs:194–200`:

```rust
Effect::Exit(n) => {
    // Deferred: effect loop checks at end-of-tick so co-scheduled FSMs can finish.
    if ctx.exit_requested.is_none() {
        ctx.exit_requested = Some(*n as i32);
    }
    EffectResult::NoResult
}
```

The dispatch continues through the rest of the current tick's effects. After
`dispatch_all` returns, the scheduler checks (scheduler.rs:443–456):

```rust
// Exit takes priority over halt/event-wait.
if ctx.exit_requested.is_some() {
    return Ok(LoopResult {
        steps: step_count,
        halted_clean: true,
        exit_code: ctx.exit_requested,
    });
}
```

First `Exit` wins (subsequent `Exit` effects in the same tick are ignored).

### (d) `max_steps` ceiling — `halted_clean: false`

`while step_count < opts.max_steps` (default 10,000 per `LoopOpts::default`).
When the loop exits naturally, returns `halted_clean: false` (scheduler.rs:507–512).
This is the "infinite loop guard" path; no production program should hit it.

### UNSAT — `halted_clean: false`

If `rt.query_with_pins_and_given` returns `!r.satisfied` for any FSM on any
tick, the scheduler returns immediately with `halted_clean: false`
(scheduler.rs:284–298). This indicates a constraint violation, not a clean halt.

## What "implicit halt" means

There is no `Done` or `Halt` name convention anywhere in the runtime. There is
no fixpoint heuristic (no "did the world state change?"). The program halts
when a tick completes and no FSM was scheduled, with no pending async event
to wake one. The relevant code is scheduler.rs:459:

```rust
if scheduled_this_tick.iter().all(|s| !s) && pending_world_writes.is_empty() {
```

In practice an FSM becomes permanently un-schedulable when:
- Its state is absorbing (e.g. `Done → Done`) and emits no effects
  (`had_effects_last` stays false), so it never satisfies any wake trigger.
- No other FSM writes to world fields it reads (`pending_changes` stays empty).
- No async sources are installed (or all sources have gone dead).

The `Done` enum variant that appears in examples (test_01, test_02) is purely
a user-chosen name; the runtime ignores it. An FSM in `Done` state that emits
`Exit(0)` halts via path (c); one that emits nothing halts via path (b).

## `exit_code` propagation

```
Effect::Exit(n)                     (emitted in FSM effects list)
  → effect_dispatch.rs:197          ctx.exit_requested = Some(n as i32)
  → scheduler.rs:451                LoopResult { exit_code: ctx.exit_requested, ... }
  → effect_loop/mod.rs:79           pub exit_code: Option<i32>
  → commands/effect_run.rs          process::exit(exit_code.unwrap_or(0))
```

The field `LoopResult.exit_code: Option<i32>` is documented in
effect_loop/mod.rs:78–79: `"Some(code) iff a FSM emitted Effect::Exit; set at
end-of-tick."` A clean implicit halt (paths a, b) returns `exit_code:
ctx.exit_requested`, which is `None` unless a prior tick set it — so process
exit is 0. Only the `max_steps` and UNSAT paths can return `halted_clean:
false`; the CLI maps those to a non-zero exit code independently of
`exit_code`.

## Fixture candidates

The following are the highest-priority fixtures for the behavior-contract
capture. Each should appear in the portable contract as a tick-level snapshot
with a `halt: true` metadata flag.

1. **`test_08_exit_code.ev` — `Exit(42)` graceful halt (path c).** The
   canonical exit-code fixture. State `Init`, effects `⟨Println("exiting with
   code 42"), Exit(42)⟩`. The existing `sat_init_exits_42` claim pins state +
   effects exactly. Fixture should assert: `effects` contains `Exit(42)`,
   `halt: true`, `exit_code: 42`. This is the load-bearing fixture for
   path (c) — "halt must be representable in the portable contract" via the
   `Exit` variant.

2. **`test_01_hello.ev` — `Exit(0)` one-shot then implicit halt (paths c + b
   combined).** State `Done`, effects `⟨⟩`, no wake triggers — the basis for
   path (b) implicit halt detection. The existing `unsat_done_returns_to_init`
   and `sat_init_emits_greeting_and_exit` claims cover the transition. Add a
   fixture: state `Done`, effects `⟨⟩`, assert `halt: true` (implicit —
   nothing to schedule). This pin exercises the "emits nothing in absorbing
   state → scheduler has no trigger" logic (scheduler.rs:203–214).

3. **`test_02_counter.ev` Done-state steady-state — no-effects absorbing tick
   (path b basis).** State `Done`, effects `⟨Println("bye"), Exit(0)⟩` on the
   last real tick, then `Done → Done` with `⟨⟩` afterward. Add a fixture
   pinning `Done` state, asserting effects = `⟨⟩` and `halt: true`. The
   existing `sat_format_one_goes_done` covers the transition-to-Done; the new
   fixture would cover the absorbing-Done-emits-nothing case — the canonical
   proof that path (b) fires.

4. **UNSAT fixture (path d).** No existing example deliberately exercises the
   UNSAT return path. A fixture could be a claim with contradictory constraints
   (`count ∈ Int = 5 ∧ count ∈ Int = 6`) pinned as a "state that should be
   unsatisfiable." Fixture metadata: `halted_clean: false`. Lower priority
   since it requires a deliberately broken FSM body.
