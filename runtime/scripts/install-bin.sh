#!/usr/bin/env bash
# Build `evident` and patch its libz3 load command so the binary runs
# standalone (no DYLD_LIBRARY_PATH needed).
#
# `libz3.dylib` from python's z3-solver package has install_name
# `libz3.dylib` with no prefix, so dyld won't search rpath for it.
# We rewrite the load command to the absolute path.
#
# Usage:
#   ./scripts/install-bin.sh                  # release build (default)
#   ./scripts/install-bin.sh --debug          # debug build (faster compile, no opt)
#   ./scripts/install-bin.sh /usr/local/bin   # release + copy to a dest dir
#   ./scripts/install-bin.sh --debug /usr/local/bin

set -euo pipefail
cd "$(dirname "$0")/.."

PROFILE="release"
DEST=""
for arg in "$@"; do
    case "$arg" in
        --debug)   PROFILE="debug" ;;
        --release) PROFILE="release" ;;
        *)         DEST="$arg" ;;
    esac
done

Z3_LIB="/opt/anaconda3/lib/python3.13/site-packages/z3/lib/libz3.dylib"

if [[ "$PROFILE" == "release" ]]; then
    cargo build --release --quiet
    BIN="target/release/evident"
else
    cargo build --quiet
    BIN="target/debug/evident"
fi

# Re-patch every time (idempotent — install_name_tool just rewrites).
install_name_tool -change libz3.dylib "$Z3_LIB" "$BIN" 2>/dev/null || true

echo "built: $BIN"
"$BIN" --help

if [[ -n "$DEST" ]]; then
    cp "$BIN" "$DEST/"
    echo "copied to: $DEST/evident"
fi
