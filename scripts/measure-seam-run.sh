#!/usr/bin/env bash
# TODO: rewrite in Evident
#
# scripts/measure-seam-run.sh — wall-clock + peak-RSS measurement for one
# seam compile (kernel + compiler.smt2). Linux-only (reads /proc VmHWM).
#
# Usage:
#   measure-seam-run.sh <label> <file.ev> <claim> <out.smt2> [env VAR=1 ...]
#
# Extra args after <out.smt2> are prefixed to the kernel invocation
# (e.g. `env EVIDENT_NO_PRESIMPLIFY=1`). Prints a one-line summary:
#   <label> wall=<s> peak_rss_mb=<MB> exit=<code>
# Kernel stderr goes to <out.smt2>.stderr.

set -u -o pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
KERNEL="$ROOT/kernel/target/release/kernel"

LABEL="$1"; SRC="$2"; CLAIM="$3"; OUT="$4"; shift 4

FLAT="$(mktemp -t measure-flat.XXXXXX.ev)"
trap 'rm -f "$FLAT"' EXIT
"$ROOT/scripts/flatten-evident.sh" "$SRC" > "$FLAT" || {
    echo "$LABEL: flatten failed" >&2; exit 1; }

START=$(date +%s.%N)
printf '%s\n%s\n' "$FLAT" "$CLAIM" | "$@" "$KERNEL" "$ROOT/compiler.smt2" \
    > "$OUT" 2> "$OUT.stderr" &
PID=$!

PEAK_KB=0
while kill -0 "$PID" 2>/dev/null; do
    HWM=$(awk '/VmHWM/{print $2}' "/proc/$PID/status" 2>/dev/null) || true
    [ -n "${HWM:-}" ] && PEAK_KB=$HWM
    sleep 0.5
done
wait "$PID"; CODE=$?
END=$(date +%s.%N)

WALL=$(awk -v a="$START" -v b="$END" 'BEGIN{printf "%.1f", b-a}')
PEAK_MB=$(( PEAK_KB / 1024 ))
echo "$LABEL wall=${WALL}s peak_rss_mb=${PEAK_MB} exit=${CODE} out_lines=$(wc -l < "$OUT")"
