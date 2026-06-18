"""fsm_portraits — phase portraits / state diagrams for every Evident FSM we can render.

Each spec names a real transition claim + the (current,next) var pairs that form its
phase plane + any fixed inputs. The ported engine draws the flow, fixed points, and
boundary box. Enum-state machines (adventure) get a node graph via fsm_graph instead.
Output: viz/fsm/<name>.png
"""
import os, sys, traceback
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import matplotlib; matplotlib.use("Agg"); import matplotlib.pyplot as plt
import phaseportrait as pp
from ev_model import EvidentModel

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
OUT = os.path.join(ROOT, "viz", "fsm")

SPECS = [
    dict(name="game_engine__AxisPhysics", file="programs/sdl_demo/game_engine.ev",
         claim="AxisPhysics", axes=[("pos", "pos_next"), ("v", "v_next")],
         given={"pos_min": 0, "pos_max": 20, "won": False,
                "accel_pos": False, "accel_neg": False},
         xr=(-1, 21), yr=(-14, 14), box={"pos": (0, 20), "v": (-12, 12)},
         title="game_engine · AxisPhysics — player/ball 1-D physics\n"
               "(position, velocity) coasting: velocity decays to 0, walls clamp"),
    dict(name="queue__QueueStep", file="viz/examples/queue.ev",
         claim="QueueStep", nondet=True,
         axes=[("state.q0", "state_next.q0"), ("state.q1", "state_next.q1")],
         given={}, xr=(-0.6, 7.6), yr=(-0.6, 7.6),
         box={"state.q0": (0, 6), "state.q1": (0, 6)},
         title="queue.ev · QueueStep — 2-stage bounded-queue daemon\n"
               "transition fan (arrive / transfer / depart / idle) + boundary box"),
]


def render_spec(s):
    m = EvidentModel(s["file"], s["claim"], s["axes"], given=s.get("given"),
                     nondet=s.get("nondet", False))
    xaxis, yaxis = s["axes"][0][0], s["axes"][1][0]
    fig, ax = plt.subplots(figsize=(7.4, 7.2))
    pp.render(ax, m, xaxis, yaxis, s["xr"], s["yr"], style="fan",
              max_succ=6, equal=True, safe_box=s["box"])
    ax.set_title(s["title"], fontsize=11, loc="left")
    os.makedirs(OUT, exist_ok=True)
    path = os.path.join(OUT, s["name"] + ".png")
    fig.savefig(path, dpi=130, facecolor="white"); plt.close(fig)
    return path


def main():
    only = sys.argv[1] if len(sys.argv) > 1 else None
    for s in SPECS:
        if only and only not in s["name"]:
            continue
        try:
            print("[ok]  ", render_spec(s), flush=True)
        except Exception as e:
            print("[skip]", s["name"], "::", str(e)[:90], flush=True)
            traceback.print_exc()


if __name__ == "__main__":
    main()
