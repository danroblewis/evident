#!/usr/bin/env bash
# Self-enforced budget: hard-cap this script at 55s regardless of runner.
[ -z "${GP_TIMEBOXED:-}" ] && exec env GP_TIMEBOXED=1 timeout 55 bash "$0" "$@"
# Deletability clause: "the legacy compiler/ tree [is] deletable
# without loss of capability." That is false for as long as compiler2's
# own import closure reaches into compiler/ — today compiler2/driver.ev
# imports compiler/lexer.ev, compiler/parser.ev, compiler/translate_arith.ev
# (plus whatever those pull in transitively).
#
# Trend: the number of DISTINCT compiler/*.ev files reachable from
# compiler2/*.ev via `import "…"` (transitive closure, computed live
# from the source tree). Burns down to 0 when compiler2 is genuinely
# free of the legacy tree. Runs in milliseconds; no artifact needed.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

# BFS over import statements (paths are repo-root-relative).
tmp_seen="$(mktemp)"; tmp_q="$(mktemp)"; tmp_legacy="$(mktemp)"
trap 'rm -f "$tmp_seen" "$tmp_q" "$tmp_legacy"' EXIT
ls "$ROOT"/compiler2/*.ev | sed "s|^$ROOT/||" > "$tmp_q"

while [ -s "$tmp_q" ]; do
    f="$(head -1 "$tmp_q")"; sed -i 1d "$tmp_q"
    grep -qxF "$f" "$tmp_seen" && continue
    echo "$f" >> "$tmp_seen"
    [ -f "$ROOT/$f" ] || continue
    case "$f" in compiler/*) echo "$f" >> "$tmp_legacy" ;; esac
    grep -oE '^import "[^"]+"' "$ROOT/$f" 2>/dev/null \
        | sed 's/^import "//; s/"$//' >> "$tmp_q" || true
done

n="$(sort -u "$tmp_legacy" | grep -c . || true)"
printf '{"goal":"compiler2-selfhost","measure":"legacy_compiler_imports","kind":"trend","value":%s,"target":0,"higher_is_better":false,"unit":"count","rung":"deterministic","period_s":300,"label":"compiler/*.ev files in compiler2 import closure (must be 0 for compiler/ to be deletable)"}\n' "$n"
