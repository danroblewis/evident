#!/usr/bin/env bash
# TODO: rewrite in Evident (compiler2/passes/ once the self-hosting seam
# exists — see docs/plans/post-cutover-roadmap.md "host passes in Evident")
#
# lower-bounded-seq.sh — statically-bounded Seq → flat-scalar lowering.
#
# Reads Evident source on stdin, writes lowered source to stdout. A sibling
# of expand-fsm-autocarry.sh, run AFTER it in flatten-evident.sh: autocarry
# synthesizes the `_xs ∈ Seq(…)` dual first, and this pass then lowers BOTH
# decls, so neither pass double-declares.
#
# WHY: the frozen oracle lowers Seq to (Array Int T) + __len — opaque to
# the functionizer and slow as carried state. For a Seq whose length the
# program statically bounds by a literal N, N flat scalars + a length Int
# functionize and carry for free. The transform makes the bounded-Seq
# SURFACE compilable without the oracle ever seeing a Seq.
#
# OPT-IN GATE: a Seq is lowered iff its decl `xs ∈ Seq(Int)` / `xs ∈ Seq(R)`
# (R a record `type`) has a literal bound `#xs ≤ N` in the SAME claim.
# Unbounded Seqs (`Seq(Effect)`, …) pass through verbatim. Decls register
# GLOBALLY, so use-sites in other claims (names-match composition) lower too.
#
# REWRITE RULES (xs bounded by N; record R with fields f1..fm, zero-defaults
# Int→0 / String→"" / Bool→false):
#   decl    xs ∈ Seq(Int)                  → xs_0..xs_{N-1} ∈ Int ; xs_len ∈ Int ; 0 ≤ xs_len
#           xs ∈ Seq(R)                    → xs_k_fj ∈ Tj …       ; xs_len ∈ Int ; 0 ≤ xs_len
#   dual    _xs ∈ Seq(…)                   → the matching _ decls (no bound line)
#   literal xs = ⟨a,b⟩ (Int only)          → slot pins + xs_len = 2
#   append  xs = (is_first_tick ? ⟨⟩ : [G ?] _xs ++ ⟨v | R(e1..em)⟩ [: _xs])
#           → per slot k (per field j):
#             xs_k[_fj] = (is_first_tick ? def : (G ∧ _xs_len = k) ? e : _xs_k[_fj])
#             xs_len    = (is_first_tick ? 0   : G ? _xs_len + 1 : _xs_len)
#           (unguarded form: G ≡ true, emitted without the guard conjunct)
#   ∀       ∀ x ∈ xs : P   (Int only)      → len-guarded ∧-unroll
#   member  y ∈ xs         (Int only)      → len-guarded ∨-unroll
#   ∃       (∃ i ∈ {0..#xs-1} : P)         → ((0<xs_len)∧(P[i:=0])) ∨ …
#           (P may use xs[i] / xs[i].f — substituted per slot)
#   index   xs[k] / xs[k].f (k literal)    → xs_k / xs_k_f   (anywhere)
#   card    #xs                            → xs_len          (anywhere)
#
# COMPLETENESS CHECK: after rewriting, any surviving bare `xs` / `_xs`
# token for a registered Seq is an unsupported use — the pass FAILS LOUDLY
# (exit 1) instead of letting the oracle silently drop it.
#
# Usage: lower-bounded-seq.sh < in.ev > out.ev

set -u -o pipefail

exec awk '
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
function is_top_decl(s) { return (s ~ /^(claim|fsm|type|schema|enum|import)[ \t]/) }
function decl_name(s,    nm) {
    nm = s; sub(/^(claim|fsm|type|schema|enum|import)[ \t]+/, "", nm)
    sub(/[ \t(<].*$/, "", nm); return nm
}
function indent_of(s,    ind) { ind = s; sub(/[^ \t].*$/, "", ind); return ind }
function lead_ident(code,    nm) {
    nm = code; sub(/^[ \t]*/, "", nm)
    if (nm !~ /^[A-Za-z_][A-Za-z0-9_]*/) return ""
    sub(/[^A-Za-z0-9_].*$/, "", nm); return nm
}
function seq_inside(s,    a, b) {
    a = index(s, "⟨"); if (a == 0) return "\x01"
    b = index(s, "⟩"); if (b == 0) return "\x01"
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
function zdef(t) {
    if (t == "String") return "\"\""
    if (t == "Bool")   return "false"
    return "0"
}
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
# #xs → xs_len for every globally registered xs.
function subst_card(txt,    out, i, c, j, nm) {
    out = ""
    for (i = 1; i <= length(txt); i++) {
        c = substr(txt, i, 1)
        if (c == "#") {
            nm = ""; j = i + 1
            while (j <= length(txt) && substr(txt, j, 1) ~ /[A-Za-z0-9_]/) {
                nm = nm substr(txt, j, 1); j++
            }
            if (nm in gbnd) { out = out nm "_len"; i = j - 1; continue }
        }
        out = out c
    }
    return out
}
# xs[INT] / xs[INT].field → xs_INT / xs_INT_field for registered xs
# (also their _xs duals). Literal index only.
function subst_index(txt,    out, i, c, tok, isid, rest, idx, fld, m, base) {
    out = ""; tok = ""
    for (i = 1; i <= length(txt); i++) {
        c = substr(txt, i, 1)
        isid = (c ~ /[A-Za-z0-9_]/)
        if (isid) { tok = tok c; continue }
        if (tok != "" && c == "[") {
            base = tok; sub(/^_/, "", base)
            if (base in gbnd) {
                rest = substr(txt, i + 1)
                if (match(rest, /^[0-9]+\]/)) {
                    idx = substr(rest, 1, RLENGTH - 1)
                    i = i + RLENGTH
                    fld = ""
                    if (substr(txt, i + 1, 1) == "." &&
                        match(substr(txt, i + 2), /^[A-Za-z_][A-Za-z0-9_]*/)) {
                        fld = substr(txt, i + 2, RLENGTH)
                        i = i + 1 + RLENGTH
                    }
                    out = out tok "_" idx (fld != "" ? "_" fld : "")
                    tok = ""
                    continue
                }
            }
        }
        if (tok != "") { out = out tok; tok = "" }
        out = out c
    }
    if (tok != "") out = out tok
    return out
}
# Expand every `(∃ i ∈ {0..#xs-1} : P)` over a registered xs. P may use
# xs[i] / xs[i].f; the bound var index is substituted per slot, the
# resulting xs[k].f forms are handled by subst_index afterwards.
function subst_exists(txt,    pos, a, st, en, depth, j, ch, inner, bvar, sname, pred, Nn, k, pk, out2, repl) {
    while (1) {
        pos = index(txt, "∃")
        if (pos == 0) return txt
        # opening paren immediately before (allow spaces)
        st = pos - 1
        while (st >= 1 && substr(txt, st, 1) == " ") st--
        if (st < 1 || substr(txt, st, 1) != "(") return txt   # unsupported shape; completeness check will catch
        # find matching close paren from st
        depth = 0; en = 0
        for (j = st; j <= length(txt); j++) {
            ch = substr(txt, j, 1)
            if (ch == "(") depth++
            else if (ch == ")") { depth--; if (depth == 0) { en = j; break } }
        }
        if (en == 0) return txt
        inner = substr(txt, st + 1, en - st - 1)     # ∃ i ∈ {0..#xs-1} : P
        bvar = inner; sub(/^[ \t]*∃[ \t]*/, "", bvar); sub(/[ \t]*∈.*$/, "", bvar); bvar = trim(bvar)
        sname = inner
        if (!match(sname, /\{0\.\.#[A-Za-z_][A-Za-z0-9_]*-1\}/)) return txt
        sname = substr(sname, RSTART + 5, RLENGTH - 8)   # strip {0..# and -1}
        sub(/^#/, "", sname)
        pred = inner; sub(/^[^:]*:[ \t]*/, "", pred)
        if (!(sname in gbnd)) return txt
        Nn = gbnd[sname]; out2 = ""
        for (k = 0; k < Nn; k++) {
            pk = subst_tok(pred, bvar, k)
            out2 = out2 (k ? " ∨ " : "") "((" k " < " sname "_len) ∧ (" pk "))"
        }
        repl = "(" out2 ")"
        txt = substr(txt, 1, st - 1) repl substr(txt, en + 1)
    }
}

{ L[NR] = $0 }

END {
    N = NR

    # ── PASS 0: record types — type R(f1 ∈ T1, f2 ∈ T2, …) ──────────────
    for (i = 1; i <= N; i++) {
        s = strip_comment(L[i])
        if (s !~ /^type[ \t]+[A-Za-z_][A-Za-z0-9_]*\(/) continue
        tn = decl_name(s)
        inner = s; sub(/^[^(]*\(/, "", inner); sub(/\)[^)]*$/, "", inner)
        nf = split_commas(inner, fparts)
        cnt = 0
        for (j = 1; j <= nf; j++) {
            fp = trim(fparts[j])
            ft = fp; sub(/^.*∈[ \t]*/, "", ft); ft = trim(ft)
            nn = split(fp, fnames2, /,/)   # multi-name `a, b ∈ Int`
            # names are everything before ∈ (comma-separated)
            fnms = fp; sub(/[ \t]*∈.*$/, "", fnms)
            nn = split(fnms, fnames2, /,/)
            for (q = 1; q <= nn; q++) {
                fn = trim(fnames2[q])
                if (fn == "") continue
                cnt++
                tfield[tn, cnt] = fn
                ttype[tn, cnt]  = ft
            }
        }
        tnf[tn] = cnt
    }

    # ── PASS 1: per-claim decls + bounds; register globally ─────────────
    cur = ""
    for (i = 1; i <= N; i++) {
        s = L[i]; claimOf[i] = ""
        if (is_top_decl(s)) { cur = decl_name(s); continue }
        claimOf[i] = cur
        if (cur == "") continue
        code = strip_comment(s)
        if (code ~ /^[ \t]*_?[A-Za-z][A-Za-z0-9_]*[ \t]*∈[ \t]*Seq\([A-Za-z_][A-Za-z0-9_]*\)/) {
            el = code; sub(/^[^∈]*∈[ \t]*Seq\(/, "", el); sub(/\).*$/, "", el)
            nm = lead_ident(code)
            if (el == "Int" || (el in tnf)) {
                if (nm ~ /^_/) { dualDecl[cur, substr(nm, 2)] = 1 }
                else           { seqDecl[cur, nm] = 1; elemOf[nm] = el }
            }
        }
        if (code ~ /^[ \t]*#[A-Za-z][A-Za-z0-9_]*[ \t]*≤[ \t]*[0-9]+[ \t]*$/) {
            nm = code; sub(/^[ \t]*#/, "", nm); sub(/[ \t]*≤.*$/, "", nm)
            v  = code; sub(/^.*≤[ \t]*/, "", v); sub(/[ \t]*$/, "", v)
            boundN[cur, nm] = v
        }
    }
    for (k in seqDecl) {
        split(k, kp, SUBSEP); F = kp[1]; nm = kp[2]
        if ((F SUBSEP nm) in boundN) { bnd[F, nm] = boundN[F, nm]; gbnd[nm] = boundN[F, nm] }
    }

    # field plan per registered seq: for Int, one unnamed slot
    # (slotname xs_k); for record R, slots xs_k_f1..fm.
    # emit helpers read elemOf/tfield/ttype directly.

    # ── PASS 2: emit ─────────────────────────────────────────────────────
    on = 0
    for (i = 1; i <= N; i++) {
        s = L[i]; F = claimOf[i]
        if (F == "" || is_top_decl(s)) { O[++on] = s; continue }
        code = strip_comment(s)
        ind = indent_of(s)
        nm = lead_ident(code)

        # dual decl  `_xs ∈ Seq(…)`
        if (nm ~ /^_/ && (substr(nm, 2) in gbnd) && code ~ /∈[ \t]*Seq\(/) {
            base = substr(nm, 2); Nn = gbnd[base]; el = elemOf[base]
            for (k = 0; k < Nn; k++) {
                if (el == "Int") O[++on] = ind "_" base "_" k " ∈ Int"
                else for (j = 1; j <= tnf[el]; j++)
                    O[++on] = ind "_" base "_" k "_" tfield[el, j] " ∈ " ttype[el, j]
            }
            O[++on] = ind "_" base "_len ∈ Int"
            continue
        }

        # main decl `xs ∈ Seq(…)` (Int may carry a literal)
        if ((nm in gbnd) && (F SUBSEP nm) in bnd && code ~ /^[ \t]*[A-Za-z][A-Za-z0-9_]*[ \t]*∈[ \t]*Seq\(/) {
            Nn = gbnd[nm]; el = elemOf[nm]
            inside = seq_inside(code)
            if (el == "Int" && inside != "\x01") {
                ne = split_commas(inside, els)
                if (trim(inside) == "") ne = 0
                for (k = 0; k < Nn; k++)
                    O[++on] = ind nm "_" k " ∈ Int = " (k < ne ? trim(els[k+1]) : "0")
                O[++on] = ind nm "_len ∈ Int = " ne
            } else {
                for (k = 0; k < Nn; k++) {
                    if (el == "Int") O[++on] = ind nm "_" k " ∈ Int"
                    else for (j = 1; j <= tnf[el]; j++)
                        O[++on] = ind nm "_" k "_" tfield[el, j] " ∈ " ttype[el, j]
                }
                O[++on] = ind nm "_len ∈ Int"
                O[++on] = ind "0 ≤ " nm "_len"
            }
            continue
        }

        # hold  `xs = (is_first_tick ? ⟨⟩ : _xs)` — the "no writer here" stub
        if ((nm in gbnd) &&
            code ~ /^[ \t]*[A-Za-z][A-Za-z0-9_]*[ \t]*=[ \t]*\([ \t]*is_first_tick[ \t]*\?[ \t]*⟨⟩[ \t]*:[ \t]*_[A-Za-z][A-Za-z0-9_]*[ \t]*\)[ \t]*$/) {
            Nn = gbnd[nm]; el = elemOf[nm]
            for (k = 0; k < Nn; k++) {
                if (el == "Int") O[++on] = ind nm "_" k " = (is_first_tick ? 0 : _" nm "_" k ")"
                else for (j = 1; j <= tnf[el]; j++) {
                    fn = tfield[el, j]; dv = zdef(ttype[el, j])
                    O[++on] = ind nm "_" k "_" fn " = (is_first_tick ? " dv " : _" nm "_" k "_" fn ")"
                }
            }
            O[++on] = ind nm "_len = (is_first_tick ? 0 : _" nm "_len)"
            continue
        }

        # carried-append, guarded or not:
        #   xs = (is_first_tick ? ⟨⟩ : [G ?] _xs ++ ⟨…⟩ [: _xs])
        if ((nm in gbnd) &&
            code ~ /^[ \t]*[A-Za-z][A-Za-z0-9_]*[ \t]*=[ \t]*\([ \t]*is_first_tick[ \t]*\?[ \t]*⟨⟩[ \t]*:/ &&
            index(code, "++") > 0) {
            Nn = gbnd[nm]; el = elemOf[nm]
            # text between the first `:` and `_xs ++`
            mid = code
            sub(/^[^:]*:[ \t]*/, "", mid)                # after ⟨⟩ :
            app = "_" nm " ++"
            ap = index(mid, app)
            guard = trim(substr(mid, 1, ap - 1))
            sub(/[ \t]*\?[ \t]*$/, "", guard); guard = trim(guard)
            inner = seq_inside(substr(mid, ap))           # ⟨v⟩ / ⟨R(e1,e2)⟩
            # element exprs
            ctor = inner; sub(/\(.*$/, "", ctor); ctor = trim(ctor)
            if (el != "Int" && ctor == el) {
                args = inner; sub(/^[^(]*\(/, "", args); sub(/\)[ \t]*$/, "", args)
                na = split_commas(args, aexp)
            }
            for (k = 0; k < Nn; k++) {
                if (el == "Int") {
                    v = trim(inner)
                    cond = (guard == "" ? "_" nm "_len = " k : "(" guard " ∧ _" nm "_len = " k ")")
                    O[++on] = ind nm "_" k " = (is_first_tick ? 0 : " cond " ? " v " : _" nm "_" k ")"
                } else {
                    for (j = 1; j <= tnf[el]; j++) {
                        fn = tfield[el, j]; dv = zdef(ttype[el, j]); ev = trim(aexp[j])
                        cond = (guard == "" ? "_" nm "_len = " k : "(" guard " ∧ _" nm "_len = " k ")")
                        O[++on] = ind nm "_" k "_" fn " = (is_first_tick ? " dv " : " cond " ? " ev " : _" nm "_" k "_" fn ")"
                    }
                }
            }
            if (guard == "")
                O[++on] = ind nm "_len = (is_first_tick ? 0 : _" nm "_len + 1)"
            else
                O[++on] = ind nm "_len = (is_first_tick ? 0 : " guard " ? _" nm "_len + 1 : _" nm "_len)"
            continue
        }

        # plain Int literal assignment  `xs = ⟨a,b,c⟩`
        if ((nm in gbnd) && elemOf[nm] == "Int" &&
            code ~ /^[ \t]*[A-Za-z][A-Za-z0-9_]*[ \t]*=[ \t]*⟨.*⟩[ \t]*$/) {
            Nn = gbnd[nm]
            inside = seq_inside(code)
            ne = split_commas(inside, els)
            if (trim(inside) == "") ne = 0
            for (k = 0; k < Nn; k++)
                O[++on] = ind nm "_" k " = " (k < ne ? trim(els[k+1]) : "0")
            O[++on] = ind nm "_len = " ne
            continue
        }

        # forall over an Int seq  `∀ x ∈ xs : P`
        if (code ~ /^[ \t]*∀[ \t]/) {
            body = code; sub(/^[ \t]*∀[ \t]*/, "", body)
            bvar = body; sub(/[ \t]*∈.*$/, "", bvar); bvar = trim(bvar)
            sname = body; sub(/^[^∈]*∈[ \t]*/, "", sname); sub(/[ \t]*:.*$/, "", sname); sname = trim(sname)
            pred = body; sub(/^[^:]*:[ \t]*/, "", pred)
            if ((sname in gbnd) && elemOf[sname] == "Int") {
                Nn = gbnd[sname]; out = ""
                for (k = 0; k < Nn; k++) {
                    pk = subst_tok(pred, bvar, sname "_" k)
                    out = out (k ? " ∧ " : "") "((" k " < " sname "_len) ⇒ (" pk "))"
                }
                O[++on] = ind "(" out ")"
                continue
            }
        }

        # whole-line Int membership  `y ∈ xs`
        if (code ~ /∈/) {
            rhs = code; sub(/^.*∈[ \t]*/, "", rhs); rhs = trim(rhs)
            lhs = code; sub(/[ \t]*∈.*$/, "", lhs); lhs = trim(lhs)
            if ((rhs in gbnd) && elemOf[rhs] == "Int" && lhs !~ /[ \t]/) {
                Nn = gbnd[rhs]; out = ""
                for (k = 0; k < Nn; k++)
                    out = out (k ? " ∨ " : "") "((" k " < " rhs "_len) ∧ (" lhs " = " rhs "_" k "))"
                O[++on] = ind "(" out ")"
                continue
            }
        }

        # default path: ∃-expansion, then literal-index, then card
        t = subst_exists(s)
        t = subst_index(t)
        t = subst_card(t)
        O[++on] = t
    }

    # ── COMPLETENESS CHECK: no bare registered tokens may survive ───────
    bad = 0
    for (i = 1; i <= on; i++) {
        code = strip_comment(O[i])
        ntok = ""
        for (p = 1; p <= length(code) + 1; p++) {
            c = (p <= length(code) ? substr(code, p, 1) : " ")
            if (c ~ /[A-Za-z0-9_]/) { ntok = ntok c; continue }
            if (ntok != "") {
                base = ntok; sub(/^_/, "", base)
                if ((base in gbnd) && (ntok == base || ntok == "_" base)) {
                    printf("lower-bounded-seq: line %d: unsupported use of bounded Seq `%s` survives lowering:\n    %s\n", i, ntok, trim(O[i])) > "/dev/stderr"
                    bad = 1
                }
                ntok = ""
            }
        }
    }
    if (bad) exit 1
    for (i = 1; i <= on; i++) print O[i]
}
'
