#!/usr/bin/env bash
# TODO: rewrite in Evident (compiler2/passes/ once the self-hosting seam
# exists — see docs/plans/post-cutover-roadmap.md "host passes in Evident")
#
# flatten-body-records.sh — body-bearing record membership → flat-scalar lowering.
#
# Reads Evident source on stdin, writes lowered source to stdout. Runs AFTER
# autocarry (so both `r ∈ T` and `_r ∈ T` already exist) and BEFORE
# lower-bounded-seq in flatten-evident.sh.
#
# WHY: the self-hosted compiler caps its record registry at 3 types and only
# consults it for record types that carry an INVARIANT BODY. A bare carried
# membership `r ∈ T` where T is a body-record type past the cap fails (rc=7,
# unresolved field). This pass lowers such memberships to plain underscore-
# named scalars, which the compiler+autocarry handle natively past any cap.
# Bodyless record types (no invariant lines) already work and pass through
# untouched.
#
# A "body-record type" is `type T(f1 ∈ t1, …)` followed by >=1 non-comment,
# non-blank indented line before the next top-level construct. The live
# instance `r ∈ T` lowers to `r_<f> ∈ <t>` per field plus the body invariants
# with each field name f rewritten to r_<f>; the carry `_r ∈ T` lowers to
# `_r_<f> ∈ <t>` per field (no invariants); dotted accesses `r.field`
# elsewhere become `r_field`. Detection is generic (not hardcoded).

set -u -o pipefail

exec awk '
function is_blank(s)    { return s ~ /^[[:space:]]*$/ }
function is_comment(s)  { t = s; sub(/^[[:space:]]+/, "", t); return t ~ /^--/ }
function indent_of(s,   m) { match(s, /^[[:space:]]*/); return substr(s, 1, RLENGTH) }

# Net parenthesis depth change of a line: (count of "(") - (count of ")").
function paren_delta(s,   d, k, ch) {
    d = 0
    for (k = 1; k <= length(s); k++) {
        ch = substr(s, k, 1)
        if (ch == "(") d++
        else if (ch == ")") d--
    }
    return d
}

# A top-level construct starts at column 0 (no leading whitespace) and is
# non-blank.
function is_toplevel(s) { return s !~ /^[[:space:]]/ && !is_blank(s) }

# Filter 1 (all-primitive-scalar header): true iff every header field type is
# one of Int/Nat/Bool/String/Real. Any record-name / Seq(...) / generic field
# type means this is a VALUE record (e.g. Sprite(pos ∈ IVec2)), not a carried-
# scalar-state record — disqualify. Mirrors the field parse in Pass A.
function hdr_all_primitive(plist,   np, parts, p, part, ty, pend, grp, ng, gn, g) {
    np = split(plist, parts, ",")
    pend = 0
    for (p = 1; p <= np; p++) {
        part = parts[p]
        if (index(part, "∈") > 0) {
            ty = part; sub(/^.*∈/, "", ty); gsub(/[[:space:]]/, "", ty)
            if (ty != "Int" && ty != "Nat" && ty != "Bool" &&
                ty != "String" && ty != "Real")
                return 0
            pend = 0
        } else {
            # bare name in a shared-type group; type resolved at next typed part
            pend = 1
        }
    }
    return 1
}

# Word-boundary global replace of identifier `from` with `to` in `s`.
# Identifier chars are [A-Za-z0-9_]; a match must be bounded by line
# start/end or a non-identifier char on both sides.
function wbreplace(s, from, to,   out, rest, idx, before, after, fc, lc) {
    out = ""
    rest = s
    while (1) {
        idx = index(rest, from)
        if (idx == 0) { out = out rest; break }
        before = (idx == 1) ? "" : substr(rest, idx - 1, 1)
        after  = substr(rest, idx + length(from), 1)
        # boundary check: char before and char after must not be identifier chars
        if (before ~ /[A-Za-z0-9_]/ || after ~ /[A-Za-z0-9_]/) {
            # not a boundary match: emit up through this occurrence start+1 and continue
            out = out substr(rest, 1, idx)
            rest = substr(rest, idx + 1)
        } else {
            out = out substr(rest, 1, idx - 1) to
            rest = substr(rest, idx + length(from))
        }
    }
    return out
}

# Rename dotted field access `r.field` -> `r_field` for instance var `r`.
# Left boundary on r: line start or non-identifier char. The dot+field:
# field is [A-Za-z][A-Za-z0-9_]* followed by a non-identifier boundary.
function dotreplace(s, r,   out, rest, idx, before, fld, ch, j, after) {
    out = ""
    rest = s
    while (1) {
        idx = index(rest, r ".")
        if (idx == 0) { out = out rest; break }
        before = (idx == 1) ? "" : substr(rest, idx - 1, 1)
        if (before ~ /[A-Za-z0-9_]/) {
            # not a left boundary: skip past this start
            out = out substr(rest, 1, idx)
            rest = substr(rest, idx + 1)
            continue
        }
        # collect the field name after the dot
        j = idx + length(r) + 1   # position of first field char
        fld = ""
        while (j <= length(rest)) {
            ch = substr(rest, j, 1)
            if (ch ~ /[A-Za-z0-9_]/) { fld = fld ch; j++ } else break
        }
        # field must start with a letter and be non-empty
        if (fld == "" || fld ~ /^[0-9_]/) {
            out = out substr(rest, 1, idx + length(r))   # emit through the dot
            rest = substr(rest, idx + length(r) + 1)
            continue
        }
        # valid: r.fld -> r_fld
        out = out substr(rest, 1, idx - 1) r "_" fld
        rest = substr(rest, j)
    }
    return out
}

# ---- Phase 1: scan ----
{ lines[NR] = $0 }
NR == 0 { }

END {
    n = NR
    # Pass A: identify body-record types and capture fields + body lines.
    for (i = 1; i <= n; i++) {
        line = lines[i]
        if (line ~ /^type[[:space:]]/ && line ~ /\(/) {
            # extract type name
            hdr = line
            sub(/^type[[:space:]]+/, "", hdr)
            tname = hdr
            sub(/\(.*/, "", tname)
            gsub(/[[:space:]]/, "", tname)
            # Accumulate the full header by balancing parens across lines.
            # The header may span multiple physical lines; field list ends at
            # the line where paren depth returns to 0.
            depth = paren_delta(line)
            hdrtext = line
            hdrspan[i] = 1
            hdrend = i
            while (depth > 0 && hdrend < n) {
                hdrend++
                hdrtext = hdrtext " " lines[hdrend]
                hdrspan[hdrend] = 1
                depth += paren_delta(lines[hdrend])
            }
            # extract parenthesized field list from the joined header text
            plist = hdrtext
            sub(/^[^(]*\(/, "", plist)
            sub(/\)[^)]*$/, "", plist)
            # determine if this type has a body (lines AFTER the closing line).
            # Filter 2 (constraint-only body): a body line carrying a membership
            # ` ∈ ` is a derived-field line (`offset ∈ IVec2 = …`) — that marks a
            # VALUE record, not a carried-scalar-state record. Disqualify.
            bcount = 0
            bodyhasmember = 0
            j = hdrend + 1
            while (j <= n) {
                bl = lines[j]
                if (is_toplevel(bl)) break
                if (is_blank(bl) || is_comment(bl)) { j++; continue }
                # a real indented invariant line
                if (bl ~ / ∈ / || index(bl, "∈") > 0) {
                    # body membership: this is a value record, not flattenable
                    bodyhasmember = 1
                }
                bodylines[tname, bcount] = bl
                bcount++
                j++
            }
            if (bcount > 0 && !bodyhasmember && hdr_all_primitive(plist)) {
                isbody[tname] = 1
                bodycount[tname] = bcount
                # parse fields: split plist on commas; each part "names ∈ Type"
                # (note multi-name "a, b ∈ Int" — but compiler2 body-records all
                #  use single-name parts; handle the general "name ∈ Type" only,
                #  and also expand shared-type groups split on commas naively.)
                nf = 0
                np = split(plist, parts, ",")
                pendingnames = ""
                for (p = 1; p <= np; p++) {
                    part = parts[p]
                    if (index(part, "∈") > 0 || part ~ / ∈ /) {
                        # has a type: "names ∈ Type"
                        nm = part; ty = part
                        sub(/∈.*$/, "", nm); sub(/^.*∈/, "", ty)
                        gsub(/[[:space:]]/, "", nm); gsub(/[[:space:]]/, "", ty)
                        # nm may be one name; prepend any pending shared names
                        grp = pendingnames nm
                        pendingnames = ""
                        ng = split(grp, gn, /,/)
                        for (g = 1; g <= ng; g++) {
                            fn = gn[g]; gsub(/[[:space:]]/, "", fn)
                            if (fn == "") continue
                            fields[tname, nf] = fn
                            ftype[tname, nf] = ty
                            nf++
                        }
                    } else {
                        # bare name, type comes later (shared group)
                        bn = part; gsub(/[[:space:]]/, "", bn)
                        if (bn != "") pendingnames = pendingnames bn ","
                    }
                }
                fieldcount[tname] = nf
            }
        }
    }

    # Pass B: identify body-record INSTANCE variables via `VAR ∈ T` lines
    # where T is a body-record and nothing follows T.
    ninst = 0
    for (i = 1; i <= n; i++) {
        line = lines[i]
        if (i in hdrspan) continue              # skip type-header (cont.) lines
        if (is_toplevel(line)) continue        # skip type decls / top-level
        t = line; sub(/^[[:space:]]+/, "", t)
        # match: VAR ∈ T  (T an identifier, end of line)
        if (t ~ /^_?[A-Za-z][A-Za-z0-9_]*[[:space:]]+∈[[:space:]]+[A-Za-z][A-Za-z0-9_]*[[:space:]]*$/) {
            v = t; sub(/[[:space:]]*∈.*$/, "", v); gsub(/[[:space:]]/, "", v)
            rt = t; sub(/^.*∈[[:space:]]*/, "", rt); gsub(/[[:space:]]/, "", rt)
            if (rt in isbody) {
                # strip a leading underscore to get the base instance name
                base = v
                if (substr(base, 1, 1) == "_") base = substr(base, 2)
                if (!(base in instseen)) {
                    instseen[base] = 1
                    insttype[base] = rt
                    instvars[ninst++] = base
                }
            }
        }
    }

    # order instance names longest-first for safe dotted-replace
    for (a = 0; a < ninst; a++) order[a] = instvars[a]
    for (a = 0; a < ninst; a++)
        for (b = a + 1; b < ninst; b++)
            if (length(order[b]) > length(order[a])) {
                tmp = order[a]; order[a] = order[b]; order[b] = tmp
            }

    # ---- Phase 2: emit ----
    i = 1
    while (i <= n) {
        line = lines[i]

        # Is this the `type T(...)` decl of a body-record? Keep it and its body
        # verbatim (advance past the body so we do not also treat body lines).
        if (line ~ /^type[[:space:]]/ && line ~ /\(/) {
            tn = line; sub(/^type[[:space:]]+/, "", tn); sub(/\(.*/, "", tn)
            gsub(/[[:space:]]/, "", tn)
            if (tn in isbody) {
                print line
                i++
                while (i <= n) {
                    bl = lines[i]
                    if (is_toplevel(bl)) break
                    print bl
                    i++
                }
                continue
            }
            # non-body type: fall through to generic handling below
        }

        # Is this a body-record instance membership line?  VAR ∈ T (eol)
        if (!is_toplevel(line) && !(i in hdrspan)) {
            tt = line; sub(/^[[:space:]]+/, "", tt)
            if (tt ~ /^_?[A-Za-z][A-Za-z0-9_]*[[:space:]]+∈[[:space:]]+[A-Za-z][A-Za-z0-9_]*[[:space:]]*$/) {
                v = tt; sub(/[[:space:]]*∈.*$/, "", v); gsub(/[[:space:]]/, "", v)
                rt = tt; sub(/^.*∈[[:space:]]*/, "", rt); gsub(/[[:space:]]/, "", rt)
                if (rt in isbody) {
                    ind = indent_of(line)
                    iscarry = (substr(v, 1, 1) == "_")
                    base = iscarry ? substr(v, 2) : v
                    nf = fieldcount[rt]
                    # emit one scalar decl per field
                    for (f = 0; f < nf; f++) {
                        print ind v "_" fields[rt, f] " ∈ " ftype[rt, f]
                    }
                    # live instances also get the invariants (field-renamed)
                    if (!iscarry) {
                        nb = bodycount[rt]
                        for (b = 0; b < nb; b++) {
                            bl = bodylines[rt, b]
                            # rebuild at THIS indent: strip original indent, re-add
                            body = bl; sub(/^[[:space:]]*/, "", body)
                            # rename each field name f -> v_f (word boundary)
                            for (f = 0; f < nf; f++) {
                                body = wbreplace(body, fields[rt, f], v "_" fields[rt, f])
                            }
                            print ind body
                        }
                    }
                    i++
                    continue
                }
            }
        }

        # Generic line: rename dotted field accesses of every body-record
        # instance var (both r.field and _r.field).
        out = line
        for (a = 0; a < ninst; a++) {
            r = order[a]
            out = dotreplace(out, r)
            out = dotreplace(out, "_" r)
        }
        print out
        i++
    }
}
'
