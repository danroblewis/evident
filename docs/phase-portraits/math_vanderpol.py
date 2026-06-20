#!/usr/bin/env python3
"""
Van der Pol oscillator -- THE limit cycle.

Continuous system:
    x'  = v
    v'  = mu*(1 - x^2)*v - x

The nonlinear damping mu*(1-x^2)*v is NEGATIVE (pumps energy) when |x|<1 and
POSITIVE (dissipates) when |x|>1. Result: every trajectory -- whether it starts
inside or outside -- converges to the SAME closed loop, the limit cycle.

----------------------------------------------------------------------------
FIXED-POINT INTEGER recurrence.

State (x, v) is stored scaled by S:  X = x*S,  V = v*S.

The crux is the cubic-ish term mu*(1 - x^2)*v. In real units x ~ O(2), so x^2 ~
O(4). In scaled units X = x*S, so X^2 = x^2 * S^2 -- a factor S^2 too big.
To form (1 - x^2) in scaled units we need:

    one_minus_xsq_scaled  =  S  -  (X*X)//S          # this equals (1 - x^2)*S

Then (1 - x^2)*v  in scaled units is:
    term = one_minus_xsq_scaled * V // S             # = (1-x^2)*v * S  (scaled)
and multiply by mu (a small ratio MU/MUSCALE):
    damp = MU * term // MUSCALE                       # scaled (1-x^2)*v*mu

Acceleration (scaled):  A = damp - X
Euler step with timestep 1/DT:
    V_next = V + A // DT
    X_next = X + V_next // DT     (semi-implicit: use updated V -> stable)

Parameters chosen so the loop is large & smooth and ~one lap per ~120 steps:
    S       = 1024
    DT      = 24
    MU      = 1     mu = MU/MUSCALE
    MUSCALE = 1     -> mu = 1.0  (classic van der Pol shape)
"""

S       = 1024
DT      = 24
MU      = 1
MUSCALE = 1

def rdiv(a, b):
    if a >= 0:
        return (a + b // 2) // b
    return -((-a + b // 2) // b)

def step(X, V):
    omx = S - rdiv(X * X, S)            # (1 - x^2) * S
    term = rdiv(omx * V, S)            # (1 - x^2)*v * S
    damp = rdiv(MU * term, MUSCALE)    # mu*(1-x^2)*v * S
    A = damp - X                       # acceleration (scaled)
    V_n = V + rdiv(A, DT)
    X_n = X + rdiv(V_n, DT)
    return X_n, V_n

def simulate(X0, V0, N):
    X, V = X0, V0
    traj = [(X, V)]
    for _ in range(N):
        X, V = step(X, V)
        traj.append((X, V))
    return traj

def ascii_plot(trajs, title):
    W, H = 74, 32
    CX, CY = W // 2, H // 2
    grid = [[' '] * W for _ in range(H)]
    for x in range(W): grid[CY][x] = '-'
    for y in range(H): grid[y][CX] = '|'
    grid[CY][CX] = '+'
    marks = '.oxX*#@'
    # x ranges ~[-3,3], v ~[-4,4] in real units. scale to grid.
    XR, VR = 3.0, 4.0
    for ti, traj in enumerate(trajs):
        ch = marks[ti % len(marks)]
        for (X, V) in traj:
            xr = X / S; vr = V / S
            sx = int(CX + xr / XR * (W // 2 - 1))
            sy = int(CY - vr / VR * (H // 2 - 1))
            if 0 <= sx < W and 0 <= sy < H:
                grid[sy][sx] = ch
    print(title)
    print('\n'.join(''.join(r) for r in grid))

def max_jump(traj):
    mj = 0
    for i in range(1, len(traj)):
        dx = abs(traj[i][0] - traj[i-1][0]) / S
        dy = abs(traj[i][1] - traj[i-1][1]) / S
        mj = max(mj, (dx*dx+dy*dy)**0.5)
    return mj

if __name__ == '__main__':
    import math
    N = 400
    # three seeds: tiny (inside), and two outside, all should reach the loop
    seeds = [
        (int(0.1*S), 0),        # inside the cycle -> spirals OUT to loop
        (int(2.8*S), 0),        # outside -> spirals IN to loop
        (0, int(3.5*S)),        # outside on v-axis
    ]
    print(f"S={S} DT={DT} MU={MU} MUSCALE={MUSCALE} N={N}")
    trajs = [simulate(*s, N) for s in seeds]
    for si, (s, t) in enumerate(zip(seeds, trajs)):
        # after transient, measure the loop extent (last 150 pts)
        tail = t[-150:]
        xs = [p[0]/S for p in tail]; vs = [p[1]/S for p in tail]
        print(f"seed {si} ({s[0]/S:.2f},{s[1]/S:.2f}): "
              f"loop x in [{min(xs):.2f},{max(xs):.2f}] v in [{min(vs):.2f},{max(vs):.2f}] "
              f"maxjump={max_jump(t):.3f}")
    # verify all three tails settle to the SAME extent
    print("\nFirst 6 steps of inside-seed (x,v real):")
    for (X,V) in trajs[0][:6]:
        print(f"   ({X/S:6.3f}, {V/S:6.3f})")
    ascii_plot(trajs, "\nVan der Pol limit cycle (3 seeds -> 1 loop):")

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
