#!/usr/bin/env bash
# Self-enforced budget: hard-cap this script at 55s regardless of runner.
[ -z "${GP_TIMEBOXED:-}" ] && exec env GP_TIMEBOXED=1 timeout 55 bash "$0" "$@"
# Goal: compiler2-purism — surface-rule burndown (docs/evident-purism.md).
#
# Measure 1 (trend, §3.6/V5): component-prefix namespacing of fsm
# internals. For each `fsm DriverXyz` in compiler2/*.ev, counts
# declaration-position names inside its body that begin with one of the
# fsm's own CamelCase component words, or their concatenation, plus
# "_" (DriverGroup -> group_*, DriverSetVar -> set_*/var_*/setvar_*).
# The fsm namespacing its own internals by its own name is the
# hand-namespacing symptom the goal targets; these should retire as
# claim headers / bare mentions land.
#
# Measure 2 (gate, V11): `..`-lift composition lines in
# compiler2/driver.ev. Per the goal, these should be header-based bare
# mentions, except deliberate context-sharing lifts — which must carry
# a documented justification.
#
# Survivor justification (ledger docs/purism-exemptions.md):
#   `V5 <file.ev> <prefix_*> — <reason>`   exempts that decl prefix in that file
#   `V11 driver.ev <SchemaName> — <reason>` exempts that ..-lift line
# Exempt counts are reported in labels so mass-exemption is visible.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
LEDGER="$ROOT/docs/purism-exemptions.md"

files=("$ROOT"/compiler2/*.ev)
[ -e "${files[0]}" ] || { echo "no compiler2/*.ev found" >&2; exit 1; }
[ -f "$ROOT/compiler2/driver.ev" ] || { echo "compiler2/driver.ev missing" >&2; exit 1; }

exfile="$(mktemp -t gp-purism-ex.XXXXXX)"
trap 'rm -f "$exfile"' EXIT
if [ -f "$LEDGER" ]; then
    # lines: "<class> <file> <token>"
    grep -ohE '^[-*[:blank:]]*(V5|V11)[[:blank:]]+[^[:blank:]]+[[:blank:]]+[^[:blank:]]+' "$LEDGER" \
        | sed -E 's/^[-*[:blank:]]+//' > "$exfile" || true
fi

# ── fsm component-prefix decls ──
read -r pref_cnt pref_exempt <<<"$(awk -v exfile="$exfile" '
BEGIN {
  while ((getline l < exfile) > 0) {
    split(l, f, /[[:blank:]]+/)
    if (f[1] == "V5") ex[f[2] " " f[3]] = 1
  }
}
function base(p) { n=split(p, parts, "/"); return parts[n] }
FNR==1 { fsmname="" }
/^(claim|type|schema|enum)[[:blank:]]/ { fsmname="" }
/^fsm[[:blank:]]/ {
  fsmname=$2; sub(/\(.*/, "", fsmname)
  n=0; delete words; concat=""
  w=""
  for (i=1; i<=length(fsmname); i++) {
    c = substr(fsmname, i, 1)
    if (c ~ /[A-Z]/ && w != "") { words[++n] = tolower(w); w = c } else w = w c
  }
  if (w != "") words[++n] = tolower(w)
  for (i=1; i<=n; i++) if (words[i] != "driver") concat = concat words[i]
  if (concat != "") words[++n] = concat
  next
}
fsmname != "" && /^[[:blank:]]+[a-z_][a-z_0-9]*[[:blank:]]*(,|∈|=)/ {
  line=$0; sub(/^[[:blank:]]+/, "", line); name=line; sub(/[^a-z_0-9].*/, "", name)
  for (i=1; i<=n; i++) {
    if (words[i] == "driver") continue
    p = words[i] "_"
    if (index(name, p) == 1) {
      if (ex[base(FILENAME) " " p "*"]) exempted++; else cnt++
      break
    }
  }
}
END { printf "%d %d\n", cnt+0, exempted+0 }
' "${files[@]}")"

# ── ..-lift lines in driver.ev ──
lift_cnt=0; lift_exempt=0
while IFS= read -r name; do
    [ -n "$name" ] || continue
    if grep -qE "^[-*[:blank:]]*V11[[:blank:]]+driver\.ev[[:blank:]]+${name}([[:blank:]]|$)" "$exfile"; then
        lift_exempt=$((lift_exempt+1))
    else
        lift_cnt=$((lift_cnt+1))
    fi
done <<<"$(grep -oE '^[[:blank:]]*\.\.[A-Za-z_][A-Za-z_0-9]*' "$ROOT/compiler2/driver.ev" | sed -E 's/^[[:blank:]]*\.\.//' || true)"

printf '{"goal":"compiler2-purism","measure":"fsm_prefix_decls","kind":"trend","value":%d,"target":0,"higher_is_better":false,"unit":"count","rung":"deterministic","period_s":300,"label":"decl-position names inside an fsm prefixed by the fsm'\''s own component word(s), unexempted (%d exempted via ledger)"}\n' "$pref_cnt" "$pref_exempt"
printf '{"goal":"compiler2-purism","measure":"driver_lift_compositions","kind":"gate","value":%d,"target":0,"higher_is_better":false,"unit":"count","rung":"deterministic","period_s":300,"label":"..-lift composition lines in driver.ev without a documented context-sharing justification (%d exempted)"}\n' "$lift_cnt" "$lift_exempt"
