#!/usr/bin/env bash
# TODO: rewrite in Evident
#
# prove-invariants.sh — STATICALLY prove a carried type invariant is inductive
# over an Evident FSM's one-tick transition, with Z3 (k-induction). Turns
# "add a type invariant, run it, and pray it holds / doesn't hit the ≠-trap"
# into "add it and PROVE it in sub-second."
#
# An Evident program is a Z3 transition system: the kernel already emits the
# one-tick transition as SMT-LIB, with the carry consts `_x` left FREE (the
# kernel pins them per-tick at runtime). We instead constrain `_x` by the
# invariant and ask Z3 whether the transition PRESERVES it:
#     I(_x) ∧ transition(_x → x) ⊢ I(x)
# UNSAT (of the negation) = inductive, proven for ALL reachable states.
# SAT   = a counterexample carry-state that breaks it (printed).
#
# Three checks per invariant (they catch different bugs):
#   - base    : the invariant holds at tick 0 (is_first_tick).
#   - step    : the transition preserves it (the induction above).
#   - totality: a reachable invariant-state has SOME next state (not "stuck").
#
# The invariant is EXTRACTED from the emitted SMT (the oracle already
# translated the `type` body) — an assert that mentions a carried field const
# and contains no `ite`/`is_first_tick`/`select` IS an invariant (vs the
# transition asserts, which don't). No Evident→SMT re-translation needed.
#
# Sweet spot (what Z3 proves cheaply): a CONVEX, few-variable invariant over a
# typed buffer — `0 ≤ count ≤ cap`, `sol > 0 ⇒ ctx > 0`. The hypothesis I(_x)
# is load-bearing for tractability: it lets Z3 eliminate the irrelevant carries
# (proven sub-second even on the full ~3300-assert driver.ev). A non-convex
# `≠` would re-explode the search — same discipline that keeps runtime fast.
#
# Usage:
#   prove-invariants.sh <fixture.ev> <claim> <field-const-prefix> [strengthen.smt2]
#     <field-const-prefix>  the flattened carried record's const prefix, e.g.
#       `c_` for `c ∈ Ctr` (field c.n → c_n), `tok_buf_` for an FtiBuffer.
#     [strengthen.smt2]     optional auxiliary inductive lemma — extra `(assert …)`
#       over the `_`-carries (e.g. a latch step↔field ordering). Joins the step
#       hypothesis so a not-1-inductive-but-reachable invariant can be discharged.
#       It is itself a proof obligation; whatever you assume here, prove separately.
#
# A `sat` step result is NOT automatically a bug — it means "not 1-inductive."
# Read the counterexample: an unguarded write past a bound (the buffer overrun)
# is a real bug; a latch bank that climbs in step-order is sound but needs the
# ordering lemma. The four compiler latch banks (z3ctx/z3sorts/z3nums) are the
# latter — runtime safety nets re-checked each tick, provable with the lemma.

set -euo pipefail
cd "$(dirname "$0")/.."
FIX="$1"; CLAIM="$2"; PFX="$3"
# Optional 4th arg: a file of EXTRA carry-state asserts (an auxiliary inductive
# lemma over `_`-fields, e.g. the latch step↔field correspondence). Added to the
# inductive-step hypothesis ONLY. Use it to discharge an invariant that holds on
# reachable states but isn't 1-inductive on its own (the latch-bank case). It is
# itself a proof obligation — anything you assume here you must separately prove.
STRENGTHEN="${4:-}"
ORACLE="${EVIDENT_ORACLE:-/usr/local/bin/evident-oracle}"

FLAT="$(mktemp -t pi-flat.XXXXXX.ev)"
SMT="$(mktemp -t pi-smt.XXXXXX.smt2)"
scripts/flatten-evident.sh "$FIX" > "$FLAT" 2>/dev/null \
  || { echo "FATAL: flatten failed"; exit 1; }
"$ORACLE" emit "$FLAT" "$CLAIM" -o "$SMT" 2>/dev/null \
  || { echo "FATAL: emit failed"; exit 1; }

# Reflow: the oracle emits multi-line asserts (the `⇒` type invariant, every
# `let` body). Single-line extraction truncates them. Normalize each top-level
# S-expr onto one logical line — string- and comment-aware so a `(` inside a
# string literal or a `;` line comment never miscounts parens. `;;` manifest
# lines (at depth 0) pass through verbatim.
NORM="$(mktemp -t pi-norm.XXXXXX.smt2)"
awk '
  BEGIN { depth=0; buf=""; instr=0 }
  (depth==0 && buf=="" && /^;;/) { print; next }
  {
    out=""; n=length($0)
    for (i=1;i<=n;i++) {
      c=substr($0,i,1)
      if (instr) { out=out c; if (c=="\"") instr=0; continue }
      if (c==";") break
      if (c=="\"") { instr=1; out=out c; continue }
      if (c=="(") depth++
      if (c==")") depth--
      out=out c
    }
    gsub(/[ \t]+/," ",out)
    if (out ~ /[^ ]/) { if (buf=="") buf=out; else buf=buf " " out }
    if (depth<=0 && buf ~ /[^ ]/) {
      sub(/^ /,"",buf); sub(/ $/,"",buf); print buf; buf=""; depth=0
    }
  }
' "$SMT" > "$NORM"

# Carried field consts = declared `<prefix><field>` (current state). Exclude
# the `_`-carries themselves and `__len` duals.
mapfile -t FIELDS < <(grep -oE "\(declare-fun ${PFX}[A-Za-z0-9_]+ " "$NORM" \
  | awk '{print $2}' | grep -vE "^_|__len$" | sort -u)
[ "${#FIELDS[@]}" -gt 0 ] || { echo "no field consts match prefix '$PFX'"; exit 1; }
FIELD_RE="$(printf '%s|' "${FIELDS[@]}")"; FIELD_RE="${FIELD_RE%|}"

# Invariant asserts: a single-line `(assert …)` whose ONLY identifiers are the
# carried field consts (+ their `_` carries) and the SMT logical keywords. This
# excludes the transition (`ite`/`is_first_tick`), the effects (`select`/`Exit`/
# `effects`), and any multi-line `let` assert — all of which name other consts.
ALLOWED="$(printf '%s\n' and or not true false xor distinct "${FIELDS[@]}" "${FIELDS[@]/#/_}")"
mapfile -t INV_ASSERTS < <(
  grep -E "^\(assert .*\)$" "$NORM" | grep -E "($FIELD_RE)" | while IFS= read -r line; do
    # all identifier tokens after the leading `assert`
    toks="$(printf '%s' "${line#\(assert }" | grep -oE '[A-Za-z_][A-Za-z0-9_]*')"
    ok=1
    while IFS= read -r t; do
      [ -z "$t" ] && continue
      grep -qxF "$t" <<<"$ALLOWED" || { ok=0; break; }
    done <<<"$toks"
    [ "$ok" = 1 ] && printf '%s\n' "$line"
  done
)
[ "${#INV_ASSERTS[@]}" -gt 0 ] || { echo "no invariant asserts found for '$PFX'"; exit 1; }

# Build I(current) = conjunction of the invariant predicates, and I(carry) by
# prepending `_` to each field const (word-boundary safe).
I_CUR=""
for a in "${INV_ASSERTS[@]}"; do
  p="${a#\(assert }"; p="${p%\)}"            # strip "(assert " ... ")"
  I_CUR="$I_CUR $p"
done
I_CUR="(and$I_CUR)"
I_CARRY="$I_CUR"
for C in "${FIELDS[@]}"; do
  I_CARRY="$(printf '%s' "$I_CARRY" | perl -pe "s/(?<![A-Za-z0-9_])\Q$C\E(?![A-Za-z0-9_])/_$C/g")"
done

echo "fixture : $FIX  (claim $CLAIM)"
echo "fields  : ${FIELDS[*]}"
echo "invariant I(x)   = $I_CUR"
echo "invariant I(_x)  = $I_CARRY"
echo

# Body without the manifest comments and without the current-state invariant
# asserts (so I(x) is the GOAL, not an assumption).
BODY="$(mktemp -t pi-body.XXXXXX.smt2)"
grep -vE "^;;" "$NORM" | grep -vFf <(printf '%s\n' "${INV_ASSERTS[@]}") > "$BODY"

# FULL = the transition WITH the invariant kept on x — exactly what the kernel
# solves each tick. Used by the totality/stuck check below.
FULL="$(mktemp -t pi-full.XXXXXX.smt2)"
grep -vE "^;;" "$NORM" > "$FULL"

LAST=""  # result of the most recent run() — "sat" / "unsat" / other
run() {  # <label> <extra-asserts-file>
  local label="$1" extra="$2" q
  q="$(mktemp -t pi-q.XXXXXX.smt2)"
  { cat "$BODY"; cat "$extra"; echo '(check-sat)'; } > "$q"
  LAST="$(z3 -smt2 "$q" 2>&1 | head -1)"
  printf '  %-9s %s\n' "$label:" "$LAST"
  rm -f "$q"
}

# base: holds at tick 0
B="$(mktemp)"; { echo '(assert is_first_tick)'; echo "(assert (not $I_CUR))"; } > "$B"
echo "── base case (holds at init): expect unsat ──"; run "base" "$B"

# step: transition preserves it (the induction). With a strengthening file, its
# asserts join the carry-state hypothesis (the auxiliary inductive lemma).
S="$(mktemp)"
{ echo '(assert (not is_first_tick))'; echo "(assert $I_CARRY)"
  [ -n "$STRENGTHEN" ] && { echo ";; --- strengthening lemma ($STRENGTHEN) ---"; cat "$STRENGTHEN"; }
  echo "(assert (not $I_CUR))"
} > "$S"
[ -n "$STRENGTHEN" ] && echo "strengthen: $STRENGTHEN (added to carry hypothesis)"
echo "── inductive step (preservation): unsat = PROVEN, sat = counterexample ──"; run "step" "$S"
# Only when the step is SAT is there a model to read — a `(get-value)` after an
# unsat check-sat errors ("model is not available") and z3 exits nonzero.
if [ "$LAST" = sat ]; then
# show the breaking carry-state
{ cat "$BODY"; cat "$S"; echo '(check-sat)'; echo "(get-value ($(printf '_%s ' "${FIELDS[@]}")))"; } > "${S}.cex.smt2"
CEX="$(z3 -smt2 "${S}.cex.smt2" 2>&1)"
if printf '%s' "$CEX" | head -1 | grep -q '^sat'; then
  echo "    counterexample carry-state: $(printf '%s' "$CEX" | tail -n +2 | tr -d '\n')"
  # Totality / stuck discriminator (v2). A `sat` step means "not 1-inductive",
  # which has TWO causes the tool can now tell apart automatically. Pin the
  # counterexample's record fields and ask whether the FULL body (transition WITH
  # the invariant on x — what the kernel actually solves each tick) admits ANY
  # successor (the non-field carries, e.g. `_step`, stay FREE):
  #   UNSAT = the field values FORCE a no-valid-successor state → the kernel's
  #           exit-2 "stuck" overrun. A REAL bug (the unguarded write past a bound).
  #   SAT   = a successor satisfying the invariant exists → the violation was
  #           1-induction incompleteness over an unreachable carry. Sound on
  #           reachable states; supply an ordering/monotonicity lemma (4th arg).
  PINS="$(printf '%s' "$CEX" | tail -n +2 \
    | grep -oE '\(_[A-Za-z0-9_]+ [^()]+\)' \
    | sed -E 's/\((_[A-Za-z0-9_]+) ([^()]+)\)/(assert (= \1 \2))/')"
  { cat "$FULL"; echo '(assert (not is_first_tick))'; printf '%s\n' "$PINS"; echo '(check-sat)'; } > "${S}.tot.smt2"
  TOT="$(z3 -smt2 "${S}.tot.smt2" 2>&1 | head -1)"
  if [ "$TOT" = unsat ]; then
    echo "    totality:  STUCK — pinned carry forces NO valid successor (kernel exit-2 overrun) ⇒ REAL BUG"
  elif [ "$TOT" = sat ]; then
    echo "    totality:  has-successor — reachable-but-not-1-inductive ⇒ supply an ordering/monotonicity lemma (4th arg)"
  else
    echo "    totality:  inconclusive ($TOT)"
  fi
  rm -f "${S}.tot.smt2"
fi
rm -f "${S}.cex.smt2"
fi

# The totality check above is the per-counterexample form: pin the found `_x`,
# keep the full body, and UNSAT confirms it's a genuine no-successor (exit-2)
# state. The fully general "∃ ANY invariant-state with no successor" is a
# quantifier-alternation query (∃_x ∀x ¬body) — heavier; the per-cex form covers
# the cases the step check actually surfaces.

rm -f "$FLAT" "$SMT" "$NORM" "$BODY" "$FULL" "$B" "$S" "${S}.cex.smt2"
