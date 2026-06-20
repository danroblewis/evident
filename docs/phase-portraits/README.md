# Phase portraits in Evident

A **phase portrait** is the picture of a dynamical system: trajectories and the
vector field, drawn in state space (e.g. position vs velocity). Evident renders
them natively — the recurrence that defines the system *is* the constraint, and
the solver computes the whole trajectory; the picture is the proof.

Three canonical systems were built and rendered (640×480, SDL via `packages/sdl/`):

| System | What it shows | Archetype |
|---|---|---|
| **Van der Pol** | two trajectories (one from inside, one from outside) spiral onto the *same* closed loop | limit cycle |
| **Lotka–Volterra** (predator–prey) | three nested closed orbits around the (2,2) fixed point | center / conservative orbits |
| **Damped spring** | trajectories spiral inward to the origin | spiral sink |
| **Pendulum** | nested librations ("eyes"), inner circular, outer lens-shaped near the separatrix | separatrix |

## How it's done

Each trajectory is an integer **fixed-point** recurrence (state scaled by `S`),
computed in a single solve, then each point is drawn as a small filled rect. The
smoothness recipe (see `PARAMS.md`, verified numerically in the `math_*.py`):

- scale state by `S` so integer division keeps precision;
- a timestep divisor `DT` (dt = 1/DT) so each Euler step advances a small
  fraction of an orbit → ~120–150 points per lap;
- `rdiv(a,b) = (a + b/2) / b` for round-to-nearest (kills truncation drift) —
  this works for *all* signs because Evident's `/` is Euclidean (floor);
- symplectic (semi-implicit) Euler for the conservative systems so closed orbits
  stay closed;
- the nonlinear terms (van der Pol's `(1−x²)v`, Lotka–Volterra's `x·y`) are done
  in scaled integer arithmetic — see `PARAMS.md`.

### Sine (the pendulum)

Z3 has **no usable `sin`**: `(sin 1.0)` type-checks but `(get-value …)` returns
the *symbolic* term, not a decimal, and Z3 answers `unknown` to even trivial
bounds (`0.84 < sin(1.0) < 0.85`). It's a logic engine, not a numerics engine —
transcendentals aren't algebraic, so its reasoning is incomplete and here inert.

Two integer alternatives, both giving actual numbers:
- a precomputed **sine LUT** (`Seq(Int)` indexed by angle) — correct, but the Z3
  array theory makes it slow (a 64-entry table with ~800 selects didn't solve a
  tick in 90s);
- a **Bhaskara polynomial** — `sin(x)·S ≈ 16·u·S / (5π²·S² − 4u)` with
  `u = |x|·(π−|x|)`, valid on `[−π,π]` (the libration range, no range reduction
  needed). Pure arithmetic, ~10s for the full four-libration portrait, accurate
  to **0.0016** on `[−π,π]`. This is what `gen_pendulum.py` uses.

The draws use single-effect `render_fill_rect` (one `Effect` per point, no nested
`Seq`), so the whole portrait is one flat `effects` list. It runs fine on the
functionizer (JIT); `EVIDENT_NO_JIT=1` forces the slow Z3 oracle, which is useful
for differential checking and is always available.

## Reproduce (on any machine)

### Prerequisites
- **Rust** (`cargo`) — builds the runtime.
- **Z3** dev library — the `z3-sys` crate links the *system* libz3, so it must be
  installed: `apt install libz3-dev` (Debian/Ubuntu) or `brew install z3` (macOS).
- **SDL2** runtime library — `dlopen`'d at run time as `libSDL2-2.0.so.0`:
  `apt install libsdl2-2.0-0` or `brew install sdl2`. (Needed only to render, not
  to build.)
- **Python 3** — runs the generators.
- An **X display** to draw into (a real one, or headless `Xvfb`), plus
  **ImageMagick** (`import`) if you want to screenshot to PNG.

### 1. Build the runtime
```sh
cd runtime && cargo build --release      # -> runtime/target/release/evident
```

### 2. Generate a portrait (each generator prints the `.ev` to stdout)
```sh
python3 docs/phase-portraits/gen_vdp.py 300  > /tmp/vdp.ev      # van der Pol limit cycle
python3 docs/phase-portraits/gen_spring.py   > /tmp/spring.ev   # spiral sink
python3 docs/phase-portraits/gen_lotka.py    > /tmp/lotka.ev    # predator-prey orbits
python3 docs/phase-portraits/gen_pendulum.py > /tmp/pendulum.ev # pendulum eyes
```
The optional integer arg is points-per-trajectory (default in each file).

### 3. Run it (opens an SDL window; runs on the JIT — no env var needed)
```sh
./runtime/target/release/evident effect-run /tmp/vdp.ev
```
The first solve takes ~1–9s (it computes the whole trajectory before the first
frame); the window then redraws the static portrait each tick until it exits.

### 4. Headless render to PNG (Xvfb + ImageMagick)
```sh
Xvfb :99 -screen 0 1280x800x24 &            # only if you have no display
export DISPLAY=:99
./runtime/target/release/evident effect-run /tmp/vdp.ev &
sleep 12                                     # WAIT past the first solve, or the window is blank
import -display :99 -window root /tmp/vdp.png
pkill -x evident                             # -x (exact name); NOT -f (it matches the repo path)
```
Two gotchas, both learned the hard way: screenshot *after* the first solve (the
window is transparent until the first `present`), and kill with `pkill -x evident`
— `pkill -f evident` matches the working-directory path and kills unrelated
processes (and your own shell).

(These generators are the verbose unrolled-scalar form. A clean `Seq`/`edges`
example needs the coindexed-write-into-`Seq(Effect)` gap fixed first — a noted
follow-up.)

## Runtime work this surfaced

Building these stress-tested the runtime and produced durable fixes:

- **Negative integer division was UNSAT.** The constant-folder truncated
  (`-5/3 = -1`) while Z3's `div` floors (`-5 div 3 = -2`); the contradiction
  silently poisoned any computation that went negative then divided. Fixed
  (`div_euclid`, matches the oracle). This was *the* blocker for trajectories.
- **Differential harness** (`runtime/tests/differential.rs`): runs a query both
  ways (JIT vs the slow Z3 oracle) and diffs bindings — the oracle is ground
  truth. Toggle the functionizer off with `EVIDENT_NO_JIT=1` or
  `EvidentRuntime::set_functionize_enabled(false)`.
- **The functionizer is actually correct on Seq draws** — verified two ways: the
  differential harness shows JIT bindings match the oracle on every Seq shape it
  compiles, and instrumented dispatch shows the JIT emits byte-identical SDL
  LibCalls. An earlier gate that deferred Seq-element draws to the oracle was a
  **misdiagnosis** (every "blank render" was a screenshot-timing artifact on the
  shared Xvfb, not a JIT difference) and was a ~200× pessimization; it was
  removed. The off-switch + harness are the durable correctness tools.
- **No perf pathology.** An apparent super-linear slow-path blowup was orphaned
  `evident` processes contending for CPU (and the now-removed gate forcing the
  oracle). A 120-point portrait solves a tick in ~1.3s on the oracle and far
  faster on the JIT. The slow path is the oracle and is always reachable.
