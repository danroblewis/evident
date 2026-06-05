#!/usr/bin/env bash
# TODO: rewrite in Evident
#
# scripts/run-seam-smoke.sh — fast regression test for the seam path.
#
# Compiles tests/seam/smoke_effects.ev through `kernel + compiler.smt2`
# and asserts the output contains the rendered effects body. Catches
# the silent-drop class of bug (see STATE.md "THE single ctor-arg
# blocker"): when translate_ctor.ev drops a non-atom argument, the
# effects assertion vanishes from the output and the kernel rejects
# it at load time with `var effects__len not in model`.
#
# Exit:
#   0  seam emits an effects body (✓)
#   0  compiler.smt2 not present yet (SKIP — pre-cutover state)
#   1  seam emits but body is missing → silent-drop regression
#   1  seam emit itself failed
#
# Runs in ~5 seconds. Wired into test.sh as Phase 6.

set -u -o pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
KERNEL="$ROOT/kernel/target/release/kernel"
COMPILER_SMT2="$ROOT/compiler.smt2"
FIXTURE="$ROOT/tests/seam/smoke_effects.ev"
WRAPPER_PATH="${ROOT}/scripts/evident-self"

if [ ! -f "$COMPILER_SMT2" ]; then
    echo "seam smoke: SKIP — compiler.smt2 not built yet."
    exit 0
fi

if [ ! -x "$KERNEL" ]; then
    echo "seam smoke: kernel binary missing at $KERNEL" >&2
    exit 1
fi

if [ ! -f "$FIXTURE" ]; then
    echo "seam smoke: fixture missing at $FIXTURE" >&2
    exit 1
fi

# Force the seam path even if EVIDENT_SELF_VIA_SMT2 isn't set in the caller.
WRAPPER="$(EVIDENT_SELF_VIA_SMT2=1 "$WRAPPER_PATH" bin)" || {
    echo "seam smoke: could not resolve seam wrapper" >&2
    exit 1
}

OUT="$(mktemp -t evident-seam-smoke.XXXXXX.smt2)"
trap 'rm -f "$OUT" "$WRAPPER"' EXIT

# Cap RSS at 3 GB — protects the host from a runaway compiler.smt2.
if ! MEM_CAP_MB="${MEM_CAP_MB:-3000}" "$WRAPPER" emit "$FIXTURE" main -o "$OUT" 2>/dev/null; then
    echo "seam smoke: FAIL — emit through seam failed" >&2
    exit 1
fi

fail=0
if ! grep -qE '\(declare-fun effects ' "$OUT"; then
    echo "seam smoke: FAIL — output missing '(declare-fun effects )'" >&2
    fail=1
fi
if ! grep -qE 'Exit 0' "$OUT"; then
    echo "seam smoke: FAIL — output missing 'Exit 0' (effects body dropped)" >&2
    fail=1
fi

if [ "$fail" -ne 0 ]; then
    echo "seam smoke: see STATE.md 'THE single ctor-arg blocker' — translate_ctor.ev's" >&2
    echo "  RenderExprL0 silently dropped the effects = ⟨Exit(0)⟩ constraint." >&2
    echo "  Seam output ($(wc -l < "$OUT") lines):" >&2
    sed 's/^/    /' "$OUT" >&2
    exit 1
fi

echo "seam smoke: ✓ effects body survives lex → parse → translate."
exit 0
