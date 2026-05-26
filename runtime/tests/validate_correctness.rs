//! Correctness of the self-hosted external-only check — the runtime's SOLE
//! `validate` implementation since session VALIDATE-recursive.
//!
//! This replaces `validate_equivalence.rs`, which compared the Evident pass
//! against a canonical Rust `find_ffi_call` walk. That Rust walk is now
//! deleted, so there is no oracle to compare against: instead, this test
//! pins the EXPECTED verdict (and exact diagnostic) for every shape the old
//! equivalence test exercised, and now they stand on their own.
//!
//! Four concerns:
//!   1. Verdicts + byte-exact diagnostics on hand-built schemas (the shapes
//!      the canonical walker descended into).
//!   2. `first_violation_wins` — first banned call in pre-order.
//!   3. A string-heavy constraint (the `test_26::driver` shape that once
//!      hung the in-solve string-equality design) is fast and correct.
//!   4. The production load entry (`portable::validate::enforce_external_only`,
//!      cached engine + WW resolver + bootstrap guard) loads real programs.

use std::path::Path;

use evident_runtime::ast::{
    BinOp, BodyItem, Expr, Keyword, MatchArm, MatchPattern, Pins, SchemaDecl,
};
use evident_runtime::portable::validate::{self, EvidentValidate, ValidateImpl};

const STDLIB: &str = "../stdlib";

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

/// The exact diagnostic the pass must produce — must match
/// `portable::validate::error_msg` (and the old `runtime::validate`
/// wording) byte-for-byte.
fn expected_msg(kind: &str, name: &str, call: &str) -> String {
    format!(
        "{kind} `{name}` constructs `{call}(...)` but isn't \
         declared `external`. Either mark this declaration \
         `external claim` / `external type`, or move the \
         FFI into an `external claim` helper and call it \
         from here."
    )
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

// ── Violations: Err with the exact diagnostic ────────────────────────

#[test]
fn direct_libcall_in_constraint_violates() {
    let ev = evident();
    let s = schema(Keyword::Claim, "bad_direct", false, vec![
        constraint(call("LibCall", vec![])),
    ]);
    assert_eq!(ev.enforce_external_only(&s),
               Err(expected_msg("claim", "bad_direct", "LibCall")));
}

#[test]
fn libcall_in_assignment_violates() {
    let ev = evident();
    // `eff = LibCall(...)` — the FFI call sits under EBinary(OpEq, …, ECall).
    let s = schema(Keyword::Fsm, "bad_assignment", false, vec![
        assign("eff", call("LibCall", vec![Expr::Str("libc".into())])),
    ]);
    assert_eq!(ev.enforce_external_only(&s),
               Err(expected_msg("fsm", "bad_assignment", "LibCall")));
}

#[test]
fn ffi_inside_ternary_violates() {
    let ev = evident();
    let s = schema(Keyword::Claim, "bad_ternary", false, vec![
        assign("eff", Expr::Ternary(
            Box::new(ident("cond")),
            Box::new(call("FFICall", vec![])),
            Box::new(ident("other_eff")),
        )),
    ]);
    assert_eq!(ev.enforce_external_only(&s),
               Err(expected_msg("claim", "bad_ternary", "FFICall")));
}

#[test]
fn ffi_inside_forall_violates() {
    let ev = evident();
    let s = schema(Keyword::Claim, "bad_forall", false, vec![
        constraint(Expr::Forall(
            vec!["i".into()],
            Box::new(Expr::Range(Box::new(Expr::Int(0)), Box::new(Expr::Int(5)))),
            Box::new(call("FFIOpen", vec![Expr::Str("lib".into())])),
        )),
    ]);
    assert_eq!(ev.enforce_external_only(&s),
               Err(expected_msg("claim", "bad_forall", "FFIOpen")));
}

#[test]
fn ffi_inside_match_arm_violates() {
    let ev = evident();
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
    assert_eq!(ev.enforce_external_only(&s),
               Err(expected_msg("type", "bad_match", "FFILookup")));
}

#[test]
fn ffi_inside_nested_ctor_match_arm_violates() {
    // Session SEED-marshal: the arm carries a NESTED-ctor pattern
    // `Node(Leaf(a), b)` (→ `BindCtor` in the seed) and the fallthrough is
    // a TOP-LEVEL bind `other` (→ `PatBind`). The seed marshaler is now
    // recursive, and `validate.ev`'s `MatchBind`/`MatchPattern` enums grew
    // to match, so `value_enum_to_datatype` encodes this seed instead of
    // dropping it. The banned `FFILookup` hiding in the nested-ctor arm's
    // body must still be detected — under `EVIDENT_FUNCTIONIZE=0` (the Z3
    // slow path) this is exactly the silent-drop the marshaler/enum coupling
    // would otherwise reintroduce.
    let ev = evident();
    let s = schema(Keyword::Type, "bad_nested_match", false, vec![
        assign("eff", Expr::Match(
            Box::new(ident("state")),
            vec![
                MatchArm {
                    pattern: MatchPattern::Ctor {
                        name: "Node".into(),
                        binds: vec![
                            MatchPattern::Ctor {
                                name: "Leaf".into(),
                                binds: vec![MatchPattern::Bind("a".into())],
                            },
                            MatchPattern::Bind("b".into()),
                        ],
                    },
                    body: Box::new(call("FFILookup", vec![])),
                },
                MatchArm {
                    pattern: MatchPattern::Bind("other".into()),
                    body: Box::new(ident("noop")),
                },
            ],
        )),
    ]);
    assert_eq!(ev.enforce_external_only(&s),
               Err(expected_msg("type", "bad_nested_match", "FFILookup")));
}

#[test]
fn ffi_inside_call_args_violates() {
    let ev = evident();
    // `helper(LibCall(...))` — the banned call is an arg of a safe call.
    let s = schema(Keyword::Claim, "bad_nested_call", false, vec![
        assign("out", call("helper", vec![call("LibCall", vec![])])),
    ]);
    assert_eq!(ev.enforce_external_only(&s),
               Err(expected_msg("claim", "bad_nested_call", "LibCall")));
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
    assert_eq!(ev.enforce_external_only(&s),
               Err(expected_msg("fsm", "bad_seqlit", "LibCall")));
}

#[test]
fn ffi_inside_not_violates() {
    let ev = evident();
    let s = schema(Keyword::Claim, "bad_not", false, vec![
        constraint(Expr::Not(Box::new(call("LibCall", vec![])))),
    ]);
    assert_eq!(ev.enforce_external_only(&s),
               Err(expected_msg("claim", "bad_not", "LibCall")));
}

// ── Diagnostic: every kind label × banned name ───────────────────────

#[test]
fn diagnostic_message_exact_for_every_combination() {
    let ev = evident();
    let combinations: &[(Keyword, &str, &str, &str)] = &[
        (Keyword::Fsm,      "f",  "FFICall",   "fsm"),
        (Keyword::Type,     "t",  "FFIOpen",   "type"),
        (Keyword::Claim,    "c",  "FFILookup", "claim"),
        (Keyword::Schema,   "s",  "LibCall",   "schema"),
        (Keyword::Subclaim, "sc", "LibCall",   "subclaim"),
    ];
    for (kw, name, banned, label) in combinations {
        let s = schema(kw.clone(), name, false, vec![
            assign("x", call(banned, vec![])),
        ]);
        assert_eq!(ev.enforce_external_only(&s),
                   Err(expected_msg(label, name, banned)),
                   "diagnostic mismatch for ({kw:?}, {name}, {banned})");
    }
}

// ── First-hit ordering ───────────────────────────────────────────────

#[test]
fn first_violation_wins() {
    let ev = evident();
    // Two banned calls in separate body items; the earlier one wins (the
    // per-constraint loop returns on the first violating Constraint).
    let s = schema(Keyword::Claim, "two_violations", false, vec![
        assign("a", call("FFICall", vec![])),
        assign("b", call("LibCall", vec![])),
    ]);
    assert_eq!(ev.enforce_external_only(&s),
               Err(expected_msg("claim", "two_violations", "FFICall")));
}

#[test]
fn first_violation_wins_within_one_expr() {
    let ev = evident();
    // Both banned calls in ONE expr: `helper(FFIOpen(...), LibCall(...))`.
    // Pre-order visits FFIOpen (first arg) before LibCall (second), so the
    // pass reports FFIOpen — byte-identical to the old find_ffi_call.
    let s = schema(Keyword::Claim, "two_in_one", false, vec![
        assign("out", call("helper", vec![
            call("FFIOpen", vec![]),
            call("LibCall", vec![]),
        ])),
    ]);
    assert_eq!(ev.enforce_external_only(&s),
               Err(expected_msg("claim", "two_in_one", "FFIOpen")));
}

// ── Regression: the string-heavy shape that once hung ────────────────

#[test]
fn string_heavy_ternary_is_fast_and_correct() {
    use std::time::Instant;
    let ev = evident();
    // The `test_26::driver` shape: a deeply nested ternary whose arms are
    // distinct STRING literals. The earlier in-solve string-equality design
    // hung here (Z3 string-theory blowup, minutes + GBs). The collect-and-
    // defer design walks it in well under a second.
    let msg = Expr::Ternary(
        Box::new(ident("a")), Box::new(Expr::Str("signal=10 (window 0)".into())),
        Box::new(Expr::Ternary(
            Box::new(ident("b")), Box::new(Expr::Str("signal=20 (window 1)".into())),
            Box::new(Expr::Ternary(
                Box::new(ident("c")), Box::new(Expr::Str("signal=30 (window 2)".into())),
                Box::new(Expr::Ternary(
                    Box::new(ident("d")), Box::new(Expr::Str("signal=40 (window 3)".into())),
                    Box::new(Expr::Str("signal=20 (window 5 — cache reuse)".into())),
                )),
            )),
        )),
    );
    // Clean (no banned call) → Ok, fast.
    let clean = schema(Keyword::Fsm, "driver_like", false, vec![assign("msg", msg.clone())]);
    let t = Instant::now();
    assert_eq!(ev.enforce_external_only(&clean), Ok(()));
    assert!(t.elapsed().as_secs() < 5, "string-heavy walk too slow: {:?}", t.elapsed());

    // Same shape but with a LibCall buried in the deepest arm → detected.
    let with_ffi = Expr::Ternary(
        Box::new(ident("a")), Box::new(Expr::Str("a string".into())),
        Box::new(Expr::Ternary(
            Box::new(ident("b")), Box::new(Expr::Str("another string".into())),
            Box::new(call("LibCall", vec![Expr::Str("yet another".into())])),
        )),
    );
    let bad = schema(Keyword::Fsm, "driver_like_bad", false, vec![assign("msg", with_ffi)]);
    assert_eq!(ev.enforce_external_only(&bad),
               Err(expected_msg("fsm", "driver_like_bad", "LibCall")));
}

// ── Bootstrap: the pass file validates clean through itself ──────────

#[test]
fn bootstrap_walk_fsm_passes() {
    // The pass's own `validate_walk` fsm constructs `Expr` enum VALUES
    // (named "ECall" etc.) but calls no banned FFI primitive, so it passes.
    // Loading the pass into a runtime and validating its own declaration
    // exercises the same shape the bootstrap guard short-circuits at build.
    let ev = evident();
    let mut rt = evident_runtime::EvidentRuntime::new();
    rt.load_file(Path::new("../stdlib/passes/validate.ev")).expect("load the pass");
    let walk = rt.get_schema("validate_walk").expect("validate_walk declared");
    assert_eq!(ev.enforce_external_only(walk), Ok(()),
        "validate_walk constructs Expr values, never calls FFI — must pass");
}

// ── Production entry: the cached engine + WW resolver + bootstrap guard ──

#[test]
fn production_entry_loads_real_program() {
    // `portable::validate::enforce_external_only` is what the load path
    // calls. Loading a real example through a fresh runtime drives that
    // production path over every schema in the file (building the cached
    // engine on first use — exercising the bootstrap guard — and walking
    // each schema). A clean load proves the whole path end-to-end.
    let mut rt = evident_runtime::EvidentRuntime::new();
    rt.load_file(Path::new("../stdlib/runtime.ev")).ok();
    rt.load_file(Path::new("../examples/test_26_value_cache.ev"))
        .expect("test_26 (the string-heavy driver) loads via the Evident validate");
}

#[test]
fn production_entry_agrees_with_direct_engine() {
    // The production free fn and an explicitly-constructed engine must
    // agree on a violating and a clean schema.
    let bad = schema(Keyword::Claim, "p_bad", false, vec![assign("e", call("LibCall", vec![]))]);
    let ok  = schema(Keyword::Claim, "p_ok",  false, vec![assign("e", call("safe", vec![]))]);
    let ev = evident();
    assert_eq!(validate::enforce_external_only(&bad), ev.enforce_external_only(&bad));
    assert_eq!(validate::enforce_external_only(&ok),  ev.enforce_external_only(&ok));
    assert!(validate::enforce_external_only(&bad).is_err());
    assert_eq!(validate::enforce_external_only(&ok), Ok(()));
}

// ── A curated slice of the real corpus: every schema passes ──────────

#[test]
fn corpus_slice_all_pass() {
    // A handful of representative example files — every well-formed schema
    // in them must pass `enforce_external_only` (otherwise the file
    // wouldn't load). NOT every file (that volume belongs to the demo
    // runner / conformance, which load all examples through the binary);
    // this is a focused unit-level check across varied shapes.
    let ev = evident();
    let files = [
        "../examples/test_09_two_fsms.ev",
        "../examples/test_19_prev_tick.ev",
        "../examples/test_25_per_component_jit.ev",
        "../examples/test_26_value_cache.ev",
    ];
    let mut checked = 0usize;
    for f in files {
        let mut rt = evident_runtime::EvidentRuntime::new();
        rt.load_file(Path::new("../stdlib/runtime.ev")).ok();
        rt.load_file(Path::new(f)).unwrap_or_else(|e| panic!("load {f}: {e}"));
        for (_, s) in rt.schemas_map().iter() {
            // Subclaims lifted from external parents can flag in isolation;
            // skip external-marked schemas (the load path's parent covers
            // them) — assert only on non-external schemas, which is what
            // the load path actually gates.
            if !s.external {
                assert_eq!(ev.enforce_external_only(s), Ok(()),
                    "{f} schema `{}` should pass", s.name);
                checked += 1;
            }
        }
    }
    assert!(checked >= 10, "expected ≥10 schemas; checked {checked}");
}
