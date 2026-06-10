#!/usr/bin/env bash
# Self-enforced budget: hard-cap this script at 55s regardless of runner.
[ -z "${GP_TIMEBOXED:-}" ] && exec env GP_TIMEBOXED=1 timeout 55 bash "$0" "$@"
# Goal: compiler2-purism — surface-rule burndown (docs/evident-purism.md).
#
# Trend (§3.6/V5): hand-namespacing of fsm/module internals. The
# COMBINED count of two decl-position symptoms over compiler2/*.ev
# (comments + string literals stripped first):
#
#   (a) component prefixes — for each `fsm DriverXyz`, declaration-
#       position names inside its body that begin with one of the fsm's
#       own CamelCase component words, or their concatenation, plus "_"
#       (DriverGroup -> group_*, DriverSetVar -> set_*/var_*/setvar_*).
#       The prefixes are DERIVED from the fsm's own name — no hand-kept
#       list to game.
#
#   (b) abbreviation denylist — decl-position names beginning with a
#       fixed list of known-debt ABBREVIATION prefixes that don't match
#       a component word but are namespacing debt all the same
#       (operator design decision, 2026-06-10):
#         mp_ (matchpin)  sv_ (setvar)   il_ (inline)
#         rv_ rd_ rc_ (record*)          ww_ (work-window)
#         pg_ (group)     d_pe d_m_ d_lk (decompose)
#         vf_ vfc_ (variant-field)       ed_ (enum-decl)
#         stl_ stv_ (set) qset_          bcast_ (broadcast abbr)
#       These are distinct from any auto-derived component prefix (e.g.
#       `bcast_` vs the full-word `broadcast_` that (a) catches), so the
#       two halves never double-count. Counted decl-aware across multi-
#       name decls (`a, b ∈ T` is two decl-position uses).
#
# The fsm/module namespacing its own internals by name or abbreviation
# is the hand-namespacing symptom the goal targets; both retire as
# de-prefixing rename passes land.
#
# Survivor justification (ledger docs/purism-exemptions.md):
#   `V5 <file.ev> <prefix_*> — <reason>`   exempts that decl prefix in that file
# Exempt count is reported in the label so mass-exemption is visible.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
LEDGER="$ROOT/docs/purism-exemptions.md"

# Abbreviation denylist (operator design decision 2026-06-10). Word-
# bounded decl-position uses of these are namespacing debt that the
# component-derived half cannot see (they are abbreviations, not the
# fsm's own component words).
DENY="mp_ sv_ il_ rv_ rd_ rc_ ww_ pg_ d_pe d_m_ d_lk vf_ vfc_ ed_ stl_ stv_ qset_ bcast_"

files=("$ROOT"/compiler2/*.ev)
[ -e "${files[0]}" ] || { echo "no compiler2/*.ev found" >&2; exit 1; }

exfile="$(mktemp -t gp-purism-ex.XXXXXX)"
trap 'rm -f "$exfile"' EXIT
if [ -f "$LEDGER" ]; then
    # lines: "<class> <file> <token>"
    grep -ohE '^[-*[:blank:]]*V5[[:blank:]]+[^[:blank:]]+[[:blank:]]+[^[:blank:]]+' "$LEDGER" \
        | sed -E 's/^[-*[:blank:]]+//' > "$exfile" || true
fi

# Strip comments + string literals from every file into a temp stream,
# preserving filename markers so the awk can map a hit to its file (for
# per-file V5 ledger exemptions).
strip() { sed 's/--.*//;s/"[^"]*"/""/g' "$1"; }

read -r combined exempted <<<"$(
for f in "${files[@]}"; do
    echo "@@FILE $(basename "$f")"
    strip "$f"
done | awk -v exfile="$exfile" -v deny="$DENY" '
BEGIN {
  while ((getline l < exfile) > 0) {
    split(l, f, /[[:blank:]]+/)
    if (f[1] == "V5") ex[f[2] " " f[3]] = 1   # ex["file prefix*"]
  }
  ndeny = split(deny, DP, /[[:blank:]]+/)
}
# File marker (synthetic) — resets per-file fsm state and records name.
/^@@FILE / { curfile=$2; fsmname=""; next }
# Any non-fsm schema header ends the current fsm body scope.
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
# ── decl line? leading ws + comma-separated name-list + a binding /
# membership / comparison terminator. Splits the name-list so every
# decl-position name in a multi-name decl is examined. ──
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
    matched=0
    # (a) component-prefix half — only inside an fsm body.
    if (fsmname != "") {
      for (i=1; i<=n; i++) {
        if (words[i] == "driver") continue
        p = words[i] "_"
        if (index(name, p) == 1) {
          if (ex[curfile " " p "*"]) exempted++; else cnt++
          matched=1; break
        }
      }
    }
    if (matched) continue
    # (b) abbreviation denylist half — anywhere (debt is not fsm-scoped).
    for (i=1; i<=ndeny; i++) {
      p = DP[i]
      if (index(name, p) == 1) {
        if (ex[curfile " " p "*"]) exempted++; else cnt++
        break
      }
    }
  }
}
END { printf "%d %d\n", cnt+0, exempted+0 }
'
)"

printf '{"goal":"compiler2-purism","measure":"fsm_prefix_decls","kind":"trend","value":%d,"target":0,"higher_is_better":false,"unit":"count","rung":"deterministic","period_s":300,"label":"decl-position names hand-namespaced by an fsm component word OR a known abbreviation prefix (mp_/sv_/vf_/bcast_/…), combined, unexempted (%d exempted via ledger)"}\n' "$combined" "$exempted"
