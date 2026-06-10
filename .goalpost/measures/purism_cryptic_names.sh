#!/usr/bin/env bash
# Self-enforced budget: hard-cap this script at 55s regardless of runner.
[ -z "${GP_TIMEBOXED:-}" ] && exec env GP_TIMEBOXED=1 timeout 55 bash "$0" "$@"
# Goal: compiler2-purism — surface-rule burndown (docs/evident-purism.md).
#
# Trend (§3.6): cryptic 1–3-char variable names that should become real
# words. The goal names the denylist: st, ty, nat. Counts word-bounded
# occurrences in compiler2/*.ev with comments and string literals
# stripped (so prose mentions and wire-format strings don't count).
# Expression-scoped binders (e, v, k, x) are NOT on the denylist — the
# purism doc blesses them.
#
# Survivor justification (ledger docs/purism-exemptions.md):
#   `naming <file.ev|*> <name> — <reason>`
# A `*` file field exempts the name repo-wide in compiler2/ (e.g. the
# documented `ty`-because-`type`-is-reserved field name, if the
# operator rules it stays). Exempt counts are reported in the label.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
LEDGER="$ROOT/docs/purism-exemptions.md"
DENY="st ty nat"

files=("$ROOT"/compiler2/*.ev)
[ -e "${files[0]}" ] || { echo "no compiler2/*.ev found" >&2; exit 1; }

exlist=""
if [ -f "$LEDGER" ]; then
    exlist=$(grep -ohE '^[-*[:blank:]]*naming[[:blank:]]+[^[:blank:]]+[[:blank:]]+[^[:blank:]]+' "$LEDGER" \
        | sed -E 's/^[-*[:blank:]]+//' || true)
fi

total=0; exempted=0
for name in $DENY; do
    for f in "${files[@]}"; do
        n=$( (sed 's/--.*//;s/"[^"]*"/""/g' "$f" | grep -ohE "\b${name}\b" || true) | wc -l | tr -d ' ')
        [ "$n" -gt 0 ] || continue
        b=$(basename "$f")
        if grep -qE "^naming[[:blank:]]+(\*|${b})[[:blank:]]+${name}\$" <<<"$exlist"; then
            exempted=$((exempted+n))
        else
            total=$((total+n))
        fi
    done
done

printf '{"goal":"compiler2-purism","measure":"cryptic_name_refs","kind":"trend","value":%d,"target":0,"higher_is_better":false,"unit":"count","rung":"deterministic","period_s":300,"label":"word-bounded uses of denylisted cryptic names (st, ty, nat) in compiler2/*.ev, unexempted (%d exempted via ledger)"}\n' "$total" "$exempted"
