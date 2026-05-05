#!/usr/bin/env bash
# Build the release binary and patch its libz3 reference so it runs
# standalone (no DYLD_LIBRARY_PATH needed).
#
# `libz3.dylib` from python's z3-solver package has install_name
# `libz3.dylib` with no prefix, so dyld won't search rpath for it.
# We rewrite the load command to point at the absolute path.
#
# Usage:
#   ./scripts/install-bin.sh                # builds + patches in place
#   ./scripts/install-bin.sh /usr/local/bin # also copies to a dest dir

set -euo pipefail

cd "$(dirname "$0")/.."
Z3_LIB="/opt/anaconda3/lib/python3.13/site-packages/z3/lib/libz3.dylib"
BIN="target/release/evident"

cargo build --release --quiet

# Re-patch every time (idempotent — install_name_tool just rewrites).
install_name_tool -change libz3.dylib "$Z3_LIB" "$BIN" 2>/dev/null || true

echo "built: $BIN"
"$BIN" --help

if [[ $# -gt 0 ]]; then
    DEST="$1"
    cp "$BIN" "$DEST/"
    echo "copied to: $DEST/evident"
fi
