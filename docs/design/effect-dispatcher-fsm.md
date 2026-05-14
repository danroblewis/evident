# Effect dispatcher as an Evident FSM

**Status**: Speculative future direction. Not in v1.

The near-term plan for effect ordering is **Rust-side toposort** over
edges extracted from `Seq(Effect)` literals in the FSM body. This
document is about the *eventual* replacement of the Rust dispatcher
itself with an Evident FSM — moving a piece of the runtime from Rust
into the language. Captured here because the reasoning is non-obvious
and the implementer six months from now should not have to re-derive
it.

The motivating idea: as long as dispatch is "Rust function called once
per tick that fires every effect in the model," ordering constraints
are out-of-band annotations the Rust loop has to consume. If dispatch
were itself an Evident FSM whose state includes a per-effect `Pending |
Done` flag, ordering constraints become real Bool invariants on that
state. The user's logical intuitions (especially `⇒`) start meaning
what they should mean.

## What the Rust dispatcher does today

`runtime/src/effect_loop.rs::collect_dispatchable_effects` walks the
user FSM's model bindings, gathers every `Effect` / `Seq(Effect)`
value, and returns a flat `Vec<Effect>`. The order is alphabetical by
binding name — a stable but arbitrary choice that the Mario demo
papers over with `a_` / `b_` / `c_` prefix renames.

After this list is built, `effect_dispatch.rs::dispatch_one` runs each
effect synchronously in sequence, collecting `EffectResult`s into a
`last_results` Seq fed back to the user FSM next tick.

The dispatcher has no state. It's stateless because in the current
model, every effect produced this tick fires this tick. There's no
"not yet" — the truth value of "effect e happened in this tick" is
always True at end-of-tick.

That stateless model is exactly what makes truth-functional `⇒`
vacuous as a precedence operator (see "Why ⇒ doesn't work today",
below).

## The FSM-dispatcher idea

Replace the Rust dispatcher with an Evident FSM. The FSM has:

- A snapshot of the set of effects produced by user FSMs this tick.
- Per-effect state: `Pending` or `Done` (optionally `Running` for
  long-running effects).
- An ordering-constraint set extracted from the user FSM's body
  (`Seq(Effect)` literals → edges, see toposort doc).
- A `to_run` slot — the effect chosen this dispatcher-tick.

Each dispatcher tick:

1. Pick a `Pending` effect whose dependencies are all `Done`.
2. Emit a single-effect `effects = ⟨to_run⟩` so the existing
   Rust-side effect dispatcher (now reduced to "execute one effect")
   runs it.
3. Move `to_run` from `Pending` to `Done`.
4. Halt when `Pending` is empty.

In sketch form:

```evident
enum EffectState = Pending | Done

type DispatcherSnapshot
    pending ∈ Set(EffectId)
    done    ∈ Set(EffectId)
    edges   ∈ Set(Edge<EffectId>)

fsm effect_dispatcher(
    snap, snap_next ∈ DispatcherSnapshot,
    to_run          ∈ EffectId,
    last_results    ∈ ResultList,
    effects         ∈ EffectList,
)
    -- Pick an effect to dispatch: in `pending`, all incoming
    -- edges' sources in `done`.
    to_run ∈ snap.pending
    ∀ e ∈ snap.edges : e.to = to_run ⇒ e.from ∈ snap.done

    -- Advance state
    snap_next.done    = snap.done ∪ {to_run}
    snap_next.pending = snap.pending - {to_run}
    snap_next.edges   = snap.edges

    -- One effect per dispatcher tick (the Rust side actually runs it)
    effects = ⟨resolve(to_run)⟩

    -- (or, halt when pending is empty — see "halt semantics" below)
```

The `EffectId` is some identity — likely the user FSM's binding name
as a String, since two structurally-equal Effect values (two
`Println("hi")` in different bindings) are distinct dispatch targets.
The `resolve(to_run)` step maps `EffectId` back to the concrete Effect
value the user FSM produced; the Rust side keeps a `HashMap<EffectId,
Effect>` updated per user-tick.

## Why ⇒ becomes natural

The Rust dispatcher's "every effect fires this tick" model means
`b_eff happened` is always True at any point where the question can be
asked. Logical implication `b_eff ⇒ a_eff` reduces to
`a_eff happened`, also vacuously True. No temporal information carried.

In the FSM-dispatcher model, `a.state` and `b.state` are real
state-machine values that pass through `Pending` before becoming
`Done`. The constraint `b.done ⇒ a.done` is non-vacuous: it forbids
the state `b.done = True ∧ a.done = False`, which directly encodes
"a must transition to Done before b can." That's precedence as a Bool
invariant on the dispatcher's state space, asserted on every tick.

The user-facing surface could be either form, with the runtime
rewriting to constraint set:

```evident
-- (a) Existing Seq form — chains
⟨a_eff, b_eff, c_eff⟩       -- adds edges a→b, b→c

-- (b) Direct edge declarations (would need a new shape)
b_eff ⇒ a_eff               -- adds edge a→b
```

The pragmatic answer is to expose only (a) for now — Seq literals are
already in the language and the user already knows them. The `⇒`
spelling is a future cleanup once the dispatcher FSM is in place and
the meaning is sound.

## Trade-offs

**Plus side**:

- Real dogfooding. The runtime's own effect dispatcher is just an
  Evident program — the same multi-FSM scheduler that runs the
  user's code runs the dispatcher.
- Verifiable. You can pose "is there a state where dispatching
  deadlocks?" or "is there always at least one Pending effect with
  satisfied deps?" as constraint queries against the dispatcher
  schema.
- `⇒` becomes sound. The user's logical intuitions translate
  directly.
- Ordering bugs in the dispatcher show up as UNSAT solves (cycle in
  the deps), not "the program dispatched the wrong thing" — much
  louder failure mode.

**Cost side**:

- One solver invocation per dispatched effect, not per tick. A
  user FSM emitting 20 effects per frame at 60 Hz means
  20 × 60 = 1200 solver runs per second. Each is small (a Set of
  ~20 EffectIds, a Set of ~few edges), but the overhead is real.
  Bigger budget than the current free-Rust-loop dispatcher.
- The dispatcher FSM has to integrate with the multi-FSM
  scheduler. Today, the scheduler ticks user FSMs and dispatches
  their effects in one phase. The dispatcher FSM would be a
  *second* FSM ticked between user FSM ticks — a structural
  change.
- Effect identity becomes a thing the runtime has to maintain.
  Today, `Vec<Effect>` is enough; with the FSM, each effect needs
  a stable ID (likely the binding name) and a way for the
  dispatcher's `resolve(...)` step to find the concrete value.
- New halt semantics. When all effects this tick are Done, what
  triggers the next user-FSM tick? Probably the dispatcher's halt
  → scheduler advances → user FSMs tick again. But the boundary
  needs design.

## Open questions

These don't have answers yet; the FSM-dispatcher work would need to
resolve each:

1. **One dispatcher per tick, or one across the program lifetime?**
   Per-tick is simpler (snapshot reset each user-tick); lifetime is
   more powerful (effects could be deferred across user-ticks if
   their deps aren't ready yet).

2. **Effect identity model.** Use binding name? Hash of the AST
   path? Compound ID `(fsm_name, binding_name)` to handle
   multi-FSM programs? Affects how `Seq(Effect)` literals get
   rewritten to edges.

3. **Cycle detection.** Today, a cycle is "alphabetical order
   silently picks something wrong." Under the FSM dispatcher, a
   cycle is UNSAT — but UNSAT on which solve? The first tick where
   no Pending effect has all deps Done. The error message needs
   to identify the cycle, not just "no satisfying assignment."

4. **Long-running effects.** A `LibCall` to a blocking I/O could
   take milliseconds. Should there be a `Running` state between
   `Pending` and `Done`? If so, the dispatcher needs to yield
   back to the scheduler mid-effect.

5. **Cross-FSM ordering.** Two user FSMs emitting effects this
   tick — does the dispatcher merge them and run with a global
   partial order, or run each user FSM's effects in isolation?

6. **Effect production mid-dispatch.** If dispatching effect A
   produces a new effect B (via a callback, say), does B join the
   current dispatcher run or wait until next tick?

## Relationship to the v1 Seq-toposort approach

The v1 approach is:

- Rust-side dispatcher unchanged structurally.
- Walk the user FSM body for `Seq(Effect)` literals, extract them as
  ordering edges.
- Toposort the effect bindings against those edges in Rust.
- Random tie-break on free choices (so bugs surface).
- Mario's `a_` / `b_` prefixes disappear.

The v1 approach **does not constrain** the eventual FSM-dispatcher
shape. The user-facing surface (Seq literals as ordering edges) stays
identical; only the Rust dispatcher internals get replaced when we're
ready. So the v1 work isn't throwaway — it's the part of this design
that ships today, with the FSM-dispatcher as the long-term cleanup.

The migration path:

1. **v1 (done)**: Rust toposort over Seq-derived edges.
2. **v1.5**: Refactor the Rust dispatcher into a shape that takes a
   pre-sorted `Vec<(EffectId, Effect)>` instead of a Vec<Effect>.
   No behavior change, but isolates the ordering step from the
   execution step.
3. **v2**: Implement the FSM dispatcher; replace the Rust ordering
   step with a multi-FSM-scheduler invocation. Execution step
   stays Rust (still calls `dispatch_one` for each effect ID the
   FSM picks). User-visible behavior identical.
4. **v2.5**: Open up `⇒` and any other state-aware shapes once the
   FSM is the source of truth.

## What this connects to

- `docs/design/toposort.md` — the generic toposort primitive that
  the v1 Rust dispatcher uses (and the v2 FSM dispatcher would also
  use, via constraints rather than direct calls).
- `docs/design/multi-fsm.md` — the scheduler the v2 dispatcher
  would integrate with. The dispatcher FSM is just another FSM
  from the scheduler's perspective.
- `examples/test_21_mario/main.ev` — the canonical use case
  driving this. The `a_` / `b_` prefix renames there are the
  symptom; both v1 and v2 fix it.
- `examples/COUNTEREXAMPLES.md` #25 — "no Seq(Seq(T))" is what
  prevents an `AllPermutations<Effect>(...)` claim from being the
  alternative shape here. The FSM approach sidesteps it: state
  evolves over time, not all permutations materialized as data.
