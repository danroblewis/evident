#!/usr/bin/env python3
"""Damped spring phase portrait (spiral sink) as Evident.  pos' = vel,
vel' = -k*pos - c*vel, fixed-point integer.  Prints the .ev to stdout:

    python3 gen_spring.py [N] > /tmp/spring.ev
"""
import sys, math
N = int(sys.argv[1]) if len(sys.argv) > 1 else 240
S, DT, K, C, KS = 256, 16, 16, 3, 16     # k=1.0, c=0.1875
CX, CY, DIV = 320, 240, 366              # screen: sx = CX + pos/DIV
def rd(e, b): return f"(({e}) + {b//2}) / {b}"
def traj(name, p0, v0):
    L = [f"    {name}p0 ∈ Int = {p0}", f"    {name}v0 ∈ Int = {v0}"]; Dr = []; E = []
    for i in range(N):
        if i > 0:
            p, v = f"{name}p{i-1}", f"{name}v{i-1}"
            L += [f"    {name}ac{i} ∈ Int = {rd(f'(0 - {K})*{p} - {C}*{v}', KS)}",
                  f"    {name}v{i} ∈ Int = {v} + {rd(f'{name}ac{i}', DT)}",
                  f"    {name}p{i} ∈ Int = {p} + {rd(f'{name}v{i}', DT)}"]
        Dr.append(f"    win.render_fill_rect(IVec2(CX + {name}p{i} / {DIV} - 1, "
                  f"CY - {name}v{i} / {DIV} - 1), IVec2(2, 2), {name}e{i})")
        E.append(f"{name}e{i}")
    return L, Dr, E
t1 = traj("a", 200*S, 0); t2 = traj("b", 0, 180*S)
field = []; fe = []; k = 0
for r in range(-3, 4):
    for c in range(-4, 5):
        px, pv = c*42*S//3, r*42*S//3
        dp = pv; dv = ((0-K)*px - C*pv)//KS
        mag = max(1, int(math.hypot(dp//DIV, dv//DIV)))
        ax = (dp//DIV)*20//mag; av = (dv//DIV)*20//mag
        bx, by = CX+px//DIV, CY-pv//DIV
        field.append(f"    win.draw_line(IVec2({bx}, {by}), IVec2({bx+ax}, {by-av}), fe_{k})")
        fe.append(f"fe_{k}"); k += 1
body = "\n".join(t1[0] + t2[0])
draws = "\n".join(field) + "\n    win.set_draw_color((80, 230, 120, 255), c1)\n" + "\n".join(t1[1]) + \
        "\n    win.set_draw_color((250, 170, 50, 255), c2)\n" + "\n".join(t2[1])
elist = ", ".join(fe) + ",\n        c1, " + ", ".join(t1[2]) + ",\n        c2, " + ", ".join(t2[2])
print(f'''import "stdlib/runtime.ev"
import "packages/sdl/window.ev"
import "packages/sdl/render.ev"
fsm portrait
    win ∈ SDL_Window (title ↦ "Phase Portrait — damped spring", width ↦ 640, height ↦ 480)
    CX ∈ Int = {CX}
    CY ∈ Int = {CY}
{body}
    win.set_draw_color((10, 12, 20, 255), bg_eff)
    win.render_clear(clear_eff)
    win.set_draw_color((50, 60, 90, 255), fcol)
    win.draw_line(IVec2(0, CY), IVec2(640, CY), axh)
    win.draw_line(IVec2(CX, 0), IVec2(CX, 480), axv)
{draws}
    win.render_present(present_eff)
    sdl_delay(80, delay_eff)
    frame ∈ Int = (is_first_tick ? 0 : _frame + 1)
    done ∈ Effect = (frame ≥ 200 ? Exit(0) : NoEffect)
    effects ∈ Seq(Effect) = ⟨
        bg_eff, clear_eff, fcol, axh, axv,
        {elist},
        present_eff, delay_eff, done
    ⟩''')
