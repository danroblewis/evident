#!/usr/bin/env bash
# Cross-check the seed C runtime (evidentc) against the Rust runtime (the spec /
# oracle) on the fixtures in this directory. Compares sat/unsat verdicts for
# every claim, and exact model values for the forced-model fixtures.
#
# Usage:  runtime-c/tests/crosscheck.sh            (from repo root or anywhere)
# Assumes both binaries are already built:
#   runtime-c/build/evidentc
#   runtime/target/release/evident
set -u

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
EVC="$ROOT/runtime-c/build/evidentc"
RUST="$ROOT/runtime/target/release/evident"
FIX="$HERE/fixtures"

fail=0

if [[ ! -x "$EVC" ]]; then echo "missing $EVC — build runtime-c first"; exit 2; fi
if [[ ! -x "$RUST" ]]; then echo "missing $RUST — build the Rust runtime (cargo build --release)"; exit 2; fi

# Extract "VERDICT name" pairs (SAT/UNSAT only) from an --all dump, normalized.
norm_verdicts() {
    awk '$1=="SAT"||$1=="UNSAT"{print $1, $2}' | sort
}

echo "== verdict cross-check (evidentc --all  vs  evident sample --all) =="
for f in "$FIX"/*.ev; do
    name="$(basename "$f")"
    c_out="$("$EVC" "$f" --all 2>/dev/null | norm_verdicts)"
    r_out="$("$RUST" sample "$f" --all 2>/dev/null | norm_verdicts)"
    if [[ "$c_out" == "$r_out" ]]; then
        n=$(echo "$c_out" | grep -c .)
        echo "  OK   $name  ($n claims agree)"
    else
        echo "  FAIL $name  — verdict mismatch:"
        diff <(echo "$c_out") <(echo "$r_out") | sed 's/^/      /'
        fail=1
    fi
done

# Forced-model checks: (file claim expected-binding). Both runtimes must produce
# the same unique model value.
echo "== forced-model cross-check (exact value parity) =="
check_forced() {
    local file="$1" claim="$2" var="$3" expect="$4"
    local c r
    c="$("$EVC" "$FIX/$file" "$claim" 2>/dev/null | awk -v v="$var" '$1==v{print $3}')"
    r="$("$RUST" sample "$FIX/$file" "$claim" -n 1 2>/dev/null | awk -F= -v v="$var" '$1==v{print $2}')"
    if [[ "$c" == "$expect" && "$r" == "$expect" ]]; then
        echo "  OK   $claim: $var = $expect  (C and Rust agree)"
    else
        echo "  FAIL $claim: $var  expected '$expect'  got C='$c' Rust='$r'"
        fail=1
    fi
}
check_forced forced.ev forced_int      x 7
check_forced forced.ev forced_real_half x 1.5
check_forced forced.ev forced_bool     q true
check_forced forced.ev forced_negative x -5
check_forced forced.ev forced_string   s '"hello"'
# Enums (M4a): nullary, payload ctor, match extraction, matches recognizer.
check_forced enums.ev forced_color             c Green
check_forced enums.ev forced_color_by_elim     c Green
check_forced enums.ev forced_result_ok         r 'Ok(7)'
check_forced enums.ev forced_match_extract     n 42
check_forced enums.ev forced_matches_recognizer b true
# Quantifiers (M4b): finite range unroll forces a unique model.
check_forced quantifiers.ev forced_forall_singleton n 3
check_forced quantifiers.ev forced_forall_block     n 3
# Records (M4c): per-field leaves; field access, pins, literals, comparison lifts.
check_forced records.ev forced_vec_sum            s 7
check_forced records.ev forced_vec_eq             bx 5
check_forced records.ev forced_color_named_pin    rr 110
check_forced records.ev forced_vec_positional_pin sx 100
check_forced records.ev forced_vec_literal_eq     s 33
check_forced records.ev forced_nested_field       s 33

echo
if [[ $fail -eq 0 ]]; then echo "cross-check: PASS"; else echo "cross-check: FAIL"; fi
exit $fail
