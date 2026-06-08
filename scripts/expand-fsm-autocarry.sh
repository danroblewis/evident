#!/usr/bin/env bash
# TODO: rewrite in Evident
#
# expand-fsm-autocarry.sh — fsm prev-tick carry expansion preprocessor.
#
# Reads Evident source on stdin, writes expanded source to stdout.
#
# WHY THIS EXISTS
# ---------------
# `fsm` is, in the frozen bootstrap oracle, just a synonym for `claim`
# (see CLAUDE.md schema-keywords table). The oracle is a frozen binary;
# we cannot teach it FSM-specific semantics. So the "auto-generated
# prev-tick carry field" sugar has to be expanded as a SOURCE TRANSFORM
# that runs BEFORE the oracle ever sees the text. This is that step.
# It runs as the final pass of flatten-evident.sh, so every call site
# that flattens (i.e. all of them) gets the expansion for free.
#
# WHAT IT DOES
# ------------
# In an `fsm <Name>` claim, a bare field declaration
#
#     x ∈ T
#
# implicitly declares a prev-tick carry `_x` whenever the claim body
# references `_x` somewhere in its code. The transform:
#
#   1. rewrites the keyword `fsm <Name>` → `claim <Name>` (the oracle
#      only knows `claim`);
#   2. after each bare field decl `x ∈ T` whose `_x` is referenced in
#      the claim's *code* (comments excluded), inserts a line
#
#          _x ∈ base(T)
#
#      where base(T) is the underlying sort of T — the first
#      whitespace-delimited token, so a refined `Int < 65534` carries
#      as plain `Int`.
#
# A `claim` (no `fsm`) is passed through untouched, so pure helpers are
# unaffected. The expansion is reference-driven: a field with no `_x`
# reference gets no carry (it is computed fresh each tick).
#
# THE RULE, PRECISELY
# -------------------
#   carry(x) is emitted  ⟺  x has a bare decl `x ∈ T` in the fsm body
#                            AND token `_x` appears in the body's code.
#   carry type            =  base(T) = first whitespace token of T.
#
# This was validated to reproduce the 347 hand-written carry pairs in
# compiler2/driver.ev exactly (no over- or under-generation, all types
# matching). See docs/plans/fsm-autocarry.md.
#
# Usage:
#   expand-fsm-autocarry.sh < in.ev > out.ev

set -u -o pipefail

exec awk '
# --- helpers -------------------------------------------------------------
# Strip an Evident `--` line comment, respecting double-quoted strings so a
# `--` inside a string literal is not treated as a comment. Used only for
# reference detection, never for emitted text.
function strip_comment(s,    out, i, c, instr) {
    out = ""; instr = 0
    for (i = 1; i <= length(s); i++) {
        c = substr(s, i, 1)
        if (c == "\"") { instr = !instr; out = out c; continue }
        if (!instr && c == "-" && substr(s, i+1, 1) == "-") break
        out = out c
    }
    return out
}

# Is this line a top-level declaration header (column-0 keyword)?
function is_top_decl(s) {
    return (s ~ /^(claim|fsm|type|schema|enum|import)[ \t]/)
}

# Flush the buffered fsm claim: detect referenced carries, then re-emit
# each line, inserting `_x ∈ base(T)` after each anchoring bare decl.
function flush_fsm(    i, line, code, refset, n, toks, j, fld, typ, indent, base) {
    # Pass 1: collect the set of referenced `_name` tokens across the body
    # code (comments stripped). Stored as keys "_name" in refset[].
    delete refset
    for (i = 0; i < nbuf; i++) {
        code = strip_comment(buf[i])
        # walk the code, pulling out _<ident> tokens not preceded by an
        # ident char (so `__foo` and `a_b` interior underscores do not
        # spuriously match the carry shape `_<base>`).
        n = split(code, toks, /[^A-Za-z0-9_]+/)
        for (j = 1; j <= n; j++)
            if (toks[j] ~ /^_[A-Za-z][A-Za-z0-9_]*$/)
                refset[toks[j]] = 1
    }
    # Pass 2: re-emit, inserting carries after their anchor field decls.
    for (i = 0; i < nbuf; i++) {
        line = buf[i]
        print line
        code = strip_comment(line)
        # bare field decl: `<indent>name ∈ Type`  (no `=`, name not `_`-led)
        if (code ~ /^[ \t]*[A-Za-z][A-Za-z0-9_]*[ \t]*∈[ \t]*[^=]+$/) {
            indent = code; sub(/[^ \t].*$/, "", indent)
            fld = code; sub(/^[ \t]*/, "", fld); sub(/[ \t]*∈.*$/, "", fld)
            typ = code; sub(/^[^∈]*∈[ \t]*/, "", typ); sub(/[ \t]*$/, "", typ)
            if (("_" fld) in refset) {
                base = typ; sub(/[ \t].*$/, "", base)   # first token of T
                print indent "_" fld " ∈ " base
            }
        }
    }
    nbuf = 0
}

# --- driver --------------------------------------------------------------
BEGIN { in_fsm = 0; nbuf = 0 }

{
    if (in_fsm) {
        # A new column-0 decl ends the current fsm claim.
        if (is_top_decl($0)) {
            flush_fsm()
            in_fsm = 0
            # fall through to handle this line as a normal/new header
        } else {
            buf[nbuf++] = $0
            next
        }
    }
    if ($0 ~ /^fsm[ \t]/) {
        # Open an fsm claim: rewrite the keyword, buffer the header.
        hdr = $0; sub(/^fsm/, "claim", hdr)
        buf[nbuf++] = hdr
        in_fsm = 1
        next
    }
    print
}

END { if (in_fsm) flush_fsm() }
'
