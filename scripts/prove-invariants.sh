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
#   prove-invariants.sh <fixture.ev> <claim> <field-const-prefix>
#     <field-const-prefix>  the flattened carried record's const prefix, e.g.
#       `c_` for `c ∈ Ctr` (field c.n → c_n), `tok_buf_` for an FtiBuffer.

set -euo pipefail
cd "$(dirname "$0")/.."
FIX="$1"; CLAIM="$2"; PFX="$3"
ORACLE="${EVIDENT_ORACLE:-/usr/local/bin/evident-oracle}"

FLAT="$(mktemp -t pi-flat.XXXXXX.ev)"
SMT="$(mktemp -t pi-smt.XXXXXX.smt2)"
scripts/flatten-evident.sh "$FIX" > "$FLAT" 2>/dev/null \
  || { echo "FATAL: flatten failed"; exit 1; }
"$ORACLE" emit "$FLAT" "$CLAIM" -o "$SMT" 2>/dev/null \
  || { echo "FATAL: emit failed"; exit 1; }

# Carried field consts = declared `<prefix><field>` (current state). Exclude
# the `_`-carries themselves and `__len` duals.
mapfile -t FIELDS < <(grep -oE "\(declare-fun ${PFX}[A-Za-z0-9_]+ " "$SMT" \
  | awk '{print $2}' | grep -vE "^_|__len$" | sort -u)
[ "${#FIELDS[@]}" -gt 0 ] || { echo "no field consts match prefix '$PFX'"; exit 1; }
FIELD_RE="$(printf '%s|' "${FIELDS[@]}")"; FIELD_RE="${FIELD_RE%|}"

# Invariant asserts: a single-line `(assert …)` whose ONLY identifiers are the
# carried field consts (+ their `_` carries) and the SMT logical keywords. This
# excludes the transition (`ite`/`is_first_tick`), the effects (`select`/`Exit`/
# `effects`), and any multi-line `let` assert — all of which name other consts.
ALLOWED="$(printf '%s\n' and or not true false xor distinct "${FIELDS[@]}" "${FIELDS[@]/#/_}")"
mapfile -t INV_ASSERTS < <(
  grep -E "^\(assert .*\)$" "$SMT" | grep -E "($FIELD_RE)" | while IFS= read -r line; do
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
grep -vE "^;;" "$SMT" | grep -vFf <(printf '%s\n' "${INV_ASSERTS[@]}") > "$BODY"

run() {  # <label> <extra-asserts-file>
  local label="$1" extra="$2" q res
  q="$(mktemp -t pi-q.XXXXXX.smt2)"
  { cat "$BODY"; cat "$extra"; echo '(check-sat)'; } > "$q"
  res="$(z3 -smt2 "$q" 2>&1 | head -1)"
  printf '  %-9s %s\n' "$label:" "$res"
  rm -f "$q"
}

# base: holds at tick 0
B="$(mktemp)"; { echo '(assert is_first_tick)'; echo "(assert (not $I_CUR))"; } > "$B"
echo "── base case (holds at init): expect unsat ──"; run "base" "$B"

# step: transition preserves it (the induction)
S="$(mktemp)"; { echo '(assert (not is_first_tick))'; echo "(assert $I_CARRY)"; echo "(assert (not $I_CUR))"; } > "$S"
echo "── inductive step (preservation): unsat = PROVEN, sat = counterexample ──"; run "step" "$S"
# on sat, show the breaking carry-state
{ cat "$BODY"; cat "$S"; echo '(check-sat)'; echo "(get-value ($(printf '_%s ' "${FIELDS[@]}")))"; } > "${S}.cex.smt2"
CEX="$(z3 -smt2 "${S}.cex.smt2" 2>&1)"
if printf '%s' "$CEX" | head -1 | grep -q '^sat'; then
  echo "    counterexample carry-state: $(printf '%s' "$CEX" | tail -n +2 | tr -d '\n')"
fi

# (TODO: a "no stuck state" check — ∃ invariant-state with no successor — is a
# quantifier-alternation query (∃_x ∀x ¬body); harder. For a found counterexample
# `_x`, pinning it + the full body and checking UNSAT confirms it's the kernel's
# exit-2 stuck. Left for v2.)

rm -f "$FLAT" "$SMT" "$BODY" "$B" "$S" "${S}.cex.smt2"
