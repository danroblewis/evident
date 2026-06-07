#!/usr/bin/env bash
# TODO: rewrite in Evident
#
# stitch-stage0.sh — splice fossil-compiled claim bodies into a
# hand-written driver shell (the stage-0 architecture sizing spike;
# see docs/plans/stage0-sizing.md).
#
# The committed compiler.smt2 ("the fossil") can compile subset-
# disciplined claims (match→ite, bare-name composition, single-binop
# pins) but cannot emit the multi-tick capture-driver shape (conditional
# effects, last_results readback, payload extraction, phase machine).
# So: the shell is hand-written .smt2, the per-node logic is fossil-
# compiled, and this script stitches the two.
#
# Template marker syntax (a line anywhere in the template):
#
#   ;; @splice <emit-file> <old_>=<new_> [<old_>=<new_> ...]
#
# For each marker, this script reads <emit-file> (resolved relative to
# EMIT_DIR), extracts the CLAIM BODY lines — `(declare-fun X …)` where X
# starts with a registered prefix, and `(assert …)` lines mentioning a
# registered prefix — and applies the renames. Identity renames
# (`pd_=pd_`) register a prefix without renaming, which is how a
# single-instantiation claim declares its prefixes.
#
# NAMING DISCIPLINE (the stitcher contract): every claim-local variable
# in a fossil-compiled .ev source MUST begin with a prefix that is
# (a) unique per source file, (b) registered on the @splice marker, and
# (c) never a substring of any shell symbol or another claim's prefix.
# The fossil's preamble (manifest comments, datatype decls,
# is_first_tick / last_results / effects declares-asserts) carries no
# registered prefix and is dropped. One fossil compile (~50 s) can be
# instantiated N times under N prefixes — rename does the
# parameterization the subset's parameterless claims cannot.
#
# Usage:
#   scripts/stitch-stage0.sh <template> <emit-dir> > stitched.smt2
set -eu

TEMPLATE="${1:?usage: stitch-stage0.sh <template> <emit-dir>}"
EMIT_DIR="${2:?usage: stitch-stage0.sh <template> <emit-dir>}"

while IFS= read -r tline; do
    case "$tline" in
        ";; @splice "*)
            spec="${tline#*@splice }"
            set -- $spec
            emit="$EMIT_DIR/$1"; shift
            [ -f "$emit" ] || { echo "stitch-stage0: missing emit $emit" >&2; exit 1; }
            # Build the prefix alternation and the sed rename program.
            prefixes=""
            sedprog=""
            for pair in "$@"; do
                old="${pair%%=*}"; new="${pair#*=}"
                prefixes="${prefixes:+$prefixes|}$old"
                if [ "$old" != "$new" ]; then
                    sedprog="$sedprog -e s/$old/$new/g"
                fi
            done
            echo ";; -- spliced from $(basename "$emit") [$*] --"
            # shellcheck disable=SC2086
            grep -E "^\(declare-fun ($prefixes)|^\(assert .*($prefixes)" "$emit" \
                | { if [ -n "$sedprog" ]; then sed $sedprog; else cat; fi; }
            ;;
        *)
            printf '%s\n' "$tline"
            ;;
    esac
done < "$TEMPLATE"
