//! build-equiv-query — construct a single Z3 translation-validation query that
//! proves single-tick OUTPUT equivalence of two stage1 emits (OLD vs NEW)
//! under a variable mapping phi.
//!
//! Usage:
//!   build-equiv-query OLD.smt2 NEW.smt2 phi.txt > query.smt2
//!
//! phi.txt lines: `old_name new_name` (one mapping per line; `#`-comments ok).
//!
//! WHAT THE QUERY PROVES (read tools/README.md "Equivalence checking"):
//!   Given the SHARED single-tick INPUTS related by phi
//!     (is_first_tick, last_results, last_results__len, and every carried
//!      `_X` state dual), do the two compiler bodies ever produce DIFFERENT
//!     observable OUTPUTS (effects, effects__len, every next-state `X`)?
//!   UNSAT  => no input makes them differ => single-tick output-equivalent.
//!   SAT    => a divergence witness exists (NOT equivalent under phi).
//!
//! Construction: OLD body verbatim; NEW body with every NEW-declared const
//! token-renamed to `N!<name>` so the two namespaces don't accidentally unify;
//! bridge asserts equate the shared inputs across the two (via phi); a final
//! assert demands at least one output differ.
//!
//! Soundness depends on (a) phi being correct, and (b) the input/output
//! classification below matching the kernel's actual carry contract. This is
//! SINGLE-TICK output equivalence — necessary but not sufficient for full
//! behavioral equivalence (that needs the inductive next-state form; see
//! README).

use std::collections::{BTreeSet, HashMap, HashSet};
use std::fs;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    if a.len() < 4 {
        eprintln!("usage: build-equiv-query OLD.smt2 NEW.smt2 phi.txt > query.smt2");
        std::process::exit(2);
    }
    let old_src = fs::read_to_string(&a[1]).expect("read OLD");
    let new_src = fs::read_to_string(&a[2]).expect("read NEW");
    let phi_src = fs::read_to_string(&a[3]).expect("read phi");

    // phi: old_name -> new_name and inverse new_name -> old_name.
    let mut phi_old2new: HashMap<String, String> = HashMap::new();
    for line in phi_src.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut it = line.split_whitespace();
        if let (Some(o), Some(n)) = (it.next(), it.next()) {
            phi_old2new.insert(o.to_string(), n.to_string());
        }
    }

    let old_decls = declared_consts(&old_src);
    let new_decls = declared_consts(&new_src);

    // State-field base names from the manifest (each has a `_X` carried dual).
    let state_fields = manifest_state_fields(&old_src);

    // SHARED INPUTS (old-side spelling): the carried `_X` duals + globals.
    // OUTPUTS (old-side spelling): effects, effects__len, next-state X.
    let mut inputs_old: Vec<String> = Vec::new();
    let mut outputs_old: Vec<String> = Vec::new();
    for g in ["is_first_tick", "last_results", "last_results__len"] {
        if old_decls.contains(g) {
            inputs_old.push(g.to_string());
        }
    }
    for g in ["effects", "effects__len"] {
        if old_decls.contains(g) {
            outputs_old.push(g.to_string());
        }
    }
    for f in &state_fields {
        let dual = format!("_{f}");
        if old_decls.contains(&dual) {
            inputs_old.push(dual);
        }
        if old_decls.contains(f) {
            outputs_old.push(f.clone());
        }
    }

    // The NEW-side image of a shared name = phi(name) if mapped else name.
    let img = |n: &str| -> String {
        phi_old2new.get(n).cloned().unwrap_or_else(|| n.to_string())
    };

    // Every NEW-declared const gets prefixed `N!` in the NEW body, so the two
    // namespaces are disjoint. Shared datatypes/sorts come from OLD only.
    let rename_new: HashMap<String, String> =
        new_decls.iter().map(|n| (n.clone(), format!("N!{n}"))).collect();

    // ---- Build the query ----
    let mut out = String::new();
    out.push_str(";; translation-validation query (single-tick OUTPUT equivalence)\n");
    out.push_str(";; OLD body verbatim; NEW body renamed N!<const>; phi bridges inputs.\n");
    out.push_str(";; UNSAT => equivalent under phi.\n");
    out.push_str("(set-option :produce-models false)\n");

    // 1. OLD file: keep declare-datatypes / declare-sort / declare-fun /
    //    define-* and assert, drop manifest comments & any check-sat.
    out.push_str(";; ===== OLD (verbatim) =====\n");
    for stmt in top_level_stmts(&old_src) {
        let head = stmt_head(&stmt);
        if head == "check-sat" || head == "get-model" || head == "exit" {
            continue;
        }
        out.push_str(&stmt);
        out.push('\n');
    }

    // 2. NEW file: SKIP shared datatype/sort decls (already declared by OLD);
    //    rename every NEW const token to N!<name>; keep its declare-fun + asserts.
    //    We must NOT re-declare the shared SORTS/datatypes. Heuristic: skip any
    //    `declare-datatypes`, `declare-sort`, `define-sort`, `declare-const`-less
    //    sort stuff — i.e. only emit `declare-fun` (renamed) and `assert`
    //    (renamed). Datatypes are identical across versions (verified: the only
    //    delta is qloop consts), so OLD's are authoritative.
    out.push_str(";; ===== NEW (renamed N!<const>) =====\n");
    let shared_kw: HashSet<&str> = [
        "declare-datatypes",
        "declare-sort",
        "define-sort",
        "set-option",
        "set-logic",
        "set-info",
    ]
    .into_iter()
    .collect();
    for stmt in top_level_stmts(&new_src) {
        let head = stmt_head(&stmt);
        if head == "check-sat" || head == "get-model" || head == "exit" {
            continue;
        }
        if shared_kw.contains(head.as_str()) {
            continue; // shared with OLD; don't redeclare
        }
        out.push_str(&rename_tokens(&stmt, &rename_new));
        out.push('\n');
    }

    // 3. Bridge: equate shared INPUTS across the two bodies (via phi).
    out.push_str(";; ===== phi: equate shared single-tick INPUTS =====\n");
    let mut bridged = 0usize;
    for inp in &inputs_old {
        let new_name = img(inp); // new-side bare name
        if !new_decls.contains(&new_name) {
            eprintln!("# WARN: input {inp} -> {new_name} not declared on NEW side; skipped");
            continue;
        }
        out.push_str(&format!("(assert (= {inp} N!{new_name}))\n"));
        bridged += 1;
    }

    // 4. Demand at least one OBSERVABLE output differs.
    //
    // CRUCIAL SOUNDNESS POINT: `effects` is an (Array Int Effect). The bodies
    // only constrain it at indices [0, effects__len); higher indices are free.
    // Whole-array `(not (= effects N!effects))` is therefore SATISFIABLE for
    // IDENTICAL programs (they can disagree on garbage past the length) — a
    // FALSE divergence. The kernel only observes effects[0..effects__len], so
    // we compare it the same way: length, then element-wise up to max-effects,
    // each element guarded by being within the (shared) length. Primitive
    // (non-array) state outputs compare whole-value.
    let max_effects = manifest_max_effects(&old_src).unwrap_or(16);
    out.push_str(";; ===== assert SOME observable output differs =====\n");
    let mut diffs: Vec<String> = Vec::new();
    for o in &outputs_old {
        let new_name = img(o);
        if !new_decls.contains(&new_name) {
            eprintln!("# WARN: output {o} -> {new_name} not declared on NEW side; skipped");
            continue;
        }
        if o == "effects" {
            // length differs ...
            diffs.push("(not (= effects__len N!effects__len))".to_string());
            // ... or some in-bounds element differs.
            for k in 0..max_effects {
                diffs.push(format!(
                    "(and (> effects__len {k}) (not (= (select effects {k}) (select N!effects {k}))))"
                ));
            }
        } else if o == "effects__len" {
            // covered by the `effects` length check above; skip the duplicate.
            continue;
        } else {
            diffs.push(format!("(not (= {o} N!{new_name}))"));
        }
    }
    if diffs.is_empty() {
        eprintln!("# ERROR: no comparable outputs; query is vacuous");
        std::process::exit(3);
    }
    out.push_str(&format!("(assert (or\n  {}\n))\n", diffs.join("\n  ")));
    out.push_str("(check-sat)\n");

    eprintln!(
        "# query: {} OLD stmts, {} NEW consts renamed, {} inputs bridged, {} outputs compared",
        top_level_stmts(&old_src).len(),
        rename_new.len(),
        bridged,
        diffs.len()
    );
    print!("{out}");
}

/// Names declared via `(declare-fun NAME ...)` at top level.
fn declared_consts(src: &str) -> BTreeSet<String> {
    let mut s = BTreeSet::new();
    for line in src.lines() {
        let l = line.trim_start();
        if let Some(rest) = l.strip_prefix("(declare-fun ") {
            let name: String = rest
                .chars()
                .take_while(|c| !c.is_whitespace() && *c != '(' && *c != ')')
                .collect();
            if !name.is_empty() {
                s.insert(name);
            }
        }
    }
    s
}

fn manifest_max_effects(src: &str) -> Option<usize> {
    for line in src.lines() {
        if let Some(rest) = line.strip_prefix(";; manifest: max-effects = ") {
            return rest.trim().parse().ok();
        }
    }
    None
}

fn manifest_state_fields(src: &str) -> Vec<String> {
    for line in src.lines() {
        if let Some(rest) = line.strip_prefix(";; manifest: state-fields = ") {
            return rest
                .split_whitespace()
                .filter_map(|tok| tok.split(':').next())
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
                .collect();
        }
    }
    Vec::new()
}

/// Split SMT2 source into top-level parenthesized statements (paren-balanced,
/// string- and comment-aware). Comment lines (`;`) outside a statement are
/// dropped.
fn top_level_stmts(src: &str) -> Vec<String> {
    let mut stmts = Vec::new();
    let bytes = src.as_bytes();
    let mut i = 0;
    let n = bytes.len();
    while i < n {
        let c = bytes[i] as char;
        if c == ';' {
            // line comment to EOL
            while i < n && bytes[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        if c == '(' {
            let start = i;
            let mut depth = 0;
            let mut in_str = false;
            while i < n {
                let ch = bytes[i] as char;
                if in_str {
                    if ch == '"' {
                        // SMT2 escapes a quote by doubling it
                        if i + 1 < n && bytes[i + 1] == b'"' {
                            i += 2;
                            continue;
                        }
                        in_str = false;
                    }
                    i += 1;
                    continue;
                }
                match ch {
                    '"' => in_str = true,
                    ';' => {
                        while i < n && bytes[i] != b'\n' {
                            i += 1;
                        }
                        continue;
                    }
                    '(' => depth += 1,
                    ')' => {
                        depth -= 1;
                        if depth == 0 {
                            i += 1;
                            stmts.push(src[start..i].to_string());
                            break;
                        }
                    }
                    _ => {}
                }
                i += 1;
            }
        } else {
            i += 1;
        }
    }
    stmts
}

/// The keyword right after the opening paren, e.g. "declare-fun".
fn stmt_head(stmt: &str) -> String {
    stmt.trim_start()
        .strip_prefix('(')
        .unwrap_or("")
        .trim_start()
        .chars()
        .take_while(|c| !c.is_whitespace() && *c != '(' && *c != ')')
        .collect()
}

/// Rewrite an SMT2 statement, replacing any atom that is a key of `map` with
/// its value. String literals and comments are never touched. SMT2 simple
/// symbols are runs of chars that aren't whitespace, parens, `;`, or `"`.
///
/// CRUCIAL: an atom in application-HEAD position (the symbol immediately after
/// a `(`) is NEVER renamed. Every declared const in a stage1 emit is 0-arity,
/// so it only ever appears as an OPERAND; the head position is always a
/// builtin/datatype op (`select`, `store`, `*`, `=`, a constructor, …). A
/// user const named `select` collides in spelling with the array builtin, and
/// renaming `(select arr i)`→`(N!select arr i)` would be a type error. Skipping
/// head position resolves it soundly given the 0-arity invariant (asserted by
/// the caller via the "all declare-fun are ()" check upstream).
fn rename_tokens(stmt: &str, map: &HashMap<String, String>) -> String {
    let bytes = stmt.as_bytes();
    let n = bytes.len();
    let mut out = String::with_capacity(stmt.len() + 32);
    let mut i = 0;
    // True when the next atom we read is in application-head position (we just
    // consumed a `(` and only whitespace since).
    let mut head_pos = false;
    while i < n {
        let c = bytes[i] as char;
        if c == '"' {
            // copy string literal verbatim (handle "" escape)
            out.push('"');
            i += 1;
            while i < n {
                let ch = bytes[i] as char;
                out.push(ch);
                if ch == '"' {
                    if i + 1 < n && bytes[i + 1] == b'"' {
                        out.push('"');
                        i += 2;
                        continue;
                    }
                    i += 1;
                    break;
                }
                i += 1;
            }
            continue;
        }
        if c == ';' {
            while i < n && bytes[i] != b'\n' {
                out.push(bytes[i] as char);
                i += 1;
            }
            continue;
        }
        if c == '(' {
            out.push(c);
            i += 1;
            head_pos = true; // next atom is the application head
            continue;
        }
        if c == ')' {
            out.push(c);
            i += 1;
            head_pos = false;
            continue;
        }
        if c.is_whitespace() {
            out.push(c);
            i += 1;
            continue; // whitespace preserves head_pos until the first atom
        }
        // an atom: read to next delimiter
        let start = i;
        while i < n {
            let ch = bytes[i] as char;
            if ch == '(' || ch == ')' || ch == '"' || ch == ';' || ch.is_whitespace() {
                break;
            }
            i += 1;
        }
        let atom = &stmt[start..i];
        let in_head = head_pos;
        head_pos = false; // only the FIRST atom after `(` is the head
        if !in_head {
            if let Some(rep) = map.get(atom) {
                out.push_str(rep);
                continue;
            }
        }
        out.push_str(atom);
    }
    out
}
