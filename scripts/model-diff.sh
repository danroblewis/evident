#!/usr/bin/env bash
# TODO: rewrite in Evident
#
# model-diff.sh — SOLUTION-SPACE comparison for two Evident claims. Given two
# `.ev` claims over a shared interface (a claim and its refactor), report whether
# their satisfying-assignment SETS are the same over that interface — and when
# they differ, print a concrete WITNESS of how. This is the regression oracle for
# a semantics-preserving refactor: the predicates may change; the solution space
# (over inputs + observed outputs) must not, except where the change is intended.
#
# THE TECHNIQUE (namespace surgery on emitted SMT) ----------------------------
# An emitted Evident claim is a Z3 formula over consts. Three roles:
#   INPUTS/givens   — `is_first_tick`, the `_x` carries, `last_results[*]`, plus
#                     any const the user names with --inputs. The kernel supplies
#                     these per-tick; they parametrize the solution space.
#   OUTPUTS/observed— `effects`, `effects__len`, the top-level state memberships
#                     (manifest `state-fields`), plus any --observe const. What we
#                     compare. (A state-field that is really a free input should be
#                     moved with --inputs.)
#   INTERNAL        — everything else (helper vars, scaffolding). Hidden.
#
# We compare A and B by suffixing per-side EVERYTHING that is not a shared input:
#   SHARED inputs  → keep the canonical name in BOTH bodies (same const). A
#                    renamed input is mapped back to canonical with --map a=b.
#   outputs+internals → A's get `__a`, B's get `__b`, so they never collide.
# Word-boundary-safe substitution (whole-token, like prove-invariants' `_`-carry
# rename). An injectivity check asserts no two consts collapsed onto one name.
# Concatenate A-body and B-body: they now share ONLY the canonical inputs.
#
# v1 — OBSERVATIONAL (functional) equivalence. The tractable core; covers the
#   compiler, where outputs are determined by inputs. Assert both bodies, then
#   assert the observed outputs DIFFER:
#       (or (distinct out__a out__b) (distinct effects__a effects__b) …)
#   UNSAT = outputs agree for every input  = SAME solution space over the iface.
#   SAT   = a witness input where they diverge; we get-value the inputs + both
#           sides' outputs and print the divergence.
#
# v2 — DIRECTIONAL set-difference. The general predicate case (no clean in/out
#   split). A∖B nonempty  ⟺  body_A(σ,intA) ∧ ¬∃intB body_B(σ,intB), handed to
#   Z3 as a quantified formula (the internals of the OTHER side existentially
#   eliminated under a `forall … (not body)`). Run BOTH directions:
#       A∖B empty ∧ B∖A empty → EQUIVALENT
#       A∖B empty ∧ B∖A nonempty → A ⊊ B (A refines B / B relaxes A)
#       A∖B nonempty ∧ B∖A empty → A ⊋ B
#       both nonempty → OVERLAP (each has solutions the other lacks)
#   Each non-empty difference comes with an interface witness. The quantified
#   query is heavier; each Z3 call is timeboxed (default 30s) and a timeout is
#   reported as inconclusive (rc 2), never a false verdict.
#
# Usage:
#   model-diff.sh <a.ev> <b.ev> <claim> [opts]
#     --inputs  v,…   extra consts to treat as SHARED inputs (move a free
#                     state-field here). Default: is_first_tick, _*-carries,
#                     last_results, last_results__len.
#     --observe w,…   OVERRIDE the observed-output set (default: effects,
#                     effects__len + all manifest state-fields not in --inputs).
#     --map a=b,…     correspondence map: B's const `b` is A's input `a`
#                     (rename b→a in B before suffixing).
#     --v1-only       skip v2 (directional). v1 is the load-bearing check.
#     --v2-only       skip v1.
#     --timeout SEC   per-Z3-query timebox (default 30).
#
# Exit code:  0 = EQUIVALENT   1 = DIFFER (any direction)   2 = error/timeout.

set -uo pipefail
cd "$(dirname "$0")/.."

ORACLE="${EVIDENT_ORACLE:-/usr/local/bin/evident-oracle}"
Z3="${Z3:-z3}"

die() { echo "model-diff: $*" >&2; exit 2; }

# ── args ─────────────────────────────────────────────────────────────────────
A_EV=""; B_EV=""; CLAIM=""
USER_INPUTS=""; OBSERVE_OVERRIDE=""; MAP=""
DO_V1=1; DO_V2=1; TIMEOUT=30
pos=()
while [ $# -gt 0 ]; do
  case "$1" in
    --inputs)   USER_INPUTS="$2"; shift 2;;
    --observe)  OBSERVE_OVERRIDE="$2"; shift 2;;
    --map)      MAP="$2"; shift 2;;
    --v1-only)  DO_V2=0; shift;;
    --v2-only)  DO_V1=0; shift;;
    --timeout)  TIMEOUT="$2"; shift 2;;
    -*)         die "unknown flag $1";;
    *)          pos+=("$1"); shift;;
  esac
done
[ "${#pos[@]}" -eq 3 ] || die "usage: model-diff.sh <a.ev> <b.ev> <claim> [opts]"
A_EV="${pos[0]}"; B_EV="${pos[1]}"; CLAIM="${pos[2]}"
[ -f "$A_EV" ] || die "no such file: $A_EV"
[ -f "$B_EV" ] || die "no such file: $B_EV"

TMP="$(mktemp -d -t modeldiff.XXXXXX)"
trap 'rm -rf "$TMP"' EXIT

# ── flatten + emit ───────────────────────────────────────────────────────────
emit() {  # <file.ev> <out.smt2>
  local ev="$1" out="$2" flat
  flat="$TMP/$(basename "$ev").flat.ev"
  scripts/flatten-evident.sh "$ev" > "$flat" 2>/dev/null \
    || die "flatten failed: $ev"
  "$ORACLE" emit "$flat" "$CLAIM" -o "$out" 2>"$TMP/emit.err" \
    || { sed 's/^/  /' "$TMP/emit.err" >&2; die "emit failed: $ev"; }
}
emit "$A_EV" "$TMP/a.smt2"
emit "$B_EV" "$TMP/b.smt2"

# ── reflow (verbatim from prove-invariants.sh) ───────────────────────────────
# Collapse each top-level S-expr onto one logical line, string/comment-aware so a
# `(` inside a string literal or a `;` line comment never miscounts parens.
reflow() {  # <in.smt2> <out>
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
  ' "$1" > "$2"
}
reflow "$TMP/a.smt2" "$TMP/a.norm"
reflow "$TMP/b.smt2" "$TMP/b.norm"

# ── split a reflowed model into declares / asserts / manifest-state-fields ────
# The oracle dumps the datatype+declare preamble TWICE; dedup declares (keep
# first). Asserts are the body. Manifest `state-fields` lists the observable
# top-level memberships.
state_fields() {  # <norm>  → "name name …" (drops the :Type tags)
  grep -E '^;; manifest: state-fields =' "$1" | head -1 \
    | sed -E 's/^;; manifest: state-fields = //' \
    | tr ' ' '\n' | sed -E 's/:.*$//' | grep -E '.' | sort -u | tr '\n' ' '
}
asserts() { grep -E '^\(assert ' "$1"; }

A_SF="$(state_fields "$TMP/a.norm")"
B_SF="$(state_fields "$TMP/b.norm")"

# ── const inventory (declare-fun names; datatypes/sorts are shared, untouched) ─
fun_consts() { grep -E '^\(declare-fun ' "$1" | awk '{print $2}' | sort -u; }
mapfile -t A_CONSTS < <(fun_consts "$TMP/a.norm")
mapfile -t B_CONSTS < <(fun_consts "$TMP/b.norm")

# ── classify ─────────────────────────────────────────────────────────────────
# INPUTS (shared, canonical): is_first_tick, last_results, last_results__len, the
# `_`-carries, plus user --inputs.
is_default_input() {  # <name>
  case "$1" in
    is_first_tick|last_results|last_results__len) return 0;;
    _*) return 0;;
  esac
  return 1
}
declare -A IS_INPUT=()
for c in "${A_CONSTS[@]}" "${B_CONSTS[@]}"; do
  is_default_input "$c" && IS_INPUT["$c"]=1
done
IFS=',' read -ra _ui <<< "$USER_INPUTS"
for c in "${_ui[@]}"; do [ -n "$c" ] && IS_INPUT["$c"]=1; done

# Correspondence map: B-name=A-name (rename in B so it's the canonical input).
declare -A B2A=()
IFS=',' read -ra _maps <<< "$MAP"
for m in "${_maps[@]}"; do
  [ -z "$m" ] && continue
  a="${m%%=*}"; b="${m#*=}"
  [ "$a" = "$m" ] && die "bad --map entry '$m' (want a=b)"
  B2A["$b"]="$a"
  IS_INPUT["$a"]=1
done

# OBSERVED outputs: override, or default (effects, effects__len, state-fields ∉ inputs).
declare -A IS_OBSERVE=()
if [ -n "$OBSERVE_OVERRIDE" ]; then
  IFS=',' read -ra _ov <<< "$OBSERVE_OVERRIDE"
  for c in "${_ov[@]}"; do [ -n "$c" ] && IS_OBSERVE["$c"]=1; done
else
  for c in effects effects__len; do IS_OBSERVE["$c"]=1; done
  for c in $A_SF $B_SF; do
    [ -n "${IS_INPUT[$c]:-}" ] && continue
    IS_OBSERVE["$c"]=1
  done
fi

# The observed set must exist on BOTH sides (so distinctness is well-typed). Keep
# only consts that appear (post-map) in both inventories.
declare -A A_HAS=(); for c in "${A_CONSTS[@]}"; do A_HAS["$c"]=1; done
declare -A B_HAS=(); for c in "${B_CONSTS[@]}"; do B_HAS["$c"]=1; done
b_has_canonical() {  # <canonical-name> : present in B either directly or via reverse map
  [ -n "${B_HAS[$1]:-}" ] && return 0
  for b in "${!B2A[@]}"; do [ "${B2A[$b]}" = "$1" ] && [ -n "${B_HAS[$b]:-}" ] && return 0; done
  return 1
}
OBSERVE_SHARED=()
for c in "${!IS_OBSERVE[@]}"; do
  if [ -n "${A_HAS[$c]:-}" ] && b_has_canonical "$c"; then OBSERVE_SHARED+=("$c"); fi
done
[ "${#OBSERVE_SHARED[@]}" -gt 0 ] || die "no observed outputs present in both models (try --observe)"
IFS=$'\n' OBSERVE_SHARED=($(sort <<<"${OBSERVE_SHARED[*]}")); unset IFS

# ── word-boundary-safe rename of a body ──────────────────────────────────────
# Renames a set of whole-token consts via perl (whole-token, longest-first so a
# substring const never clobbers a longer one). Pairs: "old=new" lines on stdin.
rename_tokens() {  # <body-file> <pairs-file>  → stdout
  local body="$1" pairs="$2"
  perl -e '
    my ($bf,$pf)=@ARGV;
    open(P,"<",$pf) or die; my @pairs;
    while(<P>){ chomp; next unless /\S/; my ($o,$n)=split(/=/,$_,2); push @pairs,[$o,$n]; }
    # longest old-name first → no substring shadowing
    @pairs = sort { length($b->[0]) <=> length($a->[0]) } @pairs;
    open(B,"<",$bf) or die; local $/; my $s=<B>;
    for my $p (@pairs){
      my ($o,$n)=@$p;
      my $qo=quotemeta($o);
      $s =~ s/(?<![A-Za-z0-9_])$qo(?![A-Za-z0-9_])/$n/g;
    }
    print $s;
  ' "$body" "$pairs"
}

# Build the suffix/rename plan for one side.
#   side="a"/"b". Inputs keep canonical names (after applying B→A map on side b).
#   Everything else (a declare-fun const that is NOT a shared input) → name__SIDE.
build_side() {  # <side> <norm> <pairs-out>
  local side="$1" norm="$2" out="$3"
  : > "$out"
  local c canon
  for c in $(fun_consts "$norm"); do
    canon="$c"
    if [ "$side" = b ] && [ -n "${B2A[$c]:-}" ]; then canon="${B2A[$c]}"; fi
    if [ -n "${IS_INPUT[$canon]:-}" ]; then
      [ "$c" != "$canon" ] && echo "$c=$canon" >> "$out"   # map B-rename to canonical
    else
      echo "$c=${c}__${side}" >> "$out"
    fi
  done
}
build_side a "$TMP/a.norm" "$TMP/a.pairs"
build_side b "$TMP/b.norm" "$TMP/b.pairs"

# Injectivity check: no two distinct source consts may map to the same target.
inj_check() {  # <pairs>
  awk -F= '{ if ($2 in seen && seen[$2]!=$1){ print "COLLISION "$1" "$2; ec=1 } seen[$2]=$1 } END{ exit ec }' "$1" \
    || die "rename collision in $1 (two consts collapsed onto one name)"
}
inj_check "$TMP/a.pairs"
inj_check "$TMP/b.pairs"

# ── emit suffixed declares + asserts per side ────────────────────────────────
# Shared (datatype/sort) declares come from A once. Per-side declare-fun + asserts
# are renamed. Inputs are declared ONCE (shared) from A's side.
SHARED_DECLS="$TMP/shared.decls"
grep -E '^\((declare-datatypes|declare-sort) ' "$TMP/a.norm" | awk '!seen[$0]++' > "$SHARED_DECLS"

# input declares (canonical), taken from A (they exist there; carries always do)
INPUT_DECLS="$TMP/input.decls"
: > "$INPUT_DECLS"
grep -E '^\(declare-fun ' "$TMP/a.norm" | awk '!seen[$0]++' | while read -r line; do
  nm="$(awk '{print $2}' <<<"$line")"
  [ -n "${IS_INPUT[$nm]:-}" ] && echo "$line"
done >> "$INPUT_DECLS"
# any canonical input present only in B (after map) — add its declare too
grep -E '^\(declare-fun ' "$TMP/b.norm" | awk '!seen[$0]++' | while read -r line; do
  nm="$(awk '{print $2}' <<<"$line")"
  canon="$nm"; [ -n "${B2A[$nm]:-}" ] && canon="${B2A[$nm]}"
  if [ -n "${IS_INPUT[$canon]:-}" ] && ! grep -qE "^\(declare-fun $canon " "$INPUT_DECLS"; then
    echo "$line" | sed -E "s/^\(declare-fun $nm /(declare-fun $canon /"
  fi
done >> "$INPUT_DECLS"

# per-side declare-fun (non-input) + asserts, renamed
side_body() {  # <side> <norm> <pairs>  → stdout (renamed declares-of-nonInput + asserts)
  local side="$1" norm="$2" pairs="$3" raw
  raw="$TMP/$side.raw"
  { grep -E '^\(declare-fun ' "$norm" | awk '!seen[$0]++' | while read -r line; do
      nm="$(awk '{print $2}' <<<"$line")"
      canon="$nm"; [ "$side" = b ] && [ -n "${B2A[$nm]:-}" ] && canon="${B2A[$nm]}"
      [ -n "${IS_INPUT[$canon]:-}" ] && continue   # inputs declared once, shared
      echo "$line"
    done
    asserts "$norm" | awk '!seen[$0]++'
  } > "$raw"
  rename_tokens "$raw" "$pairs"
}
side_body a "$TMP/a.norm" "$TMP/a.pairs" > "$TMP/a.body"
side_body b "$TMP/b.norm" "$TMP/b.pairs" > "$TMP/b.body"

# Common preamble for any query.
PREAMBLE="$TMP/preamble"
{ cat "$SHARED_DECLS"; cat "$INPUT_DECLS"; } > "$PREAMBLE"

# Observed-output names per side (canonical → name__side).
obs_name() { echo "${1}__$2"; }   # observed are never inputs ⇒ always suffixed

# max-effects from the manifest (Array index bound for element-wise compares).
MAXEFF="$(grep -E '^;; manifest: max-effects =' "$TMP/a.norm" | head -1 | sed -E 's/.*= //' | tr -dc '0-9')"
[ -n "$MAXEFF" ] || MAXEFF=16

# Is this observed const Array-typed?  (effects/last_results lower to (Array …)).
# An Array output must be compared element-wise over its live prefix, NOT by
# whole-array `distinct` — the tail past `__len` is unconstrained, so a naive
# distinct is ALWAYS sat (spurious "differ"). We compare its `__len` (a separate
# scalar observable) plus elements 0..len-1.
is_array_obs() {  # <canon>
  grep -qE "^\(declare-fun $1 \(\) \(Array " "$TMP/a.norm"
}

# The "this observed output differs between a and b" predicate.
differ_pred() {  # <canon>  → SMT bool expr
  local c="$1" a b
  a="$(obs_name "$c" a)"; b="$(obs_name "$c" b)"
  if is_array_obs "$c"; then
    # length differs, or some live element differs. The live length is the
    # canonical `${c}__len` companion (itself an observed scalar, suffixed too).
    local la="${c}__len__a" lb="${c}__len__b" i terms=""
    for ((i=0; i<MAXEFF; i++)); do
      # element i matters only while i < that side's len; compare under (i<len_a ∨ i<len_b)
      terms="$terms (and (or (< $i $la) (< $i $lb)) (distinct (select $a $i) (select $b $i)))"
    done
    echo "(or (distinct $la $lb)$terms)"
  else
    echo "(distinct $a $b)"
  fi
}

# Pretty-printer for a witness: get-value over inputs + both sides' observed.
WITNESS_VARS=()
for c in "${!IS_INPUT[@]}"; do
  # only inputs that actually appear; skip last_results array (unwieldy) but keep its len
  case "$c" in last_results) continue;; esac
  WITNESS_VARS+=("$c")
done
# Witness observables: scalars get-value cleanly; raw Arrays print verbosely, so
# we show their `__len` (a scalar observable) instead and, for divergence
# legibility, the live element at index 0 of each side (`(select eff 0)`).
WITNESS_OBS=()
for c in "${OBSERVE_SHARED[@]}"; do
  if is_array_obs "$c"; then
    WITNESS_OBS+=("(select $(obs_name "$c" a) 0)" "(select $(obs_name "$c" b) 0)")
  else
    WITNESS_OBS+=("$(obs_name "$c" a)" "$(obs_name "$c" b)")
  fi
done

run_z3() {  # <query-file>  → prints first line to stdout, full model to $TMP/z3.out
  $Z3 -smt2 -T:"$TIMEOUT" "$1" > "$TMP/z3.out" 2>&1
  head -1 "$TMP/z3.out"
}

# Pretty-print the (get-value …) blocks from a z3 output: one binding per line.
# Each get-value response is `((b1)(b2)…)`; we split into the inner `(name val)`
# bindings (balanced-paren aware, string-safe) and indent one per line.
print_witness() {  # reads z3 output on stdin
  perl -0777 -ne '
    s/^(sat|unsat|unknown)\s*//;            # drop the check-sat verdict line(s)
    s/\b(sat|unsat|unknown)\b\s*//g if 0;
    my @out; my $d=0; my $cur=""; my $instr=0;
    for my $ch (split //, $_) {
      if ($instr) { $cur.=$ch; $instr=0 if $ch eq "\""; next }
      if ($ch eq "\"") { $instr=1; $cur.=$ch; next }
      if ($ch eq "(") { $d++; if($d==2){$cur="("; next} }
      if ($ch eq ")") { $d--; if($d==1){$cur.=")"; push @out,$cur; $cur=""; next} }
      $cur.=$ch if $d>=2;
    }
    for my $b (@out){ $b=~s/\s+/ /g; print "      $b\n"; }
  '
}

EXIT=0

echo "═══ model-diff: $A_EV  vs  $B_EV   (claim $CLAIM) ═══"
echo "inputs (shared) : $(printf '%s ' "${!IS_INPUT[@]}" | tr ' ' '\n' | sort | tr '\n' ' ')"
echo "observed        : ${OBSERVE_SHARED[*]}"
[ -n "$MAP" ] && echo "map (b→a)       : $MAP"
echo

# ─────────────────────────────────────────────────────────────────────────────
# v1 — observational equivalence
# ─────────────────────────────────────────────────────────────────────────────
V1_RESULT=""
if [ "$DO_V1" = 1 ]; then
  echo "── v1: observational (functional) equivalence ──"
  Q="$TMP/v1.smt2"
  {
    cat "$PREAMBLE"
    cat "$TMP/a.body"
    cat "$TMP/b.body"
    # outputs differ?
    diffs=""
    for c in "${OBSERVE_SHARED[@]}"; do
      diffs="$diffs $(differ_pred "$c")"
    done
    echo "(assert (or$diffs))"
    echo "(check-sat)"
    [ "${#WITNESS_VARS[@]}" -gt 0 ] && echo "(get-value ($(printf '%s ' "${WITNESS_VARS[@]}")))"
    [ "${#WITNESS_OBS[@]}" -gt 0 ]  && echo "(get-value ($(printf '%s ' "${WITNESS_OBS[@]}")))"
  } > "$Q"
  R="$(run_z3 "$Q")"
  case "$R" in
    unsat)
      echo "  unsat ⇒ outputs AGREE for every input  ⇒  EQUIVALENT (observational)"
      V1_RESULT="equiv";;
    sat)
      echo "  sat ⇒ found an input where the outputs DIVERGE (witness):"
      print_witness < "$TMP/z3.out"
      V1_RESULT="differ"; EXIT=1;;
    *)
      echo "  inconclusive: $R (timebox ${TIMEOUT}s)"
      head -3 "$TMP/z3.out" | sed 's/^/    /'
      V1_RESULT="inconclusive"; [ "$EXIT" = 0 ] && EXIT=2;;
  esac
  echo
fi

# ─────────────────────────────────────────────────────────────────────────────
# v2 — directional set-difference (A∖B, then B∖A)
# ─────────────────────────────────────────────────────────────────────────────
# A∖B nonempty ⟺  body_A ∧ ∀ intB. ¬body_B   (over shared inputs σ).
# We keep one side's full body (its internals free/top-level) and put the OTHER
# side's internals (its non-input declare-funs) under a universal
# `(forall (...) (not (and body_other)))`. The shared inputs + the kept side's
# outputs stay free, so a SAT yields a witness.
V2_RESULT=""
mk_dir_query() {  # <keepSide> <forallSide>  → query file path
  local keep="$1" fa="$2" q
  q="$TMP/v2_${keep}_${fa}.smt2"
  local fdecls; fdecls="$(grep -E '^\(declare-fun ' "$TMP/$fa.body")"
  local binders
  binders="$(printf '%s\n' "$fdecls" | sed -E 's/^\(declare-fun ([^ ]+) \(\) (.*)\)$/(\1 \2)/' | tr '\n' ' ')"
  local fbody
  fbody="$(grep -E '^\(assert ' "$TMP/$fa.body" | sed -E 's/^\(assert (.*)\)$/\1/' | tr '\n' ' ')"
  {
    cat "$PREAMBLE"
    cat "$TMP/$keep.body"
    if [ -n "$(printf '%s' "$binders" | tr -d ' ')" ]; then
      echo "(assert (forall ($binders) (not (and $fbody))))"
    else
      echo "(assert (not (and $fbody)))"
    fi
    echo "(check-sat)"
    [ "${#WITNESS_VARS[@]}" -gt 0 ] && echo "(get-value ($(printf '%s ' "${WITNESS_VARS[@]}")))"
    local ow=()
    for c in "${OBSERVE_SHARED[@]}"; do ow+=("$(obs_name "$c" "$keep")"); done
    [ "${#ow[@]}" -gt 0 ] && echo "(get-value ($(printf '%s ' "${ow[@]}")))"
  } > "$q"
  echo "$q"
}

if [ "$DO_V2" = 1 ]; then
  echo "── v2: directional set-difference (quantified) ──"
  QAB="$(mk_dir_query a b)"; RAB="$(run_z3 "$QAB")"
  AB_MODEL="$(cat "$TMP/z3.out")"
  QBA="$(mk_dir_query b a)"; RBA="$(run_z3 "$QBA")"
  BA_MODEL="$(cat "$TMP/z3.out")"

  echo "  A∖B : $RAB"
  [ "$RAB" = sat ] && { echo "    witness (in A, not B):"; print_witness <<<"$AB_MODEL"; }
  echo "  B∖A : $RBA"
  [ "$RBA" = sat ] && { echo "    witness (in B, not A):"; print_witness <<<"$BA_MODEL"; }

  case "$RAB,$RBA" in
    unsat,unsat) echo "  ⇒ EQUIVALENT (each side's solution set ⊆ the other)"; V2_RESULT="equiv";;
    unsat,sat)   echo "  ⇒ A ⊊ B   (A refines B; B has solutions A lacks)";     V2_RESULT="A_sub_B"; EXIT=1;;
    sat,unsat)   echo "  ⇒ A ⊋ B   (B refines A; A has solutions B lacks)";     V2_RESULT="A_sup_B"; EXIT=1;;
    sat,sat)     echo "  ⇒ OVERLAP  (each has solutions the other lacks)";        V2_RESULT="overlap"; EXIT=1;;
    *)           echo "  ⇒ inconclusive (A∖B=$RAB B∖A=$RBA; timebox ${TIMEOUT}s)"; V2_RESULT="inconclusive"; [ "$EXIT" = 0 ] && EXIT=2;;
  esac
  echo
fi

# ── final verdict ────────────────────────────────────────────────────────────
echo "═══ verdict ═══"
if [ "$DO_V1" = 1 ]; then echo "  v1 observational : ${V1_RESULT:-skipped}"; fi
if [ "$DO_V2" = 1 ]; then echo "  v2 directional   : ${V2_RESULT:-skipped}"; fi
case "$EXIT" in
  0) echo "  VERDICT: EQUIVALENT";;
  1) echo "  VERDICT: DIFFER";;
  2) echo "  VERDICT: INCONCLUSIVE / ERROR";;
esac
exit "$EXIT"
