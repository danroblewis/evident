#!/usr/bin/env bash
# Self-enforced budget: hard-cap this script at 55s regardless of runner.
[ -z "${GP_TIMEBOXED:-}" ] && exec env GP_TIMEBOXED=1 timeout 55 bash "$0" "$@"
# Goal: compiler2-purism — surface-rule burndown (docs/evident-purism.md).
#
# SECONDARY SIGNAL: the evident-critic baseline reports under
# docs/critic-reports/*baseline*.md. The greps in the sibling purism_*
# measures are the primary, live ground truth; the critic report
# cross-checks them with full-rulebook judgment, so this measure parses
# the LATEST report (by its `**Date:**` field) and emits:
#
#   1. critic_v18_v9_findings (trend): summary-table findings whose
#      Rule column cites V18 or V9 — the goal's two named violation
#      classes — summed over BLOCKER + WARN columns.
#   2. critic_report_age_days (gate): days since the latest report's
#      date. A stale report can't masquerade as current truth; the
#      grep measures stay live regardless.
#
# Ruler-broken (exit 2) when no baseline report with a parseable
# verdict line exists at all. A report whose table legitimately has no
# V18/V9 rows reads as 0 — the age gate plus the live greps keep that
# honest.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

latest=""; latest_date=""
for f in "$ROOT"/docs/critic-reports/*baseline*.md; do
    [ -f "$f" ] || continue
    d=$(grep -ohE '\*\*Date:\*\*[[:blank:]]*[0-9]{4}-[0-9]{2}-[0-9]{2}' "$f" | head -1 | grep -oE '[0-9]{4}-[0-9]{2}-[0-9]{2}' || true)
    [ -n "$d" ] || continue
    if [ -z "$latest_date" ] || [[ "$d" > "$latest_date" ]]; then
        latest="$f"; latest_date="$d"
    fi
done
[ -n "$latest" ] || { echo "no dated critic baseline report found" >&2; exit 2; }
grep -qE 'VIOLATIONS:[[:blank:]]*[0-9]+ BLOCKER' "$latest" \
    || { echo "latest report $latest has no parseable verdict line" >&2; exit 2; }

# Sum BLOCKER + WARN over summary-table rows whose Rule column cites
# V18 or V9. Table shape: | class | rule | BLOCKER | WARN | NOTE |
findings=$(awk -F'|' '
    NF >= 6 && $3 ~ /V18|V9([^0-9]|$)/ {
        b = $4; w = $5
        gsub(/[^0-9]/, "", b); gsub(/[^0-9]/, "", w)
        sum += (b == "" ? 0 : b) + (w == "" ? 0 : w)
    }
    END { print sum + 0 }
' "$latest")

age_days=$(awk -v now="$(date +%s)" -v d="$latest_date" 'BEGIN {
    split(d, p, "-")
    ts = mktime(p[1] " " p[2] " " p[3] " 12 0 0")
    printf "%.1f", (now - ts) / 86400
}')

printf '{"goal":"compiler2-purism","measure":"critic_v18_v9_findings","kind":"trend","value":%d,"target":0,"higher_is_better":false,"unit":"count","rung":"deterministic","period_s":300,"label":"V18+V9 BLOCKER/WARN findings in latest critic baseline (%s) — secondary signal"}\n' "$findings" "$(basename "$latest")"
printf '{"goal":"compiler2-purism","measure":"critic_report_age_days","kind":"gate","value":%s,"target":14,"higher_is_better":false,"unit":"d","rung":"deterministic","period_s":300,"label":"age of latest critic baseline report (dated %s)"}\n' "$age_days" "$latest_date"
