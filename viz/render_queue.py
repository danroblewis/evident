"""Render the phase portrait of a REAL Evident FSM (viz/examples/queue.ev)."""
import os, sys
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import matplotlib; matplotlib.use("Agg"); import matplotlib.pyplot as plt
import phaseportrait as pp
from ev_model import EvidentModel

def main():
    m = EvidentModel("viz/examples/queue.ev", "QueueStep", ["q0", "q1"])
    fig, ax = plt.subplots(figsize=(7, 7))
    pp.render(ax, m, "q0", "q1", (-0.6, 7.6), (-0.6, 7.6), style="fan",
              max_succ=6, equal=True, safe_box={"q0": (0, 6), "q1": (0, 6)})
    ax.set_title("queue.ev — phase portrait of a real Evident FSM\n"
                 "(transition fan + the [0,6]² boundary box)", fontsize=12, loc="left")
    out = os.path.join(os.path.dirname(__file__), "results", "queue_phaseportrait.png")
    os.makedirs(os.path.dirname(out), exist_ok=True)
    fig.savefig(out, dpi=130, facecolor="white"); print("wrote", out)

if __name__ == "__main__": main()
