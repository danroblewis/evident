#!/usr/bin/env bash
# TODO: rewrite in Evident
#
# lower-bounded-seq.sh — statically-bounded Seq(Int) → flat-scalar lowering.
#
# Reads Evident source on stdin, writes lowered source to stdout. A sibling
# of expand-fsm-autocarry.sh: a line/AST source transform that runs BEFORE
# the oracle ever sees the text. It is SELF-CONTAINED — it emits its own
# `_xs_k` carry duals — so it is order-independent w.r.t. the autocarry pass
# and can run as a post-flatten step on fully resolved source.
#
# WHY THIS EXISTS
# ---------------
# The frozen oracle lowers `Seq(Int)` to Z3 (Array Int Int) + a __len const.
# That representation is opaque to the functionizer and ~250x slower than
# scalars on Z3. For a Seq whose length the program statically bounds by a
# literal N, we can instead keep N plain Int slots + an Int length, which
# functionizes and carries cheaply. This transform performs that lowering
# for ONE narrow, opt-in pattern (slice 1).
#
# RECOGNIZED PATTERN (opt-in via an explicit bound)
# -------------------------------------------------
# Inside a single claim, a field is a "bounded Seq" iff BOTH hold:
#   * it has a declaration   `xs ∈ Seq(Int)`            (xs not starting `_`)
#   * it has a literal bound  `#xs ≤ N`  (N a decimal literal) somewhere in
#                                         the SAME claim body.
# The bound is the safety gate: a Seq with no `#xs ≤ N` line is left
# completely untouched, so every stdlib `Seq(Effect)`/`Seq(LibArg)` and any
# unbounded user Seq passes through verbatim.
#
# REWRITE RULES (for a bounded Seq xs with bound N)
# -------------------------------------------------
#   decl   `xs ∈ Seq(Int)`            → xs_0..xs_{N-1} ∈ Int ; xs_len ∈ Int ;
#                                        0 ≤ xs_len
#   carry  `_xs ∈ Seq(Int)`           → _xs_0.._xs_{N-1} ∈ Int ; _xs_len ∈ Int
#   lit    `xs ∈ Seq(Int) = ⟨a,b,c⟩`  → xs_0 ∈ Int = a … (rest = 0) ;
#                                        xs_len ∈ Int = 3
#   lit    `xs = ⟨a,b,c⟩`             → xs_0 = a … (rest = 0) ; xs_len = 3
#   carried-append (the chosen carried surface):
#          `xs = (is_first_tick ? ⟨⟩ : _xs ++ ⟨v⟩)`
#                                       → for k in 0..N-1:
#                                         xs_k = (is_first_tick ? 0
#                                                 : (_xs_len = k ? v : _xs_k))
#                                         xs_len = (is_first_tick ? 0
#                                                   : _xs_len + 1)
#   forall `∀ x ∈ xs : P`             → ((0<xs_len ⇒ (P[x:=xs_0])) ∧ … ∧
#                                         ((N-1)<xs_len ⇒ (P[x:=xs_{N-1}])))
#   member `y ∈ xs`                   → (((0<xs_len)∧(y=xs_0)) ∨ … ∨
#                                         (((N-1)<xs_len)∧(y=xs_{N-1})))
#   card   `#xs`  (anywhere else)     → xs_len
#
# Everything is fully parenthesised to defeat Evident's precedence footguns
# (`=` tighter than comparisons; `⇒` tighter than `∧`). The user's own
# `#xs ≤ N` bound line falls through the card rule, becoming `xs_len ≤ N`
# (the upper bound); the decl rule supplies the matching `0 ≤ xs_len`.
#
# NON-GOALS (slice 1): records, nested Seqs, `++` other than the singleton
# append above, dynamic bounds, embedded membership inside a larger
# expression, and any Seq element type other than Int.
#
# Usage:
#   lower-bounded-seq.sh < in.ev > out.ev

set -u -o pipefail

exec awk '
# ── helpers ──────────────────────────────────────────────────────────────
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
function is_top_decl(s) {
    return (s ~ /^(claim|fsm|type|schema|enum|import)[ \t]/)
}
function decl_name(s,    nm) {
    nm = s
    sub(/^(claim|fsm|type|schema|enum|import)[ \t]+/, "", nm)
    sub(/[ \t(<].*$/, "", nm)
    return nm
}
function indent_of(s,    ind) {
    ind = s; sub(/[^ \t].*$/, "", ind); return ind
}
# Leading identifier of a stripped line (or "").
function lead_ident(code,    nm) {
    nm = code; sub(/^[ \t]*/, "", nm)
    if (nm !~ /^[A-Za-z_][A-Za-z0-9_]*/) return ""
    sub(/[^A-Za-z0-9_].*$/, "", nm)
    return nm
}
# Text between the first ⟨ and the matching last ⟩ (byte-safe: the multibyte
# angle glyphs never contain ASCII ( ) , so comma splitting stays correct).
function seq_inside(s,    a, b) {
    a = index(s, "⟨"); if (a == 0) return "\x01"   # sentinel: no literal
    b = index(s, "⟩"); if (b == 0) return "\x01"
    # ⟨ and ⟩ are 3 bytes each in UTF-8.
    return substr(s, a + 3, b - (a + 3))
}
function split_commas(s, arr,    p, ch, depth, cur, n) {
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
function trim(s) { gsub(/^[ \t]+|[ \t]+$/, "", s); return s }

# Substitute whole-token VAR with REP in TXT (identifier boundaries).
function subst_tok(txt, vr, rep,    out, i, c, tok, isid) {
    out = ""; tok = ""
    for (i = 1; i <= length(txt); i++) {
        c = substr(txt, i, 1)
        isid = (c ~ /[A-Za-z0-9_]/)
        if (isid) { tok = tok c; continue }
        if (tok != "") { out = out (tok == vr ? rep : tok); tok = "" }
        out = out c
    }
    if (tok != "") out = out (tok == vr ? rep : tok)
    return out
}
# Replace #NAME (card) with NAME_len everywhere NAME is bounded in claim F.
function subst_card(txt, F,    out, i, c, j, nm) {
    out = ""
    for (i = 1; i <= length(txt); i++) {
        c = substr(txt, i, 1)
        if (c == "#") {
            nm = ""; j = i + 1
            while (j <= length(txt) && substr(txt, j, 1) ~ /[A-Za-z0-9_]/) {
                nm = nm substr(txt, j, 1); j++
            }
            if (nm != "" && (F SUBSEP nm) in bnd) {
                out = out nm "_len"; i = j - 1; continue
            }
        }
        out = out c
    }
    return out
}

{ L[NR] = $0 }

END {
    N = NR

    # ── PASS 1: per-claim, find Seq(Int) decls and their #name ≤ N bounds ──
    cur = ""
    for (i = 1; i <= N; i++) {
        s = L[i]; claimOf[i] = ""
        if (is_top_decl(s)) { cur = decl_name(s); continue }
        claimOf[i] = cur
        if (cur == "") continue
        code = strip_comment(s)
        # Seq(Int) decl (possibly with `= ⟨…⟩`); name must not start with _.
        if (code ~ /^[ \t]*_?[A-Za-z][A-Za-z0-9_]*[ \t]*∈[ \t]*Seq\(Int\)/) {
            nm = lead_ident(code)
            if (nm ~ /^_/) seqCarryDecl[cur, substr(nm, 2)] = 1
            else           seqDecl[cur, nm] = 1
        }
        # literal bound `#name ≤ N`
        if (code ~ /^[ \t]*#[A-Za-z][A-Za-z0-9_]*[ \t]*≤[ \t]*[0-9]+[ \t]*$/) {
            nm = code; sub(/^[ \t]*#/, "", nm); sub(/[ \t]*≤.*$/, "", nm)
            v  = code; sub(/^.*≤[ \t]*/, "", v); sub(/[ \t]*$/, "", v)
            boundN[cur, nm] = v
        }
    }
    for (k in seqDecl) {
        split(k, kp, SUBSEP); F = kp[1]; nm = kp[2]
        if ((F SUBSEP nm) in boundN) bnd[F, nm] = boundN[F, nm]
    }

    # ── PASS 2: emit ──────────────────────────────────────────────────────
    for (i = 1; i <= N; i++) {
        s = L[i]; F = claimOf[i]
        if (F == "" || is_top_decl(s)) { print s; continue }
        code = strip_comment(s)
        ind = indent_of(s)
        nm = lead_ident(code)

        # carry decl  `_xs ∈ Seq(Int)`
        if (nm ~ /^_/ && (F SUBSEP substr(nm, 2)) in bnd &&
            code ~ /∈[ \t]*Seq\(Int\)/) {
            base = substr(nm, 2); Nn = bnd[F, base]
            for (k = 0; k < Nn; k++) print ind "_" base "_" k " ∈ Int"
            print ind "_" base "_len ∈ Int"
            continue
        }

        # main decl  `xs ∈ Seq(Int)` (with optional `= ⟨…⟩`)
        if ((F SUBSEP nm) in bnd && code ~ /^[ \t]*[A-Za-z][A-Za-z0-9_]*[ \t]*∈[ \t]*Seq\(Int\)/) {
            Nn = bnd[F, nm]
            inside = seq_inside(code)
            if (inside == "\x01") {                # bare decl, no literal
                for (k = 0; k < Nn; k++) print ind nm "_" k " ∈ Int"
                print ind nm "_len ∈ Int"
                print ind "0 ≤ " nm "_len"
            } else {                               # decl + literal
                ne = split_commas(inside, els)
                if (trim(inside) == "") ne = 0
                for (k = 0; k < Nn; k++)
                    print ind nm "_" k " ∈ Int = " (k < ne ? trim(els[k+1]) : "0")
                print ind nm "_len ∈ Int = " ne
            }
            continue
        }

        # carried-append  `xs = (is_first_tick ? ⟨⟩ : _xs ++ ⟨v⟩)`
        if ((F SUBSEP nm) in bnd &&
            code ~ /^[ \t]*[A-Za-z][A-Za-z0-9_]*[ \t]*=[ \t]*\([ \t]*is_first_tick[ \t]*\?[ \t]*⟨⟩[ \t]*:[ \t]*_[A-Za-z][A-Za-z0-9_]*[ \t]*\+\+[ \t]*⟨.*⟩[ \t]*\)/) {
            Nn = bnd[F, nm]
            v = code; sub(/.*\+\+[ \t]*⟨/, "", v); sub(/⟩.*$/, "", v); v = trim(v)
            for (k = 0; k < Nn; k++)
                print ind nm "_" k " = (is_first_tick ? 0 : (_" nm "_len = " k " ? " v " : _" nm "_" k "))"
            print ind nm "_len = (is_first_tick ? 0 : _" nm "_len + 1)"
            continue
        }

        # plain literal assignment  `xs = ⟨a,b,c⟩`
        if ((F SUBSEP nm) in bnd &&
            code ~ /^[ \t]*[A-Za-z][A-Za-z0-9_]*[ \t]*=[ \t]*⟨.*⟩[ \t]*$/) {
            Nn = bnd[F, nm]
            inside = seq_inside(code)
            ne = split_commas(inside, els)
            if (trim(inside) == "") ne = 0
            for (k = 0; k < Nn; k++)
                print ind nm "_" k " = " (k < ne ? trim(els[k+1]) : "0")
            print ind nm "_len = " ne
            continue
        }

        # forall  `∀ x ∈ xs : P`
        if (code ~ /^[ \t]*∀[ \t]/) {
            body = code; sub(/^[ \t]*∀[ \t]*/, "", body)
            bvar = body; sub(/[ \t]*∈.*$/, "", bvar); bvar = trim(bvar)
            sname = body; sub(/^[^∈]*∈[ \t]*/, "", sname); sub(/[ \t]*:.*$/, "", sname); sname = trim(sname)
            pred = body; sub(/^[^:]*:[ \t]*/, "", pred)
            if ((F SUBSEP sname) in bnd) {
                Nn = bnd[F, sname]; out = ""
                for (k = 0; k < Nn; k++) {
                    pk = subst_tok(pred, bvar, sname "_" k)
                    out = out (k ? " ∧ " : "") "((" k " < " sname "_len) ⇒ (" pk "))"
                }
                print ind "(" out ")"
                continue
            }
        }

        # membership  `y ∈ xs`  (whole-line constraint)
        if (code ~ /∈/ && nm !~ /^Seq/) {
            rhs = code; sub(/^.*∈[ \t]*/, "", rhs); rhs = trim(rhs)
            lhs = code; sub(/[ \t]*∈.*$/, "", lhs); lhs = trim(lhs)
            if ((F SUBSEP rhs) in bnd) {
                Nn = bnd[F, rhs]; out = ""
                for (k = 0; k < Nn; k++)
                    out = out (k ? " ∨ " : "") "((" k " < " rhs "_len) ∧ (" lhs " = " rhs "_" k "))"
                print ind "(" out ")"
                continue
            }
        }

        # default: card-substitute #name → name_len, pass through
        print subst_card(s, F)
    }
}
'
