#!/usr/bin/env python3
"""
Lotka-Volterra predator-prey -- nested CLOSED orbits.

Continuous system (x = prey, y = predator), both > 0:
    x' =  a*x - b*x*y
    y' = -c*y + d*x*y

Trajectories are closed loops around the fixed point (x*, y*) = (c/d, a/b).
There is a conserved quantity, so orbits do NOT spiral -- they must close on
themselves. That is the test of correctness: after one lap the curve returns to
its start (no drift).

----------------------------------------------------------------------------
FIXED-POINT INTEGER recurrence.

State (X, Y) scaled by S:  X = x*S, Y = y*S.

The bilinear term x*y in scaled units is  X*Y = x*y*S^2  -- a factor S too big
(want x*y*S). So:
    xy_scaled = (X * Y) // S          # = x*y*S

Coefficients a,b,c,d as ratios over CSCALE:
    x' = a*x - b*(x*y)   -> scaled:  (A*X - B*xy_scaled) // CSCALE
    y' = -c*y + d*(x*y)  -> scaled:  (-C*Y + D*xy_scaled) // CSCALE

Euler with timestep 1/DT.  To keep orbits CLOSED (no numerical drift inward or
outward) we use the SYMPLECTIC (semi-implicit) update: advance X first, then use
the NEW X in the predator equation. Plain Euler spirals outward; symplectic
Euler conserves the invariant to first order and the loops stay closed.

Fixed point at (x*,y*) = (c/d, a/b) = (C/D, A/B). With the constants below:
    x* = 20/10 = 2.0 ,  y* = 20/10 = 2.0    (centered, nice)

Parameters:
    S      = 4096
    DT     = 64
    A=20 B=10 C=20 D=10   (over CSCALE=10  -> a=2,b=1,c=2,d=1)
    CSCALE = 10
"""

S      = 4096
DT     = 64
A, B, C, D = 20, 10, 20, 10
CSCALE = 10

def rdiv(a, b):
    if a >= 0:
        return (a + b // 2) // b
    return -((-a + b // 2) // b)

def step(X, Y):
    xy = rdiv(X * Y, S)                       # x*y*S
    dX = rdiv(A * X - B * xy, CSCALE)         # x' scaled
    X_n = X + rdiv(dX, DT)
    # symplectic: use updated X in predator eqn
    xy2 = rdiv(X_n * Y, S)
    dY = rdiv(-C * Y + D * xy2, CSCALE)       # y' scaled
    Y_n = Y + rdiv(dY, DT)
    return X_n, Y_n

def simulate(X0, Y0, N):
    X, Y = X0, Y0
    traj = [(X, Y)]
    for _ in range(N):
        X, Y = step(X, Y)
        traj.append((X, Y))
    return traj

def ascii_plot(trajs, title):
    W, H = 70, 30
    grid = [[' '] * W for _ in range(H)]
    XMAX, YMAX = 5.0, 5.0   # both populations in [0,5]
    # axes at origin (bottom-left quadrant since pops>0)
    marks = '.oxX*#'
    for ti, traj in enumerate(trajs):
        ch = marks[ti % len(marks)]
        for (X, Y) in traj:
            xr = X / S; yr = Y / S
            sx = int(xr / XMAX * (W - 1))
            sy = int(H - 1 - yr / YMAX * (H - 1))
            if 0 <= sx < W and 0 <= sy < H:
                grid[sy][sx] = ch
    # mark fixed point
    fx = int(2.0/XMAX*(W-1)); fy = int(H-1-2.0/YMAX*(H-1))
    if 0<=fx<W and 0<=fy<H: grid[fy][fx]='+'
    print(title)
    print('\n'.join(''.join(r) for r in grid))

if __name__ == '__main__':
    import math
    N = 600
    seeds = [
        (int(2.0*S), int(1.0*S)),   # close orbit
        (int(2.0*S), int(0.5*S)),   # bigger orbit
        (int(2.0*S), int(0.2*S)),   # biggest orbit
    ]
    print(f"S={S} DT={DT} A={A} B={B} C={C} D={D} CSCALE={CSCALE} N={N}")
    print(f"fixed point (x*,y*) = ({C/D},{A/B})")
    trajs = [simulate(*s, N) for s in seeds]
    for si,(s,t) in enumerate(zip(seeds,trajs)):
        # closure test: find return-to-start. measure drift between first point
        # and the nearest approach after half the run.
        x0,y0 = t[0]
        best=min(((p[0]-x0)**2+(p[1]-y0)**2, i) for i,p in enumerate(t[N//3:],N//3))
        drift = math.sqrt(best[0])/S
        xs=[p[0]/S for p in t]; ys=[p[1]/S for p in t]
        print(f"seed{si} ({s[0]/S:.1f},{s[1]/S:.1f}): "
              f"x in [{min(xs):.2f},{max(xs):.2f}] y in [{min(ys):.2f},{max(ys):.2f}] "
              f"closure drift={drift:.3f} (loop closes if small)")
    print("\nFirst 6 steps seed0 (prey,pred):")
    for (X,Y) in trajs[0][:6]:
        print(f"   ({X/S:6.3f}, {Y/S:6.3f})")
    ascii_plot(trajs, "\nLotka-Volterra closed orbits (prey x-axis, pred y-axis, +=fixed pt):")

# ---------------------------------------------------------------------------
# VECTOR FIELD (integer). G x G grid spanning the visible region.
# At each grid point take ONE step of the recurrence to get the local flow,
# then normalize to a fixed arrow length L (integer) for rendering.
def vector_field(lo_a, hi_a, lo_b, hi_b, G=11, L=8):
    """lo/hi are scaled-state bounds for axis a (horizontal) and b (vertical).
    Returns list of (tail_a, tail_b, head_a, head_b) in SCALED units."""
    import math
    out = []
    for j in range(G):
        for i in range(G):
            A0 = lo_a + (hi_a - lo_a) * i // (G - 1)
            B0 = lo_b + (hi_b - lo_b) * j // (G - 1)
            A1, B1 = step(A0, B0)
            dA = A1 - A0; dB = B1 - B0
            mag = math.isqrt(dA * dA + dB * dB) or 1
            ah = dA * L * S // mag      # arrow in scaled units, length ~L*S
            bh = dB * L * S // mag
            out.append((A0, B0, A0 + ah, B0 + bh))
    return out
