# Writing effect-driven state machines

Run with: `evident effect-run <file.ev>`

This guide is what you wish you'd read before debugging your first
program for an hour. Read it once end to end; refer back when
something behaves wrong.

## The execution model in one paragraph

Every effect-driven program is a finite state machine that the
runtime steps. Each step the solver picks `(state_next, effects)`
that satisfy `main` given the *current* `state` and the
`last_results` from the *previous* step's dispatched effects.
The runtime then performs the new effects (Print, ReadLine,
LibCall, …), collects their results into a list, and feeds them
back as `last_results` to the next solve. Halt: `state.variant ∈
{"Done", "Halt"}` AND `effects = EffNil`.

```
       ┌─────────────────────────────────┐
state →│ solver: pick state_next + effects│→ effects
       └─────────────────────────────────┘    │
              ▲                              ▼
              │              ┌──────────────────┐
   last_results◄─────────────│ dispatch effects │
                             └──────────────────┘
```

## Required shape of `claim main`

```evident
import "stdlib/runtime.ev"

enum MyState =
    Init                  -- first variant = initial state (pinned at step 0)
    ...
    Done                  -- name `Done` or `Halt` for the runtime to detect halt

claim main
    state, state_next ∈ MyState   -- the state pair
    last_results ∈ ResultList     -- MUST be named "last_results"
    effects      ∈ EffectList     -- MUST be named "effects"

    -- ... your state-transition + effect-emission logic here
```

The runtime detects this shape via `effect_loop::detect_main_shape`.
The names `last_results` and `effects` are required (the loop's
HashMap lookup is keyed on those exact strings); the state pair name
is auto-detected by finding two enum-typed Memberships where one
ends in `_next`.

## Initial state

The runtime pins step 0's `state` to the **first variant** of the
state enum. So put your "Init" / "Start" / etc. variant first.
Without this pin, Z3 was free to skip your setup and silently
choose a later variant — bad bug to track down.

## Computing `state_next` and `effects`

The cleanest pattern uses two `match state` expressions, one per
output:

```evident
state_next = match state
    Init               ⇒ AwaitInput
    AwaitInput         ⇒ Processing(handle_out)
    Processing(h)      ⇒ Done
    Done               ⇒ Done

effects = match state
    Init               ⇒ EffCons(SomeEffect, EffNil)
    AwaitInput         ⇒ EffNil
    Processing(_)      ⇒ EffCons(OtherEffect, EffNil)
    Done               ⇒ EffNil
```

Other shapes that also work:

- **Ternary:** `state_next = (state = Init ? AwaitInput : Done)` —
  fine for binary choices.
- **Guarded claim invocation:** `state = Init ⇒ MyClaim` — useful
  when a library's behavior should fire only for one state.
- **Multiple `⇒` constraints:** `state = Init ⇒ state_next = Done`
  (one per state). More verbose; harder to confirm exhaustiveness
  by reading.

## The two-phase "Issue → Await" pattern

Effects emitted at step N produce results visible only at step N+1.
This means **state_next can't reference results from the current
step's effects** — those results don't exist yet at solve time.

When a call's RESULT needs to thread into state, use a wait state:

```evident
enum State =
    Init              -- emit OpenLib effect
    AwaitLib          -- next step: capture the lib handle from last_results
    HaveLib(Int)      -- have the handle; do something with it
    ...

state_next = match state
    Init              ⇒ AwaitLib                  -- emit, wait
    AwaitLib          ⇒ HaveLib(handle_out)       -- capture the prior result
    HaveLib(lib)      ⇒ ...
```

For pure side effects (Println, Shell command run for its effect),
no wait state is needed — the next step can just transition.

## Extracting values from `last_results`

`last_results` is a `Seq(Result)` (linked-list shape). Use a
two-level `match` to extract a typed value from the head:

```evident
handle_out ∈ Int
handle_out = match last_results
    ResCons(r, _) ⇒ match r              -- bind r as Result-typed
        HandleResult(h) ⇒ h               -- extract Int payload
        _               ⇒ 0
    _              ⇒ 0
```

Why two levels: nested patterns like `ResCons(HandleResult(h), _)`
aren't supported by the parser yet. Bind the head as
enum-typed first, then match on it.

The same pattern for Int / String / Bool / Real results:

```evident
int_out ∈ Int
int_out = match last_results
    ResCons(r, _) ⇒ match r
        IntResult(n) ⇒ n
        _            ⇒ -1
    _              ⇒ -1

str_out ∈ String
str_out = match last_results
    ResCons(r, _) ⇒ match r
        StringResult(s) ⇒ s
        _               ⇒ ""
    _              ⇒ ""
```

## State carrying payloads

State variants can carry data — handles, counters, captured strings:

```evident
enum SDLState =
    Init
    HaveWindow(Int)              -- thread the window handle
    Pumping(Int, Int)            -- (window, frames_remaining)
    Done
```

The runtime's `encode_state_value` re-encodes payload variants
between steps so the next solve sees the right values. Supported
field types: Int / Bool / String / Real / nested enum.

## Halting

The loop exits cleanly when both:
1. `state.variant` is named `Done` or `Halt`, AND
2. `effects = EffNil`

If your program hits the `--max-steps` cap (default 10000), the
loop reports "did not halt cleanly". Common causes:
- State never reaches a Done/Halt variant (transition logic bug).
- Done/Halt variant emits a non-empty effects list (loop sees
  fixpoint as not-yet-halted).

## Halting conventions in practice

Always include a self-loop on the halt state with empty effects:

```evident
Done ⇒ (state_next = Done ∧ effects = EffNil)
```

Without this, when state=Done, Z3 might pick any state_next, and
the effects might be unconstrained.

## Common pitfalls

| Symptom | Cause | Fix |
|---|---|---|
| Effects fire every step instead of once | The "effects" var name was overridden by an EffectList intermediate (now fixed; convention enforced). Or state never transitions away from Init. | Run with `EVIDENT_LOOP_TRACE=1` and check `pinned state was Some(...)`. |
| `did not halt cleanly` | No transition to Done/Halt, or Done emits effects | Add `Done ⇒ (state_next = Done ∧ effects = EffNil)`. |
| Handle-using effect gets handle=0 | One-step pattern violation: tried to use a result captured in the same step that emitted it | Insert a wait state between issue and use. |
| `did not halt cleanly` after exactly `--max-steps` | Same — state stuck at non-halt variant | Add `EVIDENT_LOOP_TRACE=1`, inspect what state Z3 picks each step. |
| Build error: "expected expression, got Exists" | `∀ x ∈ Int` — Evident doesn't quantify over types, only sets/ranges | Restructure with `match` expressions over the variant. |
| Parse error in claim body | Likely a multi-name `∀ a, b ∈ Int` (not supported) or nested `Ctor(Inner(x))` pattern | Split into nested matches; one binding per pattern. |

## Debugging

- `EVIDENT_LOOP_TRACE=1` — dispatcher prints `state_next` + `effects`
  per step, plus the pinned state for the next solve.
- `EVIDENT_FFI_TRACE=1` — every FFI effect's input + result.
- Use both together when chasing handle-threading bugs.

```bash
EVIDENT_FFI_TRACE=1 EVIDENT_LOOP_TRACE=1 \
  evident effect-run programs/demos/effect_say.ev 2>&1 | head -20
```

## Worked examples

- `programs/demos/effect_hello.ev` — 2-state Println-once.
- `programs/demos/effect_echo.ev` — read+write loop with EOF detect.
- `programs/demos/effect_say.ev` — uses `stdlib/shell.ev` library.
- `programs/demos/effect_sdl_window.ev` — 19 states (raw FFI chain).
- `programs/demos/effect_sdl_window_libcall.ev` — 11 states (LibCall).

Read the LibCall version first; it's how new programs should look.
The raw-FFI version is kept for comparison / when caching is
unwanted.
