#!/usr/bin/env bash
# TODO: rewrite in Evident
#
# tests/modeldiff/run.sh — correctness arbiter for scripts/model-diff.sh.
#
# Drives the tool over the four fixture-pair classes and asserts:
#   (1) the VERDICT matches the expected classification, AND
#   (2) for every printed WITNESS, that the witness is REAL — re-emit each side
#       independently, pin the witness's interface vars, and confirm the witness
#       satisfies the side it's claimed to be in and VIOLATES the other. This is
#       the no-external-gate-needed self-check: a verdict the tool can't back with
#       a real witness fails here.
#
# Needs EVIDENT_KERNEL (the flatten pass shells out to the kernel) — defaults to
# the repo's release build; set it if you run from a worktree without one built.

set -uo pipefail
cd "$(dirname "$0")/../.."
ROOT="$PWD"

export EVIDENT_KERNEL="${EVIDENT_KERNEL:-$ROOT/kernel/target/release/kernel}"
ORACLE="${EVIDENT_ORACLE:-/usr/local/bin/evident-oracle}"
Z3="${Z3:-z3}"
MD="$ROOT/scripts/model-diff.sh"
DIR="$ROOT/tests/modeldiff"
TMP="$(mktemp -d -t mdrun.XXXXXX)"
trap 'rm -rf "$TMP"' EXIT

PASS=0; FAIL=0
ok()   { echo "  PASS: $*"; PASS=$((PASS+1)); }
bad()  { echo "  FAIL: $*"; FAIL=$((FAIL+1)); }

# Re-emit one side and check whether a pinned interface assignment is SAT.
#   side_sat <file.ev> <claim> <pins-smt>  → "sat" / "unsat"
side_sat() {
  local ev="$1" claim="$2" pins="$3" flat smt
  flat="$TMP/$(basename "$ev").flat.ev"; smt="$TMP/$(basename "$ev").smt2"
  "$ROOT/scripts/flatten-evident.sh" "$ev" > "$flat" 2>/dev/null
  "$ORACLE" emit "$flat" "$claim" -o "$smt" 2>/dev/null
  { grep -vE '^;;' "$smt"; printf '%s\n' "$pins"; echo '(check-sat)'; } > "$TMP/chk.smt2"
  $Z3 -smt2 -T:30 "$TMP/chk.smt2" 2>&1 | head -1
}

# Verify a witness `(= x VAL)` is in `inside.ev` and NOT in `outside.ev`.
verify_witness() {  # <label> <inside.ev> <outside.ev> <claim> <pin-smt>
  local label="$1" inside="$2" outside="$3" claim="$4" pin="$5"
  local si so
  si="$(side_sat "$inside"  "$claim" "$pin")"
  so="$(side_sat "$outside" "$claim" "$pin")"
  if [ "$si" = sat ] && [ "$so" = unsat ]; then
    ok "$label witness real (sat in claimed side, unsat in other): $pin"
  else
    bad "$label witness NOT real (inside=$si outside=$so): $pin"
  fi
}

# Pull the first `(x VAL)` interface binding out of the tool's witness block for
# the named var. Returns an `(assert (= x VAL))` pin, or empty.
pin_from_output() {  # <tool-output-file> <varname>
  local out="$1" var="$2" val
  val="$(grep -oE "\($var (-?[0-9]+|true|false)\)" "$out" | head -1 | sed -E "s/\($var (.*)\)/\1/")"
  [ -n "$val" ] && echo "(assert (= $var $val))"
}

# Runs the tool, checks the verdict, and leaves the captured output at
# $LAST_OUT (a stable path) for the caller's witness extraction. Counters update
# in the main shell (no command substitution).
LAST_OUT=""
run_case() {  # <name> <a.ev> <b.ev> <claim> <mode v1|v2> <expected-verdict-token> <inputs>
  local name="$1" a="$2" b="$3" claim="$4" mode="$5" expect="$6" inputs="$7"
  echo "── $name ──"
  local out="$TMP/$name.out" flag=""
  [ "$mode" = v1 ] && flag="--v1-only"
  [ "$mode" = v2 ] && flag="--v2-only"
  "$MD" "$a" "$b" "$claim" --inputs "$inputs" $flag > "$out" 2>&1
  local rc=$?
  LAST_OUT="$out"
  local got
  if [ "$mode" = v1 ]; then
    got="$(grep -E '^  v1 observational' "$out" | sed -E 's/.*: //')"
  else
    got="$(grep -E '^  v2 directional' "$out" | sed -E 's/.*: //')"
  fi
  if [ "$got" = "$expect" ]; then ok "$name verdict=$got (rc=$rc)"; else bad "$name verdict=$got want=$expect (rc=$rc)"; sed 's/^/    | /' "$out"; fi
}

echo "═══ model-diff fixture suite ═══"
echo "kernel: $EVIDENT_KERNEL"
echo

# ── class 1: equivalent, different predicates ────────────────────────────────
run_case eq_range    "$DIR/eq_range_a.ev"    "$DIR/eq_range_b.ev"    main v1 equiv x
run_case eq_demorgan "$DIR/eq_demorgan_a.ev" "$DIR/eq_demorgan_b.ev" main v1 equiv p,q

# ── class 2: renamed internals ───────────────────────────────────────────────
run_case internals   "$DIR/internals_a.ev"   "$DIR/internals_b.ev"   main v1 equiv x

# ── class 4: functional output ───────────────────────────────────────────────
run_case func        "$DIR/func_a.ev"        "$DIR/func_b.ev"        main v1 equiv x

# ── class 3: deliberately different (directional) + witness re-check ──────────
# A ⊊ B (B relaxes): witness lives in B, not A.
run_case sub_superset "$DIR/diff_base.ev" "$DIR/diff_super.ev" main v2 A_sub_B x
verify_witness "A⊊B (B∖A)" "$DIR/diff_super.ev" "$DIR/diff_base.ev" main "$(pin_from_output "$LAST_OUT" x)"

# A ⊋ B (B tightens): witness lives in A, not B.
run_case sub_subset "$DIR/diff_base.ev" "$DIR/diff_sub.ev" main v2 A_sup_B x
verify_witness "A⊋B (A∖B)" "$DIR/diff_base.ev" "$DIR/diff_sub.ev" main "$(pin_from_output "$LAST_OUT" x)"

# OVERLAP: two witnesses — one per direction. Re-check BOTH.
run_case overlap "$DIR/diff_base.ev" "$DIR/diff_overlap.ev" main v2 overlap x
PIN_AB="$(grep -A8 'in A, not B' "$LAST_OUT" | grep -oE '\(x -?[0-9]+\)' | head -1 | sed -E 's/\(x (.*)\)/(assert (= x \1))/')"
PIN_BA="$(grep -A8 'in B, not A' "$LAST_OUT" | grep -oE '\(x -?[0-9]+\)' | head -1 | sed -E 's/\(x (.*)\)/(assert (= x \1))/')"
verify_witness "OVERLAP (A∖B)" "$DIR/diff_base.ev"    "$DIR/diff_overlap.ev" main "$PIN_AB"
verify_witness "OVERLAP (B∖A)" "$DIR/diff_overlap.ev" "$DIR/diff_base.ev"    main "$PIN_BA"

# ── class 5: ternary → set-theoretic (the canonical refactor shape) ───────────
# A total ternary vs set-theoretic rewrites. Under-coverage (free output) is
# caught by v1 (outputs diverge); over-coverage (empty for some input) by v2.
run_case tern_equiv_v1   "$DIR/ternary_a.ev" "$DIR/ternary_eq_b.ev"    main v1 equiv   sel
run_case tern_equiv_v2   "$DIR/ternary_a.ev" "$DIR/ternary_eq_b.ev"    main v2 equiv   sel
run_case tern_undercover "$DIR/ternary_a.ev" "$DIR/ternary_under_b.ev" main v1 differ  sel
run_case tern_overcover  "$DIR/ternary_a.ev" "$DIR/ternary_over_b.ev"  main v2 A_sup_B sel

echo
echo "═══ $PASS passed, $FAIL failed ═══"
[ "$FAIL" -eq 0 ]
