# Phase portraits in Evident

A **phase portrait** is the picture of a dynamical system: trajectories and the
vector field, drawn in state space (e.g. position vs velocity). In Evident a
daemon written as an `fsm` **is** a dynamical system — its body relates the
previous state (`_state`) to the next (`state`) — so the portrait is intrinsic.
See [`docs/design/phase-portraits.md`](../design/phase-portraits.md) for the full
"the proof is the picture" thesis.

## The generic tool

```sh
evident phase-portrait <daemon.ev> --axes a,b [--seeds "a,b;a,b"] [--steps N] \
                       [--text] [--svg PATH]
```

It reads **no hardcoded dynamics**. Given a daemon and two state axes, it:

1. **integrates trajectories** from each seed by repeatedly querying the
   transition (pin `_a,_b`, solve for `a,b`, repeat);
2. **samples a grid** and queries the transition at each cell to draw the
   **vector field** (the displacement, normalized to a fixed arrow length);
3. **renders** both, auto-ranging the view to the trajectories' extent.

Because the dynamics come from *querying the runtime*, the same tool draws any
daemon. The implementation is `runtime/src/viz.rs`.

### Three output modes

| Flag | Output | Use |
|---|---|---|
| *(default)* | live **SDL** window | interactive viewing on a display |
| `--text` | **ASCII** to stdout (arrow-glyph field + per-seed trajectory glyphs) | headless terminals, quick check, CI logs |
| `--svg PATH` | **SVG file** (`<line>`/`<circle>`, dark theme, per-seed colors) | crisp committable artifacts; no display needed |

`--text` and `--svg` are fully headless (no SDL, no display, no screenshot) and
produce byte-identical results on macOS and Linux — they draw from the same
`arrows`/`trajs` data the SDL path uses.

To make a committable image on demand, run a daemon with `--svg out.svg` — the
tool never writes anywhere unless you pass an explicit path.

## The four example daemons

Each is ~7 lines of dynamics in `examples/daemons/` — the *only* thing that
differs between systems:

| Daemon | Archetype | Run |
|---|---|---|
| `spring.ev` | spiral sink | `evident phase-portrait examples/daemons/spring.ev --axes state.pos,state.vel --seeds "180,0;120,40;60,90"` |
| `vanderpol.ev` | limit cycle | `evident phase-portrait examples/daemons/vanderpol.ev --axes state.x,state.v --seeds "2867,0;123,0;0,2700" --steps 320` |
| `lotka.ev` | nested closed orbits | `evident phase-portrait examples/daemons/lotka.ev --axes state.x,state.y --seeds "8192,6554;8192,4096;8192,2048" --steps 300` |
| `pendulum.ev` | librations / separatrix | `evident phase-portrait examples/daemons/pendulum.ev --axes state.th,state.om --seeds "0,2048;0,4096;0,6144;0,7782" --steps 300` |

The daemons use integer **fixed-point** arithmetic (state scaled by `S`) with
symplectic Euler in explicit `_state` form, so each transition is determined (one
successor per state) and the conservative orbits stay closed.

### Sine, without a working `sin` (the pendulum)

Z3 has **no usable `sin`**: `(sin 1.0)` type-checks but `(get-value …)` returns
the *symbolic* term, not a decimal, and Z3 answers `unknown` to even trivial
bounds (`0.84 < sin(1.0) < 0.85`). It's a logic engine, not a numerics engine —
transcendentals aren't algebraic, so its reasoning is incomplete and here inert.
The pendulum daemon therefore computes sine with a **Bhaskara polynomial** —
`sin(x)·S ≈ 16·u·S / (5π²·S² − 4u)` with `u = |x|·(π−|x|)`, valid on `[−π,π]`
(the libration range, no range reduction), accurate to **0.0016**. Pure
arithmetic, in six lines of the daemon.

## Reproduce (on any machine)

**Prerequisites:** Rust (`cargo`); **Z3** dev library (the `z3-sys` crate links
the system libz3 — `apt install libz3-dev` / `brew install z3`); **SDL2** runtime
(`dlopen`'d as `libSDL2-2.0.so.0` — `apt install libsdl2-2.0-0` / `brew install
sdl2`); an X display (real or headless `Xvfb`), and ImageMagick (`import`) for PNG
capture.

```sh
cd runtime && cargo build --release && cd ..          # -> runtime/target/release/evident
./runtime/target/release/evident phase-portrait examples/daemons/spring.ev \
    --axes state.pos,state.vel --seeds "180,0;120,40;60,90"
```

**Headless, no display needed (preferred):** add `--svg out.svg` (a file you can
commit / view anywhere) or `--text` (ASCII straight to the terminal). Neither
needs SDL, an X server, or a screenshot tool — they only require Rust + Z3.

**SDL screenshot capture (only if you want the windowed render as a raster):**
`Xvfb :99 ... &` then `DISPLAY=:99`, run in the background, `sleep` a few seconds
past the first solve, `import -display :99 -window root out.png`, and `pkill -x
evident` (exact name — `-f` matches the repo path and kills unrelated processes).
On macOS there is no Xvfb; use `--svg`/`--text` instead.

Write your own: any `fsm` whose body computes the next `state` from `_state`
works — point the tool at it with `--axes`.

## Runtime work this surfaced

Building these stress-tested the runtime and produced durable fixes:

- **Negative integer division was UNSAT.** The constant-folder truncated
  (`-5/3 = -1`) while Z3's `div` floors (`-5 div 3 = -2`); the contradiction
  silently poisoned any computation that went negative then divided. Fixed
  (`div_euclid`). This was *the* blocker for trajectories.
- **Differential harness** (`runtime/tests/differential.rs`): runs a query both
  ways (functionizer vs the slow Z3 path) and diffs bindings — the slow path is
  ground truth. Toggle the functionizer off with `EVIDENT_NO_JIT=1` or
  `EvidentRuntime::set_functionize_enabled(false)`.
- **The functionizer is correct on Seq draws** — verified via the differential
  harness (bindings match) and instrumented dispatch (byte-identical SDL
  LibCalls). An earlier gate deferring Seq-element draws to the slow path was a
  misdiagnosis (a screenshot-timing artifact, not a JIT difference) and a ~200×
  pessimization; it was removed.
