//! Integration tests mirroring Python's `runtime/tests/test_end_to_end.py`.

use evident_runtime::{EvidentRuntime, Value};

/// M0 — toolchain check. Just verifies z3 + cargo + linker work end-to-end.
#[test]
fn z3_hello_world() {
    use z3::{ast::{Ast, Int}, Config, Context, SatResult, Solver};
    let cfg = Config::new();
    let ctx = Context::new(&cfg);
    let s = Solver::new(&ctx);
    let n = Int::new_const(&ctx, "n");
    s.assert(&n.gt(&Int::from_i64(&ctx, 5)));
    assert!(matches!(s.check(), SatResult::Sat));
    let m = s.get_model().unwrap();
    let v = m.eval(&n, true).unwrap().as_i64().unwrap();
    assert!(v > 5);
}

/// M6 — first end-to-end test. Mirrors the Python test:
///   schema SimpleNat
///       n ∈ Nat
///       n > 5
#[test]
fn simple_nat_satisfied_with_n_gt_5() {
    let mut rt = EvidentRuntime::new();
    rt.load_source("schema SimpleNat\n    n ∈ Nat\n    n > 5\n").unwrap();
    let r = rt.query("SimpleNat").unwrap();
    assert!(r.satisfied);
    let n = r.bindings.get("n").expect("missing binding for n");
    match n {
        Value::Int(v) => assert!(*v > 5, "expected n > 5, got {}", v),
        other => panic!("expected Int, got {:?}", other),
    }
}

/// Mirror `test_load_source_unsat` from the Python suite:
///   schema Impossible
///       n ∈ Nat
///       n > 10
///       n < 3
#[test]
fn impossible_is_unsat() {
    let mut rt = EvidentRuntime::new();
    rt.load_source("schema Impossible\n    n ∈ Nat\n    n > 10\n    n < 3\n").unwrap();
    let r = rt.query("Impossible").unwrap();
    assert!(!r.satisfied);
}

/// Multiple variables + a relation.
#[test]
fn two_vars_relation() {
    let mut rt = EvidentRuntime::new();
    rt.load_source("schema Pair\n    a ∈ Nat\n    b ∈ Nat\n    a + b = 10\n    a > 0\n    b > 0\n").unwrap();
    let r = rt.query("Pair").unwrap();
    assert!(r.satisfied);
    if let (Some(Value::Int(a)), Some(Value::Int(b))) =
        (r.bindings.get("a"), r.bindings.get("b"))
    {
        assert_eq!(a + b, 10);
        assert!(*a > 0 && *b > 0);
    } else { panic!("missing bindings"); }
}

/// Bool variable + a logical constraint.
#[test]
fn bool_implies() {
    let mut rt = EvidentRuntime::new();
    // p ⇒ q forces q to be true when p is true.
    rt.load_source("schema P\n    p ∈ Bool\n    q ∈ Bool\n    p = true\n    p ⇒ q\n").unwrap();
    let r = rt.query("P").unwrap();
    assert!(r.satisfied);
    assert_eq!(r.bindings.get("q"), Some(&Value::Bool(true)));
}
