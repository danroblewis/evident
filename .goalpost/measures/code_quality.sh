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

# awk: walk every decl, record span sizes; report max, avg, count, #>500,
# and the name of the largest. Reset the open decl at each file boundary.
read -r mx avg cnt over mxname <<EOF
$(awk '
  function close_decl() {
    if (name != "") { c++; tot += cur; if (cur > mx) { mx = cur; mxname = name }
                      if (cur > 500) over++ }
  }
  FNR == 1 { close_decl(); name = ""; cur = 0 }
  /^(claim|fsm|type|schema|enum)[ \t]/ { close_decl(); name = $2; cur = 1; next }
  name != "" { cur++ }
  END {
    close_decl()
    avg = (c > 0) ? tot / c : 0
    printf "%d %.1f %d %d %s\n", mx, avg, c, over, (mxname == "" ? "-" : mxname)
  }
' $files)
EOF

printf '{"goal":"code-quality","measure":"max_claim_lines","kind":"trend","value":%s,"target":500,"higher_is_better":false,"unit":"lines","rung":"deterministic","period_s":300,"label":"largest single claim (%s) — should fall to <=500 as the monolith decomposes"}\n' "$mx" "$mxname"
printf '{"goal":"code-quality","measure":"avg_claim_lines","kind":"trend","value":%s,"target":150,"higher_is_better":false,"unit":"lines","rung":"deterministic","period_s":300,"label":"average claim footprint across compiler2 + stdlib"}\n' "$avg"
printf '{"goal":"code-quality","measure":"oversized_claims","kind":"gate","value":%s,"target":0,"higher_is_better":false,"unit":"count","rung":"deterministic","period_s":300,"label":"claims over 500 lines (the monolith ceiling)"}\n' "$over"
printf '{"goal":"code-quality","measure":"claim_count","kind":"trend","value":%s,"target":0,"higher_is_better":true,"unit":"count","rung":"deterministic","period_s":300,"label":"top-level declarations in production Evident source"}\n' "$cnt"
