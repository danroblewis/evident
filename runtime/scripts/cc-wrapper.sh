#!/usr/bin/env bash
# Linker wrapper for the `evident` binary.
#
# Wraps `cc` and, after a successful link, patches the output binary's
# libz3.dylib load command to an absolute path. The bundled libz3 from
# python's z3-solver package has install_name `libz3.dylib` (no prefix),
# so dyld won't find it through `-rpath`. ld-prime (the default linker
# in Xcode 15+) doesn't accept `-Wl,-change`, so we patch post-link.
#
# Configured as the `aarch64-apple-darwin` linker in `.cargo/config.toml`.
# Replaces the manual `install_name_tool` step that lived in
# `scripts/install-bin.sh` for the "build via cargo + run binary" flow.

set -euo pipefail

Z3_LIB="/opt/anaconda3/lib/python3.13/site-packages/z3/lib/libz3.dylib"

cc "$@"

# Find the -o argument so we can patch the output.
out=""
prev=""
for arg in "$@"; do
    if [[ "$prev" == "-o" ]]; then
        out="$arg"
        break
    fi
    prev="$arg"
done

# Only patch a Mach-O file that already references the bare libz3 name.
if [[ -n "$out" && -f "$out" ]]; then
    if otool -L "$out" 2>/dev/null | grep -q '^	libz3\.dylib '; then
        install_name_tool -change libz3.dylib "$Z3_LIB" "$out" 2>/dev/null || true
    fi
fi
