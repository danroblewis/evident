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

set -euo pipefail

EXTS="${EXTS:-rs toml ev md}"

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
        cat "$f"
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
