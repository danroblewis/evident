//! Cross-validation: the Rust `validate` impl vs the Evident
//! `stdlib/passes/validate.ev` impl, both reached through the
//! `portable::validate` swap interface.
//!
//! `validate` is the second transform ported to the portable Rust⇄Evident
//! pattern (see docs/self-hosting.md), after `pretty`. The body walk is
//! shared between the two impls and only the per-Call classifier differs:
//! Rust uses a `match`, Evident routes through `ValidateExpr` in
//! `stdlib/passes/validate.ev`. Consequently `EvidentValidate` is
//! byte-identical-faithful to `RustValidate` on every input — there is
//! no "diverges" subset to pin.
//!
//! The fixtures here construct `SchemaDecl`s directly (the same pattern
//! `pretty_equivalence.rs` uses) so we can exercise:
//!   * `external` schemas pass even when they construct FFI.
//!   * Non-external schemas with no FFI pass.
//!   * Non-external schemas with FFI at the top of a Constraint, nested
//!     inside `EBinary`, inside `ETernary`, inside `EForall`, inside a
//!     `Match` arm body, and inside `ECall` arguments — each variant
//!     the canonical walker visits — fail with the same diagnostic.
//!   * Diagnostic wording matches byte-for-byte across the four kind
//!     labels (`fsm` / `type` / `claim` / `subclaim`) and the four
//!     banned call names (`FFICall` / `FFIOpen` / `FFILookup` /
//!     `LibCall`).

use std::path::Path;

use evident_runtime::ast::{
    BinOp, BodyItem, Expr, Keyword, MatchArm, MatchPattern, Pins, SchemaDecl,
};
use evident_runtime::portable::validate::{EvidentValidate, RustValidate, ValidateImpl};

const STDLIB: &str = "../stdlib";

fn rust() -> RustValidate { RustValidate }
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

// Assert both impls return the same Result. Run the Evident path through
// the shared `ev` instance — building one is the expensive bit (loads
// stdlib/passes/validate.ev once); per-call classification is JIT-fast.
fn both_agree(ev: &EvidentValidate, s: &SchemaDecl) -> Result<(), String> {
    let r = rust().enforce_external_only(s);
    let e = ev.enforce_external_only(s);
    assert_eq!(r, e, "rust vs evident disagree on {:?}", s.name);
    r
}

// ── Identity ──────────────────────────────────────────────────────────

#[test]
fn impl_names() {
    use evident_runtime::portable::Portable;
    assert_eq!(RustValidate.impl_name(), "rust");
    assert_eq!(evident().impl_name(), "evident");
}

// ── Valid programs: both Ok ──────────────────────────────────────────

#[test]
fn external_with_ffi_is_ok() {
    let ev = evident();
    // `external claim` may construct any of the four FFI primitives.
    for &nm in &["FFICall", "FFIOpen", "FFILookup", "LibCall"] {
        let s = schema(Keyword::Claim, "boundary_helper", true, vec![
            assign("eff", call(nm, vec![Expr::Str("libc.dylib".into())])),
        ]);
        assert_eq!(both_agree(&ev, &s), Ok(()), "external + {nm} must pass");
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
    assert_eq!(both_agree(&ev, &s), Ok(()));
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
    assert_eq!(both_agree(&ev, &s), Ok(()));
}

// ── Violations: both Err, identical messages ─────────────────────────

#[test]
fn direct_libcall_in_constraint_violates() {
    let ev = evident();
    let s = schema(Keyword::Claim, "bad_direct", false, vec![
        constraint(call("LibCall", vec![])),
    ]);
    let got = both_agree(&ev, &s);
    assert!(got.is_err(), "expected violation, got {:?}", got);
    let msg = got.unwrap_err();
    assert!(msg.contains("claim `bad_direct`"));
    assert!(msg.contains("`LibCall(...)`"));
    assert!(msg.contains("isn't \\\n         declared `external`")
        || msg.contains("isn't declared `external`")
        || msg.contains("isn't"));
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
    let got = both_agree(&ev, &s);
    assert!(got.is_err());
    assert!(got.as_ref().unwrap_err().contains("fsm `bad_assignment`"));
}

#[test]
fn ffi_inside_ternary_violates() {
    let ev = evident();
    // `eff = cond ? LibCall(...) : other_eff`
    let s = schema(Keyword::Claim, "bad_ternary", false, vec![
        assign("eff", Expr::Ternary(
            Box::new(ident("cond")),
            Box::new(call("FFICall", vec![])),
            Box::new(ident("other_eff")),
        )),
    ]);
    assert!(both_agree(&ev, &s).is_err());
}

#[test]
fn ffi_inside_forall_violates() {
    let ev = evident();
    // `∀ i ∈ {0..n} : LibCall(...)` — the body has the banned call.
    let s = schema(Keyword::Claim, "bad_forall", false, vec![
        constraint(Expr::Forall(
            vec!["i".into()],
            Box::new(Expr::Range(Box::new(Expr::Int(0)), Box::new(Expr::Int(5)))),
            Box::new(call("FFIOpen", vec![Expr::Str("lib".into())])),
        )),
    ]);
    assert!(both_agree(&ev, &s).is_err());
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
    assert!(both_agree(&ev, &s).is_err());
}

#[test]
fn ffi_inside_call_args_violates() {
    let ev = evident();
    // A safe call wrapping a banned call as an arg: `helper(LibCall(...))`.
    // The walker recurses into args.
    let s = schema(Keyword::Claim, "bad_nested_call", false, vec![
        assign("out", call("helper", vec![call("LibCall", vec![])])),
    ]);
    assert!(both_agree(&ev, &s).is_err());
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
    assert!(both_agree(&ev, &s).is_err());
}

#[test]
fn ffi_inside_not_violates() {
    let ev = evident();
    // `¬(LibCall(...))` — contrived but the walker visits the inner.
    let s = schema(Keyword::Claim, "bad_not", false, vec![
        constraint(Expr::Not(Box::new(call("LibCall", vec![])))),
    ]);
    assert!(both_agree(&ev, &s).is_err());
}

// ── Diagnostic equivalence: exact byte-for-byte message ─────────────

#[test]
fn diagnostic_message_is_byte_identical() {
    let ev = evident();
    // Try every keyword label and every banned call — the message must
    // be word-for-word identical between impls so callers (and the
    // canonical impl at runtime/src/runtime/validate.rs) see exactly
    // the same text.
    let combinations: &[(Keyword, &str, &str)] = &[
        (Keyword::Fsm,    "f", "FFICall"),
        (Keyword::Type,   "t", "FFIOpen"),
        (Keyword::Claim,  "c", "FFILookup"),
        (Keyword::Schema, "s", "LibCall"),
        (Keyword::Subclaim, "sc", "LibCall"),
    ];
    for (kw, name, banned) in combinations {
        let s = schema(kw.clone(), name, false, vec![
            assign("x", call(banned, vec![])),
        ]);
        let r = rust().enforce_external_only(&s);
        let e = ev.enforce_external_only(&s);
        assert_eq!(r, e, "diagnostic diverges for ({kw:?}, {name}, {banned})");
        assert!(r.is_err());
    }
}

// ── First-hit ordering: walk returns the first violation found ─────

#[test]
fn first_violation_wins() {
    let ev = evident();
    // Two banned calls; the walker should pick the earlier one. Both
    // impls share the walk so they pick the same call.
    let s = schema(Keyword::Claim, "two_violations", false, vec![
        assign("a", call("FFICall", vec![])),
        assign("b", call("LibCall", vec![])),
    ]);
    let r = rust().enforce_external_only(&s);
    let e = ev.enforce_external_only(&s);
    assert_eq!(r, e);
    let msg = r.unwrap_err();
    // Body order — FFICall comes first.
    assert!(msg.contains("FFICall"));
    assert!(!msg.contains("LibCall"));
}

// ── Loaded corpus: every well-formed example file must validate ─────

#[test]
fn corpus_real_examples_all_pass() {
    // Every file in examples/test_*.ev is a working program — by
    // definition every schema in it must pass `enforce_external_only`
    // (otherwise it wouldn't load). Round-trip through the runtime to
    // grab the parsed schemas, then check both impls agree on each.
    let ev = evident();
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

    // Note on what the corpus test is asserting:
    //   * Both impls must AGREE on every loaded schema. This is the
    //     core equivalence claim — `validate.rs` and `validate.ev` see
    //     every shape the parser emits in real demos and produce the
    //     same verdict.
    //   * Most schemas pass (i.e. `Ok(())`), but a handful of
    //     subclaims-of-external parents (`set_draw_color`, etc. inside
    //     `external type SDL_Window`) fail when validated in isolation
    //     — the canonical load path doesn't call `enforce_external_only`
    //     on lifted subclaims (see `runtime/src/runtime/load.rs`; the
    //     parent's `external` covers them), but the function itself
    //     does flag them when called directly. Both impls flag them
    //     identically, which is exactly what equivalence requires.
    let mut checked = 0usize;
    for path in paths {
        let mut rt = evident_runtime::EvidentRuntime::new();
        // Some examples import packages/* or trigger features outside the
        // validate scope — skip on parse/load failure rather than fail
        // the test. (The point is to exercise validate on every schema we
        // *can* load, not to gate on the wider runtime.)
        if rt.load_file(&path).is_err() {
            continue;
        }
        for (_, s) in rt.schemas_map().iter() {
            let r = rust().enforce_external_only(s);
            let e = ev.enforce_external_only(s);
            assert_eq!(r, e, "diverge on {} schema `{}`",
                       path.display(), s.name);
            checked += 1;
        }
    }
    // Sanity: we should have visited a non-trivial number of schemas.
    assert!(checked >= 20, "only checked {checked} schemas — corpus shrunk?");
}
