#!/usr/bin/env python3
"""
Damped spring / spiral sink.

Continuous system:
    pos' = vel
    vel' = -k*pos - c*vel      (k = stiffness, c = damping)

Eigenvalues have negative real part -> trajectories spiral INWARD to origin.

FIXED-POINT INTEGER recurrence.
State (pos, vel) stored scaled by S. We want each Euler step to advance only a
small fraction of an orbit, so the curve is densely sampled and smooth.

Use a small dimensionless timestep encoded as a divisor DT (i.e. dt = 1/DT):
    vel_next = vel + (-K*pos - C*vel) // (KSCALE * DT)
    pos_next = pos + vel_next // DT

K, C are integer numerators; KSCALE normalizes them. We pick:
    S    = 256    (state scale)
    DT   = 16     (timestep 1/16)
    K    = 16     -> spring constant k = K/KSCALE = 16/16 = 1.0
    C    = 1      -> damping c = 1/16 ~ 0.0625  (light damping -> many visible loops)
    KSCALE = 16

Note: pos, vel are already in scaled units (multiplied by S). The force
-k*pos - c*vel is linear so scaling passes through cleanly: no extra /S needed
because k, c are dimensionless ratios applied to scaled pos/vel.
"""

S      = 256
DT     = 16
K      = 16     # numerator for k
C      = 3      # numerator for c
KSCALE = 16     # k = K/KSCALE, c = C/KSCALE

def rdiv(a, b):
    # round-to-nearest integer division (symmetric) -> kills truncation drift
    if a >= 0:
        return (a + b // 2) // b
    return -((-a + b // 2) // b)

def step(pos, vel):
    # acceleration (scaled): a = -k*pos - c*vel, in scaled units
    acc = rdiv(-K * pos - C * vel, KSCALE)
    vel_n = vel + rdiv(acc, DT)
    pos_n = pos + rdiv(vel_n, DT)
    return pos_n, vel_n

def simulate(pos0, vel0, N):
    pos, vel = pos0, vel0
    traj = [(pos, vel)]
    for _ in range(N):
        pos, vel = step(pos, vel)
        traj.append((pos, vel))
    return traj

def ascii_plot(trajs, title):
    W, H = 70, 30
    CX, CY = W // 2, H // 2
    grid = [[' '] * W for _ in range(H)]
    # axes
    for x in range(W): grid[CY][x] = '-'
    for y in range(H): grid[y][CX] = '|'
    grid[CY][CX] = '+'
    marks = '.oO@#xX*'
    for ti, traj in enumerate(trajs):
        ch = marks[ti % len(marks)]
        for (p, v) in traj:
            # map scaled state to ascii: divide by S to get screen-units, then scale
            sx = CX + (p // S) * (W // 2) // 320
            sy = CY - (v // S) * (H // 2) // 240
            if 0 <= sx < W and 0 <= sy < H:
                grid[sy][sx] = ch
    print(title)
    print('\n'.join(''.join(r) for r in grid))

def smoothness(traj):
    # max single-step screen jump (in pixels at S scale)
    mj = 0
    for i in range(1, len(traj)):
        dx = abs(traj[i][0] - traj[i-1][0]) // S
        dy = abs(traj[i][1] - traj[i-1][1]) // S
        mj = max(mj, dx + dy)
    return mj

if __name__ == '__main__':
    # seed: pos far from origin, vel zero -> classic spiral
    seed = (200 * S, 0)
    N = 200
    traj = simulate(*seed, N)
    print(f"S={S} DT={DT} K={K} C={C} KSCALE={KSCALE} seed={seed} N={N}")
    print("First 8 points (pos//S, vel//S):")
    for (p, v) in traj[:8]:
        print(f"   ({p//S:5d}, {v//S:5d})")
    print("Last 4 points:")
    for (p, v) in traj[-4:]:
        print(f"   ({p//S:5d}, {v//S:5d})")
    print(f"max single-step screen jump: {smoothness(traj)} px")
    final = traj[-1]
    print(f"final |state|//S ~ {abs(final[0])//S + abs(final[1])//S} (should be small -> sink)")
    ascii_plot([traj], "Damped spring (spiral sink):")

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
