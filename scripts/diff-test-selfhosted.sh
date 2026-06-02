#!/usr/bin/env bash
# diff-test-selfhosted.sh — exercise the self-hosted pipeline fixtures.
#
# Phase E2 of docs/plans/completion-roadmap.md. The full E2 vision is
# "byte-for-byte match between Rust and self-hosted output on a corpus."
# This script is the harness skeleton: it runs the three Phase-D
# pipeline-driver fixtures (Evident programs that compose
# lex + parse + translate stages) through the existing Rust bootstrap
# (`evident emit`) + kernel, and reports coverage.
#
# Each fixture is itself a self-hosted pipeline sketch:
#   * test_pipeline_full.ev      — proof-of-concept SMT-LIB emitter
#   * test_pipeline_full_d2.ev   — lex + parse + translate composed
#   * test_pipeline_lex_parse.ev — multi-tick lex + parse FSM
#
# We capture stdout from the kernel run and verify each `-- expect:
# stdout = "…"` line is produced (in order), plus the `-- expect:
# exit = N` exit code is matched.
#
# Future work (full E2): drive the whole tests/kernel/ corpus through
# BOTH the Rust pipeline and the eventual Phase-D/E self-hosted pipeline
# and assert byte-for-byte SMT-LIB equality. That requires the
# self-hosted compiler to actually exist (Phase E1) — for now we
# report the skeleton coverage.
#
# Exit codes:
#   0 — all fixtures composed end-to-end as expected
#   1 — any fixture failed (emit error, exit-code mismatch, or
#       missing expected stdout line)

set -e -o pipefail

here() { cd "$(dirname "$0")/.." && pwd; }
ROOT="$(here)"

EVIDENT="${ROOT}/bootstrap/runtime/target/release/evident"
KERNEL="${ROOT}/kernel/target/release/kernel"

FIXTURES=(
    "tests/kernel/test_pipeline_full.ev"
    "tests/kernel/test_pipeline_full_d2.ev"
    "tests/kernel/test_pipeline_lex_parse.ev"
)

die() { echo "diff-test-selfhosted: $*" >&2; exit 2; }

[ -x "$EVIDENT" ] || die "bootstrap runtime missing at $EVIDENT (run ./test.sh --rust-only first)"
[ -x "$KERNEL" ]  || die "kernel binary missing at $KERNEL (run ./test.sh --rust-only first)"

TMPDIR_DT="$(mktemp -d)"
trap 'rm -rf "$TMPDIR_DT"' EXIT

# extract_expected_stdout <file> → newline-joined list of expected stdout lines.
extract_expected_stdout() {
    awk '
        /^[[:space:]]*--[[:space:]]*expect:[[:space:]]*stdout[[:space:]]*=/ {
            # Strip everything up through the "= "
            sub(/^[^=]*=[[:space:]]*/, "", $0)
            # Trim surrounding quotes if present
            if (substr($0, 1, 1) == "\"") { sub(/^"/, "", $0); sub(/"[[:space:]]*$/, "", $0) }
            print
        }
    ' "$1"
}

extract_expected_exit() {
    awk '
        /^[[:space:]]*--[[:space:]]*expect:[[:space:]]*exit[[:space:]]*=/ {
            sub(/^[^=]*=[[:space:]]*/, "", $0)
            gsub(/[[:space:]]/, "", $0)
            print
            exit
        }
        END { if (NR == 0) print "0" }
    ' "$1"
}

run_one() {
    local fixture="$1"
    local path="${ROOT}/${fixture}"
    [ -f "$path" ] || { echo "  missing: $fixture"; return 1; }

    local smt="${TMPDIR_DT}/$(basename "$fixture" .ev).smt2"
    local stdout_file="${TMPDIR_DT}/$(basename "$fixture" .ev).out"

    # Step 1: emit (Rust bootstrap — the seam the self-hosted pipeline
    # will eventually replace).
    if ! "$EVIDENT" emit "$path" main -o "$smt" >/dev/null 2>"${TMPDIR_DT}/emit.err"; then
        echo "  emit-failed: $fixture: $(cat "${TMPDIR_DT}/emit.err")"
        return 1
    fi

    # Step 2: run on the kernel (this is the pipeline composition under
    # test — the .ev fixture IS a sketch of the self-hosted pipeline).
    set +e
    "$KERNEL" "$smt" >"$stdout_file" 2>"${TMPDIR_DT}/kernel.err"
    local actual_exit=$?
    set -e

    local expected_exit
    expected_exit="$(extract_expected_exit "$path")"
    if [ "$actual_exit" != "$expected_exit" ]; then
        echo "  exit-mismatch: $fixture: expected=$expected_exit got=$actual_exit"
        return 1
    fi

    local expected_stdout
    expected_stdout="$(extract_expected_stdout "$path")"
    if [ -n "$expected_stdout" ]; then
        local actual_stdout
        actual_stdout="$(cat "$stdout_file")"
        # Strip a single trailing newline for clean comparison.
        actual_stdout="${actual_stdout%$'\n'}"
        if [ "$actual_stdout" != "$expected_stdout" ]; then
            echo "  stdout-mismatch: $fixture"
            echo "    expected:"
            printf '      %s\n' "$expected_stdout" | sed 's/$//'
            echo "    got:"
            printf '      %s\n' "$actual_stdout" | sed 's/$//'
            return 1
        fi
    fi

    return 0
}

passed=0
failed=0
total=${#FIXTURES[@]}

for fixture in "${FIXTURES[@]}"; do
    if run_one "$fixture"; then
        echo "  ok: $fixture"
        passed=$((passed + 1))
    else
        failed=$((failed + 1))
    fi
done

echo
echo "selfhosted-pipeline: ${passed}/${total} fixtures compose lex+parse+translate end-to-end"

if [ "$failed" -eq 0 ]; then
    exit 0
else
    exit 1
fi
