#!/usr/bin/env bash
# .goalpost/bin/run-sample.sh — goal clause (3): compiler2 correctly
# compiles the legacy sat-check driver source, compiler/sample.ev.
#
# "Correctly" is defined behaviourally, against the committed
# sample.smt2 artifact (the known-good compilation of the same source):
#
#   1. compile compiler/sample.ev (claim main) via kernel+compiler2.
#   2. for each reference input, run BOTH the committed sample.smt2 and
#      the compiler2-built candidate through the kernel (stdin line 1 =
#      flattened input path — sample wire protocol), pipe each emitted
#      check-sat program through `z3 -in`, and compare the
#      (claim-name, sat/unsat verdict) sequences.
#   3. equivalent on every reference input ⇒ sample_ev_equiv.
#
# Writes .goalpost/artifacts/compiler2-sample.json.
#
# env: EVIDENT_C2_SAMPLE_TIMEOUT  compile cap, s (default 14400)
#      EVIDENT_C2_RUN_TIMEOUT     per sample-driver run, s (default 240
#                                 in lib.sh; raised to 1800 here)

set -u
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib.sh"

GP_RUN_TIMEOUT="${EVIDENT_C2_RUN_TIMEOUT:-1800}"
SAMPLE_TIMEOUT="${EVIDENT_C2_SAMPLE_TIMEOUT:-14400}"
OUT_JSON="$GP_ART/compiler2-sample.json"
REF_SAMPLE="$GP_ROOT/sample.smt2"
# Small, stable reference inputs with multiple sat/unsat claims each.
REF_INPUTS=(
    "$GP_ROOT/tests/lang_tests/test_enums_basic.ev"
    "$GP_ROOT/tests/lang_tests/test_matches.ev"
)

gp_require_tools
command -v z3 >/dev/null || gp_die "z3 not on PATH"
[ -f "$REF_SAMPLE" ] || gp_die "committed sample.smt2 missing (behavioural reference)"
gp_build_stage1

TMP="$(mktemp -d)"; trap 'rm -rf "$TMP"' EXIT
started="$(gp_now)"
CAND="$TMP/sample-candidate.smt2"

compiled=false; timedout=false; equiv=false; details=()

gp_c2_compile "$GP_STAGE1" "$GP_ROOT/compiler/sample.ev" main "$CAND" "$SAMPLE_TIMEOUT"
rc=$?
if [ "$rc" -eq 124 ]; then
    timedout=true; details+=("compile exceeded ${SAMPLE_TIMEOUT}s")
elif [ "$rc" -ne 0 ]; then
    details+=("compile error")
elif [ "$(wc -l < "$CAND")" -lt 100 ]; then
    # the no-such-claim stub is structurally valid but ~12 lines
    details+=("candidate is a stub ($(wc -l < "$CAND") lines)")
else
    compiled=true
fi

# verdicts <driver.smt2> <input.ev>  →  "name=sat" lines (claim order)
verdicts() {
    local driver="$1" input="$2" flat="$TMP/in.ev" prog="$TMP/prog.smt2"
    "$GP_FLATTEN" "$input" > "$flat" || return 1
    printf '%s\n' "$flat" \
        | timeout "$GP_RUN_TIMEOUT" "$GP_KERNEL" "$driver" 2>/dev/null \
        | grep -v '^\[functionizer\]' > "$prog"
    [ "${PIPESTATUS[1]}" -eq 0 ] || return 1
    [ -s "$prog" ] || return 1
    local names verds
    names="$(grep '^;; claim: ' "$prog" | sed 's/^;; claim: //')"
    [ -n "$names" ] || return 1
    verds="$(z3 -in < "$prog" 2>/dev/null | grep -E '^(sat|unsat|unknown)$')"
    paste -d= <(printf '%s\n' "$names") <(printf '%s\n' "$verds")
}

if [ "$compiled" = true ]; then
    equiv=true
    for input in "${REF_INPUTS[@]}"; do
        ref="$(verdicts "$REF_SAMPLE" "$input")" \
            || { equiv=false; details+=("reference sample.smt2 failed on $(basename "$input")"); continue; }
        cand="$(verdicts "$CAND" "$input")" \
            || { equiv=false; details+=("candidate failed on $(basename "$input")"); continue; }
        if [ "$ref" != "$cand" ]; then
            equiv=false; details+=("verdict divergence on $(basename "$input")")
        fi
    done
fi

jq -n \
    --argjson ts "$(gp_now)" \
    --argjson compiled "$compiled" \
    --argjson timedout "$timedout" \
    --argjson equiv "$equiv" \
    --argjson cap "$SAMPLE_TIMEOUT" \
    --arg builder "$(gp_stage1_builder)" \
    --argjson started "$started" \
    --argjson details "$(printf '%s\n' "${details[@]:-}" | grep -v '^$' | jq -R . | jq -s .)" \
    '{ts:$ts, compiled:$compiled, compile_timeout:$timedout, equiv:$equiv,
      compile_timeout_s:$cap, wall_s:($ts-$started),
      stage1_builder:$builder, details:$details}' \
    > "$OUT_JSON"

echo "wrote $OUT_JSON  (compiled=$compiled equiv=$equiv)"
