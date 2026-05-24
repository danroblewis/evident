#!/usr/bin/env bash
# profile-all.sh — sweep `evident profile` across every queryable schema
# in examples/ + stdlib/ and regenerate docs/perf/bottlenecks.md.
#
# For each source file we run `evident profile <file> --all --json`, which
# profiles every schema *defined in that file* (imports excluded, test
# claims / generic templates skipped) and emits one JSON document. The
# raw JSONs are merged into the markdown report by gen_bottlenecks.py.
#
# Usage:
#   ./scripts/profile-all.sh            # build if needed, profile, write report
#   ./scripts/profile-all.sh --no-build # skip the cargo build
#
# Takes a few minutes: each claim runs N pin-solves (N = solved-for var
# count). Progress prints per claim.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

BIN="$ROOT/runtime/target/release/evident"
OUT="$ROOT/docs/perf/bottlenecks.md"
RAW_DIR="$(mktemp -d "${TMPDIR:-/tmp}/evident-bottlenecks.XXXXXX")"
trap 'rm -rf "$RAW_DIR"' EXIT

# Pure-Z3 baseline: the JIT would otherwise mask the solve cost we are
# trying to rank. With it off, query_us is exactly the Z3 work.
export EVIDENT_FUNCTIONIZE=0

if [[ "${1:-}" != "--no-build" ]]; then
    echo "==> building release binary"
    cargo build --release --manifest-path "$ROOT/runtime/Cargo.toml"
fi
[[ -x "$BIN" ]] || { echo "error: $BIN not found — build first" >&2; exit 1; }

# Files to profile: every examples/test_*.ev, every examples/test_*/*.ev
# (multi-file demos like test_21_mario/main.ev), and every stdlib/*.ev.
# (read loop instead of mapfile — macOS ships bash 3.2.)
FILES=()
while IFS= read -r line; do
    [[ -n "$line" ]] && FILES+=("$line")
done < <(
    { ls "$ROOT"/examples/test_*.ev 2>/dev/null || true
      ls "$ROOT"/examples/test_*/*.ev 2>/dev/null || true
      ls "$ROOT"/stdlib/*.ev 2>/dev/null || true
    } | sed "s#^$ROOT/##" | sort -u
)

echo "==> profiling ${#FILES[@]} files (EVIDENT_FUNCTIONIZE=0)"
i=0
for f in "${FILES[@]}"; do
    i=$((i + 1))
    safe="$(echo "$f" | tr '/.' '__')"
    echo "[$i/${#FILES[@]}] $f"
    # `--all` profiles every queryable schema defined in the file. The
    # tool handles per-claim UNSAT/error internally and exits 0; a whole
    # -file load failure (missing import, parse error) is tolerated — we
    # warn and move on rather than abort the whole sweep.
    if ! "$BIN" profile "$f" --all --json --top 5 \
            >"$RAW_DIR/$safe.json" 2>>"$RAW_DIR/progress.log"; then
        echo "    warn: profile failed for $f (skipped)" >&2
        rm -f "$RAW_DIR/$safe.json"
    fi
done

echo "==> rendering report -> $OUT"
mkdir -p "$(dirname "$OUT")"
python3 "$ROOT/scripts/gen_bottlenecks.py" "$RAW_DIR" >"$OUT"

echo "==> done. $(grep -c '^####' "$OUT") claims documented."
echo "    $OUT"
