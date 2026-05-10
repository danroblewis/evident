# Counterexamples found while building the new demo set

This is the punch list of edge cases / footguns / runtime gaps
discovered while rebuilding `programs/demos/` from scratch (one
demo per primitive, every program tested via inline `sat_*` /
`unsat_*` claims plus `evident effect-run` end-to-end).

The runtime works for **every demo we shipped**, but each item
below is a place where the user had to know something subtle to
make the program work — the runtime should ideally make these go
away or surface a clearer error.

## 1. First state-variant must be nullary

**Where:** `test_02_counter` (note in header)

If the FSM's state enum has a payload first variant
(`enum S = Count(Int) | Done`), the runtime can't seed tick 0 —
Z3 picks the simplest satisfying state (often `Done`), and the
program exits immediately.

Workaround: prepend a nullary `Start` variant.

Fix idea: let `state` be supplied as an init pin (like FTI
config pins).

## 2. Nested constructor patterns in `match` don't parse

**Where:** `test_04_parse_int` (note in body)

`ResCons(_, ResCons(r, _))` fails with `parse error: expected
RParen, got LParen`. The match parser doesn't recurse into
constructor patterns inside a constructor pattern.

Workaround: descend with an intermediate `match` that pulls
`tail`, then match on `tail`.

Fix idea: extend the pattern parser to recurse into nested
ctor args.

## 3. Enum variant names are global

**Where:** `test_09_two_fsms` (note in header)

Two enums in the same file can't both have a variant named
`Done`. (Documented in CLAUDE.md but very easy to trip on with
two short FSMs in one file.)

Workaround: prefix variants per enum (`PEnd`, `CEnd`).

Fix idea: scope variant names per-enum, or auto-suffix on
collision with a warning.

## 4. FTI pins parse only in claim BODY, not signature

**Where:** `test_13_timer`, `test_17_sdl_gl_window` (notes in
header / body)

`claim x(t ∈ Timer (interval_ms ↦ 50), …)` is a parse error
(`expected ',' or ')' after param group`). Moving the
declaration into the body works:

```evident
claim x(state, …, effects ∈ EffectList)
    t ∈ Timer (interval_ms ↦ 50)
    …
```

Fix idea: extend the param-list grammar to accept the pin
syntax inline.

## 5. FTI values don't propagate into `match state` transitions

**Where:** `test_11_frameclock`, `test_13_timer` (notes)

A state-transition that reads an FTI value:

```evident
state_next = match state
    Watching ⇒ (clock.tick_count ≥ 5 ? Done : Watching)
```

never picks `Done` — Z3 sees the threshold as un-met every tick,
even when the bridge has written `clock.tick_count = 5`.

Workaround: gate exit on `effects` directly:

```evident
state_next = Watching
effects = (clock.tick_count ≥ 3 ? ⟨Exit(0)⟩ : ⟨⟩)
```

Fix idea: trace why the per-FSM view's FTI-prefix-stripped
pins don't bind into the state-transition equation. Likely an
encoding-order issue where the state pin is built before the
FTI pins are merged.

## 6. Bool result from binding inside match arm doesn't propagate

**Where:** test_07_time investigation (workaround already in the
file)

```evident
got = match last_results
    ResCons(r, _) ⇒ match r
        IntResult(n) ⇒ n > 0      -- Z3 picks false even when n is large
        _            ⇒ false
```

The bound payload `n` is in scope for the arm but `n > 0`
yields false. Returning `n` as an Int and computing the
comparison outside the match works.

Fix idea: pattern-bound payload values may not be inserted
into the env that the arm's RHS expression sees.

## 7. SDL+GL renders black through Effect dispatch

**Status:** unfixed. The demo file was REMOVED from
`programs/demos/` because its presence implied it worked. The
source is embedded at the bottom of this file under
`Appendix A: SDL+GL counterexample source` so contributors can
reproduce.

Per-frame `glClearColor` / `glClear` / `SwapWindow` calls
dispatched through Evident's effect loop don't visually
present, even though:

  - Same thread (ThreadId(1)) as bridge install
  - Same args, same function pointers
  - GL context current (`glGetString(GL_VERSION)` returns
    `"4.1 Metal - 89.3"`)
  - `glGetError` returns 0

The same calls work when issued INLINE inside the bridge
install, OR when the entire SDL+GL init is bundled into one
`Effect::Seq` as the (now-deleted) `effect_multi_fsm_triangle`
demo did.

**Things tried (none fixed it):**

  1. `glViewport(0, 0, w, h)` at install time — Apple's
     GL-on-Metal default viewport is 0×0; setting it didn't
     restore rendering (still needed though).
  2. `SDL_GL_SetAttribute` reordered to BEFORE
     `SDL_CreateWindow` (was being silently ignored in the
     wrong order — fixed independently).
  3. `glLinkProgram` status check (would have caught silent
     link failures — wasn't the cause).
  4. `SDL_ShowWindow` + `SDL_RaiseWindow` after
     CreateWindow — got the window onscreen, didn't fix the
     black render.
  5. Two priming swaps inside the bridge install (so the
     drawable is "exercised" before the first user tick) —
     no effect.
  6. Re-`SDL_GL_MakeCurrent` per frame from the user FSM —
     no effect.
  7. `glFlush` + `glFinish` before `SDL_GL_SwapWindow` from
     the user FSM — no effect.
  8. `NSApplicationLoad()` at bridge install (Cocoa
     bootstrap for command-line tools) — no effect.

**Working hypothesis:** a Cocoa runloop / NSOpenGLContext
drawable-liveness boundary between bridge return and the
first FSM tick. Likely needs either:

  * a Cocoa-aware runloop driver in the runtime
    (NSApp.run-style, with the FSM scheduler integrated as
    a runloop source), OR
  * deferred FTI install — bridge waits to do
    SDL_CreateWindow + GL context creation until INSIDE the
    first user tick's Effect dispatch, so the drawable's
    creation, first use, and first swap all happen on the
    same Cocoa runloop iteration.

The working multi-FSM GL demo (`effect_multi_fsm_triangle`,
deleted) put the entire SDL+GL init inside a single user
`Effect::Seq` on tick 0 and rendered fine. That's the only
known-working GL pattern in this runtime.

## 8. SpawnFsm + same-tick Exit drops the spawned FSM's first effect

**Where:** `test_10_spawn` (note in header)

If parent emits `⟨SpawnFsm("worker", N), Exit(0)⟩` in a single
tick, the worker is registered but `Exit(0)` halts the runtime
before the worker ticks → "worker spawned" never prints.

Workaround: parent transitions to a Wait state and exits a
few ticks later, giving the spawned FSM time to fire.

Fix idea: drain newly-spawned FSMs' tick-0 effects before
honoring `exit_requested`.

## 9. `Effect::Seq` doesn't share renderer/window handles across ticks

**Where:** `test_16_sdl_red` (note in body)

A renderer pointer created via `SDL_CreateRenderer` inside one
`Effect::Seq` (the setup tick) isn't accessible to subsequent
`Effect::Seq` invocations (the per-frame ticks) — there's no
cross-Seq state. The workaround is to call `SDL_CreateRenderer`
again at the head of each frame's Seq and reference its result
via `ArgPriorResult(0)`. Functionally OK (libffi caches lib +
sym handles) but wasteful.

Fix idea: an `SDL_Renderer` FTI bridge, analogous to
`GL_Program`, that owns the renderer pointer and exposes it as
a known `Int` field on the type. Then per-frame ops can be
plain stdlib calls on the known handle — no `Seq`, no
`PriorResult`.

## 10. Stdlib helpers can't take `ArgPriorResult` without explicit `*_after` variants

**Where:** `stdlib/sdl/render.ev` (the new `*_after` family)

A wrapper claim like `render_clear(renderer ∈ Int, out)` builds
its own `ArgList` with `ArgHandle(renderer)`. To get an
`ArgPriorResult(N)` slot in that list instead, the wrapper has
to be re-coded with `ArgPriorResult(prior_idx)` and the
`prior_idx` exposed as a parameter (`render_clear_after`). So
every stdlib FFI helper grows a parallel `_after` variant for
in-Seq use. Not great.

Fix idea: a generic mechanism for converting a wrapper's typed
`Int` arg into an `ArgPriorResult` inside a Seq (perhaps a
phantom value `prior_at(N)` that the call-site translator
recognizes), or move toward FTI bridges so most C resources
have known typed handles instead of needing in-Seq chaining.

## What works without caveat

Every demo ships in green:

| # | Demo | Primitive |
|---|---|---|
| 01 | hello | Println, Exit |
| 02 | counter | state-pair, payload-state via Start prefix |
| 03 | seq_chain | Effect::Seq |
| 04 | parse_int | ParseInt → Int / Error result |
| 05 | int_to_str | IntToStr → String result |
| 06 | shell_run | ShellRun → captured stdout |
| 07 | time | Time → IntResult |
| 08 | exit_code | non-zero exit propagation |
| 09 | two_fsms | shared World, writer-first scheduling |
| 10 | spawn | SpawnFsm with Int arg, spawnable_only marker |
| 11 | frameclock | FrameClock FTI |
| 12 | hostname | Hostname FTI (one-shot bridge) |
| 13 | timer | per-instance Timer with `interval_ms ↦ N` |
| 14 | stdin | StdinSource plugin-as-writer |
| 15 | signal | SigintSource plugin-as-writer |
| 16 | sdl_red | SDL_Renderer (renderer-based, not GL) |
| 17 | sdl_triangle | SDL_RenderGeometry triangle (everything in one Seq on tick 0) |

Plus inline `sat_*` / `unsat_*` static tests and the Rust
driver in `runtime-rust/tests/demos.rs`.

---

## Appendix A: SDL+GL counterexample source (counterexample #7)

This file used to live at `programs/demos/test_17_sdl_gl_window.ev`.
It was removed because its presence in the demos directory
implied it worked. The runtime can't currently render through
this pattern — see counterexample #7 above for the diagnostic
findings and what's been tried.

Reproduces the bug: window appears (titled "Counterexample")
but stays black. Save as a `.ev` file and run with
`evident effect-run`.

```evident
import "stdlib/runtime.ev"
import "stdlib/sdl/gl.ev"
import "stdlib/sdl/window.ev"
import "stdlib/shader/program.ev"

enum WState = WInit | WLoop(Int) | WEnd

claim gl_demo(state, state_next ∈ WState,
              last_results ∈ ResultList,
              effects ∈ EffectList)
    win ∈ SDL_Window (title ↦ "Counterexample", width ↦ 640, height ↦ 480)

    state_next = match state
        WInit    ⇒ WLoop(60)
        WLoop(n) ⇒ (n ≤ 1 ? WEnd : WLoop(n - 1))
        WEnd     ⇒ WEnd

    set_color_eff ∈ Effect
    gl_clear_color(0.9, 0.1, 0.1, 1.0, set_color_eff)
    clear_eff ∈ Effect
    gl_clear(16384, clear_eff)
    swap_eff ∈ Effect
    gl_swap_window(win.handle, swap_eff)
    pump_eff ∈ Effect
    sdl_pump_events(pump_eff)
    delay_eff ∈ Effect
    sdl_delay(33, delay_eff)

    frame_inner ∈ EffectList
    frame_inner = ⟨set_color_eff, clear_eff, swap_eff, pump_eff, delay_eff⟩
    frame_seq ∈ Effect
    frame_seq = Seq(frame_inner)

    effects = match state
        WInit    ⇒ ⟨⟩
        WLoop(n) ⇒ (n > 0 ? ⟨frame_seq⟩ : ⟨Println("done"), Exit(0)⟩)
        WEnd     ⇒ ⟨⟩
```
