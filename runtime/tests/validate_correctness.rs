//! Correctness of the SOLE external-only validator,
//! `stdlib/passes/validate.ev` driven through
//! `portable::validate::EvidentValidate`.
//!
//! Replaces the old `validate_equivalence.rs`, which cross-checked the
//! Evident impl against a now-deleted `RustValidate` oracle. With one
//! implementation there is nothing to compare against, so this test
//! pins the *expected behaviour* directly:
//!
//!   * Accept: `external` schemas (even when they construct FFI) and
//!     non-`external` schemas with no FFI / only safe calls.
//!   * Reject: non-`external` schemas that construct a banned FFI
//!     primitive — at the top of a Constraint, nested inside `EBinary`,
//!     `ETernary`, `EForall`, a `Match` arm, `ECall` arguments,
//!     `ESeqLit`, and `ENot` — each with the exact diagnostic.
//!   * Diagnostic wording across the kind labels (`fsm` / `type` /
//!     `claim` / `schema` / `subclaim`) and the four banned call names
//!     (`FFICall` / `FFIOpen` / `FFILookup` / `LibCall`).
//!   * First-violation-wins ordering.
//!   * Corpus: loading every `examples/test_*.ev` through the real
//!     runtime — whose load path now routes through this validator —
//!     never produces an external-only false positive.

use std::path::Path;

use evident_runtime::ast::{
    BinOp, BodyItem, Expr, Keyword, MatchArm, MatchPattern, Pins, SchemaDecl,
};
use evident_runtime::portable::validate::{EvidentValidate, ValidateImpl};

const STDLIB: &str = "../stdlib";

// The substring every external-only diagnostic contains. Used by the
// corpus test to distinguish a validate false-positive (a bug) from an
// unrelated load failure (skipped).
const DIAG_MARKER: &str = "isn't declared `external`";

fn evident() -> EvidentValidate {
    EvidentValidate::new(Path::new(STDLIB)).expect("load stdlib/passes/validate.ev")
}

// ── AST builders ──────────────────────────────────────────────────────

fn schema(keyword: Keyword, name: &str, external: bool, body: Vec<BodyItem>) -> SchemaDecl {
    SchemaDecl {
        keyword,
        name: name.to_string(),
        type_params: vec![],
        body,
        param_count: 0,
        external,
    }
}

fn ident(n: &str) -> Expr { Expr::Identifier(n.to_string()) }
fn call(n: &str, args: Vec<Expr>) -> Expr { Expr::Call(n.to_string(), args) }
fn constraint(e: Expr) -> BodyItem { BodyItem::Constraint(e) }
fn assign(lhs: &str, rhs: Expr) -> BodyItem {
    constraint(Expr::Binary(BinOp::Eq, Box::new(ident(lhs)), Box::new(rhs)))
}

// ── Identity ──────────────────────────────────────────────────────────

#[test]
fn impl_name_is_evident() {
    use evident_runtime::portable::Portable;
    assert_eq!(evident().impl_name(), "evident");
}

// ── Valid programs: Ok ────────────────────────────────────────────────

#[test]
fn external_with_ffi_is_ok() {
    let ev = evident();
    // `external claim` may construct any of the four FFI primitives.
    for &nm in &["FFICall", "FFIOpen", "FFILookup", "LibCall"] {
        let s = schema(Keyword::Claim, "boundary_helper", true, vec![
            assign("eff", call(nm, vec![Expr::Str("libc.dylib".into())])),
        ]);
        assert_eq!(ev.enforce_external_only(&s), Ok(()), "external + {nm} must pass");
    }
}

#[test]
fn non_external_no_ffi_is_ok() {
    let ev = evident();
    let s = schema(Keyword::Claim, "harmless", false, vec![
        BodyItem::Membership { name: "x".into(), type_name: "Int".into(), pins: Pins::None },
        constraint(Expr::Binary(BinOp::Lt, Box::new(ident("x")), Box::new(Expr::Int(10)))),
        assign("y", Expr::Int(5)),
        constraint(call("some_user_claim", vec![ident("x"), ident("y")])),
    ]);
    assert_eq!(ev.enforce_external_only(&s), Ok(()));
}

#[test]
fn non_external_with_safe_calls_is_ok() {
    let ev = evident();
    // Calls that aren't on the banned list (built-ins, user claims) pass.
    let s = schema(Keyword::Type, "ok_calls", false, vec![
        assign("c", call("coindexed", vec![ident("a"), ident("b")])),
        assign("e", call("edges", vec![ident("xs")])),
        assign("z", call("Color", vec![Expr::Int(1), Expr::Int(2), Expr::Int(3)])),
    ]);
    assert_eq!(ev.enforce_external_only(&s), Ok(()));
}

// ── Violations: Err, exact message ────────────────────────────────────

#[test]
fn direct_libcall_in_constraint_violates() {
    let ev = evident();
    let s = schema(Keyword::Claim, "bad_direct", false, vec![
        constraint(call("LibCall", vec![])),
    ]);
    let got = ev.enforce_external_only(&s);
    assert!(got.is_err(), "expected violation, got {:?}", got);
    let msg = got.unwrap_err();
    assert!(msg.contains("claim `bad_direct`"), "msg: {msg}");
    assert!(msg.contains("`LibCall(...)`"), "msg: {msg}");
    assert!(msg.contains(DIAG_MARKER), "msg: {msg}");
}

#[test]
fn libcall_in_assignment_violates() {
    let ev = evident();
    // The realistic shape: `eff = LibCall(...)` — the FFI call sits
    // under an EBinary(OpEq, EIdentifier, ECall). Walker recurses
    // into the RHS.
    let s = schema(Keyword::Fsm, "bad_assignment", false, vec![
        assign("eff", call("LibCall", vec![Expr::Str("libc".into())])),
    ]);
    let got = ev.enforce_external_only(&s);
    assert!(got.is_err());
    assert!(got.as_ref().unwrap_err().contains("fsm `bad_assignment`"));
}

#[test]
fn ffi_inside_ternary_violates() {
    let ev = evident();
    // `eff = cond ? FFICall(...) : other_eff`
    let s = schema(Keyword::Claim, "bad_ternary", false, vec![
        assign("eff", Expr::Ternary(
            Box::new(ident("cond")),
            Box::new(call("FFICall", vec![])),
            Box::new(ident("other_eff")),
        )),
    ]);
    assert!(ev.enforce_external_only(&s).is_err());
}

#[test]
fn ffi_inside_forall_violates() {
    let ev = evident();
    // `∀ i ∈ {0..n} : FFIOpen(...)` — the body has the banned call.
    let s = schema(Keyword::Claim, "bad_forall", false, vec![
        constraint(Expr::Forall(
            vec!["i".into()],
            Box::new(Expr::Range(Box::new(Expr::Int(0)), Box::new(Expr::Int(5)))),
            Box::new(call("FFIOpen", vec![Expr::Str("lib".into())])),
        )),
    ]);
    assert!(ev.enforce_external_only(&s).is_err());
}

#[test]
fn ffi_inside_match_arm_violates() {
    let ev = evident();
    // The walker dives into each arm's body.
    let s = schema(Keyword::Type, "bad_match", false, vec![
        assign("eff", Expr::Match(
            Box::new(ident("state")),
            vec![
                MatchArm {
                    pattern: MatchPattern::Ctor { name: "Init".into(), binds: vec![] },
                    body: Box::new(call("FFILookup", vec![])),
                },
                MatchArm {
                    pattern: MatchPattern::Wildcard,
                    body: Box::new(ident("noop")),
                },
            ],
        )),
    ]);
    assert!(ev.enforce_external_only(&s).is_err());
}

#[test]
fn ffi_inside_call_args_violates() {
    let ev = evident();
    // A safe call wrapping a banned call as an arg: `helper(LibCall(...))`.
    // The walker recurses into args.
    let s = schema(Keyword::Claim, "bad_nested_call", false, vec![
        assign("out", call("helper", vec![call("LibCall", vec![])])),
    ]);
    assert!(ev.enforce_external_only(&s).is_err());
}

#[test]
fn ffi_inside_seqlit_violates() {
    let ev = evident();
    // `effects = ⟨other_eff, LibCall(...), more⟩`
    let s = schema(Keyword::Fsm, "bad_seqlit", false, vec![
        assign("effects", Expr::SeqLit(vec![
            ident("other_eff"),
            call("LibCall", vec![]),
            ident("more"),
        ])),
    ]);
    assert!(ev.enforce_external_only(&s).is_err());
}

#[test]
fn ffi_inside_not_violates() {
    let ev = evident();
    // `¬(LibCall(...))` — contrived but the walker visits the inner.
    let s = schema(Keyword::Claim, "bad_not", false, vec![
        constraint(Expr::Not(Box::new(call("LibCall", vec![])))),
    ]);
    assert!(ev.enforce_external_only(&s).is_err());
}

// ── Diagnostic wording across kinds and banned names ──────────────────

#[test]
fn diagnostic_message_covers_every_kind_and_banned_name() {
    let ev = evident();
    let combinations: &[(Keyword, &str, &str, &str)] = &[
        (Keyword::Fsm,      "f",  "FFICall",   "fsm `f`"),
        (Keyword::Type,     "t",  "FFIOpen",   "type `t`"),
        (Keyword::Claim,    "c",  "FFILookup", "claim `c`"),
        (Keyword::Schema,   "s",  "LibCall",   "schema `s`"),
        (Keyword::Subclaim, "sc", "LibCall",   "subclaim `sc`"),
    ];
    for (kw, name, banned, kind_label) in combinations {
        let s = schema(kw.clone(), name, false, vec![
            assign("x", call(banned, vec![])),
        ]);
        let got = ev.enforce_external_only(&s);
        let msg = got.expect_err(&format!("expected violation for ({kw:?}, {name}, {banned})"));
        assert!(msg.contains(kind_label), "missing kind label {kind_label:?}: {msg}");
        assert!(msg.contains(&format!("`{banned}(...)`")), "missing banned name: {msg}");
        assert!(msg.contains(DIAG_MARKER), "missing diagnostic marker: {msg}");
    }
}

// ── First-hit ordering: walk returns the first violation found ────────

#[test]
fn first_violation_wins() {
    let ev = evident();
    // Two banned calls; the walker picks the earlier one (body order).
    let s = schema(Keyword::Claim, "two_violations", false, vec![
        assign("a", call("FFICall", vec![])),
        assign("b", call("LibCall", vec![])),
    ]);
    let msg = ev.enforce_external_only(&s).unwrap_err();
    assert!(msg.contains("FFICall"), "msg: {msg}");
    assert!(!msg.contains("LibCall"), "msg: {msg}");
}

// ── Corpus: real examples never trip a false positive ─────────────────

#[test]
fn corpus_real_examples_have_no_false_positive() {
    // Every file in examples/test_*.ev is a working program, so loading
    // it through the runtime — whose load path now routes through this
    // validator — must never fail with an external-only diagnostic. A
    // load failure for any *other* reason (a package import the test
    // env can't resolve, an unrelated runtime gap) is skipped: the
    // point is that the SOLE validator accepts every valid program, not
    // to gate on the wider runtime.
    let examples_dir = Path::new("../examples");
    let mut paths: Vec<_> = std::fs::read_dir(examples_dir)
        .expect("read examples/")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.file_name()
            .and_then(|s| s.to_str())
            .map(|n| n.starts_with("test_") && n.ends_with(".ev"))
            .unwrap_or(false))
        .collect();
    paths.sort();

    let mut loaded = 0usize;
    for path in &paths {
        let mut rt = evident_runtime::EvidentRuntime::new();
        match rt.load_file(path) {
            Ok(()) => loaded += 1,
            Err(e) => {
                let msg = e.to_string();
                assert!(
                    !msg.contains(DIAG_MARKER),
                    "external-only FALSE POSITIVE loading {}: {msg}",
                    path.display(),
                );
            }
        }
    }
    // Sanity: a non-trivial slice of the corpus must actually load
    // (so the no-false-positive assertion isn't vacuous).
    assert!(loaded >= 15, "only {loaded} examples loaded — corpus shrunk?");
}
