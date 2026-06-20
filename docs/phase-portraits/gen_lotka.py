#!/usr/bin/env python3
"""Lotka-Volterra predator-prey phase portrait (nested closed orbits) as Evident.
x'=a*x - b*x*y, y'=-c*y + d*x*y, fixed-point integer, symplectic.  Prints to stdout:

    python3 gen_lotka.py [N] > /tmp/lotka.ev
"""
import sys
N = int(sys.argv[1]) if len(sys.argv) > 1 else 240
S, DT, SC = 4096, 64, 50
A, B, C, D, CS = 20, 10, 20, 10, 10      # a=2,b=1,c=2,d=1
CX, CY, FP = 320, 240, 2*4096            # fixed point (prey*,pred*) = (2,2)
def rd(e, b): return f"(({e}) + {b//2}) / {b}"
def orbit(name, x0, y0):
    L = [f"    {name}X0 ∈ Int = {x0}", f"    {name}Y0 ∈ Int = {y0}"]; Dr = []; E = []
    for i in range(N):
        if i > 0:
            X, Y = f"{name}X{i-1}", f"{name}Y{i-1}"
            L += [f"    {name}xy{i} ∈ Int = {rd(f'{X} * {Y}', S)}",
                  f"    {name}dx{i} ∈ Int = {rd(f'{A}*{X} - {B}*{name}xy{i}', CS)}",
                  f"    {name}X{i} ∈ Int = {X} + {rd(f'{name}dx{i}', DT)}",
                  f"    {name}xz{i} ∈ Int = {rd(f'{name}X{i} * {Y}', S)}",
                  f"    {name}dy{i} ∈ Int = {rd(f'(0 - {C})*{Y} + {D}*{name}xz{i}', CS)}",
                  f"    {name}Y{i} ∈ Int = {Y} + {rd(f'{name}dy{i}', DT)}"]
        Dr.append(f"    win.render_fill_rect(IVec2(CX + {rd(f'({name}X{i} - {FP}) * {SC}', S)} - 1, "
                  f"CY - {rd(f'({name}Y{i} - {FP}) * {SC}', S)} - 1), IVec2(2, 2), {name}e{i})")
        E.append(f"{name}e{i}")
    return L, Dr, E
orbits = [("a", 2*S, (16*S)//10, "(90, 220, 255, 255)"),
          ("b", 2*S, 1*S, "(240, 120, 240, 255)"),
          ("c", 2*S, (5*S)//10, "(250, 220, 90, 255)")]
body = []; draws = []; elist = []
for nm, x0, y0, col in orbits:
    L, Dr, E = orbit(nm, x0, y0)
    body += L
    draws.append(f"    win.set_draw_color({col}, {nm}col)"); draws += Dr
    elist.append(f"{nm}col"); elist += E
print(f'''import "stdlib/runtime.ev"
import "packages/sdl/window.ev"
import "packages/sdl/render.ev"
fsm portrait
    win ∈ SDL_Window (title ↦ "Phase Portrait — predator-prey", width ↦ 640, height ↦ 480)
    CX ∈ Int = {CX}
    CY ∈ Int = {CY}
{chr(10).join(body)}
    win.set_draw_color((10, 14, 18, 255), bg_eff)
    win.render_clear(clear_eff)
    win.set_draw_color((55, 65, 80, 255), fcol)
    win.draw_line(IVec2(0, CY), IVec2(640, CY), axh)
    win.draw_line(IVec2(CX, 0), IVec2(CX, 480), axv)
{chr(10).join(draws)}
    win.render_present(present_eff)
    sdl_delay(80, delay_eff)
    frame ∈ Int = (is_first_tick ? 0 : _frame + 1)
    done ∈ Effect = (frame ≥ 200 ? Exit(0) : NoEffect)
    effects ∈ Seq(Effect) = ⟨
        bg_eff, clear_eff, fcol, axh, axv,
        {", ".join(elist)},
        present_eff, delay_eff, done
    ⟩''')
