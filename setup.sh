#!/usr/bin/env bash
# Bootstrap the Evident + diagram environment.
set -euo pipefail
cd "$(dirname "$0")"
PIP=(python3 -m pip install --break-system-packages)

echo "== python deps (parser=lark, solver=z3, plots=matplotlib) =="
"${PIP[@]}" lark==1.3.1 z3-solver==4.15.4.0 matplotlib

echo "== verify =="
python3 -c "import lark, z3, matplotlib; print('lark', lark.__version__, '| z3', z3.get_version_string(), '| matplotlib', matplotlib.__version__)"

cat <<'EOT'
OK. Try:
  python3 evident.py check ide/examples/circle.ev      # run the Evident runtime
  python3 viz/render_queue.py    # phase portrait of a real Evident FSM -> viz/results/
  python3 viz/fsm_graph.py       # the adventure state machine          -> viz/results/
  python3 viz/generate_all.py    # auto-diagram every example schema     -> viz/diagrams/
EOT
