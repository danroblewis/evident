#!/usr/bin/env bash
# Self-enforced budget: hard-cap this script at 55s regardless of runner.
[ -z "${GP_TIMEBOXED:-}" ] && exec env GP_TIMEBOXED=1 timeout 55 bash "$0" "$@"
# Goal: compiler2-purism — surface-rule burndown (docs/evident-purism.md).
#
# Trend: hand-namespacing of variable names by a shared prefix. HONEST
# auto-detector (replaces the 2026-06-10 component-word + hand-denylist
# heuristic, which was gameable — it missed every prefix family that
# wasn't an fsm's own name or on the list, and a rename could "win" by
# trading one unseen prefix for another).
#
# Definition: over compiler2/*.ev (comments + string literals stripped),
# a decl-position name's leading `<word>_` segment is its PREFIX. A prefix
# is NAMESPACING DEBT when it is shared by >= MINFAM (default 4) DISTINCT
# decl-names — i.e. the word is being used as a namespace, not as a one-off
# descriptive qualifier (`walk_state` alone is fine; `parse_a parse_b
# parse_c parse_d` is a namespace). Every decl-position occurrence under a
# qualifying prefix counts. Carry-duals (`_x`) are skipped. The idiomatic
# boolean prefixes `is_`/`has_` are exempt by allowlist.
#
# This is un-gameable by renaming: inventing a fresh prefix for >=MINFAM
# names just makes that new prefix count. The only way down is to remove
# the shared prefix — by encapsulation (bare-mention hiding lets internals
# drop the prefix) or by giving names varied descriptive (non-shared) names.
#
# Survivor justification (ledger docs/purism-exemptions.md):
#   `V5 <file.ev> <prefix_*> — <reason>`   exempts that prefix in that file
# Exempt count is reported in the label so mass-exemption is visible.
# A per-family breakdown (family size + occurrences) prints to stderr.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
LEDGER="$ROOT/docs/purism-exemptions.md"
MINFAM="${GP_PREFIX_MINFAM:-4}"

files=("$ROOT"/compiler2/*.ev)
[ -e "${files[0]}" ] || { echo "no compiler2/*.ev found" >&2; exit 1; }

exfile="$(mktemp -t gp-purism-ex.XXXXXX)"
trap 'rm -f "$exfile"' EXIT
if [ -f "$LEDGER" ]; then
    grep -ohE '^[-*[:blank:]]*V5[[:blank:]]+[^[:blank:]]+[[:blank:]]+[^[:blank:]]+' "$LEDGER" \
        | sed -E 's/^[-*[:blank:]]+//' > "$exfile" || true
fi

strip() { sed 's/--.*//;s/"[^"]*"/""/g' "$1"; }

read -r combined exempted <<<"$(
for f in "${files[@]}"; do
    echo "@@FILE $(basename "$f")"
    strip "$f"
done | awk -v exfile="$exfile" -v K="$MINFAM" '
BEGIN {
  while ((getline l < exfile) > 0) {
    split(l, f, /[[:blank:]]+/)
    if (f[1] == "V5") ex[f[2] " " f[3]] = 1   # ex["file prefix*"]
  }
  allow["is_"]=1; allow["has_"]=1
}
/^@@FILE / { curfile=$2; next }
# decl line? leading ws + comma-separated name-list + a binding terminator.
{
  line=$0
  if (line !~ /^[[:space:]]+[a-z_]/) next
  seg=line
  if (match(seg, /(∈|≤|≥|=|<|>)/)) seg=substr(seg,1,RSTART-1); else next
  test=seg; gsub(/[[:space:]]/, "", test)
  if (test !~ /^[a-z_][a-z_0-9]*(,[a-z_][a-z_0-9]*)*$/) next
  m=split(test, names, /,/)
  for (j=1; j<=m; j++) {
    name=names[j]
    if (name ~ /^_/) continue                  # carry dual
    if (name !~ /^[a-z][a-z0-9]*_/) continue    # no prefix segment
    p=name; sub(/_.*/, "_", p)                  # leading <word>_
    if (p in allow) continue
    occ[curfile SUBSEP p]++                      # occurrences per (file,prefix)
    distinct[p SUBSEP name]=1                    # for distinct-name-per-prefix
  }
}
END {
  for (k in distinct) { split(k, a, SUBSEP); dc[a[1]]++ }
  cnt=0; exempted=0
  for (k in occ) {
    split(k, a, SUBSEP); file=a[1]; p=a[2]
    if (dc[p] < K) continue
    n=occ[k]
    if (ex[file " " p "*"]) exempted += n; else cnt += n
  }
  printf "%d %d\n", cnt, exempted
  # breakdown to stderr, biggest families first
  for (k in occ) { split(k, a, SUBSEP); p=a[2]; if (dc[p] >= K) tot[p]+=occ[k] }
  n=0; for (p in tot) { order[n++]=p }
  for (i=0;i<n;i++) for (jj=i+1;jj<n;jj++) if (tot[order[jj]]>tot[order[i]]) { t=order[i]; order[i]=order[jj]; order[jj]=t }
  for (i=0;i<n;i++) { p=order[i]; printf "  %-16s family=%d  occ=%d\n", p, dc[p], tot[p] > "/dev/stderr" }
}
'
)"

printf '{"goal":"compiler2-purism","measure":"prefix_decls","kind":"trend","value":%d,"target":0,"higher_is_better":false,"unit":"count","rung":"deterministic","period_s":300,"label":"decl-position names under a shared <word>_ prefix (>=%s distinct names share it; is_/has_ exempt), all families auto-detected, unexempted (%d exempted via ledger)"}\n' "$combined" "$MINFAM" "$exempted"
