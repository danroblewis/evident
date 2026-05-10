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

**Where:** `test_17_sdl_gl_window` (full counterexample, with
diagnostic findings in the file header)

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

Working hypothesis: a Cocoa runloop / NSOpenGLContext
drawable-liveness boundary. Fix likely needs a Cocoa-aware
runloop driver.

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

Plus 22 inline `sat_*` / `unsat_*` static tests and the Rust
driver in `runtime-rust/tests/demos.rs`.
