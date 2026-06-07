#!/usr/bin/env bash
# .goalpost/bin/run-conformance.sh — drive the FULL conformance corpus
# (tests/conformance/features/[0-9]*) through compiler2 and write the
# machine-readable result artifact the conformance measures read.
#
# Per feature (same checks as the corpus's own runner.sh, but with
# compiler2 as the backend):
#   1. flatten source.ev; compile via kernel+compiler2-stage1 (wave-4o
#      stdin protocol) with the claim from claim.txt (default: main).
#   2. every non-empty line of expected/smt2-contains must be a
#      substring of the emitted unit.
#   3. if expected/stdout or expected/exit exists, run the emitted unit
#      under the kernel and compare stdout / exit code.
#
# Verdicts: pass | fail | timeout. Timeouts are NOT passes; the
# per-fixture compile cap is recorded in the artifact so a reduced-
# budget run cannot masquerade as a full one.
#
# Expensive (minutes per fixture today). Run it from CI / a cron / by
# hand; the measure scripts only parse the artifact.
#
# Usage: .goalpost/bin/run-conformance.sh
#   env: EVIDENT_C2_TIMEOUT (s/fixture, default 1800)
#        EVIDENT_C2_JOBS    (parallel workers, default 8)

set -u
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib.sh"

FEATURES="$GP_ROOT/tests/conformance/features"
OUT_JSON="$GP_ART/compiler2-conformance.json"

slurp() { [ -f "$1" ] && printf '%s' "$(cat "$1")" || printf ''; }

# ── worker mode: judge ONE feature dir, write verdict file ──────────
if [ "${1:-}" = "--one" ]; then
    dir="${2%/}"; vdir="$3"
    name="$(basename "$dir")"
    claim="main"; [ -f "$dir/claim.txt" ] && claim="$(slurp "$dir/claim.txt")"
    tmp="$(mktemp -d)"; trap 'rm -rf "$tmp"' EXIT
    unit="$tmp/out.smt2"

    verdict=fail; why=""
    gp_c2_compile "$GP_STAGE1" "$dir/source.ev" "$claim" "$unit" "$GP_C2_TIMEOUT"
    rc=$?
    if [ "$rc" -eq 124 ]; then
        verdict=timeout; why="compile exceeded ${GP_C2_TIMEOUT}s"
    elif [ "$rc" -ne 0 ]; then
        why="compile error"
    else
        verdict=pass
        # smt2-contains
        if [ -f "$dir/expected/smt2-contains" ]; then
            while IFS= read -r line || [ -n "$line" ]; do
                [ -z "$line" ] && continue
                grep -Fq -- "$line" "$unit" || { verdict=fail; why="smt2 missing: $line"; break; }
            done < "$dir/expected/smt2-contains"
        fi
        # run checks
        if [ "$verdict" = pass ] && { [ -f "$dir/expected/stdout" ] || [ -f "$dir/expected/exit" ]; }; then
            ec="$(gp_run_unit "$unit" "$tmp/stdout")"
            if [ -f "$dir/expected/stdout" ]; then
                want="$(slurp "$dir/expected/stdout")"
                got="$(printf '%s' "$(cat "$tmp/stdout")")"
                [ "$got" = "$want" ] || { verdict=fail; why="stdout mismatch"; }
            fi
            want_ec=0
            [ -f "$dir/expected/exit" ] && want_ec="$(slurp "$dir/expected/exit")"
            if [ "$verdict" = pass ] && [ "$ec" != "$want_ec" ]; then
                verdict=fail; why="exit: want=$want_ec got=$ec"
            fi
        fi
    fi
    printf '%s\t%s\t%s\n' "$name" "$verdict" "$why" > "$vdir/$name"
    echo "[$verdict] $name${why:+ — $why}" >&2
    exit 0
fi

# ── dispatcher ───────────────────────────────────────────────────────
gp_require_tools
gp_build_stage1
export GP_STAGE1 GP_C2_TIMEOUT

VDIR="$(mktemp -d)"; trap 'rm -rf "$VDIR"' EXIT
started="$(gp_now)"

ls -d "$FEATURES"/[0-9]*/ | while read -r d; do
    [ -f "$d/source.ev" ] && printf '%s\n' "${d%/}"
done | xargs -P "$GP_JOBS" -I {} "${BASH_SOURCE[0]}" --one {} "$VDIR"

total=0; passed=0; failed=0; timeouts=0
failures="[]"
for f in "$VDIR"/*; do
    [ -f "$f" ] || continue
    total=$((total+1))
    v="$(cut -f2 "$f")"
    case "$v" in
        pass)    passed=$((passed+1)) ;;
        timeout) timeouts=$((timeouts+1)) ;;
        *)       failed=$((failed+1)) ;;
    esac
done
failures="$(cat "$VDIR"/* | awk -F'\t' '$2!="pass"{print $1": "$2($3!=""?" ("$3")":"")}' | jq -R . | jq -s .)"

jq -n \
    --argjson ts "$(gp_now)" \
    --argjson started "$started" \
    --argjson total "$total" \
    --argjson passed "$passed" \
    --argjson failed "$failed" \
    --argjson timeouts "$timeouts" \
    --argjson cap "$GP_C2_TIMEOUT" \
    --arg builder "$(gp_stage1_builder)" \
    --argjson failures "$failures" \
    '{ts:$ts, corpus:"conformance", total:$total, passed:$passed,
      failed:$failed, timeouts:$timeouts, per_fixture_timeout_s:$cap,
      wall_s:($ts-$started), stage1_builder:$builder, failures:$failures}' \
    > "$OUT_JSON"

echo "wrote $OUT_JSON  ($passed/$total passed, $failed failed, $timeouts timed out)"
