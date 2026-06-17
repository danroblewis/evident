#!/usr/bin/env bash
# Bootstrap the Evident prototype Python environment.
#
#   ./setup.sh
#
# Installs the plotting stack (numpy, matplotlib) from requirements.txt and
# ensures a working z3 (libz3 4.15.4). If the system already provides a suitable
# z3 (>= 4.15, e.g. the apt `python3-z3` package layered over libz3 4.15.4), it is
# KEPT and the pinned z3-solver wheel is skipped — we do not shadow the measured
# runtime. On a clean machine with no usable z3, the pinned wheel is installed.
set -euo pipefail
cd "$(dirname "$0")"

PIP=(python3 -m pip install --break-system-packages)
REQ=requirements.txt

echo "== plotting deps (from $REQ, excluding z3) =="
"${PIP[@]}" -r <(grep -vE '^\s*(#|z3-solver)' "$REQ")

echo "== z3 (target libz3 4.15.4) =="
if python3 - <<'PY'
import sys
try:
    import z3
    v = tuple(int(x) for x in z3.get_version_string().split("."))
    sys.exit(0 if v[:2] >= (4, 15) else 1)
except Exception:
    sys.exit(1)
PY
then
    echo "   keeping existing z3 $(python3 -c 'import z3; print(z3.get_version_string())') (not shadowing it)"
else
    Z3PIN=$(grep -E '^\s*z3-solver==' "$REQ" | tr -d '[:space:]')
    echo "   no suitable z3 found -> installing pinned $Z3PIN"
    "${PIP[@]}" "$Z3PIN"
fi

echo "== verifying =="
python3 - <<'PY'
import numpy, matplotlib, z3
print(f"  numpy       {numpy.__version__}")
print(f"  matplotlib  {matplotlib.__version__}")
print(f"  z3          {z3.get_version_string()}")
PY
echo "OK: environment ready.  Try:  cd prototype && python3 phase_portrait.py"
