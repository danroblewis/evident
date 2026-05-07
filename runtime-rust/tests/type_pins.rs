//! Type-use mapsto pins.
//!
//! `name ∈ Type (field mapsto value, …)` is sugar for the declaration
//! plus per-pin equality constraints. Same `mapsto` semantics as
//! claim-call composition — a type at a declaration site is the same
//! parameterizable set, narrowed by the same mechanism.
//!
//! Six things to pin down so this can't regress:
//!   - basic two-field pin (IVec2)
//!   - declaration-order independence (Color: r, g, b)
//!   - partial pinning (some fields free, others pinned)
//!   - mixed `mapsto` / `↦` spellings (already-tested via normalizer)
//!   - works alongside no-pin form on the same type
//!   - shape mismatch (pin field that doesn't exist) surfaces an error

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

/// Color's declaration order is r, g, b. Pinning by NAME (not position)
/// must produce the right values regardless of pin order in source —
/// proves we're not silently sorting alphabetically (which would map
/// the first pin to `b`, the alphabetically first leaf).
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

/// Pin some fields, leave others free. The free fields stay
/// unconstrained at declaration time — Z3 picks any value (we just
/// verify the pinned ones).
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

/// `mapsto` keyword and `↦` symbol both work — the lexer maps them to
/// the same token, so this is mostly checking the grammar accepts the
/// alternate spelling for type-use the same as claim-call.
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

/// Pinned and unpinned membership on the same type can coexist —
/// pin parsing must not break the fall-through to the bare-membership
/// path.
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

/// Pin value is an arithmetic expression, not just a literal. The
/// pin's RHS goes through the same translator as a regular constraint,
/// so any expression should work.
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

/// Pin a field that doesn't exist on the type. `IVec2.z` is bogus.
/// The pin fires `v.z = 0` as a constraint; since `v.z` isn't in env,
/// the constraint can't translate and drops with the runtime's
/// dropped-constraint error. Subprocess-tested so we can observe the
/// fatal exit.
#[test]
fn pin_unknown_field_is_an_error() {
    use std::io::Write;
    use std::process::Command;
    let mut path = std::env::temp_dir();
    path.push(format!("evident-pin-bogus-{}.ev", std::process::id()));
    let mut f = std::fs::File::create(&path).unwrap();
    let src = format!("{VEC2}schema S\n    v ∈ IVec2 (z ↦ 0)\n");
    f.write_all(src.as_bytes()).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_evident"))
        .args(["query", path.to_str().unwrap(), "S"])
        .output().unwrap();
    assert!(!out.status.success(), "unknown-field pin must error");
}
