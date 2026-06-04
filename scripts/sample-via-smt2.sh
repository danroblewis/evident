#!/usr/bin/env bash
# TODO: rewrite in Evident
#
# sample-via-smt2.sh — the self-hosted `sample` verb (wave 4m: lex-once).
#
# The bootstrap binary's `sample` is a one-shot satisfiability check: for
# each top-level claim, solve its constraint set and report sat/unsat. The
# self-hosted toolchain is exactly `kernel + *.smt2`, which only *emits*
# SMT-LIB; it has no solve verb. This wrapper closes that gap.
#
# WALL 1 (wave 4j) was per-claim recompile cost: the old wrapper ran
# `kernel + compiler.smt2` ONCE PER CLAIM, re-lexing the whole file each
# time (~90 s × N claims ⇒ hours for one `--lang` pass). Wave 4m folds
# that wall with the wave-4i Option 1 design: a SAMPLE driver
# (compiler/sample.ev → sample.smt2) that lexes the file ONCE and emits
# EVERY claim's constraints in a single kernel run, each wrapped as:
#
#     <shared prelude: Result + last_results decls>      ← before any push
#     <shared enum datatypes>                            ← before any push
#     ;; claim: <name>
#     (push) <claim's declares + asserts> (check-sat) (pop)
#     ;; claim: <name>
#     (push) … (check-sat) (pop)
#     …
#
# A SINGLE `z3 -in` then decides every claim in order (push/pop resets the
# per-claim declares; shared decls sit before the first push so they
# survive every pop). z3 prints one sat/unsat line per (check-sat), in
# claim order; we zip those against the `;; claim:` markers embedded in
# the emitted program. sat → "true", unsat → "false" — the same verdict
# bootstrap's `query` computes, by the same SMT solver.
#
# Usage:
#   sample-via-smt2.sh <file.ev> --all [--json]      # every claim → {name:bool,…}
#   sample-via-smt2.sh <file.ev> <claim> [--json]    # one claim

set -u -o pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

KERNEL="$ROOT/kernel/target/release/kernel"
FLATTEN="$ROOT/scripts/flatten-evident.sh"
SAMPLE_SMT2="$ROOT/sample.smt2"
INPUT_PATH="/tmp/compiler-input.ev"          # path sample.ev ReadFile's
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
[ -f "$SAMPLE_SMT2" ]    || die "sample.smt2 not built (run scripts/build-sample-smt2.sh)"
[ -x "$FLATTEN" ]        || die "flatten preprocessor missing at $FLATTEN"
[ -n "$Z3" ]             || die "z3 not found on PATH"

# Flatten once; the same input file is reused for the single sample run.
FLAT="$(mktemp -t sample-flat.XXXXXX.ev)"
SAMPLE_OUT="$(mktemp -t sample-out.XXXXXX.smt2)"
trap 'rm -f "$FLAT" "$SAMPLE_OUT"' EXIT
"$FLATTEN" "$SRC" > "$FLAT" || die "flatten failed for $SRC"

# ── LEX-ONCE: emit every claim's check-sat block in ONE kernel run ──
# sample.ev ReadFile's `/tmp/compiler-input.ev` on its first tick (the
# path is baked into the compiled sample.smt2). When multiple
# sample-via-smt2 invocations run in parallel, they race on that
# shared path — one wrapper's `cp` can clobber another wrapper's
# input mid-kernel-run, yielding cross-talk between unrelated files.
#
# Serialize the cp → kernel-run pair with a flock so the racy section
# is single-writer. The parallelism cost: the kernel call IS the long
# part (~tens of minutes per file), so a parallel run effectively
# becomes serial; that's correctness over speed for now. The proper
# fix is in compiler/sample.ev (read source from stdin or a path
# passed through the kernel) — left to a later wave.
# mkdir-based portable lock (macOS has no flock by default).
LOCK_DIR="/tmp/sample-via-smt2.lock.d"
while ! mkdir "$LOCK_DIR" 2>/dev/null; do sleep 0.5; done
trap 'rm -rf "$LOCK_DIR"; rm -f "$FLAT" "$SAMPLE_OUT"' EXIT
cp "$FLAT" "$INPUT_PATH"
"$KERNEL" "$SAMPLE_SMT2" < "$FLAT" 2>/dev/null \
    | grep -v '^\[functionizer\]' > "$SAMPLE_OUT" \
    || { rm -rf "$LOCK_DIR"; die "sample.smt2 run failed for $SRC"; }
rm -rf "$LOCK_DIR"

[ -s "$SAMPLE_OUT" ] || die "sample.smt2 produced no output for $SRC"

# Claim names in emit order (the `;; claim:` markers sample.ev embeds).
# Plain indexed arrays only — macOS ships bash 3.2 (no mapfile / declare -A).
NAMES=()
while IFS= read -r n; do NAMES+=("$n"); done \
    < <(grep '^;; claim: ' "$SAMPLE_OUT" | sed 's/^;; claim: //')

# One sat/unsat/unknown line per (check-sat), in the same order.
VERDICTS=()
while IFS= read -r v; do VERDICTS+=("$v"); done \
    < <("$Z3" -in < "$SAMPLE_OUT" 2>/dev/null | grep -E '^(sat|unsat|unknown)$')

[ "${#NAMES[@]}" -gt 0 ] || die "no ';; claim:' markers in sample output for $SRC"

# Map z3's verdict (by position) to a JSON bool. unknown/error → false
# (mirrors bootstrap query's .unwrap_or(false)).
verdict_to_bool() {
    case "$1" in
        sat)   echo "true" ;;
        *)     echo "false" ;;   # unsat / unknown / missing
    esac
}

if [ "$ALL" -eq 1 ]; then
    parts=()
    i=0
    for name in "${NAMES[@]}"; do
        b="$(verdict_to_bool "${VERDICTS[$i]:-unknown}")"
        parts+=("\"$name\":$b")
        i=$((i + 1))
    done
    joined="$(IFS=,; echo "${parts[*]}")"
    if [ "$JSON" -eq 1 ]; then
        printf '{%s}\n' "$joined"
    else
        for p in "${parts[@]}"; do echo "$p"; done
    fi
    exit 0
fi

[ -n "$CLAIM" ] || die "missing <claim> (or pass --all)"
# Single-claim: find CLAIM's position, read the matching verdict.
b="false"
i=0
for name in "${NAMES[@]}"; do
    if [ "$name" = "$CLAIM" ]; then
        b="$(verdict_to_bool "${VERDICTS[$i]:-unknown}")"
        break
    fi
    i=$((i + 1))
done
if [ "$JSON" -eq 1 ]; then
    printf '{"%s":%s}\n' "$CLAIM" "$b"
else
    [ "$b" = "true" ] && echo "satisfied: true" || echo "satisfied: false"
    [ "$b" = "true" ] && exit 0 || exit 1
fi
