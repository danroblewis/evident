#!/usr/bin/env python3
"""Smoke-test the IDE's view renderers — the one thing ./test.sh's Rust tests + demos do NOT cover.

A refactor (or any viz/ change) that breaks a renderer is otherwise caught only by manually driving the
browser; this exercises the exact server render path headlessly. For a couple of scalar/enum samples it
exports the transition + renders EVERY registered view, asserting each produces a non-empty PNG with no
exception. Run from the repo root: `python3 ide/render_smoke.py` (exit non-zero on any failure).

Covers scalar, enum, AND effect (payload-enum) FSMs. Effect FSMs used to fail the z3 re-parse with
"repeated accessor f0" (the runtime named every variant's fields f0/f1, colliding across constructors);
that's fixed at the source (the parser uniquifies them per variant), and the `effects` sample guards it.
All samples load in the dev env's z3 and exercise both the dynamics and the function views.
"""
import os
import sys
import tempfile

sys.path.insert(0, "ide/web")
sys.path.insert(0, "viz")

from runtime_io import _export                              # noqa: E402
from render import RENDERERS, VIEWS                         # noqa: E402

SAMPLES = {
    "traffic": (
        "enum Light = Red | Green | Yellow\n"
        "fsm traffic\n"
        "    light ∈ Light\n"
        "    0 ≤ timer ∈ Int ≤ 2\n"
        "    is_first_tick ⇒ (light = Red ∧ timer = 0)\n"
        "    (¬is_first_tick ∧ _timer < 2) ⇒ (light = _light ∧ timer = _timer + 1)\n"
        "    (¬is_first_tick ∧ _timer = 2 ∧ _light = Red) ⇒ (light = Green ∧ timer = 0)\n"
        "    (¬is_first_tick ∧ _timer = 2 ∧ _light = Green) ⇒ (light = Yellow ∧ timer = 0)\n"
        "    (¬is_first_tick ∧ _timer = 2 ∧ _light = Yellow) ⇒ (light = Red ∧ timer = 0)\n"),
    "counter": (
        "fsm counter\n"
        "    count ∈ Int\n"
        "    is_first_tick ⇒ count = 0\n"
        "    ¬is_first_tick ⇒ Δcount = (_count < 5 ? 1 : 0)\n"
        "    done ∈ Bool = (count ≥ 5)\n"),
    # An EFFECT FSM (payload-enum datatype): used to fail the z3 re-parse with "repeated accessor f0"
    # until the parser was fixed to uniquify variant-field accessors. This sample guards that fix.
    "effects": (
        'fsm ticker\n'
        '    count ∈ Int\n'
        '    is_first_tick ⇒ count = 0\n'
        '    ¬is_first_tick ⇒ Δcount = 1\n'
        '    effects ∈ Seq(Effect) = ⟨Println("tick")⟩\n'),
    # A SEQ-CARRYING FSM: the whole carried state is a Seq(Int) shifted +1 each tick (an
    # unbounded orbit [1,2,3]→[2,3,4]→…). Guards the viz's kind=="seq" handling end-to-end —
    # the value decoders, element-wise pinning, list-hashable node keys, and every renderer's
    # treatment of a vector-valued (non-scalar-axis) state var. Without seq support the model
    # loads but every reachable()/initial_state() query raised "unknown kind seq".
    "shift": (
        'fsm shift\n'
        '    xs ∈ Seq(Int)\n'
        '    #xs = 3\n'
        '    is_first_tick ⇒ xs = ⟨1, 2, 3⟩\n'
        '    ¬is_first_tick ⇒ ∀ (cur, nxt) ∈ coindexed(_xs, xs) : nxt = cur + 1\n'),
    # Rule 90 — the HEADLINE space_time case: a Seq(Int) of 0/1 whose next row is the
    # cellwise XOR of its neighbours. The space_time raster of this is the Sierpiński
    # triangle (binary cmap). Guards the deterministic-trajectory + binary-raster path.
    "rule90": (
        'fsm rule90\n'
        '    cells ∈ Seq(Int)\n'
        '    #cells = 11\n'
        '    is_first_tick ⇒ cells = ⟨0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0⟩\n'
        '    ∀ i ∈ {1..9} : cells[i] = ((_cells[i-1] + _cells[i+1]) = 1 ? 1 : 0)\n'
        '    (cells[0] = 0 ∧ cells[10] = 0)\n'),
    # A multi-VALUED Seq(Int) diffusion buffer — exercises space_time's COLORMAP (non-binary)
    # path: a 1-D box-blur smooths an initial spike, so cells take a range of integer values.
    "diffuse": (
        'fsm diffuse\n'
        '    buf ∈ Seq(Int)\n'
        '    #buf = 9\n'
        '    is_first_tick ⇒ buf = ⟨0, 0, 0, 0, 64, 0, 0, 0, 0⟩\n'
        '    ∀ i ∈ {1..7} : buf[i] = (_buf[i-1] + 2 * _buf[i] + _buf[i+1]) / 4\n'
        '    (buf[0] = 0 ∧ buf[8] = 0)\n'),
    # CONTINUOUS / CHAOTIC Real-valued samples — the crash-robustness guard set. These
    # used to throw uncaught exceptions out of the dynamics renderers instead of clamping
    # or rendering an honest N/A card:
    #   * logistic map → chord_diagram: OverflowError in model_codec float(as_fraction())
    #     when the chaotic orbit's Z3 rational blows up (and in the numeric bin aggregation).
    #   * predator-prey (Lotka-Volterra) → nullcline_field / occupancy_heatmap: the diverging
    #     coupled map overflows the same float(as_fraction()) codec read.
    #   * thermostat → nullcline_field: the single-numeric+facet path with no second
    #     categorical (yv=None) hit a TypeError in _cat_levels(None) / _num_range on Real temp.
    # Guarded in viz/model_codec.py (clamp ±1e18), viz/chord_channels.py (numeric-bin try/except),
    # and viz/render_nullcline_field.py (None yv + Real-range + overflow → N/A). render_smoke renders
    # EVERY view for each sample, so these assert no view raises on a continuous/chaotic model.
    "logistic": (
        'fsm logistic\n'
        '    x ∈ Real\n'
        '    is_first_tick ⇒ x = 0.3\n'
        '    ¬is_first_tick ⇒ x = 3.7 * _x * (1.0 - _x)\n'),
    "predator_prey": (
        'fsm predator_prey\n'
        '    prey ∈ Real\n'
        '    pred ∈ Real\n'
        '    is_first_tick ⇒ (prey = 40.0 ∧ pred = 9.0)\n'
        '    ¬is_first_tick ⇒ Δprey = _prey * 0.1 - _prey * _pred * 0.01\n'
        '    ¬is_first_tick ⇒ Δpred = _prey * _pred * 0.005 - _pred * 0.1\n'),
    "thermostat": (
        'enum Mode = Heating | Idle\n'
        'fsm thermostat\n'
        '    temp ∈ Real\n'
        '    mode ∈ Mode\n'
        '    is_first_tick ⇒ (temp = 15.0 ∧ mode = Heating)\n'
        '    (¬is_first_tick ∧ _mode = Heating) ⇒ Δtemp = 1.0\n'
        '    (¬is_first_tick ∧ _mode = Idle) ⇒ Δtemp = 0.0 - 0.5\n'
        '    (¬is_first_tick ∧ _temp ≥ 22.0) ⇒ mode = Idle\n'
        '    (¬is_first_tick ∧ _temp ≤ 18.0) ⇒ mode = Heating\n'
        '    (¬is_first_tick ∧ 18.0 < _temp ∧ _temp < 22.0) ⇒ mode = _mode\n'),
}


def main():
    fails = []
    for name, src in SAMPLES.items():
        with tempfile.TemporaryDirectory() as work:
            ok, prefix, dropped, msg = _export(src, work)
            if not ok:
                fails.append(f"{name}: export failed: {msg.splitlines()[0][:60]}")
                continue
            for view in VIEWS:
                out = os.path.join(work, f"{view}.png")
                try:
                    RENDERERS[view](prefix + ".smt2", prefix + ".schema.json", out)
                    if not (os.path.exists(out) and os.path.getsize(out) > 0):
                        fails.append(f"{name}/{view}: produced no (or empty) PNG")
                except Exception as e:
                    fails.append(f"{name}/{view}: {type(e).__name__}: {e}")
    if fails:
        print("RENDER SMOKE-TEST FAILURES:")
        for f in fails:
            print("  ✗", f)
        return 1
    print(f"✓ render smoke-test: {len(SAMPLES)} samples × {len(VIEWS)} views all rendered a PNG")
    return 0


if __name__ == "__main__":
    sys.exit(main())
