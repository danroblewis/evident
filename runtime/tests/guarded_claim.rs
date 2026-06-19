use evident_runtime::{EvidentRuntime, Value};

#[test]
fn guarded_claim_fires_when_true() {
    let mut rt = EvidentRuntime::new();
    let src = "claim Init\n    n ∈ Int\n    n = 42\nschema S\n    n ∈ Int\n    cond ∈ Bool\n    cond = true\n    cond ⇒ Init\n";
    rt.load_source(src).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    assert_eq!(r.bindings.get("n"), Some(&Value::Int(42)));
}

#[test]
fn guarded_claim_constraints_vacuous_when_false() {
    let mut rt = EvidentRuntime::new();
    let src = "claim Init\n    n ∈ Int\n    n = 42\nschema S\n    n ∈ Int\n    cond ∈ Bool\n    cond = false\n    cond ⇒ Init\n    n = 99\n";
    rt.load_source(src).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    assert_eq!(r.bindings.get("n"), Some(&Value::Int(99)));
}

#[test]
fn unguarded_invocation_would_conflict() {
    let mut rt = EvidentRuntime::new();
    let src = "claim Init\n    n ∈ Int\n    n = 42\nschema S\n    n ∈ Int\n    ..Init\n    n = 99\n";
    rt.load_source(src).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(!r.satisfied, "unguarded Init pins n=42, conflicting with n=99");
}

#[test]
fn guarded_claim_declarations_fire_unconditionally() {
    let mut rt = EvidentRuntime::new();
    let src = "claim Init\n    helper ∈ Nat\n    helper = 5\nschema S\n    helper ∈ Nat\n    cond ∈ Bool\n    cond = false\n    cond ⇒ Init\n    helper = 100\n";
    rt.load_source(src).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    assert_eq!(r.bindings.get("helper"), Some(&Value::Int(100)));
}

#[test]
fn guarded_claim_multi_constraint() {
    let mut rt = EvidentRuntime::new();
    let src = "claim Init\n    a ∈ Int\n    b ∈ Int\n    a = 10\n    b = 20\nschema S\n    a ∈ Int\n    b ∈ Int\n    cond ∈ Bool\n    cond = true\n    cond ⇒ Init\n";
    rt.load_source(src).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    assert_eq!(r.bindings.get("a"), Some(&Value::Int(10)));
    assert_eq!(r.bindings.get("b"), Some(&Value::Int(20)));
}

#[test]
fn nested_guards_compose() {
    let mut rt = EvidentRuntime::new();
    let src = "claim Inner\n    n ∈ Int\n    n = 5\nclaim Outer\n    n ∈ Int\n    inner_flag ∈ Bool\n    inner_flag ⇒ Inner\nschema S\n    n ∈ Int\n    inner_flag ∈ Bool\n    outer_flag ∈ Bool\n    outer_flag = true\n    inner_flag = true\n    outer_flag ⇒ Outer\n";
    rt.load_source(src).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    assert_eq!(r.bindings.get("n"), Some(&Value::Int(5)));
}

#[test]
fn nested_guards_inner_false_skips() {
    let mut rt = EvidentRuntime::new();
    let src = "claim Inner\n    n ∈ Int\n    n = 5\nclaim Outer\n    n ∈ Int\n    inner_flag ∈ Bool\n    inner_flag ⇒ Inner\nschema S\n    n ∈ Int\n    inner_flag ∈ Bool\n    outer_flag ∈ Bool\n    outer_flag = true\n    inner_flag = false\n    outer_flag ⇒ Outer\n    n = 99\n";
    rt.load_source(src).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied, "inner_flag = false suppresses n = 5");
    assert_eq!(r.bindings.get("n"), Some(&Value::Int(99)));
}

#[test]
fn guarded_subclaim_init_pattern() {
    let mut rt = EvidentRuntime::new();
    let src = "schema S\n    n ∈ Int\n    step ∈ Nat\n    subclaim Init\n        n = 42\n    step = 0 ⇒ Init\n    step = 0\n";
    rt.load_source(src).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    assert_eq!(r.bindings.get("n"), Some(&Value::Int(42)));
    assert_eq!(r.bindings.get("step"), Some(&Value::Int(0)));
}

#[test]
fn guarded_subclaim_skipped_when_step_nonzero() {
    let mut rt = EvidentRuntime::new();
    let src = "schema S\n    n ∈ Int\n    step ∈ Nat\n    subclaim Init\n        n = 42\n    step = 0 ⇒ Init\n    step = 5\n    n = 999\n";
    rt.load_source(src).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    assert_eq!(r.bindings.get("n"), Some(&Value::Int(999)));
}
