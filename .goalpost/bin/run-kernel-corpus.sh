#!/usr/bin/env bash
# .goalpost/bin/run-kernel-corpus.sh — drive the FULL kernel fixture
# corpus (tests/kernel/test_*.ev) through compiler2 and write the
# artifact the kernel-corpus measures read.
#
# Per fixture (mirrors scripts/run-kernel-tests.sh semantics, with
# compiler2 as the backend):
#   - expectations come from `-- expect: stdout = "…"` (stacking) and
#     `-- expect: exit = N` header comments; defaults: stdout "", exit 0.
#     Empty expected stdout ⇒ stdout check skipped (exit only).
#   - claim name: file stem minus `test_`, else main / hello / app —
#     first that exists as a top-level claim/fsm/type/schema decl.
#   - compile via kernel+compiler2-stage1 (wave-4o stdin protocol),
#     run the emitted unit under the kernel, compare.
#
# Verdicts: pass | fail | timeout (compile cap recorded in artifact).
#
# Usage: .goalpost/bin/run-kernel-corpus.sh
#   env: EVIDENT_C2_TIMEOUT (s/fixture, default 1800)
#        EVIDENT_C2_JOBS    (parallel workers, default 8)

set -u
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib.sh"

TESTS="$GP_ROOT/tests/kernel"
OUT_JSON="$GP_ART/compiler2-kernel.json"

# ── worker mode: judge ONE fixture ───────────────────────────────────
if [ "${1:-}" = "--one" ]; then
    src="$2"; vdir="$3"
    name="$(basename "$src" .ev)"
    tmp="$(mktemp -d)"; trap 'rm -rf "$tmp"' EXIT
    unit="$tmp/out.smt2"

    # expectations (port of run-kernel-tests.sh parse_expectations)
    exp_stdout=""; exp_exit=0; first=1
    stdout_re='^--[[:space:]]*expect:[[:space:]]*stdout[[:space:]]*=[[:space:]]*(.*)$'
    exit_re='^--[[:space:]]*expect:[[:space:]]*exit[[:space:]]*=[[:space:]]*(-?[0-9]+)'
    while IFS= read -r line || [ -n "$line" ]; do
        if [[ $line =~ $stdout_re ]]; then
            val="${BASH_REMATCH[1]}"
            val="${val%"${val##*[![:space:]]}"}"
            if [[ ${#val} -ge 2 && ${val:0:1} == '"' && ${val: -1} == '"' ]]; then
                val="${val:1:${#val}-2}"
            fi
            if [ "$first" -eq 1 ]; then exp_stdout="$val"; first=0
            else exp_stdout="$exp_stdout"$'\n'"$val"; fi
        elif [[ $line =~ $exit_re ]]; then
            exp_exit="${BASH_REMATCH[1]}"
        fi
    done < "$src"

    # claim guess (port of run-kernel-tests.sh guess_claim_name)
    claim=""
    for c in "${name#test_}" main hello app; do
        if grep -qE "^[[:space:]]*(claim|fsm|type|schema)[[:space:]]+${c}([^a-zA-Z0-9_]|\$)" "$src"; then
            claim="$c"; break
        fi
    done

    verdict=fail; why=""
    if [ -z "$claim" ]; then
        why="no entry claim found"
    else
        gp_c2_compile "$GP_STAGE1" "$src" "$claim" "$unit" "$GP_C2_TIMEOUT"
        rc=$?
        if [ "$rc" -eq 124 ]; then
            verdict=timeout; why="compile exceeded ${GP_C2_TIMEOUT}s"
        elif [ "$rc" -ne 0 ]; then
            why="compile error"
        else
            ec="$(gp_run_unit "$unit" "$tmp/stdout")"
            got="$(printf '%s' "$(cat "$tmp/stdout")")"
            verdict=pass
            if [ -n "$exp_stdout" ] && [ "$got" != "$exp_stdout" ]; then
                verdict=fail; why="stdout mismatch"
            fi
            if [ "$verdict" = pass ] && [ "$ec" != "$exp_exit" ]; then
                verdict=fail; why="exit: want=$exp_exit got=$ec"
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

ls "$TESTS"/test_*.ev | xargs -P "$GP_JOBS" -I {} "${BASH_SOURCE[0]}" --one {} "$VDIR"

total=0; passed=0; failed=0; timeouts=0
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
    '{ts:$ts, corpus:"kernel", total:$total, passed:$passed,
      failed:$failed, timeouts:$timeouts, per_fixture_timeout_s:$cap,
      wall_s:($ts-$started), stage1_builder:$builder, failures:$failures}' \
    > "$OUT_JSON"

echo "wrote $OUT_JSON  ($passed/$total passed, $failed failed, $timeouts timed out)"
