#!/usr/bin/env bash
# Mechanical lint runner. Loads each active rule's check function,
# runs them against the working tree, prints per-rule pass/fail,
# exits non-zero if anything failed.
#
# Convention: each `check_*` function corresponds 1:1 to a rule
# file under `lints/rules/`. The function's first comment line
# cites the rule by ID.
#
# Add a new rule: write the rule file in lints/rules/, add a
# `check_<short-name>` function below, append its name to ACTIVE.
#
# Run with `./test.sh --lints-only` for fast feedback.

set -u -o pipefail   # NOT -e — we want to run all checks even if
                     # one fails, then aggregate.

cd "$(dirname "$0")/.."

if [ -t 1 ]; then
    GREEN=$(printf '\033[0;32m'); RED=$(printf '\033[0;31m')
    DIM=$(printf '\033[2m');     OFF=$(printf '\033[0m')
else
    GREEN=''; RED=''; DIM=''; OFF=''
fi

# Per-check helpers. `report` records the result of one check
# (rule_id + pass/fail + offending lines if any). `failed_count`
# tracks total failures.
failed_count=0
results=()

report() {
    local rule_id="$1"
    local result="$2"   # "pass" or "fail"
    local detail="${3:-}"
    if [ "$result" = "pass" ]; then
        if [ -n "$detail" ]; then
            echo "${GREEN}✓${OFF} $rule_id  $detail"
        else
            echo "${GREEN}✓${OFF} $rule_id"
        fi
    else
        echo "${RED}✗${OFF} $rule_id"
        if [ -n "$detail" ]; then
            echo "$detail" | sed 's/^/    /'
        fi
        failed_count=$((failed_count + 1))
    fi
    results+=("$rule_id:$result")
}

# ── Helpers used by multiple checks ─────────────────────────────

# strip_ev_comments <file>: prints the file with `--` comments
# stripped (entire line comments AND trailing comments).
strip_ev_comments() {
    sed -E 's/--.*$//' "$1"
}

# strip_rs_comments <file>: prints the file with // and /// and /*…*/
# (single-line) comments stripped. Doesn't handle multi-line block
# comments (we don't really use them); good enough.
strip_rs_comments() {
    sed -E 's|//.*$||' "$1"
}

# strip_rs_test_modules <file>: prints the file with the contents of
# any `#[cfg(test)]`-gated item (mod, fn, impl, …) replaced with
# blank lines (so line numbers stay stable for downstream grep
# diagnostics). Recognizes compound cfg predicates that include
# `test` as one alternative — `#[cfg(test)]`, `#[cfg(any(test, …))]`,
# `#[cfg(all(test, …))]`, `#[cfg_attr(test, …)]`. Tests legitimately
# reference real libraries (libc to exercise FFI etc.); they're not
# subject to "no library-specific in language-core" rules.
#
# Detection: regex match the cfg attribute, then track brace depth
# from the next `{` until it returns to 0.
#
# `test` is matched with explicit non-word-char lookahead
# (`test[^a-zA-Z0-9_]`) rather than `\<test\>` because BSD awk
# (macOS default) doesn't support GNU's word-boundary escapes.
# Side-benefit: identifiers like `testfeature` won't false-match.
#
# Caveat: brace counting does not account for braces inside string
# or char literals. A test fn like `let s = "}}";` would trip the
# counter early. None of today's test code does this, but
# parser-level fixtures might in the future — fix when it actually
# breaks.
strip_rs_test_modules() {
    awk '
        /^[[:space:]]*#\[(cfg|cfg_attr)\([^)]*test[^a-zA-Z0-9_]/ {
            in_test = 1; depth = 0; print ""; next
        }
        in_test {
            opens = gsub(/\{/, "&")
            closes = gsub(/\}/, "&")
            depth += opens - closes
            print ""
            if (depth <= 0 && (opens > 0 || closes > 0)) in_test = 0
            next
        }
        { print }
    ' "$1"
}

# strip_py_comments <file>: prints the file with # comments stripped.
strip_py_comments() {
    sed -E 's/#.*$//' "$1"
}

# ── AP-001: no library-specific in language-core ──────────────
check_no_library_specific_in_language_core() {
    # AP-001: forbidden tokens in language-core role files.
    local files=(
        runtime/src/ast.rs
        runtime/src/lexer.rs
        runtime/src/parser.rs
        runtime/src/pretty.rs
        runtime/src/subscriptions.rs
        runtime/src/runtime.rs
        runtime/src/effect_loop.rs
        runtime/src/effect_dispatch.rs
        runtime/src/ffi.rs
    )
    while IFS= read -r f; do files+=("$f"); done < <(find runtime/src/translate -name '*.rs')

    local pattern='SDL_|Sdl[A-Z][a-zA-Z]|\bGl[A-Z]|Glsl|Audio[A-Z]|\.dylib|\.framework/|/opt/homebrew/lib/|/usr/lib/lib'
    local violations=""
    for f in "${files[@]}"; do
        [ -f "$f" ] || continue
        # Strip #[cfg(test)] mod blocks (test code legitimately
        # references real libraries to exercise the FFI primitive),
        # then find lines matching the pattern, then drop pure-
        # comment lines. `grep -n` reports line numbers from the
        # stripped text, which uses blank-line replacement to
        # preserve original line numbers.
        local hits
        hits=$(strip_rs_test_modules "$f" \
               | grep -nE "$pattern" \
               | grep -vE ':[[:space:]]*//' \
               || true)
        if [ -n "$hits" ]; then
            violations+="$f:"$'\n'"$hits"$'\n'
        fi
    done
    if [ -z "$violations" ]; then
        report AP-001 pass
    else
        report AP-001 fail "$violations"
    fi
}

# ── AP-002: no raw FFI in examples ─────────────────────────────
check_no_raw_ffi_in_examples() {
    # AP-002: word-boundary FFI primitives in examples/*.ev.
    local pattern='\b(LibCall|FFICall|FFIOpen|FFILookup)\b'
    local violations=""
    for f in examples/*.ev; do
        [ -f "$f" ] || continue
        # Strip -- comments and re-grep.
        local hits
        hits=$(strip_ev_comments "$f" | grep -nE "$pattern" || true)
        if [ -n "$hits" ]; then
            violations+="$f:"$'\n'"$hits"$'\n'
        fi
    done
    if [ -z "$violations" ]; then
        report AP-002 pass
    else
        report AP-002 fail "$violations"
    fi
}

# ── AP-003: no platform paths or C symbols in examples ────────
check_no_platform_paths_or_c_symbols_in_examples() {
    # AP-003: dylib paths + literal C-symbol-name strings.
    local path_pattern='\.dylib|\.framework/|/opt/homebrew/lib/|/usr/lib/lib|/usr/lib/x86_64'
    local sym_pattern='"SDL_[A-Z]|"gl[A-Z]|"NS[A-Z]'
    local pattern="($path_pattern|$sym_pattern)"
    local violations=""
    for f in examples/*.ev; do
        [ -f "$f" ] || continue
        local hits
        hits=$(strip_ev_comments "$f" | grep -nE "$pattern" || true)
        if [ -n "$hits" ]; then
            violations+="$f:"$'\n'"$hits"$'\n'
        fi
    done
    if [ -z "$violations" ]; then
        report AP-003 pass
    else
        report AP-003 fail "$violations"
    fi
}

# ── AP-004: no skip / xfail in conformance ─────────────────────
check_no_skip_or_xfail_in_conformance() {
    # AP-004: pytest skip/xfail markers and the KNOWN_FAILING dict
    # pattern that the previous triage removed.
    local pattern='pytest\.mark\.(xfail|skip)|pytest\.skip\(|add_marker.*xfail|^\s*KNOWN_FAILING\s*='
    local violations=""
    for f in $(find tests/conformance -name '*.py' 2>/dev/null); do
        [ -f "$f" ] || continue
        local hits
        hits=$(strip_py_comments "$f" | grep -nE "$pattern" || true)
        if [ -n "$hits" ]; then
            violations+="$f:"$'\n'"$hits"$'\n'
        fi
    done
    if [ -z "$violations" ]; then
        report AP-004 pass
    else
        report AP-004 fail "$violations"
    fi
}

# ── AP-005: no #[ignore] in rust tests ─────────────────────────
check_no_ignore_in_rust_tests() {
    # AP-005: #[ignore] on tests under runtime/tests/.
    local pattern='#\[ignore'
    local violations=""
    for f in $(find runtime/tests -name '*.rs' 2>/dev/null); do
        [ -f "$f" ] || continue
        local hits
        hits=$(strip_rs_comments "$f" | grep -nE "$pattern" || true)
        if [ -n "$hits" ]; then
            violations+="$f:"$'\n'"$hits"$'\n'
        fi
    done
    if [ -z "$violations" ]; then
        report AP-005 pass
    else
        report AP-005 fail "$violations"
    fi
}

# ── AP-009: no solver.assert in declare.rs ─────────────────────
check_no_solver_assert_in_declare() {
    # AP-009: declaration must not assert on the Solver — only
    # allocate Z3 constants. Asserting belongs in `inline`.
    local f=runtime/src/translate/declare.rs
    [ -f "$f" ] || { report AP-009 pass "(file missing — skip)"; return; }
    local pattern='solver\.(assert|add)\b'
    local hits
    hits=$(strip_rs_test_modules "$f" \
           | grep -nE "$pattern" \
           | grep -vE ':[[:space:]]*//' \
           || true)
    if [ -z "$hits" ]; then
        report AP-009 pass
    else
        report AP-009 fail "$f:"$'\n'"$hits"
    fi
}

# ── AP-010: no Z3 expression construction in preprocess.rs ────
check_no_z3_exprs_in_preprocess() {
    # AP-010: preprocess is an AST→AST rewrite stage. Z3
    # expression construction belongs in `exprs.rs`.
    local f=runtime/src/translate/preprocess.rs
    [ -f "$f" ] || { report AP-010 pass "(file missing — skip)"; return; }
    local pattern='z3::ast::(Int|Bool|Real|String|Datatype)::(new_const|new|from_)'
    local hits
    hits=$(strip_rs_test_modules "$f" \
           | grep -nE "$pattern" \
           | grep -vE ':[[:space:]]*//' \
           || true)
    if [ -z "$hits" ]; then
        report AP-010 pass
    else
        report AP-010 fail "$f:"$'\n'"$hits"
    fi
}

# ── AP-011: no preprocess ↔ exprs cycle ────────────────────────
check_no_preprocess_exprs_cycle() {
    # AP-011: shared helpers between preprocess and exprs belong
    # in types.rs (the shared data leaf). Mutual imports are
    # forbidden — that's the historical cycle this rule guards.
    local pre=runtime/src/translate/preprocess.rs
    local exp=runtime/src/translate/exprs.rs
    local violations=""
    if [ -f "$pre" ]; then
        local hits
        hits=$(strip_rs_test_modules "$pre" \
               | grep -nE 'use (super::|crate::translate::)exprs' \
               | grep -vE ':[[:space:]]*//' \
               || true)
        if [ -n "$hits" ]; then
            violations+="$pre:"$'\n'"$hits"$'\n'
        fi
    fi
    if [ -f "$exp" ]; then
        local hits
        hits=$(strip_rs_test_modules "$exp" \
               | grep -nE 'use (super::|crate::translate::)preprocess' \
               | grep -vE ':[[:space:]]*//' \
               || true)
        if [ -n "$hits" ]; then
            violations+="$exp:"$'\n'"$hits"$'\n'
        fi
    fi
    if [ -z "$violations" ]; then
        report AP-011 pass
    else
        report AP-011 fail "$violations"
    fi
}

# ── AP-012: no specific bridge structs in scheduler ────────────
check_no_specific_bridges_in_scheduler() {
    # AP-012: effect_loop.rs runs against a registry of bridges,
    # not against named struct types. Scheduler must not reach
    # past the abstraction.
    local f=runtime/src/effect_loop.rs
    [ -f "$f" ] || { report AP-012 pass "(file missing — skip)"; return; }
    local pattern='event_sources::(FrameTimer|SigintSource|StdinSource|WallClockSource|FileWatcherSource|FileLineReader|OneShotShellSource|SdlWindowSource|GlProgramSource|GlContext)'
    local hits
    hits=$(strip_rs_test_modules "$f" \
           | grep -nE "$pattern" \
           | grep -vE ':[[:space:]]*//' \
           || true)
    if [ -z "$hits" ]; then
        report AP-012 pass
    else
        report AP-012 fail "$f:"$'\n'"$hits"
    fi
}

# ── AP-013: no stdlib/*.ev path literals in language-core ──────
check_no_stdlib_paths_in_language_core() {
    # AP-013: hardcoded stdlib shim paths are a registry-layer
    # concern. They must not appear in language-core files.
    local files=(
        runtime/src/ast.rs
        runtime/src/lexer.rs
        runtime/src/parser.rs
        runtime/src/pretty.rs
        runtime/src/subscriptions.rs
        runtime/src/runtime.rs
        runtime/src/effect_loop.rs
        runtime/src/effect_dispatch.rs
        runtime/src/ffi.rs
    )
    while IFS= read -r f; do files+=("$f"); done < <(find runtime/src/translate -name '*.rs')

    local pattern='"stdlib/[^"]*\.ev"'
    local violations=""
    for f in "${files[@]}"; do
        [ -f "$f" ] || continue
        local hits
        hits=$(strip_rs_test_modules "$f" \
               | grep -nE "$pattern" \
               | grep -vE ':[[:space:]]*//' \
               || true)
        if [ -n "$hits" ]; then
            violations+="$f:"$'\n'"$hits"$'\n'
        fi
    done
    if [ -z "$violations" ]; then
        report AP-013 pass
    else
        report AP-013 fail "$violations"
    fi
}

# ── AP-014: every cmd_X file has main.rs dispatch ──────────────
check_cmd_files_have_dispatch_arm() {
    # AP-014: each commands/X.rs (other than common/mod) must
    # declare `pub fn cmd_X` AND main.rs must dispatch the verb
    # form (kebab-case) to it.
    local main_rs=runtime/src/main.rs
    [ -f "$main_rs" ] || { report AP-014 fail "missing $main_rs"; return; }
    local violations=""
    for f in runtime/src/commands/*.rs; do
        [ -f "$f" ] || continue
        local base
        base=$(basename "$f" .rs)
        case "$base" in
            common|mod) continue ;;
        esac
        if ! grep -q "pub fn cmd_$base" "$f"; then
            violations+="$f: missing \`pub fn cmd_$base\`"$'\n'
        fi
        local verb="${base//_/-}"
        # Look for the verb as a quoted match arm key in main.rs.
        if ! grep -qE "\"$verb\"[[:space:]]*=>" "$main_rs"; then
            violations+="$main_rs: missing dispatch arm \"$verb\" => commands::$base::cmd_$base(...)"$'\n'
        fi
    done
    if [ -z "$violations" ]; then
        report AP-014 pass
    else
        report AP-014 fail "$violations"
    fi
}

# ── AP-015: every pub mod in lib.rs has external consumer ─────
check_pub_mod_has_external_use() {
    # AP-015: a `pub mod X` in lib.rs must have a corresponding
    # evident_runtime::X reference somewhere external (tests,
    # benches, or commands). Otherwise the `pub` is speculative.
    local lib=runtime/src/lib.rs
    [ -f "$lib" ] || { report AP-015 fail "missing $lib"; return; }
    local violations=""
    # Extract `pub mod NAME;` from lib.rs (production code only).
    local mods
    mods=$(strip_rs_test_modules "$lib" \
           | grep -vE '^[[:space:]]*//' \
           | grep -oE '^pub mod [a-z_]+' \
           | awk '{print $3}')
    for m in $mods; do
        # Match either `evident_runtime::<m>` (path use, with `::`
        # or word boundary after) OR `<m>` inside a brace import
        # `use evident_runtime::{..., <m>, ...}`.
        local pattern="evident_runtime::($m\b|\{[^}]*\b$m\b)"
        local hits
        hits=$(grep -rEln "$pattern" runtime/tests/ runtime/benches/ runtime/src/commands/ 2>/dev/null || true)
        if [ -z "$hits" ]; then
            violations+="$lib: \`pub mod $m\` has no external consumer (searched runtime/tests, runtime/benches, runtime/src/commands)"$'\n'
        fi
    done
    if [ -z "$violations" ]; then
        report AP-015 pass
    else
        report AP-015 fail "$violations"
    fi
}

# ── ACTIVE rules (all `check_*` functions to run) ──────────────
ACTIVE=(
    check_no_library_specific_in_language_core   # AP-001
    check_no_raw_ffi_in_examples                  # AP-002
    check_no_platform_paths_or_c_symbols_in_examples  # AP-003
    check_no_skip_or_xfail_in_conformance         # AP-004
    check_no_ignore_in_rust_tests                 # AP-005
    check_no_solver_assert_in_declare             # AP-009
    check_no_z3_exprs_in_preprocess               # AP-010
    check_no_preprocess_exprs_cycle               # AP-011
    check_no_specific_bridges_in_scheduler        # AP-012
    check_no_stdlib_paths_in_language_core        # AP-013
    check_cmd_files_have_dispatch_arm             # AP-014
    check_pub_mod_has_external_use                # AP-015
    # AP-006 / AP-007 / AP-008 are AST-based — see runtime/tests/lints.rs
)

# ── Run ────────────────────────────────────────────────────────
echo "${DIM}Running ${#ACTIVE[@]} mechanical lint checks…${OFF}"
for fn in "${ACTIVE[@]}"; do
    "$fn"
done

echo
if [ "$failed_count" -eq 0 ]; then
    echo "${GREEN}lints: all ${#ACTIVE[@]} checks passed.${OFF}"
    exit 0
else
    echo "${RED}lints: $failed_count check(s) failed.${OFF}"
    exit 1
fi
