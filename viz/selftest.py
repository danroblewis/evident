"""Validate evident_viz against the three sample IRs. Run:
    python3 viz/selftest.py /tmp/ir
"""
import sys
from evident_viz import load

ir = sys.argv[1] if len(sys.argv) > 1 else "/tmp/ir"
for name in ("vanderpol", "dungeon", "vending"):
    m = load(f"{ir}/{name}.smt2", f"{ir}/{name}.schema.json")
    print(f"\n=== {name}  ({'discrete' if m.is_discrete() else 'has-numeric'}) ===")
    print("  state vars:", [(v["name"], v["kind"]) for v in m.state_vars])
    init = m.initial_state()
    print("  initial   :", m.label(init) if init else None)
    succ = m.successor(init) if init else None
    print("  successor :", m.label(succ) if succ else None)
    if init:
        fan = m.successors(succ if succ else init)
        print(f"  fan-out   : {len(fan)} successors from {m.label(succ if succ else init)}")
    traj = m.trajectory(steps=30)
    print(f"  trajectory: {len(traj)} states, ends {m.label(traj[-1]) if traj else None}")
    if m.is_discrete():
        states, edges = m.reachable()
        print(f"  reachable : {len(states)} states, {len(edges)} edges")
print("\nOK")
