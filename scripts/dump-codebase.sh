#!/usr/bin/env bash
# dump-codebase.sh — dump source files as markdown with file paths so a
# Claude session can ingest a whole codebase upfront instead of acquiring
# it iteratively.
#
# Usage:
#   scripts/dump-codebase.sh [PATH...]           # dump given paths
#   scripts/dump-codebase.sh                     # default: runtime/src
#   scripts/dump-codebase.sh -- runtime stdlib   # dump multiple roots
#
# Output: markdown to stdout. Each file becomes a `## <path>` heading
# followed by a fenced code block. A summary block at the end gives
# total files and total lines.
#
# File-extension filter: defaults to .rs/.toml/.ev/.md. Override with
# the EXTS env var (e.g. EXTS="rs" to dump only Rust).
#
# Comment stripping: set STRIP_COMMENTS=1 to run each .rs file through
# scripts/strip-comments.py before emitting it. Line count is
# preserved by the stripper so downstream line numbers still match
# the original file.
#
# Line numbers: set LINE_NUMBERS=1 to prefix every line with its
# original line number in `cat -n` format (Read tool style: 6-wide
# right-justified number, tab, content). Makes the dump nearly
# equivalent to running Read on each file.

set -euo pipefail

EXTS="${EXTS:-rs toml ev md}"
STRIP_COMMENTS="${STRIP_COMMENTS:-1}"
LINE_NUMBERS="${LINE_NUMBERS:-0}"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Build a -name pattern list for `find`.
find_args=()
first=1
for e in $EXTS; do
    if [ $first -eq 1 ]; then
        find_args+=(-name "*.$e")
        first=0
    else
        find_args+=(-o -name "*.$e")
    fi
done

if [ $# -eq 0 ]; then
    PATHS=("runtime/src")
else
    PATHS=("$@")
fi

echo "# Codebase dump"
echo
echo "Generated: $(date -u +%Y-%m-%dT%H:%M:%SZ)"
echo "Paths: ${PATHS[*]}"
echo "Extensions: $EXTS"
echo

total_files=0
total_lines=0
total_bytes=0

for path in "${PATHS[@]}"; do
    if [ -d "$path" ]; then
        # Skip hidden dirs and common noise (target/, node_modules/, .git/).
        # bash 3.2 lacks mapfile, so iterate the find output directly.
        find_output=$(
            find "$path" -type f \
                \( "${find_args[@]}" \) \
                -not -path "*/target/*" \
                -not -path "*/.git/*" \
                -not -path "*/node_modules/*" \
                | LC_ALL=C sort
        )
    elif [ -f "$path" ]; then
        find_output="$path"
    else
        echo "(skipping nonexistent path: $path)"
        continue
    fi

    while IFS= read -r f; do
        [ -z "$f" ] && continue
        total_files=$((total_files + 1))
        lines=$(wc -l <"$f")
        bytes=$(wc -c <"$f")
        total_lines=$((total_lines + lines))
        total_bytes=$((total_bytes + bytes))

        case "$f" in
            *.rs)   lang=rust ;;
            *.toml) lang=toml ;;
            *.ev)   lang="" ;;
            *.md)   lang="markdown" ;;
            *)      lang="" ;;
        esac

        echo "## \`$f\`"
        echo
        echo "\`\`\`$lang"
        # Two-stage pipeline so that line numbers match the original
        # file (post-strip line counts are preserved by strip-comments.py).
        # Use awk to mimic Read's cat-n format: 6-wide right-justified
        # number, tab, content.
        if [ "$STRIP_COMMENTS" = "1" ] && [ "$lang" = "rust" ]; then
            content_cmd=(python3 "$SCRIPT_DIR/strip-comments.py" "$f")
        else
            content_cmd=(cat "$f")
        fi
        if [ "$LINE_NUMBERS" = "1" ]; then
            "${content_cmd[@]}" | awk '{printf "%6d\t%s\n", NR, $0}'
        else
            "${content_cmd[@]}"
        fi
        echo "\`\`\`"
        echo
    done <<<"$find_output"
done

echo "---"
echo
echo "**Summary**"
echo
echo "- Total files: $total_files"
echo "- Total lines: $total_lines"
echo "- Total bytes: $total_bytes"
