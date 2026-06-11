#!/usr/bin/env bash
# run-experiment.sh <new_commit> [z3_timeout_s]
#
# Translation-validation feasibility probe: prove stage1(<new>~1) ≡ stage1(<new>)
# with ONE Z3 query and TIME it. <new> must be a clean rename/restructure commit
# whose only declare-fun delta is a bijective const rename (phi auto-derived).
#
# Pipeline:
#   1. throwaway worktrees at <new>~1 (OLD) and <new> (NEW)
#   2. gp_build_stage1 recipe: flatten | evident-oracle emit driver_main
#   3. build-phi.sh diffs the two const sets → phi.txt (clean bijection or LOUD)
#   4. build-equiv-query builds the single-tick OUTPUT-equivalence query
#   5. z3 -st, timed (wall + rlimit-count); UNSAT ⇒ equivalent under phi
#
# Honesty: proves SINGLE-TICK output equivalence under phi. NOT full behavioral
# equivalence (that needs the inductive next-state form). See tools/README.md.
set -euo pipefail

NEW="${1:?usage: run-experiment.sh <new_commit> [z3_timeout_s]}"
TMO="${2:-600}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
ORACLE="${EVIDENT_ORACLE:-/usr/local/bin/evident-oracle}"
Z3="${EVIDENT_Z3:-/usr/local/bin/z3}"
BIN="$(dirname "${BASH_SOURCE[0]}")/build-equiv-query/target/release/build-equiv-query"
PHI="$(dirname "${BASH_SOURCE[0]}")/build-phi.sh"

[ -x "$ORACLE" ] || { echo "no oracle at $ORACLE" >&2; exit 2; }
[ -x "$Z3" ]     || { echo "no z3 at $Z3" >&2; exit 2; }
[ -x "$BIN" ]    || { echo "build-equiv-query not built ($BIN); cargo build --release" >&2; exit 2; }

SC="$(mktemp -d -t equiv-exp.XXXXXX)"
OLDWT="$SC/old"; NEWWT="$SC/new"
cleanup() {
  git -C "$ROOT" worktree remove --force "$OLDWT" 2>/dev/null || true
  git -C "$ROOT" worktree remove --force "$NEWWT" 2>/dev/null || true
  rm -rf "$SC"
}
trap cleanup EXIT

echo "# materializing worktrees: OLD=$NEW~1  NEW=$NEW" >&2
git -C "$ROOT" worktree add --detach "$OLDWT" "$NEW~1" >&2
git -C "$ROOT" worktree add --detach "$NEWWT" "$NEW"   >&2

emit_stage1() {  # <worktree> <out.smt2>
  local w="$1" out="$2" f; f="$(mktemp -t s1.XXXX.ev)"
  EVIDENT_KERNEL="$ROOT/kernel/target/release/kernel" \
    "$w/scripts/flatten-evident.sh" "$w/compiler2/driver.ev" > "$f"
  "$ORACLE" emit "$f" driver_main -o "$out"
  rm -f "$f"
}

echo "# emitting stage1_old / stage1_new …" >&2
emit_stage1 "$OLDWT" "$SC/stage1_old.smt2"
emit_stage1 "$NEWWT" "$SC/stage1_new.smt2"

echo "# deriving phi …" >&2
bash "$PHI" "$SC/stage1_old.smt2" "$SC/stage1_new.smt2" > "$SC/phi.txt"
echo "# phi: $(grep -vc '^#' "$SC/phi.txt") mappings" >&2

echo "# building equivalence query …" >&2
"$BIN" "$SC/stage1_old.smt2" "$SC/stage1_new.smt2" "$SC/phi.txt" > "$SC/query.smt2"

echo "# running z3 (-st, T:$TMO) — UNSAT means equivalent …" >&2
S=$(date +%s.%N)
"$Z3" -smt2 -st "$SC/query.smt2" -T:"$TMO" > "$SC/z3.out" 2>&1 || true
E=$(date +%s.%N)

RES=$(grep -m1 -E '^(sat|unsat|unknown|timeout)' "$SC/z3.out" || echo "NO-RESULT")
RLIMIT=$(grep -oE ':rlimit-count[ ]+[0-9]+' "$SC/z3.out" | grep -oE '[0-9]+' || echo "n/a")
MEM=$(grep -oE ':memory[ ]+[0-9.]+' "$SC/z3.out" | grep -oE '[0-9.]+' || echo "n/a")
TIME_ST=$(grep -oE ':time[ ]+[0-9.]+' "$SC/z3.out" | grep -oE '[0-9.]+' || echo "n/a")

echo "=================================================================="
echo "EXPERIMENT: stage1($NEW~1) ≡ stage1($NEW)  [single-tick, under phi]"
awk "BEGIN{printf \"  z3 result : %s\n\", \"$RES\"}"
awk "BEGIN{printf \"  wall      : %.2f s\n\", $E-$S}"
echo "  z3 :time  : $TIME_ST s"
echo "  rlimit    : $RLIMIT"
echo "  memory    : $MEM MB"
echo "  verdict   : $([ "$RES" = unsat ] && echo 'EQUIVALENT (no diverging input under phi)' || echo "NOT proven (see $SC/z3.out — preserved)")"
echo "=================================================================="

# preserve artifacts on non-unsat for inspection
if [ "$RES" != unsat ]; then
  KEEP="/tmp/equiv-exp-keep-$NEW"
  mkdir -p "$KEEP"; cp "$SC"/query.smt2 "$SC"/z3.out "$SC"/phi.txt "$KEEP"/ 2>/dev/null || true
  echo "# artifacts preserved in $KEEP" >&2
  trap 'git -C "$ROOT" worktree remove --force "$OLDWT" 2>/dev/null||true; git -C "$ROOT" worktree remove --force "$NEWWT" 2>/dev/null||true; rm -rf "$SC"' EXIT
fi
