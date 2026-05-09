# Phase 2: Plugin migrations

After Phase 1 lands (FFI primitive + effect dispatcher + trace
shim), each existing plugin gets ported to an Evident library.

## Parallel execution

Tasks 2.1-2.4 are independent — each touches a different plugin's
code in Rust and creates a different library directory. They CAN run
in parallel via worktrees:

```
worktree branch                  task                        owner
phase-2-stdio                    2.1 Stdin/Stdout            agent A
phase-2-sdl                      2.2 SDL plugin              agent B
phase-2-audio                    2.3 Audio plugin            agent C
phase-2-shader                   2.4 Shader plugin           agent D
phase-2-cleanup (after 2.1-2.4)  2.5 Plugin abstraction      agent E
```

To launch parallel agents, the main session would:

```
Agent({
    description: "Phase 2.1 stdio migration",
    isolation: "worktree",
    prompt: "Read docs/plans/02-plugin-migrations/01-stdio.md and execute it.",
})
```

Sent in a single message with one Agent call per task for true
parallelism. Each worktree merges back into main when its acceptance
criteria are met.

## Acceptance gate before 2.5

All four migrations must land cleanly before 2.5 (which removes the
plugin abstraction code entirely). If any migration is incomplete,
the abstraction stays.

## Per-task plans

- `01-stdio.md` — Stdin/Stdout/CharInput/CharOutput → Effects + library wrappers
- `02-sdl.md` — SDLInput/SDLOutput → stdlib/sdl/ Evident library
- `03-audio.md` — SDL_audio plugin → stdlib/audio/
- `04-shader.md` — SDLShaderOutput → stdlib/shader/
- `05-remove-plugin-abstraction.md` — strip plugin trait + lifecycle
  + executor dispatch from `executor.rs`
