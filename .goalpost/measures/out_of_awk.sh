#!/usr/bin/env bash
# Self-enforced budget: hard-cap this script at 55s regardless of runner.
[ -z "${GP_TIMEBOXED:-}" ] && exec env GP_TIMEBOXED=1 timeout 55 bash "$0" "$@"
# "Out of awk" clause: self-hosting means the pre-oracle pass pipeline
# (run by scripts/flatten-evident.sh BEFORE the compiler sees the source)
# is expressed in Evident, not shell/awk. Today four transforms still run
# as awk: expand-fsm-autocarry, flatten-body-records, lower-bounded-seq,
# hoist-decls. This measures the LINES of awk/shell still in that pipeline
# — burns to 0 as each pass is ported to compiler2/passes/ (or deleted, as
# the two-pass declaration build retires hoist-decls). Runs in ms; no
# artifact needed. See docs/plans/full-self-host-plan.md (Gate C).
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

# The active pre-oracle pipeline passes (only ones still present count).
PASSES="
scripts/passes/expand-fsm-autocarry.sh
scripts/passes/flatten-body-records.sh
scripts/passes/lower-bounded-seq.sh
scripts/passes/hoist-decls.sh
"

loc=0 present=0
for p in $PASSES; do
    if [ -f "$ROOT/$p" ]; then
        present=$((present+1))
        n="$(grep -vcE '^\s*(#|$)' "$ROOT/$p" 2>/dev/null || echo 0)"
        loc=$((loc + n))
    fi
done

printf '{"goal":"compiler2-selfhost","measure":"awk_pipeline_loc","kind":"trend","value":%s,"target":0,"higher_is_better":false,"unit":"loc","rung":"deterministic","period_s":300,"label":"non-comment lines of awk/shell still in the pre-oracle pass pipeline (0 = passes fully in Evident)"}\n' "$loc"
printf '{"goal":"compiler2-selfhost","measure":"awk_pipeline_passes","kind":"trend","value":%s,"target":0,"higher_is_better":false,"unit":"count","rung":"deterministic","period_s":300,"label":"pre-oracle passes still implemented as awk (expand-fsm-autocarry, flatten-body-records, lower-bounded-seq, hoist-decls)"}\n' "$present"
