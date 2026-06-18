//! Record literals in expression position: `IVec2(380, 280)` as a
//! value, not just at a declaration site. Threads through the lift
//! machinery's positional substitution: for each leaf in the LHS
//! record, look up the matching arg by index.

use evident_runtime::{EvidentRuntime, Value};

const VEC2: &str = "type IVec2(x, y ∈ Int)\n";
const COLOR: &str = "type Color(r, g, b ∈ Nat)\n";

fn int(v: Option<&Value>) -> i64 {
    match v {
        Some(Value::Int(n)) => *n,
        other => panic!("expected Int, got {:?}", other),
    }
}

/// Bare assignment: `pos = IVec2(380, 280)` expands to per-leaf
/// equalities by indexing into the literal's args by IVec2's
/// declaration order.
#[test]
fn record_literal_assignment_to_record_var() {
    let mut rt = EvidentRuntime::new();
    let src = format!("{VEC2}schema S\n    pos ∈ IVec2\n    pos = IVec2(380, 280)\n");
    rt.load_source(&src).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    assert_eq!(int(r.bindings.get("pos.x")), 380);
    assert_eq!(int(r.bindings.get("pos.y")), 280);
}

/// Record literal inside arithmetic — `dot.pos - IVec2(12, 12)` is the
/// headline anchor_collect.ev rendering pattern. The literal is one
/// operand of a Binary.
#[test]
fn record_literal_in_arithmetic() {
    let mut rt = EvidentRuntime::new();
    let src = format!("{VEC2}schema S\n    a ∈ IVec2\n    b ∈ IVec2\n    a = IVec2(100, 200)\n    b = a - IVec2(12, 12)\n");
    rt.load_source(&src).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    assert_eq!(int(r.bindings.get("b.x")), 88);
    assert_eq!(int(r.bindings.get("b.y")), 188);
}

/// Three-arg record literal (Color). Confirms it works for arity > 2.
#[test]
fn three_arg_record_literal() {
    let mut rt = EvidentRuntime::new();
    let src = format!("{COLOR}schema S\n    sky ∈ Color\n    sky = Color(30, 80, 120)\n");
    rt.load_source(&src).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    assert_eq!(int(r.bindings.get("sky.r")), 30);
    assert_eq!(int(r.bindings.get("sky.g")), 80);
    assert_eq!(int(r.bindings.get("sky.b")), 120);
}

/// Both sides of an Eq are record literals — both contribute to the
/// shape check, both substitute on each leaf.
#[test]
fn record_literal_on_both_sides() {
    let mut rt = EvidentRuntime::new();
    let src = format!("{VEC2}schema S\n    a ∈ IVec2\n    a = IVec2(5, 7)\n    IVec2(5, 7) = a\n");
    rt.load_source(&src).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    assert_eq!(int(r.bindings.get("a.x")), 5);
    assert_eq!(int(r.bindings.get("a.y")), 7);
}

/// Vector chained comparison with record literals as bounds.
/// `IVec2(0, 0) ≤ pos ≤ IVec2(100, 100)` expands per-axis.
#[test]
fn record_literal_in_chained_comparison() {
    let mut rt = EvidentRuntime::new();
    let src = format!("{VEC2}schema S\n    pos ∈ IVec2\n    IVec2(0, 0) ≤ pos ≤ IVec2(100, 100)\n    pos = IVec2(50, 75)\n");
    rt.load_source(&src).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    assert_eq!(int(r.bindings.get("pos.x")), 50);
    assert_eq!(int(r.bindings.get("pos.y")), 75);
}

/// The headline anchor_collect.ev player-init shape: assign a literal
/// to a sub-record field on a record-typed variable.
#[test]
fn record_literal_assigning_sub_record() {
    let mut rt = EvidentRuntime::new();
    let src = format!(
        "{VEC2}type Player\n    pos ∈ IVec2\n    vel ∈ IVec2\nschema S\n    p ∈ Player\n    p.pos = IVec2(380, 280)\n    p.vel = IVec2(0, 0)\n"
    );
    rt.load_source(&src).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    assert_eq!(int(r.bindings.get("p.pos.x")), 380);
    assert_eq!(int(r.bindings.get("p.pos.y")), 280);
    assert_eq!(int(r.bindings.get("p.vel.x")), 0);
    assert_eq!(int(r.bindings.get("p.vel.y")), 0);
}
