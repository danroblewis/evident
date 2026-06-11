#!/usr/bin/env bash
# build-phi.sh OLD.smt2 NEW.smt2  > phi.txt
#
# Derive the variable mapping phi (old_name <-> new_name) between two stage1
# emits by diffing their top-level declare-fun names. Names present in BOTH are
# identity-mapped (not emitted). Names present only in OLD and only in NEW that
# form a clean bijection under a single rewrite rule are emitted as pairs.
#
# This is the HONEST discovery step: phi is read off the ACTUAL emitted const
# names, never assumed. If the residual old-only / new-only sets are not the
# same size, phi is NOT a clean rename and the script says so loudly (the
# experiment's soundness depends on phi being correct).
set -euo pipefail
OLD="$1"; NEW="$2"

names() { grep '^(declare-fun ' "$1" | sed -E 's/^\(declare-fun ([^ ]+) .*/\1/' | sort -u; }

names "$OLD" > /tmp/phi_old_names.txt
names "$NEW" > /tmp/phi_new_names.txt

comm -23 /tmp/phi_old_names.txt /tmp/phi_new_names.txt > /tmp/phi_only_old.txt
comm -13 /tmp/phi_old_names.txt /tmp/phi_new_names.txt > /tmp/phi_only_new.txt

no=$(wc -l < /tmp/phi_only_old.txt | tr -d ' ')
nn=$(wc -l < /tmp/phi_only_new.txt | tr -d ' ')
echo "# old-only consts: $no   new-only consts: $nn" >&2
if [ "$no" != "$nn" ]; then
  echo "# WARNING: old-only and new-only counts differ ($no vs $nn)." >&2
  echo "# phi is NOT a clean bijective rename; the experiment's soundness is at risk." >&2
fi

# Pair them. A carried-record rename turns a flat name `pfx_<field>` (and its
# dual `_pfx_<field>`) into `rec.<field>` / `_rec.<field>`. We DERIVE phi
# structurally, NOT by assuming the spelling: strip each name to its
# (is_dual, field) key — drop a leading `_`, then take the substring after the
# LAST `_` or `.` separator (the field) — and pair old↔new on equal keys. This
# discovers any single-level `<core>_field → <core>.field` rename. If the keys
# don't form a bijection the script says so loudly (soundness depends on it).
key() {  # name -> "<dual>\t<field>"
  local s="$1" dual=""
  case "$s" in _*) dual="_"; s="${s#_}";; esac
  # field = substring after the last '.' or '_'
  local field="${s##*.}"; field="${field##*_}"
  printf '%s\t%s' "$dual" "$field"
}

unmatched=0
declare -A NEWBYKEY
while IFS= read -r n; do
  NEWBYKEY["$(key "$n")"]="$n"
done < /tmp/phi_only_new.txt

while IFS= read -r o; do
  k="$(key "$o")"
  n="${NEWBYKEY[$k]:-}"
  if [ -n "$n" ]; then
    echo "$o $n"
    unset 'NEWBYKEY[$k]'
  else
    echo "# UNMATCHED old-only const (no phi image for key '$k'): $o" >&2
    unmatched=$((unmatched+1))
  fi
done < /tmp/phi_only_old.txt

leftover=${#NEWBYKEY[@]}
[ "$unmatched" = 0 ] || echo "# $unmatched old-only consts had no clean image — phi incomplete." >&2
[ "$leftover" = 0 ]  || echo "# $leftover new-only consts left unpaired — phi not a bijection." >&2
