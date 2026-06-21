#!/usr/bin/env bash
# Restore the Python backend environment for the Evident web IDE.
#
# The dev image (Dockerfile.dev) bakes the Rust runtime + native libz3 4.15.4, but the
# Python deps the IDE backend needs (matplotlib/numpy/networkx for the renderers, the z3
# python binding for evident_viz, fastapi/uvicorn for the server) are NOT baked — they
# were installed ad-hoc and are lost when the container resets. Re-run this after a reset.
set -e
echo "[setup] apt: pip + prebuilt numpy/matplotlib/networkx (arm64, no compile)"
apt-get update -qq
apt-get install -y -qq python3-pip python3-numpy python3-matplotlib python3-networkx unzip curl

if ! python3 -c "import z3" 2>/dev/null; then
  echo "[setup] z3 python binding matching native libz3 4.15.4"
  curl -fsSL -o /tmp/z3.zip \
    "https://github.com/Z3Prover/z3/releases/download/z3-4.15.4/z3-4.15.4-arm64-glibc-2.34.zip"
  unzip -q -o /tmp/z3.zip -d /tmp
  cp -r /tmp/z3-4.15.4-arm64-glibc-2.34/bin/python/z3 /usr/lib/python3/dist-packages/
fi

echo "[setup] fastapi + uvicorn"
python3 -m pip install --break-system-packages --quiet fastapi "uvicorn[standard]"

echo "[setup] playwright chromium + deps (for the critic browser MCP — arm64 chromium, NOT chrome-for-testing)"
npx --yes playwright install --with-deps chromium

python3 -c "import z3,numpy,networkx,matplotlib,fastapi,uvicorn; print('[setup] backend env OK — z3', z3.get_version_string())"
echo "[setup] done. start the IDE with:  python3 ide/web/server.py"
