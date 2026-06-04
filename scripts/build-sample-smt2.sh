#!/usr/bin/env bash
# TODO: rewrite in Evident
#
# build-sample-smt2.sh — the one-time bootstrap → self-hosted handoff.
#
# Compiles the self-hosted compiler (compiler/sample.ev) to a single
# runnable SMT-LIB program (sample.smt2 at the repo root), using the
# bootstrap Rust runtime ONE LAST TIME. After this file exists, the
# kernel runs `sample.smt2` to compile every other `.ev` file and the
# bootstrap binary is no longer on the producing path — see CLAUDE.md,
# "The deletion path", step 6a/6b.
#
# Pipeline (mirrors the deletion checklist Phase 3 acceptance):
#   1. scripts/flatten-evident.sh compiler/sample.ev  > /tmp/…-flat.ev
#      (resolve every `import "…"` into one flat translation unit, since
#      kernel + sample.smt2 does NOT do import resolution).
#   2. `scripts/evident-self bin` (the bootstrap runtime) emit <flat> main
#   3. Verify the output is non-empty and starts with `;; manifest:`
#      (the kernel manifest header — required + first line; see CLAUDE.md
#      "Manifest header").
#   4. Atomically `mv` the verified output into place (so a failed build
#      never leaves a half-written sample.smt2).
#
# Usage:
#   scripts/build-sample-smt2.sh              # build → sample.smt2
#   scripts/build-sample-smt2.sh --check-only # build → /tmp preview only
#
# Exit codes:
#   0  success
#   1  compiler/sample.ev missing, flatten failed, or bad output
#   (bootstrap emit's own non-zero exit is propagated as-is)

set -u -o pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$ROOT"

# Resolve the bootstrap compiler through the single seam (scripts/evident-self),
# never the hardcoded path — that literal lives in exactly one extension-less
# file so scripts/check-deletable.sh stays clean (see its blocker-1 note). This
# build is the bootstrap → self-hosted handoff, so force the bootstrap binary
# (EVIDENT_SELF_VIA_SMT2=0) regardless of the ambient cutover env.
EVIDENT="$(EVIDENT_SELF_VIA_SMT2=0 "$ROOT/scripts/evident-self" bin)"
FLATTEN="$ROOT/scripts/flatten-evident.sh"
SRC="$ROOT/compiler/sample.ev"
OUT="$ROOT/sample.smt2"

CHECK_ONLY=0
case "${1:-}" in
    --check-only) CHECK_ONLY=1 ;;
    "") ;;
    *) echo "usage: build-sample-smt2.sh [--check-only]" >&2; exit 1 ;;
esac

die() { echo "build-sample-smt2: $*" >&2; exit 1; }

[ -f "$SRC" ]    || die "compiler source not found: ${SRC#$ROOT/} (nothing to build)"
[ -x "$EVIDENT" ] || die "bootstrap runtime missing at ${EVIDENT#$ROOT/} (run ./test.sh --rust-only first)"
[ -x "$FLATTEN" ] || die "flatten preprocessor missing at ${FLATTEN#$ROOT/}"

FLAT="$(mktemp -t sample-flat.XXXXXX.ev)"
trap 'rm -f "$FLAT"' EXIT

# Step 1 — flatten the import graph into one translation unit.
if ! "$FLATTEN" "$SRC" > "$FLAT"; then
    die "flatten failed for ${SRC#$ROOT/}"
fi

# Destination: a tmp preview under --check-only, else an atomic stage file.
if [ "$CHECK_ONLY" -eq 1 ]; then
    DEST="/tmp/sample-check.smt2"
    STAGE="$DEST"
else
    DEST="$OUT"
    STAGE="$OUT.tmp"
fi

# Step 2 — compile (bootstrap, one final time). Propagate its exit code.
if ! "$EVIDENT" emit "$FLAT" main -o "$STAGE"; then
    rc=$?
    rm -f "$STAGE"
    echo "build-sample-smt2: bootstrap emit failed (exit $rc)" >&2
    exit "$rc"
fi

# Step 3 — verify shape: non-empty + manifest header on the first line.
if [ ! -s "$STAGE" ]; then
    rm -f "$STAGE"
    die "produced an empty SMT-LIB file"
fi
if ! head -1 "$STAGE" | grep -q '^;; manifest:'; then
    rm -f "$STAGE"
    die "output does not start with ';; manifest:' (not a kernel-runnable program)"
fi

# Step 4 — atomic publish (skip for --check-only, which IS the dest).
if [ "$CHECK_ONLY" -eq 0 ]; then
    mv "$STAGE" "$DEST"
fi

bytes="$(wc -c < "$DEST" | tr -d ' ')"
lines="$(wc -l < "$DEST" | tr -d ' ')"
echo "── build-sample-smt2 ──"
echo "  source : ${SRC#$ROOT/}"
echo "  output : ${DEST}"
echo "  size   : ${bytes} bytes / ${lines} lines"
if [ "$CHECK_ONLY" -eq 1 ]; then
    echo "  (--check-only: ${OUT#$ROOT/} was NOT modified)"
else
    echo "  sample.smt2 is ready — kernel can now compile .ev files with it."
fi
