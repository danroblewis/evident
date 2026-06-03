#!/usr/bin/env bash
# TODO: rewrite in Evident
#
# check.sh — authoritative self-vs-bootstrap byte-diff for feature 200.
#
# Runs scripts/diff-vs-bootstrap.sh over the fixtures in fixtures/ and
# aggregates the result. This is NOT auto-run by runner.sh/test.sh (the
# conformance runner only globs source.ev), so it never affects the suite
# exit code — run it by hand, or wire it into the cutover gate once
# compiler.smt2 exists. See this directory's README.md.
#
# Exit codes:
#   0  every fixture MATCHED, or every fixture SKIPPED (compiler.smt2 not
#      built yet — the committed pre-cutover state)
#   1  at least one fixture DIFFERed or errored

set -u -o pipefail

DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$DIR/../../../.." && pwd)"
DIFF="$ROOT/scripts/diff-vs-bootstrap.sh"
COMPILER_SMT2="$ROOT/compiler.smt2"

[ -x "$DIFF" ] || { echo "check.sh: missing $DIFF" >&2; exit 1; }

if [ ! -f "$COMPILER_SMT2" ]; then
    echo "check.sh: BLOCKED — compiler.smt2 not built yet; diff is SKIPPED for all fixtures."
    echo "  (build it with scripts/build-compiler-smt2.sh once compiler.ev is feature-complete.)"
    exit 0
fi

fail=0
for f in "$DIR"/fixtures/*.ev; do
    if "$DIFF" "$f" main; then
        :
    else
        fail=1
    fi
done

if [ "$fail" -eq 0 ]; then
    echo "check.sh: all fixtures equivalent (self-hosted ≡ bootstrap)."
    exit 0
else
    echo "check.sh: at least one fixture DIFFERed — self-hosted compiler not yet feature-complete." >&2
    exit 1
fi
