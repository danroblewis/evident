# `packages/sdl/` — SDL2 FFI bindings

Typed Evident wrappers around SDL2 and its satellite libraries. Each
file exposes named `claim`s (and, where a handle must persist across
ticks, an `external type` FTI resource) so demos never write raw
`LibCall` / `FFICall` — see `CLAUDE.md` "Demo files MUST NOT contain
raw FFI calls".

| Module | Library | What it wraps |
|---|---|---|
| [`window.ev`](window.ev) | `libSDL2.dylib` | `SDL_Init` / `SDL_CreateWindow` / `SDL_Delay` / pump-events / keyboard state. The `SDL_Window` FTI resource installs a window + renderer and keeps the renderer handle alive across ticks; render primitives (`set_draw_color`, `render_clear`, `render_fill_rect`, `draw_rect`, …) are subclaims on it. Also defines the shared `IVec2` / `Color` / `Rect` records. |
| [`render.ev`](render.ev) | `libSDL2.dylib` | Renderer creation + the `*_after` (prior-result) render variants for single-Seq atomic batches, plus the `SDL_Vertex` packed-buffer builder for `SDL_RenderGeometry`. |
| [`gl.ev`](gl.ev) | `libSDL2.dylib` + OpenGL | `GL_Program` FTI resource (GL 3.3 Core context, shader compile/link, VAO) and draw-call wrappers. |
| [`mixer.ev`](mixer.ev) | `libSDL2_mixer.dylib` | Audio: `Mix_OpenAudio` / `Mix_LoadWAV` / `Mix_PlayChannel` / `Mix_LoadMUS` / `Mix_PlayMusic` / `Mix_HaltChannel` / `Mix_CloseAudio`. The `SDL_Mixer` FTI resource opens the device + loads a WAV at install and keeps the `Mix_Chunk*` handle alive across ticks; `play` / `halt` are subclaims on it. |

## `mixer.ev` at a glance

Requires SDL2_mixer (`brew install sdl2_mixer` → `/opt/homebrew/lib/libSDL2_mixer.dylib`).

**FTI resource — the cross-tick path.** A loaded sample handle can't
survive a tick boundary without an FTI bridge (same constraint as
`SDL_Window`'s renderer). Declare `SDL_Mixer` as an fsm body member;
the install Seq runs `SDL_Init(AUDIO)` + `Mix_OpenAudio` + `Mix_LoadWAV`
and binds the chunk handle into `chunk`:

```evident
fsm tone(state ∈ AState)
    mixer ∈ SDL_Mixer (sample_path ↦ "examples/assets/tone.wav")
    play_eff ∈ Effect
    mixer.play(0, play_eff)        -- loops=0 → play once; -1 → forever
    halt_eff ∈ Effect
    mixer.halt(halt_eff)           -- stop all channels
```

**One-shot wrappers — the single-Seq path.** `mix_open_audio`,
`mix_load_wav`, `mix_play_channel`, `mix_load_mus`, `mix_play_music`,
`mix_halt_channel`, `mix_close_audio` mirror `window.ev`'s `sdl_*`
helpers; use them where the chunk handle comes from a prior Seq step.

Worked example: [`examples/test_24_sdl_mixer.ev`](../../examples/test_24_sdl_mixer.ev).

**Graceful degradation.** A missing/failed audio device makes
`Mix_LoadWAV` return NULL → handle 0; `Mix_PlayChannel` on a null
chunk just returns -1. The program runs to a clean exit (silently)
rather than erroring — which is why the demo's CI gate is exit code,
not audio output.

## Adding a new SDL binding

1. Find the function's real exported symbol (`nm -gU lib….dylib | grep`).
   Some `Mix_*` / `SDL_*` names are header macros; the dylib may still
   export a real symbol (SDL2_mixer 2.x exports `Mix_LoadWAV` and
   `Mix_PlayChannel` directly).
2. Add a `claim` wrapper whose body is a single `LibCall` with the
   right `ret(args)` signature string (`i`=int as i64, `p`=pointer,
   `s`=string, `f`=f32, `d`=f64, `v`=void).
3. If a handle must outlive one tick, model the resource as an
   `external type` with an `install ∈ Seq(InstallStep)` body — the
   runtime auto-registers it as an FTI resource (no Rust changes).
   See `docs/guide/ffi-bindings.md`.
