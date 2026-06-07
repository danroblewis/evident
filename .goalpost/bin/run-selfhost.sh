#!/usr/bin/env bash
# .goalpost/bin/run-selfhost.sh — goal clause (4): compiler2 compiles
# ITS OWN SOURCE (compiler2/driver.ev + imports) into a WORKING
# compiler artifact.
#
#   1. stage1 = compiler2 artifact built by whatever builder exists
#      (oracle today; itself later).
#   2. stage2 = kernel+stage1 compiling flattened compiler2/driver.ev,
#      claim driver_main. "Built" requires a manifest header AND a
#      non-stub body (the no-such-claim stub is ~12 lines and exits 0 —
#      a documented trap; size alone is checked here, behaviour below).
#   3. stage2 must WORK AS A COMPILER: kernel+stage2 compiles two smoke
#      fixtures and the emitted units run correctly under the kernel:
#        - tests/kernel/test_hello.ev (claim hello) → "hello world", exit 0
#        - tests/conformance/features/001-int-arithmetic-add (claim main)
#          → expected/smt2-contains + exit 7
#
# When stage2_built && stage2_smoke hold, the bootstrap oracle and the
# legacy compiler/ tree are no longer load-bearing for building
# compiler2 — the goal's deletability claim.
#
# Writes .goalpost/artifacts/compiler2-selfhost.json.
#
# env: EVIDENT_C2_SELF_TIMEOUT   stage2 compile cap, s (default 28800)
#      EVIDENT_C2_TIMEOUT        smoke compile cap, s (default 1800)

set -u
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib.sh"

SELF_TIMEOUT="${EVIDENT_C2_SELF_TIMEOUT:-28800}"
OUT_JSON="$GP_ART/compiler2-selfhost.json"

gp_require_tools
gp_build_stage1

TMP="$(mktemp -d)"; trap 'rm -rf "$TMP"' EXIT
started="$(gp_now)"
STAGE2="$TMP/stage2.smt2"

built=false; timedout=false; smoke=false; stage2_lines=0; details=()

gp_c2_compile "$GP_STAGE1" "$GP_ROOT/compiler2/driver.ev" driver_main "$STAGE2" "$SELF_TIMEOUT"
rc=$?
if [ "$rc" -eq 124 ]; then
    timedout=true; details+=("self-compile exceeded ${SELF_TIMEOUT}s")
elif [ "$rc" -ne 0 ]; then
    details+=("self-compile error")
else
    stage2_lines="$(wc -l < "$STAGE2")"
    if ! head -1 "$STAGE2" | grep -q '^;; manifest:'; then
        details+=("stage2 missing manifest header")
    elif [ "$stage2_lines" -lt 100 ]; then
        details+=("stage2 is a stub ($stage2_lines lines)")
    else
        built=true
    fi
fi

if [ "$built" = true ]; then
    smoke=true
    # smoke A: hello world
    unit="$TMP/hello.smt2"
    if gp_c2_compile "$STAGE2" "$GP_ROOT/tests/kernel/test_hello.ev" hello "$unit" "$GP_C2_TIMEOUT"; then
        ec="$(gp_run_unit "$unit" "$TMP/hello.out")"
        got="$(printf '%s' "$(cat "$TMP/hello.out")")"
        if [ "$ec" != 0 ] || [ "$got" != "hello world" ]; then
            smoke=false; details+=("stage2 hello unit wrong (exit=$ec stdout=$got)")
        fi
    else
        smoke=false; details+=("stage2 failed to compile test_hello.ev")
    fi
    # smoke B: arithmetic (3 + 4 → exit 7, asserted smt2 shape)
    feat="$GP_ROOT/tests/conformance/features/001-int-arithmetic-add"
    unit="$TMP/arith.smt2"
    if gp_c2_compile "$STAGE2" "$feat/source.ev" main "$unit" "$GP_C2_TIMEOUT"; then
        ok=true
        while IFS= read -r line || [ -n "$line" ]; do
            [ -z "$line" ] && continue
            grep -Fq -- "$line" "$unit" || ok=false
        done < "$feat/expected/smt2-contains"
        ec="$(gp_run_unit "$unit" "$TMP/arith.out")"
        if [ "$ok" != true ] || [ "$ec" != "$(cat "$feat/expected/exit")" ]; then
            smoke=false; details+=("stage2 arith unit wrong (exit=$ec contains_ok=$ok)")
        fi
    else
        smoke=false; details+=("stage2 failed to compile 001-int-arithmetic-add")
    fi
fi

jq -n \
    --argjson ts "$(gp_now)" \
    --argjson built "$built" \
    --argjson timedout "$timedout" \
    --argjson smoke "$smoke" \
    --argjson lines "$stage2_lines" \
    --argjson cap "$SELF_TIMEOUT" \
    --arg builder "$(gp_stage1_builder)" \
    --argjson started "$started" \
    --argjson details "$(printf '%s\n' "${details[@]:-}" | grep -v '^$' | jq -R . | jq -s .)" \
    '{ts:$ts, stage2_built:$built, compile_timeout:$timedout,
      stage2_smoke:$smoke, stage2_lines:$lines, compile_timeout_s:$cap,
      wall_s:($ts-$started), stage1_builder:$builder, details:$details}' \
    > "$OUT_JSON"

echo "wrote $OUT_JSON  (stage2_built=$built stage2_smoke=$smoke)"
