#!/usr/bin/env bash
# Self-enforced budget: hard-cap this script at 55s regardless of runner.
[ -z "${GP_TIMEBOXED:-}" ] && exec env GP_TIMEBOXED=1 timeout 55 bash "$0" "$@"
# Goal: compiler2-purism — surface-rule burndown (docs/evident-purism.md).
#
# Measure 1 (trend, V18): numbered-variable families. A "family" is a
# stem that appears with >=3 distinct trailing-digit identifiers
# (dec_tok0..dec_tok7, lat_tag0..7, bind_n0..n5) in non-comment text of
# compiler2/*.ev — N scalars emulating one bounded Seq. Threshold 3
# avoids false positives on incidental pairs while catching every
# family the critic baseline lists. Comments are stripped first so a
# MODULE header *mentioning* a family doesn't count as using it.
#
# Measure 2 (gate, V18 named class): references to the hand-peeled
# cons-list bind families bind_n0..n5 / bind_h0..h5 / bind_tail0..4
# (driver_compose.ev and consumers) — the goal names this class
# explicitly; a Seq of a record type belongs there.
#
# Survivor justification: the goal allows survivors that carry a
# documented justification. The mechanical form is a ledger line in
# docs/purism-exemptions.md:  `V18 <file.ev|*> <stem> — <reason>`.
# A V18 ledger line exempts its stem (family unit spans files, so the
# file field is audit metadata; matching is class+stem). Exempt counts
# are reported in the label so mass-exemption is visible, not silent.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
LEDGER="$ROOT/docs/purism-exemptions.md"

files=("$ROOT"/compiler2/*.ev)
[ -e "${files[0]}" ] || { echo "no compiler2/*.ev found" >&2; exit 1; }

# Exempted V18 stems from the ledger (class V18, third field = stem).
exempt_stems=""
if [ -f "$LEDGER" ]; then
    exempt_stems=$(grep -ohE '^[-*[:blank:]]*V18[[:blank:]]+[^[:blank:]]+[[:blank:]]+[^[:blank:]]+' "$LEDGER" \
        | awk '{print $NF}' | sort -u || true)
fi
is_exempt() { [ -n "$exempt_stems" ] && grep -qxF "$1" <<<"$exempt_stems"; }

# Family stems: comment-stripped, identifiers with trailing digits,
# stem must end in a letter/underscore and be >=2 chars (excludes
# expression-scoped shorts like c1 and mid-digit names like smt2_x).
stems=$( (sed 's/--.*//' "${files[@]}" \
    | grep -ohE '\b[a-z_][a-z_0-9]*[a-z_][0-9]+\b' || true) | sort -u \
    | sed -E 's/[0-9]+$//' | sort | uniq -c | awk '$1>=3 {print $2}')

total=0; exempted=0
while IFS= read -r s; do
    [ -n "$s" ] || continue
    if is_exempt "$s"; then exempted=$((exempted+1)); else total=$((total+1)); fi
done <<<"$stems"

# Bind-peel refs (occurrences, not lines), comment-stripped; stems
# bind_n / bind_h / bind_tail are individually exemptable via V18 lines.
peel=0; peel_exempt=0
for stem in bind_n bind_h bind_tail; do
    n=$( (sed 's/--.*//' "${files[@]}" | grep -oE "\b${stem}[0-9]+\b" || true) | wc -l | tr -d ' ')
    if is_exempt "$stem"; then peel_exempt=$((peel_exempt+n)); else peel=$((peel+n)); fi
done

printf '{"goal":"compiler2-purism","measure":"v18_numbered_families","kind":"trend","value":%d,"target":0,"higher_is_better":false,"unit":"count","rung":"deterministic","period_s":300,"label":"numbered-variable family stems (>=3 digit-suffixed ids) in compiler2/*.ev, unexempted (%d exempted via ledger)"}\n' "$total" "$exempted"
printf '{"goal":"compiler2-purism","measure":"v18_bind_peel_refs","kind":"gate","value":%d,"target":0,"higher_is_better":false,"unit":"count","rung":"deterministic","period_s":300,"label":"refs to hand-peeled cons-list bind_n*/bind_h*/bind_tail* families, unexempted (%d exempted)"}\n' "$peel" "$peel_exempt"
