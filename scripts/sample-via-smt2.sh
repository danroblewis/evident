#!/usr/bin/env bash
# TODO: rewrite in Evident
#
# sample-via-smt2.sh — the self-hosted `sample` verb (wave 4j).
#
# The bootstrap binary's `sample` is a one-shot satisfiability check: for
# each top-level claim, solve its constraint set and report sat/unsat. The
# self-hosted toolchain is exactly `kernel + compiler.smt2`, which only
# *emits* SMT-LIB; it has no solve verb. This wrapper closes that gap by
# composing two pieces we already have:
#
#   1. compiler.smt2 selects + emits ONE claim's constraints (wave 4j
#      added the /tmp/compiler-target-claim.txt selector to compiler.ev).
#   2. a standalone `z3` runs (check-sat) on the emitted program.
#
# sat → "true", unsat → "false" — the same verdict bootstrap's `query`
# computes, by the same SMT solver, so the per-claim JSON matches.
#
# Usage:
#   sample-via-smt2.sh <file.ev> --all [--json]      # every claim → {name:bool,…}
#   sample-via-smt2.sh <file.ev> <claim> [--json]    # one claim
#
# The emitted program is an *FSM* program (manifest + Effect/Result
# datatypes + the claim's declares/asserts). For a pure sat-check we only
# need Z3 to decide the claim's own constraints, so we:
#   * strip the kernel functionizer diagnostic line ([functionizer] …),
#   * strip the `;; manifest:` comment header (Z3 ignores `;;` lines, but
#     we drop them anyway for a clean program),
#   * append `(check-sat)`.
# The shared prelude (Result/last_results/effects/is_first_tick) declares
# extra UNCONSTRAINED vars; they never make a sat claim unsat, so the
# verdict is the claim's own satisfiability — exactly bootstrap's.

set -u -o pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

KERNEL="$ROOT/kernel/target/release/kernel"
FLATTEN="$ROOT/scripts/flatten-evident.sh"
COMPILER_SMT2="$ROOT/compiler.smt2"
INPUT_PATH="/tmp/compiler-input.ev"          # path compiler.ev ReadFile's
TARGET_PATH="/tmp/compiler-target-claim.txt" # claim selector (wave 4j)
Z3="$(command -v z3 || true)"

die() { echo "sample-via-smt2: $*" >&2; exit 2; }

[ -n "${1:-}" ] || die "missing <file.ev>"
SRC=""
ALL=0
JSON=0
CLAIM=""
for a in "$@"; do
    case "$a" in
        --all)  ALL=1 ;;
        --json) JSON=1 ;;
        --*)    ;;  # ignore unknown flags (e.g. --given handled elsewhere)
        *)      if [ -z "$SRC" ]; then SRC="$a"; elif [ -z "$CLAIM" ]; then CLAIM="$a"; fi ;;
    esac
done

[ -n "$SRC" ]            || die "missing <file.ev>"
[ -f "$SRC" ]            || die "input not found: $SRC"
[ -x "$KERNEL" ]         || die "kernel binary missing at $KERNEL (run ./test.sh --rust-only)"
[ -f "$COMPILER_SMT2" ]  || die "compiler.smt2 not built (run scripts/build-compiler-smt2.sh)"
[ -x "$FLATTEN" ]        || die "flatten preprocessor missing at $FLATTEN"
[ -n "$Z3" ]             || die "z3 not found on PATH"

# Flatten once; the same input file is reused for every claim.
FLAT="$(mktemp -t sample-flat.XXXXXX.ev)"
trap 'rm -f "$FLAT" "$TARGET_PATH"' EXIT
"$FLATTEN" "$SRC" > "$FLAT" || die "flatten failed for $SRC"
cp "$FLAT" "$INPUT_PATH"

# Enumerate top-level claim/type/schema/fsm names in source order, skipping
# generic templates (`<…>` type params) — mirrors bootstrap's `schema_names`
# minus `type_params` (cmd_query_or_sample in bootstrap/runtime/src/main.rs).
list_claims() {
    grep -nE '^[[:space:]]*(claim|fsm|type|schema)[[:space:]]+[A-Za-z_]' "$SRC" \
        | sed -E 's/^[0-9]+:[[:space:]]*(claim|fsm|type|schema)[[:space:]]+([A-Za-z_][A-Za-z0-9_]*).*/\2:\0/' \
        | while IFS= read -r line; do
            name="${line%%:*}"
            decl="${line#*:}"
            # Skip generic templates: a `<` before the first newline of the head.
            case "$decl" in
                *"<"*) continue ;;
            esac
            printf '%s\n' "$name"
        done
}

# check_one <claim> → echoes "true" or "false"
check_one() {
    local claim="$1"
    printf '%s' "$claim" > "$TARGET_PATH"
    local smt
    smt="$("$KERNEL" "$COMPILER_SMT2" < "$FLAT" 2>/dev/null \
            | grep -v '^\[functionizer\]' \
            | grep -v '^;; manifest:')"
    local verdict
    verdict="$(printf '%s\n(check-sat)\n' "$smt" | "$Z3" -in 2>/dev/null | head -1)"
    case "$verdict" in
        sat)   echo "true" ;;
        unsat) echo "false" ;;
        *)     echo "false" ;;  # unknown/error → false (mirrors query .unwrap_or(false))
    esac
}

if [ "$ALL" -eq 1 ]; then
    parts=()
    while IFS= read -r name; do
        [ -z "$name" ] && continue
        b="$(check_one "$name")"
        parts+=("\"$name\":$b")
    done < <(list_claims)
    joined="$(IFS=,; echo "${parts[*]}")"
    if [ "$JSON" -eq 1 ]; then
        printf '{%s}\n' "$joined"
    else
        for p in "${parts[@]}"; do echo "$p"; done
    fi
    exit 0
fi

[ -n "$CLAIM" ] || die "missing <claim> (or pass --all)"
b="$(check_one "$CLAIM")"
if [ "$JSON" -eq 1 ]; then
    printf '{"%s":%s}\n' "$CLAIM" "$b"
else
    [ "$b" = "true" ] && echo "satisfied: true" || echo "satisfied: false"
    [ "$b" = "true" ] && exit 0 || exit 1
fi
