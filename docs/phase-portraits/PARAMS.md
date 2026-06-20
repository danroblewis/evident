# Phase-Portrait Parameters — verified integer fixed-point recurrences

All systems store state scaled by `S` (fixed-point). Divisions use `//`
(integer truncation) in the actual recurrence; a round-to-nearest helper
`rdiv(a,b)` is used to kill the truncation-drift bias that otherwise pulls
sinks/orbits off-center. Screen mapping: `sx = CX + a//S`, `sy = CY - b//S`,
CX=320, CY=240, 640×480 canvas → visible region a,b ∈ [−CX·S, CX·S].

The **coarseness→smoothness fix** is the same everywhere: pick `S` large enough
that `//` keeps precision, and a timestep divisor `DT` (dt = 1/DT) so each Euler
step advances only a small fraction of an orbit → ~120–150 points per lap. The
nonlinear systems additionally use **symplectic (semi-implicit) Euler** — advance
one coordinate, then use the *new* value for the other — so closed orbits stay
closed instead of spiraling.

`rdiv(a,b) = (a + b//2)//b` for a≥0, else `-((-a + b//2)//b)`.

---

## 1. Damped spring / spiral sink  — `math_spring.py`

Continuous: `pos' = vel ; vel' = -k·pos - c·vel`.

```
S=256  DT=16  K=16  C=3  KSCALE=16        (k = K/KSCALE = 1.0, c = C/KSCALE = 0.1875)

acc   = rdiv(-K*pos - C*vel, KSCALE)      # scaled accel
vel'  = vel + rdiv(acc, DT)
pos'  = pos + rdiv(vel', DT)              # semi-implicit
```
Seed `(200*S, 0)`, N=200 (use N≥500 to watch it reach the origin).
Verified: radius decays monotonically 200 → 5 over 600 steps. Max screen jump
15 px. **Portrait:** a clean logarithmic spiral winding inward to the origin.

---

## 2. Van der Pol oscillator — `math_vanderpol.py`  ★ THE STAR

Continuous: `x' = v ; v' = mu·(1 - x²)·v - x`.

```
S=1024  DT=24  MU=1  MUSCALE=1            (mu = 1.0)

omx  = S - rdiv(X*X, S)                   # (1 - x²)·S   — kills the extra S² in X²
term = rdiv(omx * V, S)                   # (1 - x²)·v·S
damp = rdiv(MU*term, MUSCALE)             # mu·(1 - x²)·v·S
A    = damp - X                           # scaled accel
V'   = V + rdiv(A, DT)
X'   = X + rdiv(V', DT)                   # semi-implicit
```
The nonlinear-term trick: `X² = x²·S²`, so divide once by S to get `(1-x²)·S`,
then multiply by V and divide by S again → `(1-x²)·v·S`.

Three seeds, N=400 — all converge to the SAME loop:
- `(0.1·S, 0)`  inside  → spirals **outward**
- `(2.8·S, 0)`  outside → spirals **inward**
- `(0, 3.5·S)`  outside → spirals **inward**

Verified: all three settle to x∈[−1.98,1.98], v∈[−2.66,2.66] (identical to 2 dp).
~150 steps/lap, max jump on the settled loop 0.21 units. **Portrait:** the
limit cycle — distinct seeds inside and outside collapsing onto one closed curve.
This is the most visually striking system.

---

## 3. Lotka–Volterra predator–prey — `math_lotka.py`

Continuous: `x' = a·x - b·x·y ; y' = -c·y + d·x·y`  (x=prey, y=pred, both >0).

```
S=4096  DT=64  A=20 B=10 C=20 D=10  CSCALE=10   (a=2,b=1,c=2,d=1)
fixed point (x*,y*) = (C/D, A/B) = (2.0, 2.0)

xy   = rdiv(X*Y, S)                       # x·y·S   — kills extra S in X·Y
dX   = rdiv(A*X - B*xy, CSCALE)
X'   = X + rdiv(dX, DT)
xy2  = rdiv(X'*Y, S)                       # symplectic: use updated X
dY   = rdiv(-C*Y + D*xy2, CSCALE)
Y'   = Y + rdiv(dY, DT)
```
Bilinear trick: `X·Y = x·y·S²`, divide once by S → `x·y·S`.

Three seeds at prey=2.0, pred ∈ {1.0, 0.5, 0.2}, N=600 → nested loops.
Verified: closure drift 0.001–0.013 (orbits close, no spiral). **Portrait:**
nested closed orbits around (2,2); inner orbit near-circular, outer orbits show
the asymmetric boom/crash shape.

---

## 4. Pendulum (separatrix) — `math_pendulum.py`

Continuous: `θ' = ω ; ω' = -(g/L)·sin θ`.

```
S=4096  TBL=4096  DT=40  GL=1  GLSCALE=1   (g/L = 1)
K2PI = round(2π·S) = 25736
SIN[i] = round(sin(i·2π/TBL)·S)  for i in 0..TBL-1   (precomputed table)

isin(TH): idx = (TH*TBL)//K2PI ; idx %= TBL ; return SIN[idx]   # = sin(θ)·S
acc  = rdiv(-GL*isin(TH), GLSCALE)
OM'  = OM + rdiv(acc, DT)
TH'  = TH + rdiv(OM', DT)                  # symplectic
```
sin via integer LUT: the table holds `sin·S` over one period; the angle index is
`θ·S · TBL / (2π·S) = TH·TBL // K2PI`, reduced mod TBL.

Seeds: librations `(0, ω·S)` for ω ∈ {0.6, 1.2, 1.9}; rotations
`(-π·S, ±2.4·S)`. N=500.
Verified: librations close (drift 0.003–0.017); rotations advance θ monotonically
~34 rad (over the top). **Portrait:** nested closed "eyes" at center (libration)
bounded by the separatrix, with rotation bands sweeping above and below.

---

## Vector field (all systems) — `vector_field(lo_a,hi_a,lo_b,hi_b,G=11,L=8)`

G×G grid over the visible region. At each grid point take ONE step; the local
flow direction is the delta, normalized to a fixed arrow length L (integer):
```
A0 = lo_a + (hi_a-lo_a)*i//(G-1)          # i,j in 0..G-1
B0 = lo_b + (hi_b-lo_b)*j//(G-1)
(A1,B1) = step(A0,B0) ; dA=A1-A0 ; dB=B1-B0
mag = isqrt(dA*dA + dB*dB)                 # integer magnitude
arrow_dA = dA*L*S // mag ; arrow_dB = dB*L*S // mag   # uniform-length arrow (scaled)
tail = (A0,B0)  head = (A0+arrow_dA, B0+arrow_dB)     # then map to screen
```
For van der Pol the field circulates around the origin (up-left, down-right) —
the rotational flow that carries every trajectory onto the limit cycle.
