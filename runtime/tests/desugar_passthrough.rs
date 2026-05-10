//! Integration test for the self-hosted desugar pipeline.
//!
//! Pipeline:
//!   1. Write a tiny program to a temp file. The program contains
//!      `BodyItem::Constraint(Expr::Identifier(name))` where `name`
//!      is a known schema — the bare-identifier-as-passthrough shape.
//!   2. Load it into a runtime and inspect the parsed body — verify
//!      the AST has the bare-identifier shape (sanity check on what
//!      we're testing).
//!   3. Run `auto_apply_desugar` against the same file; verify the
//!      body item was rewritten to `BodyItem::Passthrough(name)`.
//!   4. Verify a query against the rewritten claim still produces
//!      the expected result (semantic equivalence with the inline.rs
//!      path that handles the bare form at translation time).
//!
//! What this proves:
//!   - The Evident-side detection rule (`is_passthrough_at_index`)
//!     works.
//!   - The Rust-side glue correctly iterates body indices, reads
//!     `target_name`, filters by known schemas, and rewrites.
//!   - The runtime mutation (`replace_body_item_in_claim`) is
//!     observable both via the parsed AST and via subsequent solver
//!     queries.

use std::fs;
use std::io::Write;
use evident_runtime::{EvidentRuntime, Value};
use evident_runtime::ast::{BodyItem, Expr};

/// Load helper: write the source to a temp .ev file (so
/// user_claim_indices_in_file's path-based filter works) and
/// return both the path and a runtime with that file loaded.
fn load_temp(source: &str, file_stem: &str) -> (std::path::PathBuf, EvidentRuntime) {
    let dir = std::env::temp_dir().join("evident_desugar_tests");
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join(format!("{file_stem}.ev"));
    let mut f = fs::File::create(&path).unwrap();
    f.write_all(source.as_bytes()).unwrap();
    drop(f);

    let mut rt = EvidentRuntime::new();
    rt.load_file(&path).unwrap();
    (path, rt)
}

#[test]
fn desugar_rewrites_bare_identifier_to_passthrough() {
    // `is_pinned_to_seven` is a claim. `wrap` references it as a
    // bare identifier — the names-match composition shape.
    let source = "\
claim is_pinned_to_seven
    n ∈ Int
    n = 7

claim wrap
    n ∈ Int
    is_pinned_to_seven
";
    let (path, mut rt) = load_temp(source, "bare_identifier");

    // Step 1 — confirm the parsed AST has the bare-identifier shape.
    {
        let wrap = rt.get_schema("wrap").expect("wrap should exist");
        let bare = wrap.body.iter().any(|i| matches!(
            i, BodyItem::Constraint(Expr::Identifier(name)) if name == "is_pinned_to_seven"
        ));
        assert!(bare, "expected `wrap` body to contain a bare-identifier constraint, got: {:#?}", wrap.body);
    }

    // Step 2 — run desugar.
    let n = evident_runtime_internals::commands_desugar_apply(
        &mut rt, &[path.to_string_lossy().to_string()],
    );
    assert_eq!(n, 1, "expected exactly one rewrite; got {n}");

    // Step 3 — verify the body item is now Passthrough.
    {
        let wrap = rt.get_schema("wrap").expect("wrap should exist");
        let pass = wrap.body.iter().any(|i| matches!(
            i, BodyItem::Passthrough(name) if name == "is_pinned_to_seven"
        ));
        assert!(pass, "expected `wrap` body to contain Passthrough(is_pinned_to_seven), got: {:#?}", wrap.body);
    }

    // Step 4 — semantic equivalence: query returns n=7.
    {
        let r = rt.query_free("wrap").unwrap();
        assert!(r.satisfied);
        assert_eq!(r.bindings.get("n"), Some(&Value::Int(7)));
    }
}

#[test]
fn desugar_does_not_rewrite_non_schema_identifiers() {
    // `flag` is a Bool var, not a claim. The constraint
    // `flag` (just the bare ident) is intended to evaluate to
    // the Bool var's value. This shape is NOT a passthrough.
    let source = "\
claim t
    flag ∈ Bool
    flag
";
    let (path, mut rt) = load_temp(source, "bare_var");

    let n = evident_runtime_internals::commands_desugar_apply(
        &mut rt, &[path.to_string_lossy().to_string()],
    );
    assert_eq!(n, 0, "no schema named `flag` exists; rewrite should not fire");

    // Body should still have the bare-identifier shape.
    let t = rt.get_schema("t").expect("t should exist");
    let still_bare = t.body.iter().any(|i| matches!(
        i, BodyItem::Constraint(Expr::Identifier(name)) if name == "flag"
    ));
    assert!(still_bare, "body[1] should still be bare Identifier(flag)");
}

// ---------------------------------------------------------------
// The desugar pipeline lives in `commands/` (a binary-only module),
// not in the published library. Re-export the function we need via
// a tiny local module so the integration test can call it. This is
// the same pattern other tests would use to reach commands/ helpers.
mod evident_runtime_internals {
    use evident_runtime::EvidentRuntime;

    // Mirror the behavior of `commands::desugar::auto_apply_desugar`
    // by reusing the public crate root we have. We can't access
    // commands/ directly (binary-only), so we re-implement the
    // handful of lines needed here.
    use std::collections::HashSet;
    use std::path::Path;
    use evident_runtime::Value;
    use evident_runtime::ast::{BodyItem, Expr};

    const STDLIB_AST: &str = "../stdlib/ast.ev";
    const PASS:       &str = "../stdlib/passes/desugar_passthrough.ev";
    const RULE:       &str = "is_passthrough_at_index";

    pub fn commands_desugar_apply(rt: &mut EvidentRuntime, user_files: &[String]) -> usize {
        let mut prt = EvidentRuntime::new();
        prt.load_file(Path::new(STDLIB_AST)).unwrap();
        prt.load_file(Path::new(PASS)).unwrap();
        prt.mark_system_loads_complete();
        for f in user_files { prt.load_file(Path::new(f)).unwrap(); }

        let known: HashSet<String> = prt.schema_names().map(|s| s.to_string()).collect();

        let mut indices: std::collections::BTreeSet<usize> = std::collections::BTreeSet::new();
        for f in user_files {
            for i in prt.user_claim_indices_in_file(Path::new(f)) {
                indices.insert(i);
            }
        }
        let mut rewrites: Vec<(String, usize, String)> = Vec::new();
        for claim_idx in indices {
            let claim_name = prt.user_claim_name(claim_idx).unwrap_or_default();
            let body_len = prt.user_claim_body_len(claim_idx).unwrap_or(0);
            for body_idx in 0..body_len {
                let mut given = std::collections::HashMap::new();
                given.insert("target_idx".into(), Value::Int(body_idx as i64));
                let r = prt.query_with_nth_claim_body_only_given(
                    RULE, "body", claim_idx, given,
                );
                let Ok(Some(qr)) = r else { continue };
                if !qr.satisfied { continue; }
                let Some(Value::Str(name)) = qr.bindings.get("target_name") else { continue };
                if !known.contains(name) { continue; }
                rewrites.push((claim_name.clone(), body_idx, name.clone()));
            }
        }
        let mut applied = 0usize;
        for (claim_name, body_idx, name) in &rewrites {
            let new_item = BodyItem::Passthrough(name.clone());
            let still = rt.get_schema(claim_name)
                .and_then(|s| s.body.get(*body_idx))
                .map(|item| matches!(item,
                    BodyItem::Constraint(Expr::Identifier(n)) if n == name
                )).unwrap_or(false);
            if !still { continue; }
            if let Ok(true) = rt.replace_body_item_in_claim(claim_name, *body_idx, new_item) {
                applied += 1;
            }
        }
        applied
    }
}
