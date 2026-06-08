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
# WHAT IT DOES (single fsm — the original autocarry)
# --------------------------------------------------
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
# WHAT IT ALSO DOES (carry-preserving fsm COMPOSITION)
# ----------------------------------------------------
# The frozen oracle inlines a composition call `Sub(x ↦ y)` by
# substituting x→y in Sub's body, BUT it knows nothing of carry
# siblings: Sub's `_x` is not remapped and the parent's `_y` is not
# synthesized. So a composed carrying fsm breaks (the inlined body
# references a dangling `_x` and the parent field never carries).
#
# This transform closes that gap. With a global registry of which
# fields each fsm carries:
#
#   * SLOT-BIND  `Sub(x ↦ y, …)` where x is a carry of Sub:
#       inject an extra binding `_x ↦ _y`, and (because the parent
#       now references `_y`, with a bare decl `y ∈ T`) the autocarry
#       pass synthesizes `_y ∈ base(T)` in the parent. The carry then
#       travels WITH the inlined logic: `y = (… _y …)` over the
#       parent's OWN top-level y/_y, which the kernel carries.
#
#   * LIFT       `..Sub` / bare `Sub`:
#       names-match promotes Sub's `x` and (already-synthesized) `_x`
#       into the parent unchanged. No injection needed — the oracle's
#       lift copies both decls in, so the parent carries `x` for free.
#       (Verified empirically; this transform leaves lift lines alone.)
#
# Multi-call-site (`Sub(x↦a)` and `Sub(x↦b)` in one parent) gets
# independent carries `_a`/`_b` — each binds a distinct parent var, so
# it falls out naturally. Nested composition (fsm→fsm→fsm) carries
# transitively: a field bound to a sub's carry slot itself becomes a
# carry of the composing fsm, computed to a FIXPOINT over the registry.
#
# A `claim` (no `fsm`) is passed through untouched, so pure helpers are
# unaffected. Composition calls whose callee is NOT a registered fsm
# (every helper-claim call in compiler2/driver.ev) get no injection, so
# the real driver's emitted stage1 stays byte-identical.
#
# THE RULES, PRECISELY
# --------------------
#   carry(F, x) is true  ⟺  x has a bare decl `x ∈ T` in fsm F
#                            AND (token `_x` appears in F's code
#                                 OR F has a call `Sub(s ↦ x, …)` with
#                                    Sub a registered fsm and carry(Sub, s)).
#   carry type            =  base(T) = first whitespace token of T.
#   injection             =  for each call `Sub(s ↦ v)` in F with
#                            carry(Sub, s) and v a bare identifier and
#                            no `_s` binding already present, append
#                            `, _s ↦ _v` to the call.
#
# The single-fsm rule reproduces the 347 hand-written carry pairs in
# compiler2/driver.ev exactly. See docs/plans/fsm-autocarry.md and
# docs/plans/fsm-composition.md.
#
# Usage:
#   expand-fsm-autocarry.sh < in.ev > out.ev

set -u -o pipefail

exec awk '
# --- helpers -------------------------------------------------------------
# Strip an Evident `--` line comment, respecting double-quoted strings so a
# `--` inside a string literal is not treated as a comment. Used only for
# reference/structure detection, never for emitted text.
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

# Extract the schema name from a `fsm <Name>` / `claim <Name>` header.
function decl_name(s,    nm) {
    nm = s
    sub(/^(claim|fsm|type|schema|enum|import)[ \t]+/, "", nm)
    sub(/[ \t(<].*$/, "", nm)
    return nm
}

# first whitespace-delimited token of a type text (base sort).
function base_of(t,    b) {
    b = t; sub(/[ \t].*$/, "", b); return b
}

# Split a string on top-level commas (commas not enclosed in parens).
# Multibyte UTF-8 chars (the mapsto/in/angle glyphs) never contain the
# ASCII bytes ( ) , so byte-wise scanning is safe regardless of locale.
function split_toplevel(s, arr,    p, ch, depth, cur, n) {
    depth = 0; cur = ""; n = 0
    for (p = 1; p <= length(s); p++) {
        ch = substr(s, p, 1)
        if (ch == "(") depth++
        else if (ch == ")") depth--
        if (ch == "," && depth == 0) { arr[++n] = cur; cur = ""; continue }
        cur = cur ch
    }
    arr[++n] = cur
    return n
}

# Is this comment-stripped line a call `Name( … )` (callee = leading ident)?
function call_callee(code,    nm) {
    if (code !~ /^[ \t]*[A-Za-z][A-Za-z0-9_]*[ \t]*\(/) return ""
    nm = code; sub(/^[ \t]*/, "", nm); sub(/[ \t]*\(.*/, "", nm)
    return nm
}

# Is this comment-stripped line a bare field decl `name ∈ Type` (no `=`)?
# Returns the field name (or "" ). Carry decls (`_x ∈ …`) return "".
function bare_field(code,    fld) {
    if (code !~ /^[ \t]*[A-Za-z][A-Za-z0-9_]*[ \t]*∈[ \t]*[^=]+$/) return ""
    fld = code; sub(/^[ \t]*/, "", fld); sub(/[ \t]*∈.*$/, "", fld)
    if (fld ~ /^_/) return ""
    return fld
}
function field_type(code,    typ) {
    typ = code; sub(/^[^∈]*∈[ \t]*/, "", typ); sub(/[ \t]*$/, "", typ)
    return typ
}

# Parse the parenthesised argument list of a call line into slot/value
# bindings, recorded against call id c. Fills cb_*[c,*].
function parse_call(i, code, cur,    name, lp, p, ch, depth, started, inside, nseg, segs, s, seg, slot, val, c) {
    name = call_callee(code)
    if (name == "") return
    lp = index(code, "(")
    if (lp == 0) return
    depth = 0; inside = ""; started = 0
    for (p = lp; p <= length(code); p++) {
        ch = substr(code, p, 1)
        if (ch == "(") { depth++; if (depth == 1) { started = 1; continue } }
        if (ch == ")") { depth--; if (depth == 0) break }
        if (started) inside = inside ch
    }
    c = ++ncall
    lineToCall[i] = c
    callFsm[c] = cur
    callCallee[c] = name
    cbn[c] = 0
    nseg = split_toplevel(inside, segs)
    for (s = 1; s <= nseg; s++) {
        seg = segs[s]
        if (seg ~ /↦/) {
            slot = seg; sub(/↦.*/, "", slot); gsub(/[ \t]/, "", slot)
            val = seg; sub(/.*↦/, "", val); gsub(/^[ \t]+/, "", val); gsub(/[ \t]+$/, "", val)
            cbn[c]++
            cb_slot[c, cbn[c]] = slot
            cb_val[c, cbn[c]] = val
        }
    }
}

# --- main: buffer everything, build registry, then emit ------------------
{ L[NR] = $0 }

END {
    N = NR

    # ---- PASS 1: segment fsms; collect bare decls, `_x` refs, calls -----
    cur = ""
    for (i = 1; i <= N; i++) {
        s = L[i]
        if (is_top_decl(s)) {
            cur = ""
            if (s ~ /^fsm[ \t]/) {
                nm = decl_name(s)
                cur = nm
                isFsm[nm] = 1
            }
            continue
        }
        if (cur == "") continue
        code = strip_comment(s)

        fld = bare_field(code)
        if (fld != "") {
            hasBareDecl[cur, fld] = 1
            declType[cur, fld] = field_type(code)
        }

        nt = split(code, toks, /[^A-Za-z0-9_]+/)
        for (j = 1; j <= nt; j++)
            if (toks[j] ~ /^_[A-Za-z][A-Za-z0-9_]*$/)
                refUnder[cur, toks[j]] = 1

        if (call_callee(code) != "") parse_call(i, code, cur)
    }

    # ---- PASS 2: fixpoint over the carry registry -----------------------
    # carry(F,x) holds when x has a bare decl in F AND (`_x` appears in
    # F code OR F binds x to a carry slot of a registered sub-fsm).
    # Iterated to a fixpoint so nested fsm composition propagates carries.
    do {
        changed = 0
        for (key in hasBareDecl) {
            split(key, kp, SUBSEP)
            F = kp[1]; x = kp[2]
            if ((F SUBSEP x) in carry) continue
            mark = 0
            if ((F SUBSEP ("_" x)) in refUnder) mark = 1
            else {
                for (c = 1; c <= ncall; c++) {
                    if (callFsm[c] != F) continue
                    if (!(callCallee[c] in isFsm)) continue
                    for (t = 1; t <= cbn[c]; t++) {
                        if (cb_val[c, t] != x) continue
                        if (cb_val[c, t] !~ /^[A-Za-z][A-Za-z0-9_]*$/) continue
                        if ((callCallee[c] SUBSEP cb_slot[c, t]) in carry) { mark = 1; break }
                    }
                    if (mark) break
                }
            }
            if (mark) {
                carry[F SUBSEP x] = 1
                carryBase[F SUBSEP x] = base_of(declType[F, x])
                changed = 1
            }
        }
    } while (changed)

    # ---- PASS 3: emit ---------------------------------------------------
    cur = ""
    for (i = 1; i <= N; i++) {
        s = L[i]
        if (is_top_decl(s)) {
            if (s ~ /^fsm[ \t]/) {
                hdr = s; sub(/^fsm/, "claim", hdr); print hdr
                cur = decl_name(s)
            } else {
                print s; cur = ""
            }
            continue
        }
        if (cur == "") { print s; continue }

        line = s
        if (i in lineToCall) line = inject_carries(i, line)
        print line

        code = strip_comment(s)
        fld = bare_field(code)
        if (fld != "" && (cur SUBSEP fld) in carry) {
            indent = code; sub(/[^ \t].*$/, "", indent)
            print indent "_" fld " ∈ " carryBase[cur SUBSEP fld]
        }
    }
}

# Inject `_slot ↦ _val` carry-sibling bindings into a composition call
# line whose callee is a registered fsm. Idempotent: skips a slot whose
# `_slot` binding is already present.
function inject_carries(i, line,    c, callee, t, slot, val, have, add, lp, p, ch, depth, prefix, suffix, pos) {
    c = lineToCall[i]
    callee = callCallee[c]
    if (!(callee in isFsm)) return line

    delete have
    for (t = 1; t <= cbn[c]; t++) have[cb_slot[c, t]] = 1

    add = ""
    for (t = 1; t <= cbn[c]; t++) {
        slot = cb_slot[c, t]
        val  = cb_val[c, t]
        if (!((callee SUBSEP slot) in carry)) continue
        if (val !~ /^[A-Za-z][A-Za-z0-9_]*$/) continue
        if (("_" slot) in have) continue
        add = add ", _" slot " ↦ _" val
    }
    if (add == "") return line

    lp = index(line, "(")
    if (lp == 0) return line
    depth = 0; pos = 0
    for (p = lp; p <= length(line); p++) {
        ch = substr(line, p, 1)
        if (ch == "(") depth++
        else if (ch == ")") { depth--; if (depth == 0) { pos = p; break } }
    }
    if (pos == 0) return line
    prefix = substr(line, 1, pos - 1)
    suffix = substr(line, pos)
    return prefix add suffix
}
'
