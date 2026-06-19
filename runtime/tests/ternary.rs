use evident_runtime::{EvidentRuntime, Value};

#[test]
fn ternary_int_branch_true() {
    let mut rt = EvidentRuntime::new();
    rt.load_source("\
claim t
    x ∈ Int
    flag ∈ Bool
    flag = true
    x = (flag ? 10 : 20)
").unwrap();
    let r = rt.query_free("t").unwrap();
    assert!(r.satisfied);
    assert_eq!(r.bindings.get("x"), Some(&Value::Int(10)));
}

#[test]
fn ternary_int_branch_false() {
    let mut rt = EvidentRuntime::new();
    rt.load_source("\
claim t
    x ∈ Int
    flag ∈ Bool
    flag = false
    x = (flag ? 10 : 20)
").unwrap();
    let r = rt.query_free("t").unwrap();
    assert!(r.satisfied);
    assert_eq!(r.bindings.get("x"), Some(&Value::Int(20)));
}

#[test]
fn ternary_string_branches() {
    let mut rt = EvidentRuntime::new();
    rt.load_source("\
claim t
    s ∈ String
    cond ∈ Bool
    cond = true
    s = (cond ? \"yes\" : \"no\")
").unwrap();
    let r = rt.query_free("t").unwrap();
    assert!(r.satisfied);
    match r.bindings.get("s") {
        Some(Value::Str(v)) => assert_eq!(v, "yes"),
        other => panic!("expected String, got {other:?}"),
    }
}

#[test]
fn ternary_nested_right_associative() {

    let mut rt = EvidentRuntime::new();
    rt.load_source("\
claim t
    x ∈ Int
    a ∈ Bool
    c ∈ Bool
    a = false
    c = true
    x = (a ? 1 : c ? 2 : 3)
").unwrap();
    let r = rt.query_free("t").unwrap();
    assert!(r.satisfied);
    assert_eq!(r.bindings.get("x"), Some(&Value::Int(2)));
}

#[test]
fn ternary_in_arithmetic() {
    let mut rt = EvidentRuntime::new();
    rt.load_source("\
claim t
    x ∈ Int
    flag ∈ Bool
    flag = true
    x = (flag ? 10 : 20) + 5
").unwrap();
    let r = rt.query_free("t").unwrap();
    assert!(r.satisfied);
    assert_eq!(r.bindings.get("x"), Some(&Value::Int(15)));
}

#[test]
fn ternary_missing_colon_is_parse_error() {
    let mut rt = EvidentRuntime::new();
    let err = rt.load_source("\
claim t
    x ∈ Int
    flag ∈ Bool
    x = (flag ? 10)
").unwrap_err();
    let msg = format!("{err}");
    assert!(msg.to_lowercase().contains("colon") || msg.contains(":"),
        "expected colon-related parse error, got: {msg}");
}
