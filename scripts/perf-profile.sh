#!/usr/bin/env bash
# TODO: rewrite in Evident
#
# perf-profile.sh — per-constraint performance profiler for Evident.
#
# Answers "which variables / sub-constraints cost the most to solve?" by
# fusing three signals the kernel + Z3 already expose:
#
#   1. TIME  — the kernel's built-in band profiler (EVIDENT_FUNCTIONIZE_
#      TIMING) reports each constraint band's *marginal* solve cost: how
#      much that constraint added to the tick-0 solve, with the variable
#      it constrains. (Negative marginal = the constraint *sped* the solve
#      by adding ground facts — read as ~0.)
#   2. EXPR  — EVIDENT_FUNCTIONIZE_DUMP lists each flat constraint with its
#      index, so a band maps to the actual `(<= buf.count 2048)` text.
#   3. SEARCH SPACE — the emitted tick-0 model run through `z3 -st`:
#      decisions / conflicts / propagations / rlimit-count (Z3's
#      DETERMINISTIC work counter — machine-independent, no timing noise).
#
# Output: a ranked table of the costliest constraints (marginal ms + the
# expression + the variable), plus the model's global search-space stats.
#
# With --bisect, it instead binary-searches the constraint set for the
# subset whose removal most cuts rlimit-count — the deterministic
# "what's blowing up the search" finder, in O(log n) Z3 runs.
#
# Usage:
#   scripts/perf-profile.sh <file.ev> <claim> [--top N] [--bands N]
#                                              [--reps N] [--bisect]
# Env: EVIDENT_ORACLE, EVIDENT_KERNEL (defaults as elsewhere).
#
# DEPENDS ON RUST-KERNEL INSTRUMENTATION: the TIMING/DUMP env-vars live in
# kernel/src/functionize/ + tick.rs. When the functionizer is reimplemented
# in Evident (wave 5c), that instrumentation must be re-exposed or this tool
# goes dark — see docs/plans/wave-5c-functionizer-in-evident.md. (The `z3 -st`
# search-space half is external and survives the transition.)

set -u -o pipefail
DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$DIR/.." && pwd)"
ORACLE="${EVIDENT_ORACLE:-/usr/local/bin/evident-oracle}"
KERNEL="${EVIDENT_KERNEL:-$ROOT/kernel/target/release/kernel}"
FLATTEN="$ROOT/scripts/flatten-evident.sh"
Z3="$(command -v z3 || true)"

TOP=15; BANDS=""; REPS=3; BISECT=0
FILE=""; CLAIM=""
while [ $# -gt 0 ]; do
    case "$1" in
        --top)    TOP="$2"; shift 2;;
        --bands)  BANDS="$2"; shift 2;;
        --reps)   REPS="$2"; shift 2;;
        --bisect) BISECT=1; shift;;
        -*) echo "perf-profile: unknown flag $1" >&2; exit 2;;
        *) if [ -z "$FILE" ]; then FILE="$1"; elif [ -z "$CLAIM" ]; then CLAIM="$1"; fi; shift;;
    esac
done
[ -n "$FILE" ] && [ -n "$CLAIM" ] || { echo "usage: perf-profile.sh <file.ev> <claim> [--top N] [--bands N] [--reps N] [--bisect]" >&2; exit 2; }
[ -x "$ORACLE" ] && [ -x "$KERNEL" ] || { echo "perf-profile: oracle/kernel missing" >&2; exit 2; }

# ── emit ───────────────────────────────────────────────────────────
DUMP=""; TIM=""; Z3F=""; Z3O=""   # so the EXIT trap is safe under set -u
SMT="$(mktemp --suffix=.smt2)"; FLAT="$(mktemp --suffix=.ev)"
trap 'rm -f "$SMT" "$FLAT" "$DUMP" "$TIM" "$Z3F" "$Z3O" 2>/dev/null' EXIT
"$FLATTEN" "$FILE" > "$FLAT" 2>/dev/null || { echo "flatten failed" >&2; exit 2; }
"$ORACLE" emit "$FLAT" "$CLAIM" -o "$SMT" 2>/dev/null || { echo "oracle emit failed" >&2; exit 2; }

# ── DUMP: index -> expression ──────────────────────────────────────
DUMP="$(mktemp)"
EVIDENT_FUNCTIONIZE_DUMP=1 "$KERNEL" "$SMT" >/dev/null 2>"$DUMP" || true
NASSERT="$(grep -c '^\[fz/dump\] flat\[' "$DUMP" 2>/dev/null || echo 0)"
[ -n "$BANDS" ] || { BANDS="$NASSERT"; [ "$BANDS" -gt 256 ] && BANDS=256; [ "$BANDS" -lt 1 ] && BANDS=1; }

# ── build standalone z3 query (strip manifest, add stats) ──────────
Z3F="$(mktemp --suffix=.smt2)"; Z3O="$(mktemp)"
grep -v '^;;' "$SMT" > "$Z3F"
printf '(check-sat)\n(get-info :all-statistics)\n' >> "$Z3F"
z3_stat() { # key  -> value (from $Z3O), 0 if absent
    local v; v="$(grep -oE ":$1 +[0-9.]+" "$Z3O" 2>/dev/null | head -1 | grep -oE '[0-9.]+' | head -1)"
    echo "${v:-0}"
}

echo "════════════════════════════════════════════════════════════════"
echo " perf-profile: $FILE :: $CLAIM"
echo "   $NASSERT flat constraints · $BANDS timing bands · $REPS rep(s)"
echo "════════════════════════════════════════════════════════════════"

# ── global search-space stats (z3 -st) ─────────────────────────────
if [ -n "$Z3" ]; then
    "$Z3" -st "$Z3F" > "$Z3O" 2>&1 || true
    sat="$(grep -m1 -E '^(sat|unsat|unknown)$' "$Z3O" || echo '?')"
    echo "── search space (tick-0 model, z3) ──────────────────────────"
    printf "   verdict=%s  rlimit=%s  decisions=%s  conflicts=%s  propagations=%s  mem=%sMB\n" \
        "$sat" "$(z3_stat rlimit-count)" "$(z3_stat decisions)" "$(z3_stat conflicts)" "$(z3_stat propagations)" "$(z3_stat memory)"
fi

# ── functionizer summary ───────────────────────────────────────────
FZ="$(mktemp)"; EVIDENT_FUNCTIONIZE_STATS=summary "$KERNEL" "$SMT" >/dev/null 2>"$FZ" || true
echo "── functionizer ─────────────────────────────────────────────"
echo "   $(grep -oE 'functionizer\] .*z3\)' "$FZ" | head -1)"
rm -f "$FZ"

if [ "$BISECT" = 1 ]; then
    # ── deterministic rlimit-count bisect ──────────────────────────
    [ -n "$Z3" ] || { echo "perf-profile: --bisect needs z3" >&2; exit 2; }
    echo "── bisect: constraint subset that drives the search ─────────"
    BODY="$(mktemp)"; grep -v '^;;' "$SMT" | grep -vE '^\(check-sat\)|get-info' > "$BODY"
    mapfile -t ALLLINES < <(grep -nE '^\(assert ' "$BODY" | cut -d: -f1)
    rl() { # rl <space-separated assert-linenos-to-DROP>
        local drop="$1" q; q="$(mktemp)"
        awk -v d=" $drop " 'BEGIN{n=0} /^\(assert /{n++; if(index(d," "n" ")){next}} {print}' "$BODY" > "$q"
        printf '(check-sat)\n(get-info :all-statistics)\n' >> "$q"
        "$Z3" -st "$q" 2>/dev/null | grep -oE ':rlimit-count +[0-9]+' | grep -oE '[0-9]+' | head -1
        rm -f "$q"
    }
    BASE="$(rl '')"; echo "   baseline rlimit=$BASE over ${#ALLLINES[@]} asserts"
    # candidate set = all assert ordinals
    cand=(); for i in $(seq 1 ${#ALLLINES[@]}); do cand+=("$i"); done
    while [ "${#cand[@]}" -gt 1 ]; do
        half=$(( ${#cand[@]} / 2 ))
        left=("${cand[@]:0:$half}"); right=("${cand[@]:$half}")
        rl_left="$(rl "${left[*]}")"    # drop left half
        rl_right="$(rl "${right[*]}")"  # drop right half
        # whichever DROP yields the lower rlimit contains the culprit
        if [ "${rl_left:-$BASE}" -le "${rl_right:-$BASE}" ]; then
            echo "   drop L(${#left[@]})→rlimit=$rl_left  drop R(${#right[@]})→rlimit=$rl_right  → culprit in L"
            cand=("${left[@]}")
        else
            echo "   drop L(${#left[@]})→rlimit=$rl_left  drop R(${#right[@]})→rlimit=$rl_right  → culprit in R"
            cand=("${right[@]}")
        fi
    done
    ord="${cand[0]}"; ln="${ALLLINES[$((ord-1))]}"
    echo "   ⇒ dominant constraint: assert #$ord"
    echo "     $(sed -n "${ln}p" "$BODY" | sed 's/^[[:space:]]*//' | cut -c1-100)"
    echo "     removing it: rlimit $BASE → $(rl "$ord")"
    rm -f "$BODY"
    exit 0
fi

# ── TIMING: per-band marginal cost, joined with DUMP expr ──────────
TIM="$(mktemp)"
EVIDENT_FUNCTIONIZE_TIMING=1 EVIDENT_FUNCTIONIZE_TIMING_BANDS="$BANDS" \
  EVIDENT_FUNCTIONIZE_TIMING_REPS="$REPS" "$KERNEL" "$SMT" >/dev/null 2>"$TIM" || true

echo "── costliest constraints (marginal tick-0 solve ms) ─────────"
# Parse band lines:  band k/M [a..b] marginal X ms cum Y ms | shapes | var
#   join the [a..b) index range to DUMP flat[i] expressions.
awk -v dumpf="$DUMP" -v top="$TOP" '
  BEGIN{
    while ((getline line < dumpf) > 0) {
      if (match(line, /flat\[[0-9]+\] = /)) {
        idx=line; sub(/.*flat\[/,"",idx); sub(/\].*/,"",idx)
        expr=line; sub(/.*\] = /,"",expr); EX[idx+0]=expr
      }
    }
  }
  /band[[:space:]]+[0-9]+\/[0-9]+/ {
    lo=$0; sub(/.*\[/,"",lo); sub(/\.\..*/,"",lo); lo+=0
    hi=$0; sub(/.*\.\./,"",hi); sub(/\].*/,"",hi); hi+=0
    m=$0; sub(/.*marginal[[:space:]]+/,"",m); sub(/[[:space:]]*ms.*/,"",m); marg=m+0
    var=$0; sub(/.*\|[[:space:]]*/,"",var); gsub(/[[:space:]]+$/,"",var)
    # representative expression = first index in the band range
    e=EX[lo]; if(e=="") e=EX[lo]
    key=marg<0?0:marg
    n++; MARG[n]=key; EXPR[n]=e; VAR[n]=var; LO[n]=lo; HI[n]=hi
  }
  END{
    # selection sort top-N by MARG desc
    for(r=1;r<=top && r<=n;r++){
      best=r; for(j=r+1;j<=n;j++) if(MARG[j]>MARG[best]) best=j
      t=MARG[r];MARG[r]=MARG[best];MARG[best]=t
      t=EXPR[r];EXPR[r]=EXPR[best];EXPR[best]=t
      t=VAR[r];VAR[r]=VAR[best];VAR[best]=t
      t=LO[r];LO[r]=LO[best];LO[best]=t; t=HI[r];HI[r]=HI[best];HI[best]=t
      e=EXPR[r]; if(length(e)>56) e=substr(e,1,53)"..."
      v=VAR[r]; if(v=="") v="-"
      if(MARG[r] <= 0.05 && r>3) { if(r==4) print "   … (remaining bands ≤ 0.05 ms)"; break }
      printf "   %5.2f ms  [%s..%s]  %-18s %s\n", MARG[r], LO[r], HI[r], v, e
    }
    if(n==0) print "   (no timing bands parsed — model may be fully functionized with no residual solve)"
  }
' "$TIM"
echo "════════════════════════════════════════════════════════════════"
echo "tip: --bisect finds the constraint driving search-space blowup;"
echo "     --bands N / --reps N tune granularity / noise."
