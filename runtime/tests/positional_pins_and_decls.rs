use evident_runtime::{EvidentRuntime, Value};

#[test]
fn positional_pins_two_fields() {
    let mut rt = EvidentRuntime::new();
    let src = "type IVec2\n    x ∈ Int\n    y ∈ Int\nschema S\n    v ∈ IVec2(-800, 540)\n";
    rt.load_source(src).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    assert_eq!(r.bindings.get("v.x"), Some(&Value::Int(-800)));
    assert_eq!(r.bindings.get("v.y"), Some(&Value::Int(540)));
}

#[test]
fn positional_pins_follow_declaration_order_not_alphabetical() {
    let mut rt = EvidentRuntime::new();
    let src = "type Color\n    r ∈ Nat\n    g ∈ Nat\n    b ∈ Nat\nschema S\n    sky ∈ Color(30, 80, 120)\n";
    rt.load_source(src).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    assert_eq!(r.bindings.get("sky.r"), Some(&Value::Int(30)));
    assert_eq!(r.bindings.get("sky.g"), Some(&Value::Int(80)));
    assert_eq!(r.bindings.get("sky.b"), Some(&Value::Int(120)));
}

#[test]
fn positional_pins_partial_pins_leading_fields() {
    let mut rt = EvidentRuntime::new();
    let src = "type IVec2(x, y ∈ Int)\nschema S\n    v ∈ IVec2(10)\n    v.y = 99\n";
    rt.load_source(src).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    assert_eq!(r.bindings.get("v.x"), Some(&Value::Int(10)));
    assert_eq!(r.bindings.get("v.y"), Some(&Value::Int(99)));
}

#[test]
fn multi_name_body_decl() {
    let mut rt = EvidentRuntime::new();
    let src = "type IVec2\n    x, y ∈ Int\nschema S\n    v ∈ IVec2(7, 11)\n";
    rt.load_source(src).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    assert_eq!(r.bindings.get("v.x"), Some(&Value::Int(7)));
    assert_eq!(r.bindings.get("v.y"), Some(&Value::Int(11)));
}

#[test]
fn first_line_params_basic() {
    let mut rt = EvidentRuntime::new();
    let src = "type IVec2(x, y ∈ Int)\nschema S\n    v ∈ IVec2(7, 11)\n";
    rt.load_source(src).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    assert_eq!(r.bindings.get("v.x"), Some(&Value::Int(7)));
    assert_eq!(r.bindings.get("v.y"), Some(&Value::Int(11)));
}

#[test]
fn first_line_params_mixed_types() {
    let mut rt = EvidentRuntime::new();
    let src = "type Mix(x ∈ Int, y ∈ Bool)\nschema S\n    m ∈ Mix\n    m.x = 42\n    m.y = true\n";
    rt.load_source(src).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    assert_eq!(r.bindings.get("m.x"), Some(&Value::Int(42)));
    assert_eq!(r.bindings.get("m.y"), Some(&Value::Bool(true)));
}

#[test]
fn first_line_params_plus_body() {
    let mut rt = EvidentRuntime::new();
    let src = "type Mix(a, b ∈ Int)\n    c ∈ Bool\nschema S\n    m ∈ Mix(1, 2)\n    m.c = false\n";
    rt.load_source(src).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    assert_eq!(r.bindings.get("m.a"), Some(&Value::Int(1)));
    assert_eq!(r.bindings.get("m.b"), Some(&Value::Int(2)));
    assert_eq!(r.bindings.get("m.c"), Some(&Value::Bool(false)));
}

#[test]
fn named_pins_still_work_after_positional_added() {
    let mut rt = EvidentRuntime::new();
    let src = "type IVec2(x, y ∈ Int)\nschema S\n    a ∈ IVec2(10, 20)\n    b ∈ IVec2(y ↦ 99, x ↦ 88)\n";
    rt.load_source(src).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    assert_eq!(r.bindings.get("a.x"), Some(&Value::Int(10)));
    assert_eq!(r.bindings.get("a.y"), Some(&Value::Int(20)));
    assert_eq!(r.bindings.get("b.x"), Some(&Value::Int(88)));
    assert_eq!(r.bindings.get("b.y"), Some(&Value::Int(99)));
}

#[test]
fn compound_types_still_parse() {
    let mut rt = EvidentRuntime::new();
    let src = "schema S\n    items ∈ Seq(Int)\n    #items = 3\n    items[0] = 1\n    items[1] = 2\n    items[2] = 3\n";
    rt.load_source(src).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
}
