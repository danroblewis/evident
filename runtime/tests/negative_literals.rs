//! Negative number literals (#430): `-5` / `-5.0` written directly.
//!
//! Unary minus on a numeric literal folds at parse time into a single `Int`/
//! `Real` literal node, so a negative number is structurally identical to a
//! positive one and flows through every consumer that pattern-matches a literal
//! leaf (positional pins, record/seq literals, claim args, ternary arms). Unary
//! minus on a non-literal (`-x`) keeps the `0 - x` desugaring.
//!
//! These tests also guard the invariant that binary subtraction is unaffected.

use evident_runtime::{EvidentRuntime, Value};

fn real_close(v: Option<&Value>, expected: f64, label: &str) {
    match v {
        Some(Value::Real(f)) => assert!(
            (f - expected).abs() < 1e-9,
            "{label}: expected ≈ {expected}, got {f}"
        ),
        other => panic!("{label}: expected Real, got {:?}", other),
    }
}

#[test]
fn negative_real_literal_sat() {
    let mut rt = EvidentRuntime::new();
    rt.load_source("claim S\n    x ∈ Real = -5.0\n    x = -5.0\n").unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    real_close(r.bindings.get("x"), -5.0, "x");
}

#[test]
fn negative_real_literal_sign_not_dropped() {
    // If the sign were lost, x would be 5.0 and `x = 5.0` would be SAT.
    let mut rt = EvidentRuntime::new();
    rt.load_source("claim S\n    x ∈ Real = -5.0\n    x = 5.0\n").unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(!r.satisfied, "x = -5.0, so x = 5.0 must be UNSAT");
}

#[test]
fn negative_int_literal_sat() {
    let mut rt = EvidentRuntime::new();
    rt.load_source("claim S\n    x ∈ Int = -5\n    x = -5\n").unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    assert_eq!(r.bindings.get("x"), Some(&Value::Int(-5)));
}

#[test]
fn binary_subtraction_unaffected() {
    let mut rt = EvidentRuntime::new();
    rt.load_source(
        "claim S\n    a ∈ Int = 10\n    b ∈ Int = 3\n    c ∈ Int = a - b\n    c = 7\n",
    ).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    assert_eq!(r.bindings.get("c"), Some(&Value::Int(7)));
}

#[test]
fn subtract_a_negative_literal() {
    // `a - -5` must be `a + 5`, not a parse error.
    let mut rt = EvidentRuntime::new();
    rt.load_source(
        "claim S\n    a ∈ Int = 10\n    c ∈ Int = a - -5\n    c = 15\n",
    ).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    assert_eq!(r.bindings.get("c"), Some(&Value::Int(15)));
}

#[test]
fn negative_in_record_literal() {
    let mut rt = EvidentRuntime::new();
    rt.load_source(
        "type IVec2(x, y ∈ Int)\nclaim S\n    p ∈ IVec2 = IVec2(-5, 3)\n    p.x = -5\n    p.y = 3\n",
    ).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    assert_eq!(r.bindings.get("p.x"), Some(&Value::Int(-5)));
    assert_eq!(r.bindings.get("p.y"), Some(&Value::Int(3)));
}

#[test]
fn negative_in_positional_pin() {
    // The pin site pattern-matches a literal leaf — the parse-time fold is what
    // makes `IVec2(-7, -8)` pin correctly (a `0 - 7` expression would not).
    let mut rt = EvidentRuntime::new();
    rt.load_source(
        "type IVec2(x, y ∈ Int)\nclaim S\n    p ∈ IVec2(-7, -8)\n    p.x = -7\n    p.y = -8\n",
    ).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    assert_eq!(r.bindings.get("p.x"), Some(&Value::Int(-7)));
    assert_eq!(r.bindings.get("p.y"), Some(&Value::Int(-8)));
}

#[test]
fn negative_in_seq_literal() {
    let mut rt = EvidentRuntime::new();
    rt.load_source(
        "claim S\n    xs ∈ Seq(Int) = ⟨-5, -3, 2⟩\n    xs[0] = -5\n    xs[1] = -3\n    xs[2] = 2\n",
    ).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
}

#[test]
fn negative_in_ternary_arm() {
    let mut rt = EvidentRuntime::new();
    rt.load_source(
        "claim S\n    flag ∈ Bool = true\n    x ∈ Int = (flag ? -5 : 5)\n    x = -5\n",
    ).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    assert_eq!(r.bindings.get("x"), Some(&Value::Int(-5)));
}

#[test]
fn negative_claim_arg() {
    let mut rt = EvidentRuntime::new();
    rt.load_source(
        "claim add5(n ∈ Int, out ∈ Int)\n    out = n + 5\nclaim S\n    r ∈ Int\n    add5(-12, r)\n    r = -7\n",
    ).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    assert_eq!(r.bindings.get("r"), Some(&Value::Int(-7)));
}

#[test]
fn unary_minus_on_variable_still_negates() {
    // Non-literal operand keeps the `0 - x` path.
    let mut rt = EvidentRuntime::new();
    rt.load_source(
        "claim S\n    x ∈ Int = 5\n    y ∈ Int = -x\n    y = -5\n",
    ).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    assert_eq!(r.bindings.get("y"), Some(&Value::Int(-5)));
}
