# viz — diagrams from Evident programs

Run `../setup.sh` once (installs lark, z3-solver, matplotlib). Then, from the repo root:

| script | what it draws | output |
|---|---|---|
| `python3 viz/render_queue.py` | **phase portrait** of a real Evident FSM (`viz/examples/queue.ev`): transition fan + boundary box, via the ported `phaseportrait.py` engine driving the runtime | `viz/results/queue_phaseportrait.png` |
| `python3 viz/fsm_graph.py` | the adventure **state-transition diagram** (rooms + labelled moves) | `viz/results/adventure_fsm.png` |
| `python3 viz/generate_all.py` | the generic generator over every example schema (scatter / projection / bars from the sampled interface) | `viz/diagrams/*.png` |
| `python3 viz/poc.py` | the circle proof-of-concept | `viz/results/` |

Pieces: `phaseportrait.py` (ported flow-field/box engine), `ev_model.py` (adapts an
Evident FSM claim to the engine via the runtime sampler), `diagram.py` (interface-driven
generic generator). Generated images are gitignored — regenerate any time.

Design docs: `docs/design/phase-portraits.md`, `state-space-diagrams.md`, `effectful-models.md`.
