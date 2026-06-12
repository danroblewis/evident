#!/usr/bin/env bash
# TODO: rewrite in Evident (compiler2/passes/ once the self-hosting seam
# exists — see docs/plans/post-cutover-roadmap.md "host passes in Evident")
#
# hoist-decls.sh — declaration-before-use ordering fix.
#
# Reads Evident source on stdin, writes reordered source to stdout. Runs LAST
# in flatten-evident.sh (after autocarry, flatten-body-records, and
# lower-bounded-seq), so it hoists declarations those passes generate too.
#
# WHY: the self-hosted compiler requires every name to be DECLARED before it is
# REFERENCED when it builds the Z3 AST. The `..`-lift module architecture
# (after flattening) can place a carry use ahead of its declaration in the
# stream (e.g. `_decl_action`, `_source`, `_user_enum_name` used in a module
# lifted before the module that declares it). The fix: within each fsm/claim
# BODY, move every pure bare declaration (`NAME ∈ TYPE` with no `=`) to the
# front of the body, preserving relative order. A bare decl introduces a name
# and consumes none, so hoisting the block is always order-safe — no toposort.

set -u -o pipefail

exec awk '
function is_blank(s)   { return s ~ /^[[:space:]]*$/ }
function indent_of(s,  m) { match(s, /^[[:space:]]*/); return substr(s, 1, RLENGTH) }

# A top-level construct starts at column 0 (no leading whitespace), non-blank.
function is_toplevel(s) { return s !~ /^[[:space:]]/ && !is_blank(s) }

# Net parenthesis depth change: (count "(") - (count ")").
function paren_delta(s,   d, k, ch) {
    d = 0
    for (k = 1; k <= length(s); k++) {
        ch = substr(s, k, 1)
        if (ch == "(") d++
        else if (ch == ")") d--
    }
    return d
}

# Is this an fsm/claim header opening line?  `fsm Name…` / `claim Name…`
# (type/schema are NOT targets).
function is_target_header(s) {
    return s ~ /^fsm[[:space:]]/ || s ~ /^claim[[:space:]]/
}

# Is the indented body line a PURE BARE DECLARATION?
#   <indent>NAME ∈ TYPE   with NO "=" anywhere on the line.
# NAME: optional leading "_" then a letter, then ident chars; may be a
# comma-separated multi-name group (a, b, c ∈ Int). TYPE: the rest after ∈.
# The FIRST token after indent must be that identifier (or name-group)
# immediately followed by ∈ — so `∀ …`, `0 ≤ …`, `cond ⇒ …` are excluded.
function is_pure_decl(s,   t, head, tail) {
    if (index(s, "=") > 0) return 0           # any "=" disqualifies
    if (index(s, "∈") == 0) return 0          # must declare a membership
    t = s
    sub(/^[[:space:]]+/, "", t)               # strip indent
    # everything before the first ∈ must be a name or comma-list of names
    head = t
    sub(/∈.*$/, "", head)
    gsub(/[[:space:]]/, "", head)
    if (head == "") return 0
    # head is one-or-more comma-separated identifiers (leading _ allowed)
    if (head !~ /^_?[A-Za-z][A-Za-z0-9_]*(,_?[A-Za-z][A-Za-z0-9_]*)*$/) return 0
    # The TYPE part (after ∈) must be a plain type, not an instantiation /
    # pin / call. A record pin `h ∈ Holder (local ↦ 8)`, a claim call
    # `c ∈ Foo(x ↦ y)`, or a positional pin `p ∈ IVec2(380, 280)` are
    # composition, NOT declarations, and hoisting them past their
    # dependencies (e.g. a `..Lift` that supplies a free name in the body)
    # breaks resolution. Disqualify: any `↦` mapping, or a `Name(` /
    # `Name (` instantiation whose head is not a container type.
    tail = t
    sub(/^.*∈[[:space:]]*/, "", tail)         # tail = the type expression
    # A type name is CAPITALIZED (Int/Nat/Bool/String/Real/Effect/Result,
    # Seq/Set/Array, or a user Capitalized type/enum). A lowercase head means
    # the `∈` is a MEMBERSHIP CONSTRAINT on a value, not a declaration —
    # `a ∈ items` (set membership), `x ∈ xs` (seq membership) — never hoist.
    if (tail !~ /^[A-Z]/) return 0
    if (index(tail, "↦") > 0) return 0        # a mapping ⇒ pin/call
    if (tail ~ /[A-Za-z0-9_>][[:space:]]*\(/ && tail !~ /^(Seq|Set|Array)\(/) return 0
    return 1
}

{ lines[NR] = $0 }

END {
    n = NR
    i = 1
    while (i <= n) {
        line = lines[i]

        if (is_target_header(line)) {
            # Print the (possibly multi-line) header by balancing parens.
            depth = paren_delta(line)
            print line
            i++
            while (depth > 0 && i <= n) {
                print lines[i]
                depth += paren_delta(lines[i])
                i++
            }

            # Collect the body: indented lines up to the next top-level line.
            nd = 0    # hoisted pure-decl lines
            nr = 0    # remaining body lines (everything else)
            while (i <= n) {
                bl = lines[i]
                if (is_toplevel(bl)) break
                if (is_pure_decl(bl)) {
                    decls[nd++] = bl
                } else {
                    rest[nr++] = bl
                }
                i++
            }

            # Emit hoisted decls first (in original relative order), then the
            # rest (in original relative order). Net effect: pure decls move to
            # the front of the body; everything else keeps its order.
            for (d = 0; d < nd; d++) print decls[d]
            for (r = 0; r < nr; r++) print rest[r]
            continue
        }

        print line
        i++
    }
}
'
