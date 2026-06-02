#!/usr/bin/env bash
# TODO: rewrite in Evident
#
# flatten-evident.sh — import-flattening preprocessor (the cutover bridge).
#
# Walks the `import "..."` graph rooted at a single .ev file and emits one
# flat .ev source to stdout, with every imported file inlined exactly once
# in dependency order (each file appears AFTER everything it imports). The
# `import` lines themselves are commented out (Evident `--` comments) so the
# flattened source carries no live imports.
#
# Why this exists: bootstrap resolves imports natively in
#   bootstrap/runtime/src/runtime/load.rs  (load_file + resolve_import)
# but the future `kernel + compiler.smt2` compiler will NOT do import
# resolution. Feeding `flatten-evident.sh file.ev | kernel compiler.smt2`
# gives the self-hosted compiler a single resolved translation unit. See
# CLAUDE.md "The deletion path".
#
# Import syntax matched here is exactly what bootstrap accepts: a top-level
# line `import "<path>"` — see bootstrap/runtime/src/parser/program.rs
# (Token::Import -> Token::Str). Path resolution mirrors
# bootstrap/runtime/src/runtime/load.rs::resolve_import (lines 54-76):
# try the path relative to the repo root / cwd first, then relative to the
# importing file's directory.
#
# Usage:
#   scripts/flatten-evident.sh <file.ev>   > flat.ev
#
# Exit codes:
#   0  success
#   1  bad usage / file not found / import not found
#   2  import cycle detected

set -u -o pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

die() { echo "flatten-evident: $*" >&2; exit 1; }

if [ "$#" -ne 1 ]; then
    echo "usage: flatten-evident.sh <file.ev>" >&2
    exit 1
fi

ROOT_ARG="$1"
[ -f "$ROOT_ARG" ] || die "file not found: $ROOT_ARG"

# Absolutise a path (resolves the directory, keeps the basename) so the same
# file reached by different import strings dedups to one identity.
canon() {
    local p="$1" d b
    d="$(cd "$(dirname "$p")" 2>/dev/null && pwd)" || return 1
    b="$(basename "$p")"
    printf '%s/%s\n' "$d" "$b"
}

# resolve_import <import_path> <importing_file_canonical>
# Prints the resolved canonical path, or returns 1 if nothing exists.
# Order mirrors bootstrap load.rs::resolve_import.
resolve_import() {
    local imp="$1" base="$2" cand
    cand="$REPO_ROOT/$imp"; [ -f "$cand" ] && { canon "$cand"; return 0; }
    if [ -n "$base" ]; then
        cand="$(dirname "$base")/$imp"; [ -f "$cand" ] && { canon "$cand"; return 0; }
    fi
    [ -f "$imp" ] && { canon "$imp"; return 0; }
    return 1
}

# Newline-separated sets of canonical paths.
EMITTED=""   # files already inlined (printed once)
STACK=""     # files on the current DFS path (for cycle detection)

in_set() { printf '%s\n' "$2" | grep -qxF -- "$1"; }

IMPORT_RE='^[[:space:]]*import[[:space:]]+"(.+)"[[:space:]]*$'

# Depth-first post-order walk: recurse into a file's imports (in source
# order) before printing the file itself, so dependencies precede dependents.
walk() {
    local file="$1" line imp resolved
    in_set "$file" "$EMITTED" && return 0
    if in_set "$file" "$STACK"; then
        echo "flatten-evident: import cycle detected at ${file#$REPO_ROOT/}" >&2
        exit 2
    fi
    STACK="$STACK"$'\n'"$file"

    while IFS= read -r line || [ -n "$line" ]; do
        if [[ $line =~ $IMPORT_RE ]]; then
            imp="${BASH_REMATCH[1]}"
            if ! resolved="$(resolve_import "$imp" "$file")"; then
                die "import not found: \"$imp\" (from ${file#$REPO_ROOT/})"
            fi
            walk "$resolved"
        fi
    done < "$file"

    # pop from the cycle stack
    STACK="$(printf '%s\n' "$STACK" | grep -vxF -- "$file")"
    EMITTED="$EMITTED"$'\n'"$file"

    # inline the file, commenting out its own import lines
    printf '\n-- ===== %s =====\n' "${file#$REPO_ROOT/}"
    while IFS= read -r line || [ -n "$line" ]; do
        if [[ $line =~ $IMPORT_RE ]]; then
            printf -- '-- %s\n' "$line"
        else
            printf '%s\n' "$line"
        fi
    done < "$file"
}

walk "$(canon "$ROOT_ARG")"
