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

The draws use single-effect `render_fill_rect` (one `Effect` per point, no nested
`Seq`), so the whole portrait is one flat `effects` list. Rendering runs on the
slow Z3 path (the correctness oracle): `EVIDENT_NO_JIT=1`, or automatically — the
functionizer defers Seq-element draws to the oracle.

## Reproduce

```sh
python3 docs/phase-portraits/gen_vdp.py 300 > /tmp/vdp.ev   # (the generator writes /tmp/portraits/vdp.ev)
EVIDENT_NO_JIT=1 ./runtime/target/release/evident effect-run /tmp/portraits/vdp.ev
```

A 300-point, two-trajectory van der Pol portrait solves+renders in ~7s on the
oracle. (The generators here are verbose unrolled scalar form — a clean
`Seq`/`edges` example would need `++`-splice of built effect Seqs to assemble
the draw list idiomatically; that's a noted follow-up.)

## Runtime work this surfaced

Building these stress-tested the runtime and produced durable fixes:

- **Negative integer division was UNSAT.** The constant-folder truncated
  (`-5/3 = -1`) while Z3's `div` floors (`-5 div 3 = -2`); the contradiction
  silently poisoned any computation that went negative then divided. Fixed
  (`div_euclid`, matches the oracle). This was *the* blocker for trajectories.
- **Differential harness** (`runtime/tests/differential.rs`): runs a query both
  ways (JIT vs the slow Z3 oracle) and diffs bindings — the oracle is ground
  truth. It caught a JIT heap-corruption on nested `Seq` access.
- **Functionizer is correctness-bounded**: nested `Seq` index and Seq-element
  draw calls defer to the oracle (`has_known_translator_gap`) rather than emit
  wrong output. Toggle the functionizer off entirely with `EVIDENT_NO_JIT=1` or
  `EvidentRuntime::set_functionize_enabled(false)`.
- **No perf pathology.** An apparent super-linear slow-path blowup was orphaned
  `evident` processes contending for CPU — a 120-point portrait solves a tick in
  ~1.3s. The slow path is the oracle and is always reachable.
