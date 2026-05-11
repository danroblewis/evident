# AP-002: no-raw-ffi-in-examples

**Status:** active

**Pattern.** A file under `examples/` contains a raw FFI primitive
identifier: `LibCall`, `FFICall`, `FFIOpen`, or `FFILookup`.

**Why.** Demos exist to (a) demonstrate the language's typed
abstractions and (b) double as integration tests. When a demo
reaches into raw FFI it duplicates work that belongs in the
stdlib wrapper layer — every program that calls `glClear` should
do so via `gl_clear(out)`, not via
`LibCall("/System/Library/Frameworks/OpenGL.framework/OpenGL", "glClear", "v(i)", …)`.
The wrapper layer also normalizes platform paths and signatures
in one place; demos that bypass it spread platform assumptions
across the example set.

**Fix.** Add the missing wrapper to `stdlib/<library>/<file>.ev`,
then call the wrapper claim from the demo. If the wrapper exists
but doesn't fit the in-Seq use case, add a parallel
`<wrapper_name>_after(prior_idx ∈ Int, ...)` variant. See
AP-007.

**Detection.** grep

**Pattern (grep).** Word-boundary match on `LibCall`, `FFICall`,
`FFIOpen`, `FFILookup` in `.ev` files under `examples/`.
Comment-only lines (starting with `--`) are exempt.

**Scope.**
  - Apply to: `examples/*.ev`.
  - Do NOT apply to: `stdlib/*` (where wrappers live and these
    primitives are expected), `tests/lang_tests/*` (runtime
    regression fixtures may exercise the primitives directly).

**Exceptions.**
  - Tokens inside `--` comments are exempt.
  - String contents (e.g. a Println message that happens to
    contain "LibCall") are NOT exempt — programs shouldn't
    print about FFI.

**Examples.**
  - `examples/test_16_sdl_red.ev`'s first draft contained
    multiple `LibCall(...)` invocations for SDL_Init,
    SDL_CreateWindow, SDL_CreateRenderer, etc. Fixed by adding
    `sdl_init`, `sdl_create_window`, `create_renderer_after`,
    `set_draw_color_after`, `render_clear_after`,
    `render_present_after` to `stdlib/sdl/{window,render}.ev`.
