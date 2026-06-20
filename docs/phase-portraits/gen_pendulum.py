#!/usr/bin/env python3
"""Generate a pendulum phase portrait (nested librations / "eyes") as Evident.

Sine is a Bhaskara polynomial — sin(x)*S ~= 16*u*S/(5*pi^2*S^2 - 4*u) with
u = |x|*(pi-|x|) — valid on [-pi, pi] (the libration range, no range reduction).
This avoids Z3's unusable transcendental `sin` and the slow array-theory LUT.

    python3 gen_pendulum.py [N] > /tmp/portraits/pendulum.ev
    EVIDENT_NO_JIT=1 runtime/target/release/evident effect-run /tmp/portraits/pendulum.ev
"""
import sys

S, DT, SC, P = 4096, 40, 86, 12868   # S=fixed-point, P=round(pi*S)
P2 = 5 * P * P                        # 5*pi^2*S^2 = 828005120
CX, CY = 320, 240
N = int(sys.argv[1]) if len(sys.argv) > 1 else 240

def traj(name, th0, om0):
    L = [f"    {name}th0 ∈ Int = {th0}", f"    {name}om0 ∈ Int = {om0}"]
    D, E = [], []
    for i in range(N):
        if i > 0:
            th, om = f"{name}th{i-1}", f"{name}om{i-1}"
            L += [
                f"    {name}ax{i} ∈ Int = ({th} ≥ 0 ? {th} : 0 - {th})",
                f"    {name}u{i} ∈ Int = {name}ax{i} * ({P} - {name}ax{i})",
                f"    {name}sp{i} ∈ Int = (65536 * {name}u{i}) / ({P2} - 4 * {name}u{i})",
                f"    {name}si{i} ∈ Int = ({th} ≥ 0 ? {name}sp{i} : 0 - {name}sp{i})",
                f"    {name}om{i} ∈ Int = {om} - ({name}si{i} + {DT//2}) / {DT}",
                f"    {name}th{i} ∈ Int = {th} + ({name}om{i} + {DT//2}) / {DT}"]
        D.append(f"    win.render_fill_rect(IVec2(CX + {name}th{i} * {SC} / {S} - 1, "
                 f"CY - {name}om{i} * {SC} / {S} - 1), IVec2(2, 2), {name}e{i})")
        E.append(f"{name}e{i}")
    return L, D, E

# four librations: angular-velocity seeds 0.5 .. 1.9 (all stay below the separatrix)
seeds = [("a", 0, round(0.5*S), "(90,220,255,255)"), ("b", 0, round(1.0*S), "(90,235,140,255)"),
         ("c", 0, round(1.5*S), "(245,220,90,255)"), ("d", 0, round(1.9*S), "(250,150,60,255)")]
body, draws, elist = [], [], []
for nm, t0, o0, col in seeds:
    L, D, E = traj(nm, t0, o0)
    body += L
    draws.append(f"    win.set_draw_color({col}, {nm}col)"); draws += D
    elist.append(f"{nm}col"); elist += E

print(f'''import "stdlib/runtime.ev"
import "packages/sdl/window.ev"
import "packages/sdl/render.ev"
fsm portrait
    win ∈ SDL_Window (title ↦ "Phase Portrait — pendulum", width ↦ 640, height ↦ 480)
    CX ∈ Int = {CX}
    CY ∈ Int = {CY}
{chr(10).join(body)}
    win.set_draw_color((10,12,20,255), bg_eff)
    win.render_clear(clear_eff)
    win.set_draw_color((55,65,80,255), fcol)
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
