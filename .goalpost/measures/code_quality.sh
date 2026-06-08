#!/usr/bin/env bash
# Self-enforced budget: hard-cap this script at 55s regardless of runner.
[ -z "${GP_TIMEBOXED:-}" ] && exec env GP_TIMEBOXED=1 timeout 55 bash "$0" "$@"
# Goal: Evident code quality — no monolithic claims.
#
# Measures the size distribution of top-level declarations (claim / fsm /
# type / schema / enum) across the ACTIVE production Evident source
# (compiler2/ + stdlib/ — not tests/fixtures, whose sizes are arbitrary,
# and not the legacy compiler/, which is being superseded).
# Computed LIVE from the tree each run; no artifact, because parsing .ev
# is far under the measure budget.
#
# A "claim's size" = lines from its header to the next top-level header
# (or EOF) — its footprint in the file.
#
# Headline = max_claim_lines: the single biggest claim. driver_main is the
# monolith the decomposition is breaking up; this number falls toward the
# ceiling as modules extract. Targets mirror the decomposition binding spec
# (docs/plans/driver-decomposition-execution-plan.md §3/§6): a claim should
# be <= ~500 lines, and none should exceed it. avg target tracks the
# "logic lives in small claims" style guidance.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

files=$(find "$ROOT/compiler2" "$ROOT/stdlib" -name '*.ev' 2>/dev/null || true)
[ -n "$files" ] || { echo "no production Evident source found" >&2; exit 1; }

# Per-claim sizes (one "<lines>\t<name>" per declaration), reset at each
# file boundary. p95 (not avg) is the upper-tier signal: avg is dragged to
# noise by the ~190 tiny helpers, while p95 shows how big the big claims
# actually are without collapsing to a single outlier like max.
sizes=$(awk '
  function close_decl() { if (name != "") printf "%d\t%s\n", cur, name }
  FNR == 1 { close_decl(); name = ""; cur = 0 }
  /^(claim|fsm|type|schema|enum)[ \t]/ { close_decl(); name = $2; cur = 1; next }
  name != "" { cur++ }
  END { close_decl() }
' $files)
[ -n "$sizes" ] || { echo "no declarations found" >&2; exit 1; }

cnt=$(printf '%s\n' "$sizes" | wc -l | tr -d ' ')
maxline=$(printf '%s\n' "$sizes" | sort -rn | head -1)
mx=${maxline%%$'\t'*}; mxname=${maxline#*$'\t'}
# 95th percentile by nearest-rank: ceil(0.95 * n).
p95idx=$(awk -v n="$cnt" 'BEGIN{ i=0.95*n; r=int(i); if(r<i)r++; if(r<1)r=1; print r }')
p95=$(printf '%s\n' "$sizes" | cut -f1 | sort -n | awk -v i="$p95idx" 'NR==i{print; exit}')
over=$(printf '%s\n' "$sizes" | awk -F'\t' '$1>500' | wc -l | tr -d ' ')

printf '{"goal":"code-quality","measure":"max_claim_lines","kind":"trend","value":%s,"target":500,"higher_is_better":false,"unit":"lines","rung":"deterministic","period_s":300,"label":"largest single claim (%s) — should fall to <=500 as the monolith decomposes"}\n' "$mx" "$mxname"
printf '{"goal":"code-quality","measure":"p95_claim_lines","kind":"trend","value":%s,"target":300,"higher_is_better":false,"unit":"lines","rung":"deterministic","period_s":300,"label":"95th-percentile claim size (upper-tier footprint, robust to the monolith outlier)"}\n' "$p95"
printf '{"goal":"code-quality","measure":"oversized_claims","kind":"gate","value":%s,"target":0,"higher_is_better":false,"unit":"count","rung":"deterministic","period_s":300,"label":"claims over 500 lines (the monolith ceiling)"}\n' "$over"
printf '{"goal":"code-quality","measure":"claim_count","kind":"trend","value":%s,"target":0,"higher_is_better":true,"unit":"count","rung":"deterministic","period_s":300,"label":"top-level declarations in production Evident source"}\n' "$cnt"
