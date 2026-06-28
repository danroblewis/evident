# Golden-standard analysis — predator-prey · Lotka-Volterra

Sample key: `"predator-prey · Lotka-Volterra (coupled functions)"`
(`ide/web/static/app-data.js`).

This is a VALIDATION run that defines the shape for a 33-example fan-out. Every
expert expectation below was derived from the **mathematics of the model**,
independently of and **before** inspecting what our renderer emits. The
current-render section then OBSERVES our `.data.json` substrate (pulled through
the real IDE adapter `ide/web/render.py::RENDERERS`, the same path the product
uses) and scores it against those expectations.

---

## 1. The model

The Evident source:

```evident
fsm predator_prey
    prey ∈ Real := 40.0
    pred ∈ Real := 9.0
    Δprey = _prey * 0.1 - _prey * _pred * 0.01
    Δpred = _prey * _pred * 0.005 - _pred * 0.1
```

`Δprey = prey − _prey`, so this is a **forward-Euler discretization (step h = 1)**
of the Lotka–Volterra system. Writing `x = prey`, `y = pred`:

```
x_{t+1} = x_t + x_t(α − β y_t)         α = 0.1,  β = 0.01
y_{t+1} = y_t + y_t(δ x_t − γ)         γ = 0.1,  δ = 0.005
```

i.e. the continuous ODE it samples is

```
dx/dt = x(α − β y) = x(0.1 − 0.01 y)
dy/dt = y(δ x − γ) = y(0.005 x − 0.1)
```

### Key dynamical facts an expert knows

- **Two equilibria.** The trivial one at the origin `(0, 0)` (a saddle), and the
  **coexistence fixed point** (the interesting one):

  ```
  (x*, y*) = (γ/δ, α/β) = (0.1/0.005, 0.1/0.01) = (20, 10).
  ```

  Note this is NOT the initial condition `(40, 9)` — the orbit *circulates
  around* (20, 10); it does not sit at it.

- **Neutrally stable center.** Linearizing the continuous system at `(x*, y*)`
  gives a Jacobian with **purely imaginary eigenvalues** `±i·√(αγ) = ±i·0.1`.
  The fixed point is a **center**, not a spiral: in the true continuous system,
  trajectories are **closed orbits** (periodic), not decaying or growing spirals.

- **A conserved quantity (first integral).** The continuous flow conserves

  ```
  V(x, y) = δ·x − γ·ln x + β·y − α·ln y
          = 0.005·x − 0.1·ln x + 0.01·y − 0.1·ln y.
  ```

  `dV/dt = 0` along every continuous trajectory. So the orbits are exactly the
  **level sets of V** — a family of nested closed loops encircling (20, 10),
  one loop per value of V ≥ V(x*, y*). V is minimized at the fixed point
  (here V(20,10) = −0.3298) and increases outward.

- **Period.** Small oscillations near the center have angular frequency
  `ω = √(αγ) = 0.1`, i.e. period `T = 2π/ω ≈ 62.8` time units. Larger-amplitude
  orbits have a longer (amplitude-dependent) period — the system is NOT
  isochronous.

- **The discretization is NOT the continuous system — and this matters.**
  Forward-Euler does **not** preserve V. On Lotka–Volterra it injects energy
  every step, so the discrete map spirals **outward**: V grows monotonically and
  the orbit eventually blows up. Verified numerically on THIS model:

  ```
  V(t=0)=−0.2986  V(t=50)=−0.2789  V(t=100)=−0.2406  V(t=200)=−0.1322   (monotone ↑)
  ```

  The true mathematical object (closed orbits, neutral center, conserved V) and
  what a naive integrator produces (an outward spiral that diverges) **disagree**.
  A golden diagram must depict the SOLUTION SPACE of the *system* — the nested
  closed orbits — and must not be fooled into reporting "unbounded / diverges"
  just because forward-Euler at h=1 is a bad integrator. This is the single most
  important subtlety for this example.

---

## 2. What an expert wants to see

Ranked. `[HAVE]` = we have a renderer that maps to this expectation; `[NEW]` =
an expert wants it and we likely lack it (or render something unrelated).

### 1. Phase portrait — the (prey, pred) plane  `[HAVE]`  ★ the lead view

- **Insight.** The entire character of this system lives in the phase plane: a
  family of **nested closed loops** circling the coexistence point (20, 10),
  with a vector field that rotates around it (counter-clockwise: prey rises,
  then predators rise, then prey falls, then predators fall).
- **Expected data (SOLUTION SPACE).** A family of nested closed orbits — one per
  sampled initial condition — **all encircling (20, 10)**. No orbit spirals
  inward or outward (neutral). Along each loop the conserved V is ~constant. The
  fixed point (20, 10) marked as a center. This is **emphatically a
  solution-space view**, NOT a single from-init trajectory: a single loop
  conveys almost none of the structure (you cannot see "center", "neutral", or
  "nested" from one loop).
- **What would be WRONG.** (a) drawing only the orbit from (40, 9); (b) marking
  the *initial condition* as the center; (c) an outward spiral that escapes the
  frame (that is the Euler artifact, not the system).

### 2. Nullcline field — the prey- and pred-nullclines + direction field  `[HAVE renderer, no data substrate]`

- **Insight.** The nullclines are the geometric skeleton that *explains why* the
  orbits are closed. `dx/dt = 0` on `x = 0` and on the horizontal line
  `y = α/β = 10`; `dy/dt = 0` on `y = 0` and on the vertical line
  `x = γ/δ = 20`. The two interior nullclines cross at (20, 10) — that crossing
  IS the fixed point. The four quadrants they cut the plane into each have a
  fixed rotational flow sense.
- **Expected data (SOLUTION SPACE / global structure).** Horizontal nullcline at
  pred = 10, vertical nullcline at prey = 20, their intersection (20, 10), and a
  direction field whose arrows circulate around that intersection. Global, not a
  single run.

### 3. Conserved-quantity contour map — level sets of V  `[NEW]`

- **Insight.** This is the *cleanest possible* picture of the solution space: the
  orbits ARE the contours of V, so a filled/contour plot of
  `V(x,y) = 0.005x − 0.1·ln x + 0.01y − 0.1·ln y` over the positive quadrant
  shows every orbit at once, with the center as the global minimum. It also
  doubles as a correctness oracle: overlay the integrator's trajectory and the
  drift off a contour visualizes the Euler energy injection directly.
- **Expected data (SOLUTION SPACE).** A scalar field V over (prey, pred) with
  nested closed contours bottoming out at (20, 10). No per-trajectory
  integration needed — purely the analytic invariant.

### 4. Time series — prey(t) and pred(t)  `[HAVE]`  (legitimately single-run-flavored, but should show the family)

- **Insight.** The classic out-of-phase oscillation: prey and predator both
  oscillate periodically, predator peak **lags** prey peak by ~quarter period.
- **Expected data.** Two periodic waveforms, predator lagging prey, with a
  roughly constant period (~63 for small orbits). A single initial condition is
  *acceptable* here (a time series is inherently per-trajectory), but the
  honest version overlays a few amplitudes to show period grows with amplitude.
  **Caveat:** under the h=1 Euler map the amplitude *grows* each cycle — a faithful
  time series of the *continuous* system would need a better integrator or a
  much smaller step.

### 5. Period / amplitude relation (or a return map)  `[NEW]`

- **Insight.** Period depends on amplitude (non-isochronous). A plot of period vs
  orbit amplitude (or a Poincaré return map on a section through the fixed point)
  quantifies this and confirms closed orbits (return map = identity line for a
  true center).
- **Expected data (SOLUTION SPACE).** One point per orbit: amplitude → period,
  monotone increasing; or a 1-D return map lying on the diagonal (neutral).

### 6. Fixed-point / equilibrium map  `[HAVE]`

- **Insight.** Locate and classify equilibria: report (20, 10) as a **center**
  (neutral), and (0, 0) as a saddle.
- **Expected data.** ≥1 equilibrium at (20, 10) classified neutral/center; ideally
  the cycle structure (closed orbits) reported as periodic, not as "no cycles".

Lower-value-but-reasonable: `orbit_scatter` (HAVE renderer; would want it to
scatter the closed-loop point cloud), `reachable_region` (HAVE; see below — it is
actively misleading for a neutral center).

---

## 3. Current-render data-check

Offered views for this model (via `analysis._offered_views`): all 25 non-N/A
views are offered. The continuous/oscillator-relevant subset was rendered through
the IDE adapter and its `<out>.data.json` inspected. `phase_portrait` is the
auto-recommended lead view (`_recommend`: deterministic, exactly-2 numeric vars).

| Expert diagram | Our view | data.json? | Verdict |
|---|---|---|---|
| Phase portrait (nested orbits, solution space) | `phase_portrait` | yes | **FAIL — one run, not the space** |
| Nullcline field | `nullcline_field` | **none** | FAIL — PNG only, no inspectable substrate |
| Conserved-quantity contours | — | — | MISSING `[NEW]` |
| Time series | `time_series` | yes | FAIL — degenerate ("pred unbounded, no range to seed") |
| Period/amplitude · return map | — | — | MISSING `[NEW]` |
| Fixed-point map | `fixedpoint_map` | yes | PARTIAL — finds 1 FP but `cycle_count=0`, no center classification |
| Reachable region | `reachable_region` | yes | FAIL — reports `unbounded` (Euler artifact) |
| Orbit scatter | `orbit_scatter` | **none** | FAIL — PNG only |

### The headline check: does the phase portrait show the SOLUTION SPACE?

**FAIL — it shows a single from-init trajectory, not the solution space.**
Observed `phase_portrait.data.json`:

```json
"center":   { "x": 40.0, "y": 9.0 },                 ← the INITIAL CONDITION, not (20,10)
"regime":   "numeric/bounded",
"rendered": { "n": 201, "x": [1.31, 68.34], "y": [0.70, 37.81],
              "symmetric_x": false, "symmetric_y": false },
"reachable":{ "n": 598, "x": [-197.7, 1e+18], "y": [-1e+18, 466.0], "fills_corners": true }
```

Two distinct bugs, both load-bearing for the validation:

1. **One run, not the space.** `rendered.n = 201` is the 201-tick dwell from the
   single seed (40, 9) — `render_phase_portrait.py` pins
   `m.initial_state()` and follows one trajectory. The substrate exposes the
   bbox of ONE orbit, not a family of nested loops. There is no per-initial-
   condition orbit family in the data. This is the canonical
   "seed-from-initial_state ⇒ shows one run, not all initial conditions"
   regression, and it is the failure mode the whole golden effort exists to hunt.

2. **`center` = initial condition, not the fixed point.** The data labels (40, 9)
   as the center; the true center is (20, 10). A reader is told the wrong point
   is the equilibrium.

3. **Euler-divergence pollution of `reachable`.** The `reachable` cloud carries
   `1e+18` / `-1e+18` extents and `occupancy_heatmap` grids out to prey ≈ 2810 —
   forward-Euler at h=1 injects energy (V monotone ↑, verified), so the relational
   reachable set genuinely diverges. `reachable_region` then declares the system
   **`unbounded`** with `unbounded_vars: ["prey"]`. That is true of the *bad
   discretization* but false of the *system*, whose orbits are bounded closed
   loops. Reporting "unbounded" to the user is actively misleading for a neutral
   center.

### Other views

- **`time_series`** — degenerate: `note: "pred is unbounded (no finite range to
  seed)"`, n_trajectories absent. The seeding logic can't find a finite range
  (because the Euler map diverges), so it emits no waveforms. FAIL.
- **`fixedpoint_map`** — PARTIAL win: `fixed_point_count: 1`, `has_equilibria:
  true`. It does find an equilibrium. But `cycle_count: 0`, `cycle_periods: []` —
  the closed orbit is never recognized as a cycle (Euler never closes the loop),
  and there is no neutral/center classification. It also doesn't expose WHERE the
  fixed point is in the data, so we can't confirm it's (20, 10) vs the seed.
- **`nullcline_field`, `solution_space`, `orbit_scatter`, `cobweb`** — render a
  PNG but emit **no `.data.json`**, so the golden harness cannot inspect them.
  For `nullcline_field` specifically this is a gap: the nullcline structure
  (lines at prey=20, pred=10, their crossing) is exactly the kind of analytic
  data a golden check wants to assert on, and it isn't in any substrate.

---

## 4. Roadmap (ranked by value)

1. **★ Phase portrait must depict the solution space, not the from-init run.**
   The lead view should sample a FAMILY of initial conditions (e.g. a fan of
   amplitudes out from the fixed point) and draw the nested closed orbits, with
   the true fixed point (20, 10) marked as the center. The `.data.json` must
   expose the orbit family (multiple loops), and `center` must be the computed
   equilibrium, not the seed. This is THE fix; everything else is secondary.

2. **A conserved-quantity / first-integral contour view `[NEW]`.** For any system
   with a detectable first integral V (Lotka–Volterra, Hamiltonians, energy-
   conserving mechanics), contour V over the plane. This gives the exact
   solution space analytically — no integrator, no Euler divergence — and is the
   single highest-value NEW diagram for this class of model. It also exposes the
   integrator's energy drift as a diagnostic overlay.

3. **Integrator-aware reachability / don't report "unbounded" for a neutral
   center.** The reachable-set and time-series seeding read the forward-Euler
   divergence as a true unbounded system. The analysis should detect a conserved
   quantity (or a center via the Jacobian) and report **neutral/periodic**, not
   `unbounded`. At minimum, distinguish "the system is unbounded" from "this
   discretization diverges."

4. **Nullcline field must emit a `.data.json` substrate `[HAVE renderer]`.** The
   nullcline lines (prey=20, pred=10), their intersection, and the field's
   rotation sense should be inspectable data, not pixels only — both for golden
   checks and for hover/interrogation in the IDE.

5. **Fixed-point classification (center / saddle / node + cycle detection).**
   `fixedpoint_map` should classify (20, 10) as a neutral center and recognize
   the closed orbits as periodic, rather than `cycle_count: 0`.

---

## Appendix — reproduction

```bash
python3 - <<'PY'
import sys, os, tempfile, json
sys.path.insert(0,'ide/web'); sys.path.insert(0,'viz')
from runtime_io import _export
from render import RENDERERS
SRC = open('/dev/stdin').read()  # paste the fsm source
w=tempfile.mkdtemp(); ok,prefix,dr,msg=_export(SRC,w)
smt2,schema=prefix+'.smt2',prefix+'.schema.json'
out=os.path.join(w,'phase_portrait.png')
RENDERERS['phase_portrait'](smt2,schema,out,x_var='prey',y_var='pred')
print(json.dumps(json.load(open(out+'.data.json')),indent=1))
PY
```

Math sanity (independent of renderer): fixed point `(γ/δ, α/β) = (20, 10)`;
`V(x,y)=0.005x−0.1·ln x+0.01y−0.1·ln y` increases monotonically under the h=1
Euler map (−0.299→−0.132 over 200 ticks), confirming the outward-spiral artifact.
