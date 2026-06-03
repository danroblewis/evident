#!/usr/bin/env bash
# TODO: rewrite in Evident
#
# diff-vs-bootstrap.sh — equivalence check for ONE .ev source.
#
# Compiles a single source file two ways and compares the result:
#   * bootstrap  — `scripts/evident-self bin` (the bootstrap runtime) emit
#   * self-hosted — kernel + compiler.smt2
#
# TWO comparison modes (grammar wave 4b):
#
#   byte mode (DEFAULT) — diff the two `.smt2` files, treating
#     whitespace-only differences as equivalent. A clean diff means the
#     self-hosted compiler reproduces the bootstrap output BYTE-FOR-BYTE.
#     This is the strict per-source proof, but it is the WRONG bar for the
#     deletion decision: bootstrap and self-hosted may pick different but
#     equally-valid SMT-LIB encodings (e.g. `(Array Int Effect)+__len` vs
#     `seq.++`/`seq.unit` for a `Seq`, or different `max-effects`), which
#     byte-diff flags as a difference even though the kernel runs both to
#     the same observable result.
#
#   --semantic — RUN both `.smt2` files on the kernel and compare the
#     observable behaviour (stdout + exit code), NOT the bytes. The kernel
#     can run either Seq encoding; both are valid SMT-LIB. This is the
#     bar that actually backs deleting bootstrap (CLAUDE.md, "The deletion
#     path", step 4/5; coordinator decision, grammar-wave4b.md): we don't
#     care if the SMT-LIB is byte-identical, only if it BEHAVES the same.
#     This mode alone resolves the wave-4 Seq-encoding and max-effects
#     "differences" — they are semantically equivalent.
#
# Self-hosted input mechanism: compiler/compiler.ev reads its source from
# the fixed path /tmp/compiler-input.ev via a ReadFile effect on the first
# tick (see compiler/compiler.ev, "Input path"), NOT from stdin. So we
# write the flattened source there before invoking the kernel. (stdin is
# also redirected for forward-compatibility with a future stdin-reading
# driver; the current driver ignores it.)
#
# Usage:
#   scripts/diff-vs-bootstrap.sh [--semantic] <source.ev> <claim>
#
# Exit codes:
#   0  equivalent (byte: whitespace-equivalent; semantic: same stdout+exit)
#      OR cleanly SKIPPED
#   1  outputs differ, or a usage / compile error occurred
#
# SKIPPED (exit 0) when compiler.smt2 or the kernel binary is absent — so
# wiring this into a test harness before the cutover never turns it red.

set -u -o pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$ROOT"

# Bootstrap reference compiler via the single seam (never the hardcoded path —
# keeps scripts/check-deletable.sh's blocker-1 clean). Force the bootstrap
# binary (EVIDENT_SELF_VIA_SMT2=0): this leg IS the bootstrap reference we diff
# the self-hosted output against.
EVIDENT="$(EVIDENT_SELF_VIA_SMT2=0 "$ROOT/scripts/evident-self" bin)"
KERNEL="$ROOT/kernel/target/release/kernel"
FLATTEN="$ROOT/scripts/flatten-evident.sh"
COMPILER_SMT2="$ROOT/compiler.smt2"
INPUT_PATH="/tmp/compiler-input.ev"   # the path compiler.ev ReadFile's

die() { echo "diff-vs-bootstrap: $*" >&2; exit 1; }
skip() { echo "diff-vs-bootstrap: SKIPPED — $*"; exit 0; }

SEMANTIC=0
if [ "${1:-}" = "--semantic" ]; then
    SEMANTIC=1
    shift
fi

[ "$#" -eq 2 ] || die "usage: diff-vs-bootstrap.sh [--semantic] <source.ev> <claim>"
SRC="$1"; CLAIM="$2"
[ -f "$SRC" ] || die "source not found: $SRC"

# Pre-cutover guards: missing self-hosted compiler / kernel ⇒ SKIP, not fail.
[ -f "$COMPILER_SMT2" ] || skip "compiler.smt2 not built yet (run scripts/build-compiler-smt2.sh)"
[ -x "$KERNEL" ]        || skip "kernel binary not built (run ./test.sh --rust-only)"
[ -x "$EVIDENT" ]       || die  "bootstrap runtime missing at ${EVIDENT#$ROOT/}"
[ -x "$FLATTEN" ]       || die  "flatten preprocessor missing at ${FLATTEN#$ROOT/}"

ORIG="$(mktemp -t orig.XXXXXX.smt2)"
FLAT="$(mktemp -t flat.XXXXXX.ev)"
SELF="$(mktemp -t self.XXXXXX.smt2)"
trap 'rm -f "$ORIG" "$FLAT" "$SELF"' EXIT

# 1. bootstrap reference output.
if ! "$EVIDENT" emit "$SRC" "$CLAIM" -o "$ORIG" 2>"$FLAT.err"; then
    die "bootstrap emit failed: $(head -1 "$FLAT.err" 2>/dev/null)"
fi

# 2. flatten the source into one translation unit (kernel does no imports).
if ! "$FLATTEN" "$SRC" > "$FLAT"; then
    die "flatten failed for $SRC"
fi

# 3. self-hosted output: feed the flattened source to compiler.smt2.
cp "$FLAT" "$INPUT_PATH"
if ! "$KERNEL" "$COMPILER_SMT2" < "$FLAT" > "$SELF" 2>"$FLAT.kerr"; then
    die "self-hosted compile failed: $(head -1 "$FLAT.kerr" 2>/dev/null)"
fi

# 4. compare.
if [ "$SEMANTIC" -eq 0 ]; then
    # byte mode (default): diff the .smt2 files, whitespace-only ≡.
    if diff -q -w -B "$ORIG" "$SELF" >/dev/null 2>&1; then
        echo "diff-vs-bootstrap: MATCH — $SRC ($CLAIM): self-hosted ≡ bootstrap (bytes)"
        exit 0
    else
        echo "diff-vs-bootstrap: DIFFER — $SRC ($CLAIM)" >&2
        diff -w -B "$ORIG" "$SELF" | head -40 >&2
        exit 1
    fi
fi

# --semantic: run BOTH .smt2 on the kernel; compare observable behaviour
# (stdout + exit code), not the bytes. Either valid SMT-LIB encoding is
# acceptable as long as the kernel produces the same output. See the
# header and docs/plans/grammar-wave4b.md (coordinator decision).
OUT_ORIG="$(mktemp -t orig.XXXXXX.out)"
OUT_SELF="$(mktemp -t self.XXXXXX.out)"
trap 'rm -f "$ORIG" "$FLAT" "$SELF" "$OUT_ORIG" "$OUT_SELF"' EXIT

"$KERNEL" "$ORIG" </dev/null >"$OUT_ORIG" 2>/dev/null
EXIT_ORIG=$?
"$KERNEL" "$SELF" </dev/null >"$OUT_SELF" 2>/dev/null
EXIT_SELF=$?

STDOUT_MATCH=0
diff -q "$OUT_ORIG" "$OUT_SELF" >/dev/null 2>&1 && STDOUT_MATCH=1

if [ "$STDOUT_MATCH" -eq 1 ] && [ "$EXIT_ORIG" -eq "$EXIT_SELF" ]; then
    echo "diff-vs-bootstrap: SEMANTIC MATCH — $SRC ($CLAIM): kernel stdout+exit identical (exit $EXIT_ORIG)"
    exit 0
else
    echo "diff-vs-bootstrap: SEMANTIC DIFFER — $SRC ($CLAIM)" >&2
    echo "  bootstrap: exit=$EXIT_ORIG / self-hosted: exit=$EXIT_SELF" >&2
    if [ "$STDOUT_MATCH" -eq 0 ]; then
        echo "  stdout diff (bootstrap < / self-hosted >):" >&2
        diff "$OUT_ORIG" "$OUT_SELF" | head -40 >&2
    fi
    exit 1
fi
