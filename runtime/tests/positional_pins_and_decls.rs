//! Three related features:
//!   - Positional pins: `IVec2(20, 20)` uses field declaration order.
//!   - Multi-name body decl: `x, y ∈ Int` declares both as Int.
//!   - First-line param list: `type IVec2(x, y ∈ Int)` is shorthand
//!     for declaring the fields on the schema header line and
//!     establishes the canonical positional order.
//!
//! Pinning these so the parser disambiguation (named vs positional vs
//! compound type) and the translator's positional-resolve path can't
//! quietly regress.

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

/// Positional follows declaration order, not alphabetical. Color's
/// fields are r, g, b in that order — positional `(255, 128, 64)`
/// must map r=255, g=128, b=64 (NOT b=255, g=128, r=64 if we'd
/// sorted).
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

/// Partial positional: too few args pin the leading fields and
/// leave the rest free. `IVec2(10)` pins only x; y stays unconstrained.
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

/// Too many args — pinning fields that don't exist on the type.
/// Hard error.
#[test]
fn positional_pins_too_many_is_an_error() {
    use std::io::Write;
    use std::process::Command;
    let mut path = std::env::temp_dir();
    path.push(format!("evident-pos-too-many-{}.ev", std::process::id()));
    let mut f = std::fs::File::create(&path).unwrap();
    let src = "type IVec2(x, y ∈ Int)\nschema S\n    v ∈ IVec2(1, 2, 3)\n";
    f.write_all(src.as_bytes()).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_evident"))
        .args(["query", path.to_str().unwrap(), "S"])
        .output().unwrap();
    assert!(!out.status.success(), "too many positional args must error");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("too many positional"), "expected too-many error: {stderr}");
}

/// Multi-name in the body: `x, y ∈ Int` declares two fields with
/// the same type. Order preserved.
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

/// First-line param list: `type IVec2(x, y ∈ Int)` declares fields on
/// the header line. Equivalent to body declarations but more compact
/// for short types.
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

/// Mixed: first-line params with multiple groups of different types.
/// `(x ∈ Int, y ∈ Bool)` — two groups, one name each, different types.
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

/// First-line + body: header declares some fields, body adds more.
/// Order: header items first, body items after.
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

/// Named pins still work alongside positional — disambiguation is on
/// `mapsto` keyword/symbol presence after the first arg.
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

/// Compound types (`Seq(Int)`) still parse correctly — they're not
/// confused with positional pins because Seq/Set/Bag/Map are
/// hardcoded compound heads.
#[test]
fn compound_types_still_parse() {
    let mut rt = EvidentRuntime::new();
    let src = "schema S\n    items ∈ Seq(Int)\n    #items = 3\n    items[0] = 1\n    items[1] = 2\n    items[2] = 3\n";
    rt.load_source(src).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
}
