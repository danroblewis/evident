#!/usr/bin/env bash
# install.sh — Build and install the Evident VS Code extension.
#
# Usage:
#   cd /path/to/evident/vscode-evident
#   ./install.sh

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
EXT_NAME="evident-lang"
EXT_VERSION="0.1.0"
VSIX_FILE="${EXT_NAME}-${EXT_VERSION}.vsix"

cd "$SCRIPT_DIR"

echo "==> Building Evident VS Code extension..."

# ── Try vsce (the official packager) ─────────────────────────────────────────
if command -v vsce &>/dev/null; then
    vsce package --no-dependencies --out "$VSIX_FILE"
    echo "==> Packaged: $VSIX_FILE"

    if command -v code &>/dev/null; then
        code --install-extension "$VSIX_FILE"
        echo "==> Installed via 'code --install-extension'. Restart VS Code."
    else
        echo "==> 'code' CLI not found. Install the VSIX manually:"
        echo "    VS Code → Extensions → ⋯ → Install from VSIX → $(pwd)/${VSIX_FILE}"
    fi

# ── Fallback: copy directly into ~/.vscode/extensions ────────────────────────
else
    echo "==> 'vsce' not found — falling back to direct copy."
    DEST="$HOME/.vscode/extensions/${EXT_NAME}"
    mkdir -p "$DEST"
    cp -r \
        package.json \
        language-configuration.json \
        README.md \
        syntaxes \
        themes \
        "$DEST/"
    echo "==> Extension copied to: $DEST"
    echo "==> Restart VS Code for the changes to take effect."

    echo ""
    echo "    To install vsce in the future, run:"
    echo "      npm install -g @vscode/vsce"
fi
