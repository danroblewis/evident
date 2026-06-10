#!/usr/bin/env bash
# Self-enforced budget: hard-cap this script at 55s regardless of runner.
[ -z "${GP_TIMEBOXED:-}" ] && exec env GP_TIMEBOXED=1 timeout 55 bash "$0" "$@"
# Goal: compiler2-purism — surface-rule burndown (docs/evident-purism.md).
#
# Trend (V9/§3.4): value-selection and case-code ternary chains in
# compiler2/*.ev source. Detection: statements are joined by
# parenthesis balance (chains span lines); a statement is a chain when
# it contains >=2 literal-equality ternary tests — `x = <int> ?` or
# `x = "<key>" ?` in condition position (comparing a discriminant
# against successive keys/case codes to select among values).
#
# Blessed and exempt by construction (per the goal statement): the
# carried-write HOLD chain — a chain whose final else-arm is a carry
# (`: _name)`), i.e. "hold the previous value unless...". Everything
# else with >=2 key/code tests is the V9 class.
#
# Survivor justification (ledger docs/purism-exemptions.md):
#   `V9 <file.ev> <lhs_name> — <reason>`
# exempts the chain statement(s) defining <lhs_name> in that file.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
LEDGER="$ROOT/docs/purism-exemptions.md"

files=("$ROOT"/compiler2/*.ev)
[ -e "${files[0]}" ] || { echo "no compiler2/*.ev found" >&2; exit 1; }

exfile="$(mktemp -t gp-purism-ex.XXXXXX)"
trap 'rm -f "$exfile"' EXIT
if [ -f "$LEDGER" ]; then
    grep -ohE '^[-*[:blank:]]*V9[[:blank:]]+[^[:blank:]]+[[:blank:]]+[^[:blank:]]+' "$LEDGER" \
        | sed -E 's/^[-*[:blank:]]+//' > "$exfile" || true
fi

read -r cnt exempted <<<"$(awk -v exfile="$exfile" '
BEGIN {
  while ((getline l < exfile) > 0) {
    split(l, f, /[[:blank:]]+/)
    if (f[1] == "V9") ex[f[2] " " f[3]] = 1
  }
}
function base(p) { n2=split(p, parts, "/"); return parts[n2] }
FNR==1 { buf=""; depth=0 }
{
  line=$0
  sub(/--.*/, "", line)                  # strip comments
  gsub(/"[^"]*"/, "\"\"", line)          # blank string literals
  if (line ~ /^[[:blank:]]*$/ && depth == 0) next
  if (line ~ /^(claim|fsm|type|schema|enum)[[:blank:]]/ && depth == 0) { buf="" }
  buf = buf " " line
  o = gsub(/\(/, "(", line); c = gsub(/\)/, ")", line)
  depth += o - c
  if (depth <= 0) {
    s = buf; buf = ""; depth = 0
    tests = 0; tmp = s
    while (match(tmp, /=[[:blank:]]*(""|[0-9]+)[[:blank:]]*\?/)) { tests++; tmp = substr(tmp, RSTART + RLENGTH) }
    if (tests >= 2) {
      # hold-chain exemption: final else-arm is a carry _name
      if (s ~ /:[[:blank:]]*_[a-z][a-z_0-9.]*[[:blank:]]*\)*[[:blank:]]*$/) next
      # lhs: first identifier after an optional range-∀ prefix
      lhs = s
      sub(/^[[:blank:]]*∀[^:]*:[[:blank:]]*/, "", lhs)
      if (match(lhs, /[a-z_][a-z_0-9]*/)) lhs = substr(lhs, RSTART, RLENGTH); else lhs = "?"
      if (ex[base(FILENAME) " " lhs]) exempted++; else cnt++
    }
  }
}
END { printf "%d %d\n", cnt+0, exempted+0 }
' "${files[@]}")"

printf '{"goal":"compiler2-purism","measure":"v9_selection_chains","kind":"trend","value":%d,"target":0,"higher_is_better":false,"unit":"count","rung":"deterministic","period_s":300,"label":"value-selection/case-code ternary chains (>=2 key tests, non-hold) in compiler2/*.ev, unexempted (%d exempted via ledger)"}\n' "$cnt" "$exempted"
