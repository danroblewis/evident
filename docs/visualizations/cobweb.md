# Cobweb plot

A reimplementable specification of the **cobweb** visualization for an Evident
program. Reads only a transition oracle (SMT solver over the program's
difference equation) and the variable schema. Shared primitives
(`successor`, `successors`, `initial_state`, `reachable`, `state_vars`,
`numeric_vars`, `enum_variants`, `facet_var`, variable ranking) are defined in
`docs/visualizations/00-core-machinery.md` — this doc references them by name
and does not re-document them.

---

## 1. What it shows

A cobweb (Lemerey staircase) diagram is the classic graphical analysis of a
**one-dimensional map** `x_{n+1} = f(x_n)`. It answers: *given one scalar state
coordinate, what does iterating the transition do to it?* — converge to a fixed
point, diverge, settle into a 2-cycle, oscillate, or fan out
(nondeterministic). It exposes fixed points (where the map crosses the diagonal
`y = x`) and their stability (slope of `f` at the crossing) directly as
geometry.

**When to use it.** Best when the program has a single dominant *numeric* scalar
whose dynamics are interesting on its own (counters, accumulators, decaying
oscillators, van-der-Pol-like fixed-point systems). For purely *discrete*
programs it still works by ordinalizing an enum (treat the enum's variants as
`0,1,2,…`), but a state line / transition graph is usually more honest there.
For *mixed* high-D programs it projects onto the one chosen scalar and either
**facets** by a low-cardinality categorical variable (small multiples) or holds
the other variables fixed at a neutral state — see §6.

This is inherently a 1-D projection: a cobweb cannot show coupling between two
state variables. It is the right tool exactly when one scalar dominates the
dynamics.

---

## 2. The object

Let `p` be the chosen primary scalar variable with an **ordinal encoding**
`o(·)` (identity for ints/reals; `variant → index` for enums; `false→0,
true→1` for bools). Holding every other state variable at a fixed base state
`b`, the program's transition relation induces a (possibly set-valued) map

```
F(x) = { o(s'[p]) : s' is a successor of the state (b with p = o⁻¹(x)) }.
```

The cobweb plot is a square `[lo,hi]²` containing:

1. **The map graph** — the point set `{ (x, y) : x ∈ grid, y ∈ F(x) }`. When
   `F` is single-valued and continuous it reads as a curve; when set-valued
   (nondeterministic), every `x` shows a vertical *fan* of its possible images.
2. **The diagonal** `y = x` (dashed). Intersections of the map with the
   diagonal are the **fixed points** of `F`.
3. **A staircase orbit** — the polyline produced by iterating `F` from a seed
   `x₀`: alternate vertical segments (to the map) and horizontal segments (back
   to the diagonal). Its winding pattern shows the qualitative dynamics
   (inward spiral = stable, outward = unstable, rectangle = 2-cycle).

The axes are equal-aspect and share identical limits so the diagonal is exactly
45°; that is what makes fixed-point/stability reading valid.

---

## 3. Inputs (core-machinery primitives consumed)

- `state_vars`, `numeric_vars` — to rank and pick the primary scalar.
- `enum_variants[name]` — ordinal encoding of enum / facet variables.
- `facet_var(max_card, max_change)` — to choose the small-multiples panel
  variable.
- `initial_state()` — the neutral base state `b` (the held values of all
  non-primary, non-facet variables).
- `reachable(limit)` — to discover the empirically reached value spread of the
  primary scalar, used to decide the x-range (§4 step 3).
- `successor(state)` — one deterministic step, used to build the staircase
  orbit.
- `successors(state, limit)` — ALL next states of a pinned state, used to sample
  the map so a nondeterministic fan shows every branch.

Every value of `F` comes from **solving the transition**; nothing about the
dynamics is hardcoded.

---

## 4. Algorithm

### Step 1 — pick the primary scalar `p` and its mode
- If `numeric_vars` is non-empty: `p = numeric_vars[0]`, `mode = int`
  (a true 1-D map).
- Else, first enum in `state_vars`: `mode = enum-ordinal`.
- Else, first ranked `state_var`: `mode = enum-ordinal`.
- If there are no state variables at all → render a "no state var to cobweb"
  placeholder and stop.

### Step 2 — build the base (held) state `b`
`b = initial_state()` if available; otherwise a type-default neutral state
(`int→0`, `real→0.0`, `bool→false`, `enum→variants[0]`, `string→""`). Every
variable other than `p` (and the facet var, §5) is pinned to `b`'s value across
the whole plot.

### Step 3 — choose the sampling grid and range
- **enum/ordinal `p`**: grid = `0,1,…,|variants|−1`; mark *bounded*.
- **numeric `p`**: probe the empirically reached values via `reachable(limit≈400)`,
  collecting the integer values of `p` over the reached states.
  - If at least **4 distinct** values were reached AND their span
    `hi−lo ≤ 64`: treat it as a small **bounded counter**. Grid the exact range
    padded by `max(1, span/2)` on each side, step 1; mark *bounded*.
  - Otherwise (few clustered values — e.g. a large fixed-point system seeded at
    the origin reaches only its fixed point — or a wide span): use a **generous
    symmetric window** `[-3200, 3200]` with `121` equally spaced samples; mark
    *unbounded*. The ≥4-distinct guard prevents mistaking a converged
    fixed-point cluster for a small bounded range.

### Step 4 — sample the map graph `F`
For each `x` in the grid: form `state = b` with `p` set to `o⁻¹(x)`; for every
successor `s'` in `successors(state)`, emit the point `(x, o(s'[p]))`. The
result is parallel arrays `(xs, ys)` in ordinal space. Multiple `ys` per `x`
encode the nondeterministic fan. If the transition is UNSAT over the whole grid
(`xs` empty), render "transition unsat over sampled range" in that panel.

### Step 5 — compute the square frame
`lo = min(xs ∪ ys)`, `hi = max(xs ∪ ys)`, padded by `0.05·(hi−lo) + 0.5` on each
side. Draw the diagonal from `(lo,lo)` to `(hi,hi)`. Force equal aspect and
`xlim = ylim = [lo,hi]`.

### Step 6 — choose the staircase seed `x₀`
- enum-ordinal: `o(b[p])` (start from the held/initial value).
- numeric bounded: `o(b[p])` (or `lo` if absent).
- numeric unbounded: a fixed near-cycle seed (`2000`) if it lies in `[lo,hi]`,
  else the window midpoint. This places the orbit where a large limit-cycle
  system shows its winding.

### Step 7 — build the staircase orbit
Start on the diagonal: emit `(x₀, x₀)`. Then iterate up to `steps` (default 60):
- `s' = successor(b with p = o⁻¹(x))`; if `None`, stop.
- `y = o(s'[p])`.
- Emit `(x, y)` — vertical segment to the map.
- Emit `(y, y)` — horizontal segment back to the diagonal.
- **Cycle cut-off**: if `round(y,6)` has been seen before, stop (we have closed
  a cycle or hit a fixed point). Otherwise record it and set `x ← y`.

Draw the orbit polyline; mark the seed point. For a single-valued *continuous*
(unbounded numeric) branch where every grid `x` is distinct, also draw the map
as a faint connecting line for readability (in addition to the markers).

### Step 8 — axis decoration
For enum-ordinal mode, label both axes' ticks `0,1,…` with the variant names
(rotated on x). For numeric mode, label `n` (x) vs `n+1` (y).

---

## 5. Variable → channel mapping

A cobweb is intrinsically 1-D, so **both axes carry the same variable** `p`
(`x_n` on x, `x_{n+1}` on y). This consumes the entire position channel, by
Cleveland–McGill the most accurate channel — appropriate for the quantity the
plot is *about*.

| channel        | variable                                          | reasoning |
|----------------|---------------------------------------------------|-----------|
| x (`n`)        | primary scalar `p`                                | quantitative → position |
| y (`n+1`)      | primary scalar `p` (its image)                    | the map's output, same scale |
| color          | fixed roles: map=blue, diagonal=grey, orbit=red   | encodes *plot element*, not data |
| facet (panels) | a categorical var (≤ ~5 values), `≠ p`            | categorical → small multiples |

To add a second dimension honestly, the cobweb **facets**: `facet_var()` picks a
low-cardinality categorical variable (an enum mode flag, a bool) that changes
infrequently; one cobweb panel is drawn per value, with `p` ordinalized on both
axes and the facet var pinned to that value in each panel's base state. This is
Tufte/Bertin small multiples — show a high-D system as a row of comparable 1-D
maps rather than overlaying incomparable curves. The facet is suppressed if it
collides with `p`.

All variables other than `p` and the facet are **held** at `b` and listed in the
subtitle ("held: …") so the projection is explicit.

---

## 6. Degradation & edge cases

- **No state variables** → placeholder "no state var to cobweb".
- **Numeric program (ideal)** → true 1-D map over the grid; bounded counters get
  an exact integer grid, large fixed-point/limit-cycle systems get the generous
  window with a connected curve and a near-cycle seed.
- **Discrete (enum/bool) program** → ordinalize: variants map to `0,1,…`, ticks
  show variant names. The cobweb degenerates to a small staircase on the
  variant lattice — readable, but a state line / transition graph is usually
  preferable for pure discrete dynamics.
- **Mixed / high-D program** → project onto the single dominant scalar; recover
  a second dimension via faceting on a categorical variable; all other variables
  are held fixed and disclosed in the subtitle.
- **Set-valued (nondeterministic) transition** → every `x` shows the full fan of
  its images (markers, not a curve), because the map is sampled with
  `successors`, not `successor`.
- **Transition UNSAT over the sampled range** → panel shows an explicit
  "transition unsat over sampled range" message instead of an empty square.
- **Converged-at-origin trap** → the ≥4-distinct-reached-values guard prevents
  a system that only reaches its fixed point from being gridded as a tiny range;
  it falls back to the wide window so the surrounding dynamics are visible.

---

## 7. Parameters (defaults)

| parameter                       | default | meaning |
|---------------------------------|---------|---------|
| `reachable` limit               | 400     | states probed to estimate the reached value spread |
| bounded-counter distinct-values | ≥ 4     | minimum distinct reached values to treat range as bounded |
| bounded-counter max span        | ≤ 64    | max `hi−lo` to grid exactly (step 1) |
| bounded grid pad                | `max(1, span/2)` | padding added each side of the exact range |
| unbounded window                | `[-3200, 3200]` | symmetric x-range for large/fixed-point systems |
| unbounded sample count          | 121     | equally spaced grid points in the window |
| staircase steps                 | 60      | max orbit iterations before truncation |
| unbounded seed                  | 2000    | orbit seed (else window midpoint) |
| cycle-detection rounding        | 6 dp    | precision for the "already seen" orbit cut-off |
| facet max cardinality           | ~5      | max distinct values to facet (else no faceting) |
| facet columns                   | 3       | panels per row in the small-multiples grid |

---

## 8. References

- **Cobweb / Lemerey staircase diagram.** R. L. Devaney, *An Introduction to
  Chaotic Dynamical Systems* — graphical analysis of 1-D maps, fixed points, and
  stability via the slope of `f` at diagonal crossings.
- **Logistic-map dynamics.** R. M. May, "Simple mathematical models with very
  complicated dynamics," *Nature* (1976) — the canonical use of cobwebbing to
  read convergence, period-doubling, and chaos.
- **Channel effectiveness.** W. S. Cleveland & R. McGill, "Graphical perception"
  (1984); J. Mackinlay, "Automating the design of graphical presentations"
  (1986) — position is the most accurate channel, justifying spending both axes
  on the primary scalar.
- **Small multiples.** E. R. Tufte, *Envisioning Information* (1990); J. Bertin,
  *Sémiologie Graphique* (1967) — faceting a high-D system into comparable 1-D
  panels.
