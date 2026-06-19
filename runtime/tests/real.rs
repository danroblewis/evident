use std::collections::HashMap;
use evident_runtime::{EvidentRuntime, Value};

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

#[test]
fn real_membership_basic() {
    let mut rt = EvidentRuntime::new();
    rt.load_source("schema S\n    x ∈ Real\n    x = 3.14\n").unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    assert_real_close(r.bindings.get("x"), 3.14, "x");
}

#[test]
fn real_arithmetic_add() {
    let mut rt = EvidentRuntime::new();
    rt.load_source(
        "schema S\n    x ∈ Real\n    y ∈ Real\n    x = 1.5\n    y = 2.25\n    x + y = 3.75\n",
    ).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied, "x + y = 3.75 should be satisfiable");
}

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

#[test]
fn real_comparison_strict_unsat() {
    let mut rt = EvidentRuntime::new();
    rt.load_source(
        "schema S\n    x ∈ Real\n    x < 3.14\n    x > 3.14\n",
    ).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(!r.satisfied, "x < 3.14 ∧ x > 3.14 must be UNSAT");
}

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

#[test]
fn mixed_int_var_real_literal_comparison() {
    let mut rt = EvidentRuntime::new();
    rt.load_source(
        "schema S\n    n ∈ Nat\n    n ≤ 5.5\n    n ≥ 5\n",
    ).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);

    assert_eq!(r.bindings.get("n"), Some(&Value::Int(5)));
}

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
