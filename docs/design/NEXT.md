# Where to direct next session's work

After the unified-schema-model arc (see
[`../sessions/2026-05-09-unified-schema-model.md`](../sessions/2026-05-09-unified-schema-model.md)),
the runtime is in a coherent place. This file captures what's
worth working on next, in rough priority order.

## Explicitly NOT planned

### Bounded write-queues for plugin sources

Currently `WriteQueue` is `Arc<Mutex<VecDeque>>` — unbounded.
Tempting to bound for memory safety, but the user's framing
(2026-05-10): "in the future we want to expose memory more
readily to Evident models. The boundedness of queues will
emerge naturally, and the constraints will be different than
what we would implement now." When memory becomes a first-class
modeled concept, queue bounds will fall out of those constraints,
not from runtime-side fiat.

## High value, bounded scope

### 1. Parse-int / parse-real built-ins

`programs/demos/effect_guess_number.ev` works around the lack
of int parsing by comparing strings. Most realistic interactive
programs need to parse numeric input. Either:

  * Add `Effect::ParseInt(String)` returning `IntResult` or error
  * Add `Effect::ParseReal(String)` returning `RealResult` or error
  * Or write libc `strtol`/`strtod` wrappers in stdlib

Effect-based is simpler; FFI-based is more general. Either works.

### 2. Rewrite older demos using the unified model

User direction (2026-05-10): "eventually we want to remove the
old demos and replace them with more modern versions."

Targets in `programs/demos/`:
  * `effect_echo.ev` (uses legacy ReadLine pattern) → rewrite
    using StdinSource auto-install
  * `effect_hello.ev`, `effect_say.ev` → keep as minimal one-shot
    examples; no rewrite needed
  * `effect_sdl_*` and `effect_gl_*` → eventually port to FTI
    once that lands
  * `calculator.ev` → rewrite as REPL using stdin + state machine

After each rewrite + verification, retire the legacy version
(or keep with a banner pointing to the modern replacement).

### 3. More worked-example shapes

The 06–11 lang tests cover stdin echo, timer, multi-plugin,
SIGINT, request/response. Worth adding once supporting
infrastructure exists:
  * A simple TCP echo server (needs socket plugin)
  * A real REPL (stdin + state machine + parsed input)
  * A file watcher demo (needs file-watch plugin)

## Medium value, larger scope

### 4. Foreign Type Interface (FTI) prototype

See [`foreign-type-interface.md`](foreign-type-interface.md).
Pick ONE C resource (probably `SDL_Window` or a socket) and
implement the bridge plugin that materializes it from a typed
declaration. If the prototype works, the existing `stdlib/sdl/`
bindings could migrate type-by-type.

Big chunk of work but high payoff — eliminates the handle-passing
ceremony in user code.

### 5. User-FSM spawning

See [`fsm-spawning.md`](fsm-spawning.md). Wait for a concrete use
case (TCP server is the leading candidate) before implementing.
The design space is wide; pick a real driver.

## Low value, small scope (cleanup)

### 6. Remove or simplify the marker-type subscription path

`type FrameTimer` and `type Signal` in `stdlib/runtime.ev` are
legacy v3 paths that the plugin-as-writer model supersedes. They
still work for back-compat. After more bake time, deprecate then
remove.

### 7. Rust runtime warnings cleanup

A handful of unused-import warnings remain in
`runtime-rust/src/translate/`. Pre-existing, not from the
unified-model work. Trivial to clean.

### 8. Self-host more of the runtime

The schema-interface framing implies the runtime IS an Evident
model. As the language gains primitives (timers via FFI, channels,
process spawning), parts of the scheduler could move from Rust to
runtime.ev. Speculative; no concrete plan.

## Should NOT touch

  * **Constraint solving substrate** (Z3 + claim translation) —
    works, performant, well-tested.
  * **Existing FFI** (`Effect::FFICall`) — keep until FTI replaces
    it case-by-case.
  * **Spec docs** in `spec/` — describe the constraint language,
    not the runtime; correct as-is.
  * **Legacy mode** (`EVIDENT_SCHEDULER=legacy`) — keep until the
    new model has a year of bake time. Real users may rely on
    fixpoint-halt semantics.

## Patterns to keep using

  * **One commit per coherent change** with full body explaining
    motivation + trade-offs. Makes session notes write themselves.
  * **Test through the binary** for plugin behavior (subprocess
    in cargo test). Stdlib `DispatchContext::stdin` doesn't reach
    the plugin's `std::io::stdin()` — they're separate fds.
  * **Lang tests as documentation**. Writing
    `programs/lang_tests/multi_fsm/0N_*.ev` for each new pattern
    gives users a copy-pasteable starting point AND a regression
    test in one file.
  * **Cross-link docs**. Every design doc should point to its
    siblings; every guide should point to the design.
