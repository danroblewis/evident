import sys, math
N = int(sys.argv[1]) if len(sys.argv)>1 else 300
S, DT, SC = 1024, 24, 56
CX, CY = 320, 240
def rd(e,b): return f"(({e}) + {b//2}) / {b}"
def traj(name, x0, v0):
    L=[f"    {name}x0 ∈ Int = {x0}", f"    {name}v0 ∈ Int = {v0}"]; D=[]; E=[]
    for i in range(N):
        if i>0:
            xi,vi=f"{name}x{i-1}",f"{name}v{i-1}"
            L+=[f"    {name}xx{i} ∈ Int = {rd(f'{xi} * {xi}',S)}",
                f"    {name}om{i} ∈ Int = {S} - {name}xx{i}",
                f"    {name}tm{i} ∈ Int = {rd(f'{name}om{i} * {vi}',S)}",
                f"    {name}aa{i} ∈ Int = {name}tm{i} - {xi}",
                f"    {name}v{i} ∈ Int = {vi} + {rd(f'{name}aa{i}',DT)}",
                f"    {name}x{i} ∈ Int = {xi} + {rd(f'{name}v{i}',DT)}"]
        D.append(f"    win.render_fill_rect(IVec2(CX + {rd(f'{name}x{i} * {SC}',S)} - 1, "
                 f"CY - {rd(f'{name}v{i} * {SC}',S)} - 1), IVec2(2, 2), {name}e{i})")
        E.append(f"{name}e{i}")
    return L,D,E
t1=traj("a",2867,0); t2=traj("b",123,0)
field=[]; fe=[]; k=0
for r in range(-3,4):
    for c in range(-3,4):
        X,V=c*900,r*900
        xx=(X*X+S//2)//S; om=S-xx; tm=(om*V+S//2)//S; A=tm-X
        dV=(A+DT//2)//DT; dX=((V+dV)+DT//2)//DT
        mag=max(1,int(math.hypot(dX,dV))); ax=dX*20//mag; av=dV*20//mag
        bx,by=CX+X*SC//S,CY-V*SC//S
        field.append(f"    win.draw_line(IVec2({bx}, {by}), IVec2({bx+ax}, {by-av}), fe_{k})"); fe.append(f"fe_{k}"); k+=1
body="\n".join(t1[0]+t2[0])
draws="\n".join(field)+f"\n    win.set_draw_color((80, 230, 120, 255), c1)\n"+"\n".join(t1[1])+\
      f"\n    win.set_draw_color((250, 170, 50, 255), c2)\n"+"\n".join(t2[1])
elist=", ".join(fe)+",\n        c1, "+", ".join(t1[2])+",\n        c2, "+", ".join(t2[2])
src=f'''import "stdlib/runtime.ev"
import "packages/sdl/window.ev"
import "packages/sdl/render.ev"
fsm portrait
    win ∈ SDL_Window (title ↦ "Phase Portrait — van der Pol", width ↦ 640, height ↦ 480)
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
    ⟩
'''
print(src)
print(f"vdp N={N}, {len(src.splitlines())} lines", file=sys.stderr)
