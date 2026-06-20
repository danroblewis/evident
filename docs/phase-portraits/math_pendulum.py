#!/usr/bin/env python3
"""
Pendulum -- the SEPARATRIX portrait.

Continuous system (theta = angle, omega = angular velocity):
    theta' = omega
    omega' = -(g/L) * sin(theta)

Phase portrait has two regimes divided by the SEPARATRIX:
  * Low energy (|omega| small):  theta oscillates -> CLOSED loops (libration),
    centered on (0,0), eye-shaped.
  * High energy: the pendulum goes over the top -> theta increases without bound
    -> ROTATION (wavy bands above/below). The separatrix is the boundary curve
    passing through the unstable fixed points at theta = +-pi.

----------------------------------------------------------------------------
FIXED-POINT INTEGER recurrence.

State scaled by S:  TH = theta*S,  OM = omega*S.   (theta in radians)

The hard part is sin(theta). We use an INTEGER SINE LOOKUP TABLE.
Build SIN[i] = round( sin(i * 2*pi / TBL) * S )  for i in 0..TBL-1, a table of
S-scaled sine values over one period. To evaluate sin(theta) for scaled TH:
    idx = ( TH * TBL // (2*pi*S) )  reduced mod TBL
We precompute K2PI = round(2*pi*S) so idx = (TH * TBL) // K2PI, then  % TBL
(handling negatives). isin = SIN[idx]  is sin(theta)*S.

Acceleration (scaled):  OM' = -(GL) * isin // GLSCALE
where g/L = GL/GLSCALE.  isin is already *S, so OM' comes out *S correctly.

Symplectic Euler (advance OM with sin(TH), then TH with new OM) -> energy
conserved -> closed libration loops do NOT drift; rotation bands stay flat.

Parameters:
    S       = 4096
    TBL     = 4096        (sine table resolution)
    DT      = 40          timestep 1/40
    GL      = 1           g/L = 1  (GLSCALE=1)  -> period 2*pi at small amplitude
    GLSCALE = 1
"""
import math

S       = 4096
TBL     = 4096
DT      = 40
GL      = 1
GLSCALE = 1

K2PI = round(2 * math.pi * S)                       # 2*pi scaled
SIN  = [round(math.sin(i * 2 * math.pi / TBL) * S) for i in range(TBL)]

def isin(TH):
    # integer sin(theta)*S via table; TH is theta*S
    idx = (TH * TBL) // K2PI
    idx %= TBL                                        # python % is always >=0
    return SIN[idx]

def rdiv(a, b):
    if a >= 0:
        return (a + b // 2) // b
    return -((-a + b // 2) // b)

def step(TH, OM):
    acc = rdiv(-GL * isin(TH), GLSCALE)              # -(g/L) sin(theta), scaled
    OM_n = OM + rdiv(acc, DT)
    TH_n = TH + rdiv(OM_n, DT)
    return TH_n, OM_n

def simulate(TH0, OM0, N):
    TH, OM = TH0, OM0
    traj = [(TH, OM)]
    for _ in range(N):
        TH, OM = step(TH, OM)
        traj.append((TH, OM))
    return traj

def ascii_plot(trajs, title):
    W, H = 78, 28
    grid = [[' '] * W for _ in range(H)]
    CY = H // 2
    for x in range(W): grid[CY][x] = '-'
    # theta in [-2pi, 2pi] horizontally, omega in [-3,3] vertically
    THMAX = 2*math.pi; OMMAX = 3.0
    marks = '.oxX*#@+'
    for ti, traj in enumerate(trajs):
        ch = marks[ti % len(marks)]
        for (TH, OM) in traj:
            tr = TH / S; om = OM / S
            sx = int((tr / THMAX + 1) / 2 * (W - 1))
            sy = int(CY - om / OMMAX * (H // 2 - 1))
            if 0 <= sx < W and 0 <= sy < H:
                grid[sy][sx] = ch
    print(title)
    print('\n'.join(''.join(r) for r in grid))

if __name__ == '__main__':
    N = 500
    # mix of regimes to show the separatrix
    seeds = [
        (0, int(0.6*S)),    # small libration (closed loop near center)
        (0, int(1.2*S)),    # bigger libration
        (0, int(1.9*S)),    # near-separatrix libration
        (int(-2*math.pi*S/2), int(2.4*S)),  # rotation (over the top), starts left
        (int(-2*math.pi*S/2), int(-2.4*S)), # rotation other direction
    ]
    print(f"S={S} TBL={TBL} DT={DT} GL={GL} GLSCALE={GLSCALE} K2PI={K2PI} N={N}")
    trajs = [simulate(*s, N) for s in seeds]
    for si,(s,t) in enumerate(zip(seeds,trajs)):
        ths=[p[0]/S for p in t]; oms=[p[1]/S for p in t]
        spread = max(ths)-min(ths)
        kind = "ROTATION" if spread > 2*math.pi else "libration(closed)"
        # closure check for librations
        if kind.startswith("lib"):
            th0,om0=t[0]
            best=min((p[0]-th0)**2+(p[1]-om0)**2 for p in t[N//3:])
            drift=math.sqrt(best)/S
            print(f"seed{si} om0={s[1]/S:+.1f}: {kind:18s} theta in [{min(ths):+.2f},{max(ths):+.2f}] "
                  f"closure drift={drift:.3f}")
        else:
            print(f"seed{si} om0={s[1]/S:+.1f}: {kind:18s} theta spans {spread:.2f} rad "
                  f"(monotonic -> goes over top)")
    print("\nFirst 6 steps seed0 (theta,omega):")
    for (TH,OM) in trajs[0][:6]:
        print(f"   ({TH/S:+6.3f}, {OM/S:+6.3f})")
    ascii_plot(trajs, "\nPendulum separatrix (closed eye inside, rotation bands outside):")

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
