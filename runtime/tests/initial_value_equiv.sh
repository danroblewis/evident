#!/usr/bin/env bash
# Phase 2.18 — `:=` initial-value syntax ≡ explicit `is_first_tick ⇒ …` seed.
#
# Task #295: `x ∈ Int := 0` is sugar for `is_first_tick ⇒ x = 0`. A dropped or
# malformed seed is a SILENT bug (Z3 would just leave the var free), so this
# pins END-TO-END EQUIVALENCE: a program written with `:=` must produce the
# byte-identical effect-run trace to the same program written with the explicit
# guarded seed. Covers scalar, multi-name, and record-ctor seeds.
#
# Standalone: takes the path to the `evident` binary as $1 (defaults to the
# release build), runs each (seed, explicit) pair, diffs stdout. No web server,
# no python.
set -u

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
EVIDENT="${1:-$ROOT/runtime/target/release/evident}"

if [ ! -x "$EVIDENT" ]; then
    echo "FAIL: evident binary not found/executable at $EVIDENT" >&2
    exit 1
fi

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

fails=0

# Run a pair of programs and assert their effect-run output is identical.
#   $1 = case name   $2 = `:=` program text   $3 = explicit program text
check_pair() {
    local name="$1" seed_src="$2" expl_src="$3"
    printf '%s' "$seed_src" > "$TMP/$name.seed.ev"
    printf '%s' "$expl_src" > "$TMP/$name.expl.ev"
    "$EVIDENT" effect-run "$TMP/$name.seed.ev" --max-steps 40 \
        > "$TMP/$name.seed.out" 2>&1
    local rc_seed=$?
    "$EVIDENT" effect-run "$TMP/$name.expl.ev" --max-steps 40 \
        > "$TMP/$name.expl.out" 2>&1
    local rc_expl=$?

    if [ "$rc_seed" -ne "$rc_expl" ]; then
        echo "FAIL [$name]: exit codes differ (:= $rc_seed vs explicit $rc_expl)" >&2
        fails=$((fails + 1)); return
    fi
    if ! diff -q "$TMP/$name.seed.out" "$TMP/$name.expl.out" >/dev/null; then
        echo "FAIL [$name]: effect-run traces differ" >&2
        echo "  --- := ---"   >&2; sed 's/^/  /' "$TMP/$name.seed.out" >&2
        echo "  --- explicit ---" >&2; sed 's/^/  /' "$TMP/$name.expl.out" >&2
        fails=$((fails + 1)); return
    fi
    echo "ok [$name]: := ≡ is_first_tick (rc=$rc_seed, $(wc -l < "$TMP/$name.seed.out") lines)"
}

# ── Case 1: scalar seed — falling counter, seed at 10 ──────────────────────
check_pair scalar \
'import "stdlib/runtime.ev"
enum FallState = Falling | Landed
fsm gravity
    x ∈ Int := 10
    Δx = -1
    state ∈ FallState = (x ≤ 7 ? Landed : Falling)
    x_str ∈ String = to_str(x)
    effects = match state
        Falling ⇒ ⟨Println("x = " ++ x_str)⟩
        Landed  ⇒ ⟨Println("landed at " ++ x_str), Exit(0)⟩
' \
'import "stdlib/runtime.ev"
enum FallState = Falling | Landed
fsm gravity
    x ∈ Int
    is_first_tick ⇒ x = 10
    Δx = -1
    state ∈ FallState = (x ≤ 7 ? Landed : Falling)
    x_str ∈ String = to_str(x)
    effects = match state
        Falling ⇒ ⟨Println("x = " ++ x_str)⟩
        Landed  ⇒ ⟨Println("landed at " ++ x_str), Exit(0)⟩
'

# ── Case 2: multi-name seed — `x, y ∈ Int := 0` ────────────────────────────
check_pair multiname \
'import "stdlib/runtime.ev"
fsm walk
    x, y ∈ Int := 0
    Δx = 1
    Δy = 2
    sum ∈ Int = x + y
    s ∈ String = to_str(sum)
    done ∈ Bool = (sum ≥ 9)
    effects = (done ? ⟨Println("sum=" ++ s), Exit(0)⟩ : ⟨Println("sum=" ++ s)⟩)
' \
'import "stdlib/runtime.ev"
fsm walk
    x ∈ Int
    y ∈ Int
    is_first_tick ⇒ x = 0
    is_first_tick ⇒ y = 0
    Δx = 1
    Δy = 2
    sum ∈ Int = x + y
    s ∈ String = to_str(sum)
    done ∈ Bool = (sum ≥ 9)
    effects = (done ? ⟨Println("sum=" ++ s), Exit(0)⟩ : ⟨Println("sum=" ++ s)⟩)
'

# ── Case 3: record-ctor seed — `pos ∈ IVec2 := IVec2(3, 4)` ────────────────
check_pair record \
'import "stdlib/runtime.ev"
type IVec2(x, y ∈ Int)
fsm walk
    pos ∈ IVec2 := IVec2(3, 4)
    pos = _pos
    sx ∈ String = to_str(pos.x)
    sy ∈ String = to_str(pos.y)
    effects = ⟨Println("pos=" ++ sx ++ "," ++ sy), Exit(0)⟩
' \
'import "stdlib/runtime.ev"
type IVec2(x, y ∈ Int)
fsm walk
    pos ∈ IVec2
    is_first_tick ⇒ pos = IVec2(3, 4)
    pos = _pos
    sx ∈ String = to_str(pos.x)
    sy ∈ String = to_str(pos.y)
    effects = ⟨Println("pos=" ++ sx ++ "," ++ sy), Exit(0)⟩
'

if [ "$fails" -ne 0 ]; then
    echo "$fails case(s) failed" >&2
    exit 1
fi
echo "all := / is_first_tick equivalence cases passed"
exit 0
