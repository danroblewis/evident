# FSM Subscription Scheduler

Status: design (no code yet)
Supersedes the halt mechanism in `docs/design/multi-fsm.md`.

## Motivation

The multi-FSM scheduler currently ticks every alive FSM on every
iteration. Halt is a heuristic — either a name convention
(`Done`/`Halt` variants) or a fixpoint check (`state_next == state ∧
effects = ⟨⟩`). Both are proxies for the real question:

> Does this FSM have any possible future input?

If the answer is no, the FSM has nothing to compute over and should
not be scheduled. If the answer is yes, it might tick — but only
when one of those inputs actually changes.

**The reframe**: an FSM is an I/O agent. It blocks naturally when its
inputs are absent. The runtime is a scheduler that wakes FSMs when
their declared inputs produce events. Halt becomes implicit — no
inputs going forward means no schedule, no compute.

This unifies several concerns that were ad-hoc:

  * Halt — no longer a name convention or a fixpoint heuristic. An
    FSM is "done" when none of its subscriptions can fire again.
  * Polling — an FSM waiting on stdin doesn't burn cycles spinning
    in `Idle`. The runtime simply doesn't tick it until stdin has
    bytes.
  * Cleanup — an FSM that subscribes to a `Shutdown` event source
    sleeps until the runtime fires that source on program exit.
  * Parallelism — FSMs whose subscriptions don't overlap could be
    scheduled concurrently (deferred).

## The model

An FSM has zero or more **subscriptions**. Subscriptions come from
four places, all known statically at load time:

  1. **World read-set** — every `world.X` referenced anywhere in the
     FSM body. Auto-inferred from the AST. Fires when the field is
     written by some other FSM.
  2. **Plugin event sources** — declared via type membership in the
     FSM signature: `∈ Stdin`, `∈ FrameTimer`, etc. Each plugin owns
     a set of event types and decides when its events fire.
  3. **Self-feedback (effects)** — when an FSM emits effects, the
     dispatched results flow into its `last_results` on the next
     tick. Implicit; present iff the FSM emitted effects.
  4. **Self-feedback (state)** — when an FSM transitions to a new
     state value (`state_next ≠ state`), it's scheduled again next
     tick. The body can compute different things when state pins to
     a new value, even if world and last_results are unchanged.

(Only #1 and #2 are "external" subscriptions in the strict sense.
#3 and #4 are intra-FSM signals that ensure the FSM observes its
own past actions before going quiet. Without #4, an FSM that does
`Idle → Frame(N)` silently on one tick would never run its
`Frame(N)` body.)

Plus one **bootstrap event** that fires once at tick 0, scheduling
every FSM for its initial run.

A subscription is **alive** if its source can produce future events.
**Dead** if it cannot:

  * Plugin source — stdin EOF, frame timer canceled, plugin
    explicitly closed.
  * World field — the FSM that writes the field has no live
    subscriptions of its own → never ticks again → field is frozen.
    Transitive: a field written only by a dead FSM is dead.
  * Self-feedback — the FSM emitted no effects on the previous tick.

An FSM is **alive** iff at least one of its subscriptions is alive.

The **program halts** when no FSM is alive AND no plugin event source
has a pending event. There is no `Done` variant, no fixpoint detector
— just "is there any event that could schedule any FSM?"

`Effect::Exit(code)` from any FSM is the explicit kill switch,
independent of natural halt. Useful for "user pressed Q" or fatal
errors.

## Granularity: one FSM with N subscriptions vs N FSMs with one each

Two shapes are equivalent in capability:

```evident
-- Shape A: one FSM, multiple subscriptions, internal dispatch
claim main(state, state_next ∈ AppState,
           input ∈ Stdin,        -- subscription 1
           timer ∈ FrameTimer,   -- subscription 2
           ...)
    state_next = match state
        Booting   ⇒ ...
        WaitInput ⇒ (input.ready ? ProcessKey : WaitInput)
        WaitTimer ⇒ (timer.fired ? Render : WaitTimer)
        ...
```

```evident
-- Shape B: many FSMs, one subscription each, world-coordinated
claim input_handler(input ∈ Stdin,
                    world, world_next ∈ World, ...)
    -- writes world.last_key on each stdin event

claim renderer(timer ∈ FrameTimer,
               world ∈ World, ...)
    -- reads world.last_key and the timer; draws each frame
```

Runtime cost is the same (one cached Z3 model per FSM). The
difference is purely a programming model choice — Shape A
centralizes dispatch in `match`; Shape B distributes it via world
writes.

The runtime treats both identically. No special case for "one big
FSM" vs "many small FSMs."

## Implementation phases

Each phase is independently testable; phases 1–3 ship the model
end-to-end.

### Phase 1: Static read-set inference

Walk each top-level claim's AST once at load time. For every
expression of the form `world.X`, record `X` in the claim's read-set.
For `world_next.X`, record `X` in the claim's write-set.

Output: `read_sets: HashMap<String, HashSet<String>>` and
`write_sets: HashMap<String, HashSet<String>>`, indexed by claim
name.

Test: assert that for `effect_multi_fsm_transpiled.ev`,
  * `setup.write_set` is `{window, ctx, vao, prog, time_loc}`
  * `setup.read_set` is `{window, ctx, vao, prog, time_loc}` —
    same as writes, because the `Done ⇒ world.X` passthrough arm
    of each `world_next.X = match state …` reads its own field. See
    "self-write reads" below for why this matters in Phase 2.
  * `render.read_set` is `{window, prog, time_loc}` (matches what
    the body actually uses; vao and ctx are written but not read by
    render)
  * `render.write_set` is `{}` (reader only)

This is the foundation; no behavior change yet.

**Self-write reads (discovered while implementing Phase 1)**: an
FSM's read-set can include fields it itself writes, because the
common passthrough idiom `world_next.X = match state … | Other ⇒
world.X` literally reads `world.X`. Phase 2 must NOT schedule an
FSM purely on the basis of its own writes — that would create an
infinite self-loop. The rule for Phase 2 is:

  > schedule iff `read_set ∩ (changed_fields − own_writes) ≠ ∅`
  > OR self-feedback OR plugin event OR bootstrap.

Equivalently: subtract the FSM's own write-set from the changed-set
before intersecting with its read-set.

### Phase 2: World-delta scheduler

Replace the unconditional per-FSM iteration with delta-driven
scheduling.

Per tick:

  1. Compute `changed_fields = {f ∣ world_next.f ≠ world.f}` from
     the previous tick's writer solve. On tick 0, `changed_fields`
     is "everything" (bootstrap).
  2. For each FSM, schedule it iff
     `read_set ∩ (changed_fields − own_writes) ≠ ∅` OR
     it has effect-feedback pending (last tick emitted effects) OR
     it has state-feedback pending (last tick changed state) OR
     bootstrap (tick 0).
  3. Solve scheduled FSMs in declaration order; writer first if it's
     scheduled.
  4. Dispatch effects; capture results into per-FSM `last_results`.
  5. Update `world` from `world_next`; remember which fields
     changed for the next tick's scheduler decision.

Live-set maintenance:

  * An FSM remains alive as long as it is scheduled or has any
    plugin subscription.
  * An FSM that is not scheduled this tick AND has no plugin
    subscriptions AND its read-set is entirely fields-frozen →
    becomes dead. Frozen = field's writer is dead.

Halt: program halts when no FSM is alive AND no plugin source has
events.

Test: the 4 existing multi-FSM tests pass with the new scheduler.
In `effect_multi_fsm_transpiled.ev`, setup is scheduled exactly twice
(tick 0 bootstrap + tick 1 because effects from tick 0 fed back via
self-feedback) and never again — render keeps ticking because of
self-feedback (its frame_seq emits effects each tick).

### Phase 3: drop fixpoint halt in delta mode + new halt criterion

  * In `delta_mode`, the value-equality fixpoint halt
    (`state_next == state ∧ effects == ⟨⟩`) is suppressed. FSMs
    that "would have fixpointed" instead just stop being scheduled
    (no inputs to wake them).
  * New halt criterion: at end of each tick, if no FSM was
    scheduled this tick, the program halts cleanly. Detection has
    no Z3 cost — it's a plain check on the per-FSM scheduling
    booleans.
  * `Effect::Exit(code)` already works from any FSM (the
    dispatcher calls `process::exit` regardless of source).
  * Legacy mode unchanged — fixpoint halt + all-FSMs-halted check
    still apply.
  * The single-FSM path keeps its existing fixpoint heuristic;
    delta-mode promotion happens with Phase 4.
  * The `model_matches_value` `Done`/`Halt` name special case is
    no longer reachable in multi-FSM (the multi-FSM scheduler
    never used it — that was always single-FSM only). Cleanup
    pending until single-FSM converts.

Test results (delta mode):
  * The four existing `programs/lang_tests/multi_fsm/` tests pass
    — `04_halt_cascade.ev` halts cleanly even though its variants
    are named `STDone`/`LTDone` (not exact `Done`/`Halt`), because
    halt is now no-FSM-scheduled, not name-based.
  * New `runtime-rust/tests/scheduler_delta.rs::
    delta_mode_halts_cleanly_without_done_variant` proves halt
    works with no Done-style variant at all.

### Phase 4: blocking I/O via the existing dispatch model (v1 done)

Phase 4 v1: route the single-FSM path through the multi-FSM
scheduler when delta mode is on (`run_with_ctx` checks the env
flag). This gives single-FSM programs the same subscription
semantics — including the "no-FSM-scheduled = halt" criterion
that single-FSM's fixpoint heuristic doesn't have.

Stdin block-then-halt works without a new plugin abstraction:
  * `Effect::ReadLine` already blocks at dispatch time (synchronous
    `stdin.read_line`). Zero CPU while waiting.
  * EOF returns `EffectResult::Error` — the FSM body inspects
    `last_results` and transitions to a non-emitting state.
  * Once it stops emitting, no inputs wake it → delta-mode halt
    fires next tick.

Test: `runtime-rust/tests/scheduler_delta.rs::
delta_mode_single_fsm_stdin_reader_halts_on_eof` — feeds two
lines + EOF, asserts clean halt.

### Phase 4 v2: pluggable event sources (deferred)

A real plugin-as-event-source mechanism — plugins push events to
the scheduler, scheduler `select()`s when its ready set is empty,
sources can declare permanently dead. Needed for:
  * Frame timer that fires every N ms independent of FSM ticks.
  * Multiple FSMs where one waits on stdin while others run.
  * Signal handling (SIGINT → wake a shutdown FSM).

Not blocking the rest of the design. The current "block at
dispatch time" approach handles single-source-of-input programs
and the GL render loop pattern (delay-effect-as-pacing).

### Phase 5: Self-feedback as a first-class subscription

Phases 2–3 treat self-feedback as a special "did we emit anything?"
flag. Phase 5 unifies it: an FSM has a subscription on its own
`last_results`, fired by the dispatcher.

This is mostly internal cleanup — no new user-facing behavior.

## Worked examples

### Setup-then-render (transpiled triangle)

```
read_sets:
  setup:  {}                        — writer only
  render: {window, prog, time_loc}  — reads handles
write_sets:
  setup:  {window, ctx, vao, prog, time_loc}
  render: {}

Tick 0 (bootstrap): both scheduled.
  setup  → emits setup_seq, world_next unchanged from defaults
  render → reads world.prog == 0, idles, emits nothing
Tick 0 dispatch: setup_seq runs, last_results filled.

Tick 1: setup scheduled (self-feedback from tick 0's effects).
        render not scheduled (no world delta yet, no self-feedback).
  setup → captures handles into world_next, emits nothing
Tick 1 dispatch: nothing for setup.

Tick 2: world delta = {window, ctx, vao, prog, time_loc}.
        setup not scheduled (no world fields in its read-set; no
                              self-feedback from tick 1).
        render scheduled (world delta intersects read-set).
  render → emits frame_seq.
Tick 2 dispatch: frame_seq runs.

Tick 3+: render scheduled by self-feedback each tick. Setup is dead
         (no live subscriptions). Frame timer is the actual pacing
         (Phase 4).
```

Setup ticks exactly twice. Render ticks until frame counter reaches
zero (currently a body-internal mechanism; Phase 4 makes it a real
timer plugin).

### Stdin reader that blocks

```evident
claim input_loop(input ∈ Stdin,
                 state, state_next ∈ ReaderState,
                 last_results ∈ ResultList,
                 effects ∈ EffectList)
    -- echoes each input character
    state_next = state  -- always same
    effects = ⟨Println(input.line)⟩
```

Subscriptions: `Stdin` (plugin). No world reads. No self-feedback
needed for scheduling (stdin pushes).

Tick 0: scheduled (bootstrap), input.line = first line. Print.
Tick N: scheduled when stdin has another line. Print.
EOF: stdin source dies. FSM has no live subscriptions. Halt.

Zero polling. Sleeps in `select()` between lines.

### Cleanup FSM

```evident
claim shutdown_handler(signal ∈ ShutdownSignal,
                       world ∈ World,
                       state, state_next ∈ CleanupState,
                       last_results ∈ ResultList,
                       effects ∈ EffectList)
    -- runs cleanup effects when ShutdownSignal fires
    state_next = match state
        Idle    ⇒ (signal.fired ? Cleanup : Idle)
        Cleanup ⇒ Done   -- regular state, no halt magic
        Done    ⇒ Done
    effects = match state
        Cleanup ⇒ ⟨gl_delete_program(world.prog), sdl_quit⟩
        _       ⇒ ⟨⟩
```

Subscriptions: `ShutdownSignal` (plugin). World read-set:
`{prog}`. Self-feedback: when emitting cleanup effects.

Tick 0: scheduled (bootstrap). signal.fired = false. Idle.
Tick N: not scheduled (signal silent, prog unchanged).
Some tick: program receives SIGTERM (or render emits a Shutdown
  effect). Plugin fires the signal. FSM scheduled. Transitions
  Idle → Cleanup, emits cleanup effects.
Next tick: scheduled by self-feedback. Cleanup → Done. No effects.
Subsequent ticks: signal silent (already fired), no self-feedback,
  Done → Done would not fire because no input changes. FSM dead.

## Open questions

### Field-level vs whole-world deltas

Phase 2 uses field-level granularity: `read_set ∩ changed_fields`. An
alternative is "world changed" (any field) → schedule any reader.
Cheaper to compute, less precise — every reader wakes on every world
write. Probably field-level is right since we already have the AST
walker; revisit if delta computation shows up in profiles.

### Push vs pull for plugin events

Plugins could push events to the runtime (callback), or the runtime
polls plugins each tick. Push is more efficient for blocking
sources; pull is simpler. Likely a hybrid: pollable interface with
optional event-driven fast path.

### Parallel scheduling

If two scheduled FSMs have disjoint read-sets and write-sets in a
tick, they could solve in parallel. Z3 isn't thread-safe but each
FSM has its own cached `Solver` instance — separate solver
processes or thread-local Z3 contexts could enable this. Pure
optimization; defer.

### Migration of existing programs

Programs with `Done` variants and the value-equality fixpoint will
behave differently. Possible compat shim: keep the name-based halt
firing as an "implicit Exit" for one release, with a deprecation
warning.

### Static read-set under conditional reads

`world.X` inside a `match` arm or `if` is still a read; we conserva-
tively assume the worst (FSM might read X). That's the right call —
the alternative is execution-trace-dependent scheduling, which leaks
solver semantics into the scheduler.

### Read-set of plugin types

`input ∈ Stdin` is a subscription, not a world read. But the body
can reference `input.line` etc. Those should be tracked as part of
the plugin subscription, not the world read-set. Clean separation:
`world.X` → world read-set; `<plugin_var>.field` → plugin
subscription only matters that the plugin is live.

## Migration plan

1. Land Phase 1 with no behavior change. Add unit tests that pin
   the read-sets for existing demos.
2. Land Phase 2 behind an env flag (`EVIDENT_SCHEDULER=delta`)
   defaulting to the current "tick everyone" behavior. Verify
   existing tests pass under both modes.
3. Flip the default. Drop name-based halt + fixpoint halt as part
   of Phase 3.
4. Phase 4 (plugin subscriptions) fixes the polling cost for
   stdin-style FSMs.
5. Phase 5 is internal cleanup; not user-visible.

## What this replaces in `multi-fsm.md`

  * "Lifecycle: halt-per-FSM is the load-bearing semantic" — replaced
    by subscription-driven scheduling. Halt is no longer a primitive.
  * "What 'halt' means precisely" — removed; halt is "no live
    subscriptions."
  * "Program-level halt" — restated as "no live FSM and no pending
    plugin event."

The rest of `multi-fsm.md` (writer/reader pattern, world
composition, worked examples 1–5) stays valid; it's about how to
*structure* multi-FSM programs, not how the runtime schedules them.
