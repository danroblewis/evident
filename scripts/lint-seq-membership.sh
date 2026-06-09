#!/usr/bin/env bash
# TODO: rewrite in Evident
#
# lint-seq-membership.sh — reject `x ∈ <seq>` (Seq membership), which the
# frozen oracle SILENTLY DROPS (no message, exit 0; the constraint just
# vanishes and the claim goes vacuously SAT — see
# tests/seq/11_membership_direct_GAP.ev and docs/seq-bounded-catalog.md).
# This turns that silent footgun into a loud compile error until Seq
# membership is properly lowered.
#
# Narrow by design. It flags a line that USES a Seq-typed variable on the
# RIGHT of `∈` (membership `expr ∈ seqvar`) and is NOT a quantifier binder
# (`∀ … ∈ seqvar` / `∃ … ∈ seqvar`, which the oracle supports). It does not
# touch `∈ Set(…)`, `∈ Int`/type decls, `∈ {lo..hi}` ranges, `#xs`, `xs[i]`,
# `coindexed(xs,…)`, or `edges(xs)`. Multi-name `a, b ∈ Seq` only registers
# `b` (a safe miss, never a false positive).
#
# Reads FLATTENED Evident source (so all decls are present) from a file arg
# or stdin. Exit: 0 = clean, 1 = a Seq-membership use found.
#
# Implementation note: the oracle's awk is busybox/mawk in the C locale,
# which infinite-loops on a `match()` walk over the 3-byte `∈`. So we
# ASCII-ize ∈/∀/∃ with sed first and keep every awk regex pure-ASCII.

set -u
SRC="${1:-/dev/stdin}"
F="${1:-<stdin>}"

# strip `-- …` comments FIRST (their code examples like `"lit" ∈ s` and
# decls quoted in prose like `s ∈ Seq(Int)` must not register or flag),
# then ASCII-ize the operators.
sed -e 's/--.*//' -e 's/∈/ @IN@ /g' -e 's/∀/@FA@/g' -e 's/∃/@EX@/g' "$SRC" | awk -v F="$F" '
  # ── pass 1: collect Seq-typed variable names ───────────────────────
  # a `… <name> @IN@ Seq(…)` decl: the var is the identifier just before
  # `@IN@ Seq(`. Matching only that token avoids registering type names
  # (`Int`) from multi-param signatures.
  {
    line=$0; s=line
    while (match(s, /[A-Za-z_][A-Za-z0-9_]*[ ]+@IN@[ ]+Seq\(/)) {
      m=substr(s, RSTART, RLENGTH); sub(/[ ]+@IN@.*/, "", m); SEQ[m]=1
      s=substr(s, RSTART+RLENGTH)
    }
    L[NR]=line
  }
  END {
    # ── pass 2: flag membership USES of a Seq var ──────────────────────
    bad=0
    for (n=1; n<=NR; n++) {
      line=L[n]
      if (line ~ /@FA@/ || line ~ /@EX@/) continue       # quantifiers are supported
      s=line
      while (match(s, /@IN@[ ]+[A-Za-z_][A-Za-z0-9_]*/)) {
        tok=substr(s, RSTART, RLENGTH); sub(/@IN@[ ]+/, "", tok)
        if (tok in SEQ) {
          ls=line; gsub(/@IN@/, "∈", ls); gsub(/^[ ]+/, "", ls)
          printf("lint-seq-membership: %s:%d: Seq membership `∈ %s` is SILENTLY DROPPED by the oracle.\n", F, n, tok) > "/dev/stderr"
          printf("    %s\n", ls) > "/dev/stderr"
          printf("    use:  ∃ i ∈ {0..#%s-1} : %s[i] = <x>\n", tok, tok) > "/dev/stderr"
          bad=1
        }
        s=substr(s, RSTART+RLENGTH)
      }
    }
    exit bad
  }
'
