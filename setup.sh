#!/usr/bin/env bash
# Bootstrap the Evident prototype environment from a bare container.
#
#   ./setup.sh
#
# Three stages:
#   1. SYSTEM packages — python3 + pip and the native libs the wheels load at
#      runtime (libgomp for numpy's BLAS, freetype/png for matplotlib's Agg
#      backend). Detects the package manager (apt / apk / dnf / yum) so it works
#      on Debian, Alpine, or RHEL-family bases.
#   2. PYTHON packages — numpy + matplotlib, pinned in requirements.txt.
#   3. z3 (target libz3 4.15.4) — keeps an existing system z3 >= 4.15 if present
#      (so it never shadows a measured runtime); otherwise installs the pinned
#      z3-solver wheel, which bundles its own libz3 4.15.4.
set -euo pipefail
cd "$(dirname "$0")"

SUDO=""
if [ "$(id -u)" -ne 0 ] && command -v sudo >/dev/null 2>&1; then SUDO="sudo"; fi

echo "== [1/3] system packages =="
if command -v apt-get >/dev/null 2>&1; then
    $SUDO apt-get update -qq
    $SUDO apt-get install -y --no-install-recommends \
        python3 python3-pip libgomp1 libfreetype6 libpng16-16
elif command -v apk >/dev/null 2>&1; then
    # Alpine is musl, not glibc — wheels resolve to musllinux variants.
    $SUDO apk add --no-cache python3 py3-pip libstdc++ libgomp freetype libpng
elif command -v dnf >/dev/null 2>&1; then
    $SUDO dnf install -y python3 python3-pip libgomp freetype libpng
elif command -v yum >/dev/null 2>&1; then
    $SUDO yum install -y python3 python3-pip libgomp freetype libpng
else
    echo "   WARN: no known package manager (apt/apk/dnf/yum); assuming python3 + pip already present"
fi

PIP=(python3 -m pip install --break-system-packages)
REQ=requirements.txt

echo "== [2/3] python packages (from $REQ, excluding z3) =="
"${PIP[@]}" -r <(grep -vE '^\s*(#|z3-solver)' "$REQ")

echo "== [3/3] z3 (target libz3 4.15.4) =="
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
