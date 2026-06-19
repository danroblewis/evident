use evident_runtime::{EvidentRuntime, Value};

const VEC2: &str = "type IVec2\n    x ∈ Int\n    y ∈ Int\n";
const COLOR: &str = "type Color\n    r ∈ Nat\n    g ∈ Nat\n    b ∈ Nat\n";

#[test]
fn basic_two_field_pin() {
    let mut rt = EvidentRuntime::new();
    let src = format!("{VEC2}schema S\n    v ∈ IVec2 (x ↦ -800, y ↦ 540)\n");
    rt.load_source(&src).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    assert_eq!(r.bindings.get("v.x"), Some(&Value::Int(-800)));
    assert_eq!(r.bindings.get("v.y"), Some(&Value::Int(540)));
}

#[test]
fn pins_by_name_not_position() {
    let mut rt = EvidentRuntime::new();
    let src = format!("{COLOR}schema S\n    sky ∈ Color (b ↦ 200, r ↦ 30, g ↦ 80)\n");
    rt.load_source(&src).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    assert_eq!(r.bindings.get("sky.r"), Some(&Value::Int(30)));
    assert_eq!(r.bindings.get("sky.g"), Some(&Value::Int(80)));
    assert_eq!(r.bindings.get("sky.b"), Some(&Value::Int(200)));
}

#[test]
fn partial_pin_leaves_other_fields_free() {
    let mut rt = EvidentRuntime::new();
    let src = format!("{VEC2}schema S\n    v ∈ IVec2 (x ↦ 42)\n    v.y ≥ 100\n");
    rt.load_source(&src).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    assert_eq!(r.bindings.get("v.x"), Some(&Value::Int(42)));
    let y = match r.bindings.get("v.y") {
        Some(Value::Int(n)) => *n,
        other => panic!("v.y: expected Int, got {:?}", other),
    };
    assert!(y >= 100, "v.y was free + constrained ≥ 100; got {y}");
}

#[test]
fn mapsto_keyword_form() {
    let mut rt = EvidentRuntime::new();
    let src = format!(
        "{VEC2}schema S\n    v ∈ IVec2 (x mapsto 7, y mapsto 11)\n"
    );
    rt.load_source(&src).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    assert_eq!(r.bindings.get("v.x"), Some(&Value::Int(7)));
    assert_eq!(r.bindings.get("v.y"), Some(&Value::Int(11)));
}

#[test]
fn pin_and_no_pin_on_same_type() {
    let mut rt = EvidentRuntime::new();
    let src = format!(
        "{VEC2}schema S\n    a ∈ IVec2\n    b ∈ IVec2 (x ↦ 9, y ↦ 13)\n    a = b\n"
    );
    rt.load_source(&src).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    assert_eq!(r.bindings.get("a.x"), Some(&Value::Int(9)));
    assert_eq!(r.bindings.get("a.y"), Some(&Value::Int(13)));
}

#[test]
fn pin_value_can_be_an_expression() {
    let mut rt = EvidentRuntime::new();
    let src = format!(
        "{VEC2}schema S\n    base ∈ IVec2 (x ↦ 100, y ↦ 200)\n    \
         offset ∈ IVec2 (x ↦ base.x + 5, y ↦ base.y * 2)\n"
    );
    rt.load_source(&src).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    assert_eq!(r.bindings.get("offset.x"), Some(&Value::Int(105)));
    assert_eq!(r.bindings.get("offset.y"), Some(&Value::Int(400)));
}
