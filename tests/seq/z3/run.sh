#!/usr/bin/env bash
# TODO: rewrite in Evident
#
# tests/seq/z3/run.sh — Z3 ground-truth verifier for the bounded-Seq
# construction catalog. Each *.smt2 here encodes one construction the
# BOUNDED way (uninterpreted Array Int->Int + a static length / bounded
# quantifier) and asserts a positive case (sat) and a negative case
# (unsat). The header `;; expect: <tok> <tok> ...` lists the expected
# (check-sat) results in order. A construction whose bounded encoding
# returns `unknown` (semi-decidable blowup) would FAIL here — the point
# of the suite is to show none do.
#
# Usage: tests/seq/z3/run.sh [file.smt2 ...]   (default: all)
# Exit 0 iff every script's check-sat sequence matches its header AND
# no result is `unknown`/`timeout`.

set -u -o pipefail
DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
Z3="${Z3:-/usr/local/bin/z3}"
[ -x "$Z3" ] || { echo "run.sh: z3 not executable: $Z3" >&2; exit 2; }

if [ "$#" -gt 0 ]; then FILES=("$@"); else FILES=("$DIR"/*.smt2); fi

pass=0; fail=0
for f in "${FILES[@]}"; do
    [ -f "$f" ] || continue
    name="$(basename "$f")"
    expect="$(sed -n 's/^;; expect:[[:space:]]*//p' "$f" | head -1)"
    if [ -z "$expect" ]; then echo "SKIP $name — no expect header"; continue; fi
    actual="$("$Z3" -T:30 "$f" 2>/dev/null | tr '\n' ' ' | sed 's/[[:space:]]\+/ /g;s/ $//')"
    exp_norm="$(echo "$expect" | sed 's/[[:space:]]\+/ /g;s/ $//')"
    if echo "$actual" | grep -qiE 'unknown|timeout'; then
        echo "FAIL $name — NON-DECIDABLE result: [$actual]"; fail=$((fail+1)); continue
    fi
    if [ "$actual" = "$exp_norm" ]; then
        echo "PASS $name — [$actual]"; pass=$((pass+1))
    else
        echo "FAIL $name — expected [$exp_norm] got [$actual]"; fail=$((fail+1))
    fi
done
echo "---"
echo "seq/z3: $pass passed, $fail failed"
[ "$fail" -eq 0 ]
