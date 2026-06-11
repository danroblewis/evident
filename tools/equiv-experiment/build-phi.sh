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

# Pair them: each old-only X is mapped to the new-only name obtained by the
# qloop rewrite  qloop_<f> -> qloop.<f>  (and dual _qloop_<f> -> _qloop.<f>).
# We DERIVE the pair by applying that rewrite and checking the result is a
# real new-only name; anything that doesn't map cleanly is reported.
unmatched=0
while IFS= read -r o; do
  n="${o//qloop_/qloop.}"
  if grep -qxF "$n" /tmp/phi_only_new.txt; then
    echo "$o $n"
  else
    echo "# UNMATCHED old-only const (no phi image): $o" >&2
    unmatched=$((unmatched+1))
  fi
done < /tmp/phi_only_old.txt

[ "$unmatched" = 0 ] || echo "# $unmatched old-only consts had no clean image — phi incomplete." >&2
