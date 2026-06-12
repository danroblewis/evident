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
# WHY: the self-hosted compiler is a one-pass / incremental Z3-AST builder (it
# is a tick machine — it materializes each Z3 const as it walks the source),
# so it requires every name DECLARED before it is REFERENCED. The oracle never
# hit this because it is whole-program (it knows every const before it emits).
# The `..`-lift module architecture puts shared carries on the wrong side of a
# lift: `_decl_action` is declared in DriverEnum but read in DriverZInit, and
# DriverZInit is lifted FIRST. The semantic cycle is already legal (the
# back-edge is a carry `_` — a register across the tick boundary); only the
# DECLARATION ordering is stuck.
#
# Two fixes, both here:
#  1. WITHIN a fsm/claim body, move every pure bare declaration to the front.
#  2. ACROSS `..`-lift boundaries, COPY each lifted child's pure declarations
#     up into the caller's hoisted block — the "shared package loaded first"
#     resolution of a dependency cycle. The oracle dedupes a name declared in
#     both parent and lifted child (verified), and the self-hosted compiler
#     treats the child's repeat as a redeclaration (no-op), so copy is safe.
#
# A pure decl introduces a name and consumes none, so hoisting the block is
# always order-safe — the degenerate (cycle-free) topological sort.

set -u -o pipefail

exec awk '
function is_blank(s)   { return s ~ /^[[:space:]]*$/ }

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

# The declared name of an fsm/claim header: after the keyword, up to the first
# space, "(" or "<".
function header_name(s,   t) {
    t = s
    sub(/^(fsm|claim)[[:space:]]+/, "", t)
    sub(/[[:space:](<].*$/, "", t)
    return t
}

# If the (trimmed) line is a `..ChildName` LIFT, return ChildName, else "".
# Only `..` lifts share scope; bare mentions / calls are NOT touched.
function lift_name(s,   t) {
    t = s
    sub(/^[[:space:]]+/, "", t)
    if (t !~ /^\.\.[A-Z]/) return ""
    sub(/^\.\./, "", t)
    sub(/[^A-Za-z0-9_].*$/, "", t)
    return t
}

# Whitespace-stripped form of a decl line, for dedup.
function norm(s,   t) { t = s; gsub(/[[:space:]]/, "", t); return t }

# Is the indented body line a PURE BARE DECLARATION?
#   <indent>NAME ∈ TYPE   with NO "=" anywhere on the line.
# NAME: optional leading "_" then a letter, then ident chars; may be a
# comma-separated multi-name group (a, b, c ∈ Int). The FIRST token after
# indent must be that name(-group) immediately followed by ∈ — so `∀ …`,
# `0 ≤ …`, `cond ⇒ …` are excluded.  The TYPE (after ∈) must be a plain
# CAPITALIZED type, not an instantiation / pin / membership constraint.
function is_pure_decl(s,   t, head, tail) {
    if (index(s, "=") > 0) return 0           # any "=" disqualifies
    if (index(s, "∈") == 0) return 0          # must declare a membership
    t = s
    sub(/^[[:space:]]+/, "", t)               # strip indent
    head = t
    sub(/∈.*$/, "", head)
    gsub(/[[:space:]]/, "", head)
    if (head == "") return 0
    if (head !~ /^_?[A-Za-z][A-Za-z0-9_]*(,_?[A-Za-z][A-Za-z0-9_]*)*$/) return 0
    # A record pin `h ∈ Holder (local ↦ 8)`, claim call `c ∈ Foo(x ↦ y)`,
    # positional pin `p ∈ IVec2(380, 280)`, or membership constraint
    # `a ∈ items` / `x ∈ xs` are composition/constraints, NOT declarations;
    # hoisting them past their dependencies breaks resolution.
    tail = t
    sub(/^.*∈[[:space:]]*/, "", tail)         # tail = the type expression
    if (tail !~ /^[A-Z]/) return 0            # lowercase ⇒ membership on a value
    if (index(tail, "↦") > 0) return 0        # a mapping ⇒ pin/call
    if (tail ~ /[A-Za-z0-9_>][[:space:]]*\(/ && tail !~ /^(Seq|Set|Array)\(/) return 0
    return 1
}

{ lines[NR] = $0 }

END {
    n = NR

    # ---- PASS 1: record each fsm/claim block s pure-decl lines ----
    # claim_decls[name] = newline-joined pure-decl lines of that block.
    i = 1
    while (i <= n) {
        line = lines[i]
        if (is_target_header(line)) {
            cname = header_name(line)
            depth = paren_delta(line); i++
            while (depth > 0 && i <= n) { depth += paren_delta(lines[i]); i++ }
            blk = ""
            while (i <= n) {
                bl = lines[i]
                if (is_toplevel(bl)) break
                if (is_pure_decl(bl)) blk = blk bl "\n"
                i++
            }
            claim_decls[cname] = blk
            continue
        }
        i++
    }

    # ---- PASS 2: emit, hoisting own decls + lifted-childrens decls ----
    i = 1
    while (i <= n) {
        line = lines[i]

        if (is_target_header(line)) {
            depth = paren_delta(line)
            print line
            i++
            while (depth > 0 && i <= n) {
                print lines[i]
                depth += paren_delta(lines[i])
                i++
            }

            delete seen          # dedup set for the hoisted decl block
            nd = 0; nr = 0; nl = 0
            while (i <= n) {
                bl = lines[i]
                if (is_toplevel(bl)) break
                if (is_pure_decl(bl)) {
                    key = norm(bl)
                    if (!(key in seen)) { seen[key] = 1; decls[nd++] = bl }
                } else {
                    rest[nr++] = bl
                    ln = lift_name(bl)
                    if (ln != "") lifted[nl++] = ln
                }
                i++
            }

            # Cross-lift: append each `..`-lifted child s pure decls (deduped).
            for (c = 0; c < nl; c++) {
                cn = lifted[c]
                if (cn in claim_decls) {
                    m = split(claim_decls[cn], arr, "\n")
                    for (k = 1; k <= m; k++) {
                        if (arr[k] == "") continue
                        key = norm(arr[k])
                        if (!(key in seen)) { seen[key] = 1; decls[nd++] = arr[k] }
                    }
                }
            }

            for (d = 0; d < nd; d++) print decls[d]
            for (r = 0; r < nr; r++) print rest[r]
            continue
        }

        print line
        i++
    }
}
'
