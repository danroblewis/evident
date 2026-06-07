#!/usr/bin/env bash
# TODO: rewrite in Evident
#
# tests/conformance/features/runner.sh
# ─────────────────────────────────────
# Implementation-agnostic conformance runner.
#
# Each feature directory defines a language capability as an
# input/output spec (see README.md). This runner compiles each spec
# with a *swappable backend* and checks the result, so the same
# feature can be verified against the bootstrap compiler today and the
# self-hosted compiler (kernel + compiler.smt2) once it exists.
#
# Backends (select via the IMPL env var):
#   IMPL=bootstrap  (default) — `scripts/evident-self bin` emit (bootstrap today)
#   IMPL=selfhost             — kernel compiler.smt2 < source.ev
#   IMPL=both                 — compile under both, compare outputs
#
# For each feature:
#   1. compile source.ev → out.smt2
#   2. assert every line of expected/smt2-contains is a substring of out.smt2
#   3. if expected/stdout or expected/exit exists, run out.smt2 via the
#      kernel and assert stdout / exit code match.
#
# Reports "N passed / M failed / K blocked" and exits 0 only when every
# feature passed (blocked counts as not-passed for the exit code under
# selfhost/both, but does NOT fail the bootstrap default — see below).
#
# Usage:
#   tests/conformance/features/runner.sh                 # IMPL=bootstrap
#   IMPL=selfhost tests/conformance/features/runner.sh
#   IMPL=both     tests/conformance/features/runner.sh

set -u

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
cd "$ROOT"

IMPL="${IMPL:-bootstrap}"
EVIDENT="$("$ROOT/scripts/evident-self" bin)"
KERNEL="$ROOT/kernel/target/release/kernel"
COMPILER_SMT2="$ROOT/compiler.smt2"

if [ -t 1 ]; then
    GREEN=$(printf '\033[0;32m'); RED=$(printf '\033[0;31m')
    YEL=$(printf '\033[0;33m');   OFF=$(printf '\033[0m')
else
    GREEN=''; RED=''; YEL=''; OFF=''
fi

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

passed=0; failed=0; blocked=0
declare -a fail_names=() block_names=()

# normalize: strip trailing newlines from a file's content
slurp() { # path
    [ -f "$1" ] || { printf ''; return; }
    # shellcheck disable=SC2005
    printf '%s' "$(cat "$1")"
}

# compile_bootstrap SRC CLAIM OUT  → 0 ok, 1 compile error (msg in $TMP/err)
compile_bootstrap() {
    "$EVIDENT" emit "$1" "$2" -o "$3" 2>"$TMP/err"
}

# compile_selfhost SRC CLAIM OUT  → 0 ok, 1 compile error, 2 blocked (no compiler.smt2)
# Wave-4o protocol: stdin line 1 = FLATTENED source path, line 2 = claim.
# (The previous version piped raw un-flattened source text as stdin — the
# pre-wave-4o protocol — so the FSM read `import "stdlib/kernel.ev"` as a
# file path and every fixture emitted an empty stub.)
compile_selfhost() {
    [ -f "$COMPILER_SMT2" ] || return 2
    local flat="$TMP/flat.ev"
    "$ROOT/scripts/flatten-evident.sh" "$1" >"$flat" 2>"$TMP/err" || return 1
    printf '%s\n%s\n' "$flat" "$2" | "$KERNEL" "$COMPILER_SMT2" >"$3" 2>"$TMP/err"
}

# check_smt2_contains OUT EXPECTED_FILE  → 0 ok, 1 a line missing (msg in $TMP/why)
check_smt2_contains() {
    local out="$1" exp="$2" line
    [ -f "$exp" ] || return 0
    while IFS= read -r line || [ -n "$line" ]; do
        [ -z "$line" ] && continue
        if ! grep -Fq -- "$line" "$out"; then
            printf 'smt2 missing: %s' "$line" >"$TMP/why"
            return 1
        fi
    done <"$exp"
    return 0
}

# run_kernel OUT  → writes stdout to $TMP/out, exit code to $TMP/ec
run_kernel() {
    local out="$1"
    "$KERNEL" "$out" >"$TMP/out" 2>"$TMP/kerr"
    printf '%s' "$?" >"$TMP/ec"
}

# check_run FEATURE_DIR OUT  → 0 ok, 1 mismatch (msg in $TMP/why)
# Runs only when expected/stdout or expected/exit is present.
check_run() {
    local dir="$1" out="$2"
    local has_stdout=0 has_exit=0
    [ -f "$dir/expected/stdout" ] && has_stdout=1
    [ -f "$dir/expected/exit" ]   && has_exit=1
    [ "$has_stdout" -eq 0 ] && [ "$has_exit" -eq 0 ] && return 0

    run_kernel "$out"
    local actual_out actual_ec
    actual_out="$(printf '%s' "$(cat "$TMP/out")")"   # strip trailing \n
    actual_ec="$(cat "$TMP/ec")"

    if [ "$has_stdout" -eq 1 ]; then
        local want; want="$(slurp "$dir/expected/stdout")"
        if [ "$actual_out" != "$want" ]; then
            printf 'stdout: want=%q got=%q' "$want" "$actual_out" >"$TMP/why"
            return 1
        fi
    fi
    local want_ec=0
    [ "$has_exit" -eq 1 ] && want_ec="$(slurp "$dir/expected/exit")"
    if [ "$actual_ec" != "$want_ec" ]; then
        printf 'exit: want=%s got=%s (stderr: %s)' \
            "$want_ec" "$actual_ec" "$(head -1 "$TMP/kerr")" >"$TMP/why"
        return 1
    fi
    return 0
}

# verify_one_impl FEATURE_DIR CLAIM IMPL_KIND OUT_PATH
#   → 0 pass, 1 fail (msg in $TMP/why), 2 blocked (msg in $TMP/why)
verify_one_impl() {
    local dir="$1" claim="$2" kind="$3" out="$4"
    if [ "$kind" = "selfhost" ]; then
        compile_selfhost "$dir/source.ev" "$claim" "$out"
        local rc=$?
        if [ "$rc" -eq 2 ]; then
            printf 'no compiler.smt2 at repo root' >"$TMP/why"; return 2
        elif [ "$rc" -ne 0 ]; then
            printf 'selfhost compile error: %s' "$(head -1 "$TMP/err")" >"$TMP/why"; return 1
        fi
    else
        if ! compile_bootstrap "$dir/source.ev" "$claim" "$out"; then
            printf 'bootstrap emit error: %s' "$(head -1 "$TMP/err")" >"$TMP/why"; return 1
        fi
    fi
    check_smt2_contains "$out" "$dir/expected/smt2-contains" || return 1
    check_run "$dir" "$out" || return 1
    return 0
}

run_feature() {
    local dir="$1"
    local name; name="$(basename "$dir")"
    local claim="main"
    [ -f "$dir/claim.txt" ] && claim="$(slurp "$dir/claim.txt")"

    case "$IMPL" in
    bootstrap)
        if verify_one_impl "$dir" "$claim" bootstrap "$TMP/bs.smt2"; then
            echo "${GREEN}✓${OFF} $name"; passed=$((passed+1))
        else
            echo "${RED}✗${OFF} $name — $(cat "$TMP/why")"; failed=$((failed+1)); fail_names+=("$name")
        fi
        ;;
    selfhost)
        verify_one_impl "$dir" "$claim" selfhost "$TMP/sh.smt2"; local rc=$?
        if [ "$rc" -eq 0 ]; then
            echo "${GREEN}✓${OFF} $name"; passed=$((passed+1))
        elif [ "$rc" -eq 2 ]; then
            echo "${YEL}∅${OFF} $name — BLOCKED: $(cat "$TMP/why")"; blocked=$((blocked+1)); block_names+=("$name")
        else
            echo "${RED}✗${OFF} $name — $(cat "$TMP/why")"; failed=$((failed+1)); fail_names+=("$name")
        fi
        ;;
    both)
        # bootstrap leg
        if ! verify_one_impl "$dir" "$claim" bootstrap "$TMP/bs.smt2"; then
            echo "${RED}✗${OFF} $name — bootstrap: $(cat "$TMP/why")"; failed=$((failed+1)); fail_names+=("$name"); return
        fi
        run_kernel "$TMP/bs.smt2"; local bs_out bs_ec
        bs_out="$(printf '%s' "$(cat "$TMP/out")")"; bs_ec="$(cat "$TMP/ec")"
        # selfhost leg
        verify_one_impl "$dir" "$claim" selfhost "$TMP/sh.smt2"; local rc=$?
        if [ "$rc" -eq 2 ]; then
            echo "${YEL}∅${OFF} $name — BLOCKED: $(cat "$TMP/why")"; blocked=$((blocked+1)); block_names+=("$name"); return
        elif [ "$rc" -ne 0 ]; then
            echo "${RED}✗${OFF} $name — selfhost: $(cat "$TMP/why")"; failed=$((failed+1)); fail_names+=("$name"); return
        fi
        run_kernel "$TMP/sh.smt2"; local sh_out sh_ec
        sh_out="$(printf '%s' "$(cat "$TMP/out")")"; sh_ec="$(cat "$TMP/ec")"
        if [ "$bs_out" != "$sh_out" ] || [ "$bs_ec" != "$sh_ec" ]; then
            echo "${RED}✗${OFF} $name — divergence: bootstrap=(out=$bs_out exit=$bs_ec) selfhost=(out=$sh_out exit=$sh_ec)"
            failed=$((failed+1)); fail_names+=("$name")
        else
            echo "${GREEN}✓${OFF} $name — equivalent (out=$bs_out exit=$bs_ec)"; passed=$((passed+1))
        fi
        ;;
    *)
        echo "runner.sh: unknown IMPL=$IMPL (use bootstrap|selfhost|both)" >&2; exit 2
        ;;
    esac
}

[ -x "$EVIDENT" ] || { [ "$IMPL" = "selfhost" ] || { echo "runner.sh: bootstrap binary missing at $EVIDENT (run ./test.sh phase 1 or cargo build --release)" >&2; exit 2; }; }
[ -x "$KERNEL" ]  || { echo "runner.sh: kernel binary missing at $KERNEL" >&2; exit 2; }

echo "── conformance features (IMPL=$IMPL) ──"
for dir in "$SCRIPT_DIR"/[0-9]*/; do
    [ -f "$dir/source.ev" ] || continue
    run_feature "${dir%/}"
done

total=$((passed+failed+blocked))
echo
echo "${passed} passed / ${failed} failed / ${blocked} blocked  (of ${total})"

# Self-hosted toolchain has documented compiler gaps that block certain
# conformance shapes (arithmetic-in-ctor-args, match-RHS, etc. — see
# STATE.md). Allow these specific feature directories to fail under
# IMPL=selfhost while still verifying the rest. The bootstrap path
# (default) ignores the allowlist.
if [ "$IMPL" = "selfhost" ] || [ "$IMPL" = "both" ]; then
    # One feature-dir name per line. Update as compiler gaps close.
    DEFAULT_KNOWN_FAILS="$(cat <<'EOF'
001-int-arithmetic-add
003-int-multiply-to-string
004-comparison-ternary
007-int-subtraction
017-nat-membership
018-int-membership-negative
020-bool-membership
021-real-membership
022-inequality
023-less-than
024-greater-than
025-lte-gte
026-arithmetic-add
027-arithmetic-unsat
028-chained-comparison
029-chained-comparison-unsat
EOF
)"
    KNOWN_FAILS="${EVIDENT_CONFORMANCE_KNOWN_FAILS-$DEFAULT_KNOWN_FAILS}"
    unexpected=()
    expected_count=0
    for name in "${fail_names[@]:-}"; do
        [ -z "$name" ] && continue
        if printf '%s\n' "$KNOWN_FAILS" | grep -qFx "$name"; then
            expected_count=$((expected_count + 1))
        else
            unexpected+=("$name")
        fi
    done
    if [ "${#unexpected[@]}" -gt 0 ]; then
        echo "FAILED (unexpected): ${unexpected[*]}" >&2
        echo "(${expected_count} known-fails matched and excused)" >&2
        exit 1
    fi
    if [ "$expected_count" -gt 0 ]; then
        echo "(${expected_count} known-fails excused — see STATE.md)"
    fi
    [ "$blocked" -eq 0 ] || { echo "BLOCKED: ${block_names[*]}" >&2; exit 1; }
    exit 0
fi

[ "$failed" -eq 0 ] || { echo "FAILED: ${fail_names[*]}" >&2; exit 1; }
# Exit 0 only when every feature passed; blocked features (e.g. selfhost
# before compiler.smt2 exists) mean the run is incomplete, not green.
[ "$blocked" -eq 0 ] || { echo "BLOCKED: ${block_names[*]}" >&2; exit 1; }
exit 0
