//! Regression tests for Real-typed variables.
//!
//! Real arithmetic was meant to work from day one but quietly didn't —
//! `x ∈ Real` declared a variable Z3 understood, but the translator
//! had no Real path, so equality, comparison, and arithmetic all
//! silently dropped. These tests pin every piece of the pipeline so it
//! can't regress: lexer (decimal literals), declare_var (Real type
//! name), translate (literals + identifiers + arithmetic + comparison
//! + Int↔Real coercion), and model extraction (rational → f64).

use std::collections::HashMap;
use evident_runtime::{EvidentRuntime, Value};

/// Compare an extracted Real binding against an expected f64 with
/// tolerance. Z3 stores Real as exact rationals; we lossily project to
/// f64 at the boundary, so identity comparison can fail by an ulp on
/// rationals whose denominators don't divide f64's mantissa cleanly
/// (e.g. 1/3). 1e-9 is generous enough for any value we actually
/// construct from a decimal source literal.
fn assert_real_close(v: Option<&Value>, expected: f64, label: &str) {
    match v {
        Some(Value::Real(f)) => {
            assert!(
                (f - expected).abs() < 1e-9,
                "{label}: expected ≈ {expected}, got {f}"
            );
        }
        other => panic!("{label}: expected Real, got {:?}", other),
    }
}

/// Smallest possible: declare a Real, equate it to a literal, expect
/// the literal back. If this breaks the entire feature is broken.
#[test]
fn real_membership_basic() {
    let mut rt = EvidentRuntime::new();
    rt.load_source("schema S\n    x ∈ Real\n    x = 3.14\n").unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    assert_real_close(r.bindings.get("x"), 3.14, "x");
}

/// Real arithmetic: add reaches Z3's LRA path.
#[test]
fn real_arithmetic_add() {
    let mut rt = EvidentRuntime::new();
    rt.load_source(
        "schema S\n    x ∈ Real\n    y ∈ Real\n    x = 1.5\n    y = 2.25\n    x + y = 3.75\n",
    ).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied, "x + y = 3.75 should be satisfiable");
}

/// sub, mul, div together. 6.0/4.0 = 1.5; 6.0-4.0 = 2.0; 1.5*2.0 = 3.0.
#[test]
fn real_arithmetic_sub_mul_div() {
    let mut rt = EvidentRuntime::new();
    rt.load_source(
        "schema S\n    a ∈ Real\n    b ∈ Real\n    c ∈ Real\n    a = 6.0\n    b = 4.0\n    c = (a / b) * (a - b)\n",
    ).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied, "expected SAT");
    assert_real_close(r.bindings.get("c"), 3.0, "c");
}

/// Real comparison: ≤ with ≥ on the same value pins x exactly.
#[test]
fn real_comparison_le() {
    let mut rt = EvidentRuntime::new();
    rt.load_source(
        "schema S\n    x ∈ Real\n    x ≤ 3.14\n    x ≥ 3.14\n",
    ).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    assert_real_close(r.bindings.get("x"), 3.14, "x");
}

/// Strict comparisons must produce UNSAT when contradictory.
#[test]
fn real_comparison_strict_unsat() {
    let mut rt = EvidentRuntime::new();
    rt.load_source(
        "schema S\n    x ∈ Real\n    x < 3.14\n    x > 3.14\n",
    ).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(!r.satisfied, "x < 3.14 ∧ x > 3.14 must be UNSAT");
}

/// Chained inequalities with Real. The chained-comparison desugar
/// (a ≤ b ≤ c → a ≤ b ∧ b ≤ c) must compose with the Int→Real
/// fallback so `0 ≤ x ≤ 1` works for `x ∈ Real`.
#[test]
fn real_chained_comparison() {
    let mut rt = EvidentRuntime::new();
    rt.load_source(
        "schema S\n    x ∈ Real\n    0 ≤ x ≤ 1\n    x = 0.5\n",
    ).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    assert_real_close(r.bindings.get("x"), 0.5, "x");
}

/// Mixed Int + Real: an Int literal in a Real context must auto-promote.
#[test]
fn mixed_int_real_arithmetic() {
    let mut rt = EvidentRuntime::new();
    rt.load_source(
        "schema S\n    x ∈ Real\n    x = 1 + 0.5\n",
    ).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    assert_real_close(r.bindings.get("x"), 1.5, "x");
}

/// An Int variable on the LHS of a Real comparison should also coerce.
#[test]
fn mixed_int_var_real_literal_comparison() {
    let mut rt = EvidentRuntime::new();
    rt.load_source(
        "schema S\n    n ∈ Nat\n    n ≤ 5.5\n    n ≥ 5\n",
    ).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    // Only valid Nat in [5, 5.5] is 5 itself.
    assert_eq!(r.bindings.get("n"), Some(&Value::Int(5)));
}

/// Negative Real literal via unary minus.
#[test]
fn real_negative_via_unary_minus() {
    let mut rt = EvidentRuntime::new();
    rt.load_source(
        "schema S\n    x ∈ Real\n    x = -3.14\n",
    ).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    assert_real_close(r.bindings.get("x"), -3.14, "x");
}

/// `given` accepts Value::Real and pre-binds a Real variable.
#[test]
fn given_binds_real() {
    let mut rt = EvidentRuntime::new();
    rt.load_source(
        "schema S\n    x ∈ Real\n    y ∈ Real\n    x + y = 10.0\n",
    ).unwrap();
    let mut g = HashMap::new();
    g.insert("x".to_string(), Value::Real(3.5));
    let r = rt.query("S", &g).unwrap();
    assert!(r.satisfied);
    assert_real_close(r.bindings.get("x"), 3.5, "x (given)");
    assert_real_close(r.bindings.get("y"), 6.5, "y");
}

/// `given` violation surfaces as UNSAT, mirroring the Int test.
#[test]
fn given_real_violation_unsat() {
    let mut rt = EvidentRuntime::new();
    rt.load_source(
        "schema S\n    x ∈ Real\n    x > 10.0\n",
    ).unwrap();
    let mut g = HashMap::new();
    g.insert("x".to_string(), Value::Real(1.0));
    let r = rt.query("S", &g).unwrap();
    assert!(!r.satisfied);
}

/// Real inside a sub-schema (dotted-field expansion). Catches bugs
/// where declare_var works at the top level but breaks for nested
/// fields.
#[test]
fn real_in_sub_schema() {
    let mut rt = EvidentRuntime::new();
    rt.load_source(
        "type Point\n    x ∈ Real\n    y ∈ Real\nschema S\n    p ∈ Point\n    p.x = 1.5\n    p.y = 2.5\n",
    ).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    assert_real_close(r.bindings.get("p.x"), 1.5, "p.x");
    assert_real_close(r.bindings.get("p.y"), 2.5, "p.y");
}

/// Equality between two Real variables (no literal on either side) —
/// exercises the all-vars Real path in translate_bool's Eq arm.
#[test]
fn real_var_to_var_equality() {
    let mut rt = EvidentRuntime::new();
    rt.load_source(
        "schema S\n    x ∈ Real\n    y ∈ Real\n    x = y\n    x = 7.25\n",
    ).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    assert_real_close(r.bindings.get("x"), 7.25, "x");
    assert_real_close(r.bindings.get("y"), 7.25, "y");
}
