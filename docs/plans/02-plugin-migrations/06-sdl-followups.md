# SDL stdlib follow-ups

Tracks deferred work for the declarative SDL scene library
(`stdlib/sdl/scene.ev`) and the SDL FFI surface. Items are roughly
ordered by user-visible impact / unlock potential.

The renderer surface today: `RFilledRect`, `RRect` (outlined),
`RLine`, `RPoint`, `RGeometry` (per-vertex-color triangles via
`SDL_RenderGeometry`). The state machine + handle plumbing is
encapsulated in `..SDLScene`; user code is `~10` lines of scene
description.

## High impact

### `RTexture` — sprites + image loading

The biggest missing primitive. Needs:

  * `libSDL2_image.dylib` dependency (separate dylib; `IMG_Load`
    handles PNG/JPG/etc). Or fall back to built-in `SDL_LoadBMP`
    for a stepping stone.
  * **Variable-length state payload** — texture handles created at
    setup must persist across frames AND be enumerated at
    teardown. Current state enums carry fixed payloads; a list of
    handles would either need a payload-list variant or a separate
    state-side `Seq(Int)`.
  * **Texture handle lookup from renderables** — `RTexture(idx, src,
    dst)` references textures by index. The renderer needs a way to
    pluck the Nth texture handle in `render_one_texture`. (A
    `nth_handle(state_handles, idx, out)` helper claim.)
  * Setup expansion — needs to walk the user-provided
    `TextureSpecList` and emit `IMG_Load` + `CreateTextureFromSurface`
    + `FreeSurface` per texture. Recursive Seq construction.
  * Per-frame: `SDL_RenderCopy(renderer, tex, src_rect, dst_rect)`
    or `SDL_RenderCopyEx` (with rotation/flip). Both take
    `SDL_Rect*` args (we have `ArgI32Buf` for these).

API sketch:

```evident
textures ∈ TextureSpecList
textures = ⟨ TexSpec("/path/sprite.png"), TexSpec("/path/bg.png") ⟩

items = ⟨
    RTexture(1, 0, 0, 0, 0,   0, 0, 640, 480),  -- bg texture, full screen
    RTexture(0, 0, 0, 64, 64, 100, 100, 64, 64) -- sprite at 100,100
⟩
```

`TexSpec(path_string)` parses cleanly today (string payload). The
texture-handle threading is the architectural lift.

## Medium impact

### `RGeometry` extensions

  * **Indexed geometry** — `SDL_RenderGeometry` has an `indices`
    parameter we always pass NULL. With indices, you can share
    vertices across triangles (mesh rendering).
  * **Textured geometry** — the SDL_Vertex struct already has
    `tex_coord`; we just need to pass a non-NULL texture handle to
    `SDL_RenderGeometry`. Falls out of `RTexture` work.

### Animation primitives

The library exposes `frames` as a count, no per-frame state. To
animate, the user would need:

  * Access to the current frame number inside the renderable list
    (so `RRect`'s position can be a function of frame).
  * Or a callback shape: `claim build_items(frame ∈ Int, out ∈
    RenderableList)` that the library invokes each frame.

The second is cleaner but currently the user provides `items` as
state-time data, not a function. Needs a design pass.

### Input

  * `SDL_GetMouseState(*x, *y)` — needs two output-int pointers (we
    have `ArgIntOut` for one; a buffered version covers two).
  * `SDL_GetKeyboardState(*numkeys)` — returns a pointer to a
    Uint8 array. Needs an "out-byte-buffer" arg type, plus a way to
    surface the array data back to the program.
  * `SDL_PollEvent(*SDL_Event)` — `SDL_Event` is a 56-byte union.
    Caller provides buffer; SDL writes into it. Needs `ArgByteBuf`
    + struct-decode helpers.

These are useful but each is its own FFI design exercise. Defer
until there's a use case asking for them.

### Audio

`stdlib/audio/` exists separately (Phase 2.3 done). Cross-link
from scene.ev when we want music/SFX baked into the scene API.

## Low impact / cleanup

### Record payloads in enum variants

Every `RRect(Int, Int, Int, Int, Int, Int, Int, Int)` should be
`RRect(IVec2, IVec2, Color)`. Today `enum` payloads can be
primitive or another enum, but not a `type` (record). Lifting this
restriction needs the runtime to:

  * Build a Z3 datatype for each record type (today they're flat
    field expansions, not Z3 datatypes).
  * Marshal record values into / out of enum payload slots.
  * Pattern-match record-typed payloads in `match` arms.

Significant runtime work but cleans up every renderable variant
and ripples out to other domains (game state, etc.).

### `nth_result` / `nth_handle` helper

The setup-Seq → handle-extraction pattern in scene.ev cascades
matches manually:

```evident
after_1 ∈ ResultList
after_1 = match last_results
    ResCons(_, t) ⇒ t
    _             ⇒ ResNil
```

A recursive `nth_result(rs, n, out)` claim would collapse this to
one line per handle. Already prototyped in conversation; just
needs to land in `stdlib/runtime.ev`.

### Transpiler caching (cross-cutting, not SDL-only)

The GLSL transpiler in `stdlib/glsl/transpile.ev` re-runs every
solver step. For shader programs the AST is constant — should be
solved once at startup, the result string pinned via `given` for
subsequent steps. Reduces the
`programs/demos/effect_gl_transpiled_triangle.ev` runtime from
~32s for 90 frames to something close to the 33ms-per-frame target.

Same pattern applies to any deterministic computation in main's
body that doesn't depend on `state`.

### `..SDLGLScene` library variant

A scene library that pairs with the GL/shader path instead of
SDL's 2D renderer. Drop-in replacement for the triangle demo:

```evident
import "stdlib/sdl/gl_scene.ev"

claim main
    title  ∈ String      ; title  = "GL App"
    width  ∈ Int         ; width  = 640
    height ∈ Int         ; height = 480
    frames ∈ Int         ; frames = 90

    vertex_shader   ∈ Shader  ; vertex_shader   = MakeShader(...)
    fragment_shader ∈ Shader  ; fragment_shader = MakeShader(...)

    ..SDLGLScene
```

Library handles: SDL+GL setup, shader compile via the transpiler,
program link, per-frame clear+draw+swap, teardown.
