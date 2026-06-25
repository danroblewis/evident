//! User-defined operator overloading: a type binds a binary operator (`·`, `×`)
//! to a body, and infix use desugars to a fresh result + the inlined body.
//!
//! Companion to `record_lift.rs` — these tests pin the new feature AND guard the
//! greenfield invariant that a type's NON-overridden operators (`+`, `=`, scalar
//! `*`) keep their componentwise-lift behavior unchanged.

use evident_runtime::{EvidentRuntime, Value};

/// Vec2 carrying a dot-product `·` (scalar result).
const VEC2_DOT: &str = "type Vec2\n    x, y ∈ Real\n    operator (a · b) ↦ s ∈ Real\n        s = a.x * b.x + a.y * b.y\n";

/// Vec2 carrying a custom `×` whose result is itself a Vec2 (record result).
const VEC2_CROSS: &str = "type Vec2\n    x, y ∈ Real\n    operator (a × b) ↦ c ∈ Vec2\n        c.x = a.x * b.y\n        c.y = a.y * b.x\n";

fn close(v: Option<&Value>, expected: f64, label: &str) {
    match v {
        Some(Value::Real(f)) => assert!(
            (f - expected).abs() < 1e-9,
            "{label}: expected ≈ {expected}, got {f}"
        ),
        other => panic!("{label}: expected Real, got {:?}", other),
    }
}

#[test]
fn dot_product_scalar_result_sat() {
    let mut rt = EvidentRuntime::new();
    let src = format!(
        "{VEC2_DOT}claim S\n    a ∈ Vec2(3.0, 4.0)\n    b ∈ Vec2(1.0, 2.0)\n    d ∈ Real\n    d = a · b\n"
    );
    rt.load_source(&src).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    // 3·1 + 4·2 = 11
    close(r.bindings.get("d"), 11.0, "d");
}

#[test]
fn dot_product_genuinely_constrains_not_dropped() {
    // If the operator body were silently dropped, `d` would be free and `d = 99`
    // would be SAT. It must be UNSAT — proving the body really constrains `d`.
    let mut rt = EvidentRuntime::new();
    let src = format!(
        "{VEC2_DOT}claim S\n    a ∈ Vec2(3.0, 4.0)\n    b ∈ Vec2(1.0, 2.0)\n    d ∈ Real\n    d = a · b\n    d = 99.0\n"
    );
    rt.load_source(&src).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(!r.satisfied, "d = a·b = 11, so d = 99 must be UNSAT");
}

#[test]
fn operator_composes_with_solver_unknown_operand() {
    // Leave an operand field free and let the solver invert the relation.
    let mut rt = EvidentRuntime::new();
    let src = format!(
        "{VEC2_DOT}claim S\n    a ∈ Vec2(3.0, 4.0)\n    b ∈ Vec2\n    b.y = 2.0\n    d ∈ Real\n    d = a · b\n    d = 11.0\n"
    );
    rt.load_source(&src).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    // 3·b.x + 4·2 = 11 ⇒ b.x = 1
    close(r.bindings.get("b.x"), 1.0, "b.x");
}

#[test]
fn record_returning_operator_sat() {
    let mut rt = EvidentRuntime::new();
    let src = format!(
        "{VEC2_CROSS}claim S\n    a ∈ Vec2(2.0, 3.0)\n    b ∈ Vec2(5.0, 7.0)\n    c ∈ Vec2\n    c = a × b\n"
    );
    rt.load_source(&src).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    close(r.bindings.get("c.x"), 14.0, "c.x"); // 2·7
    close(r.bindings.get("c.y"), 15.0, "c.y"); // 3·5
}

#[test]
fn record_returning_operator_genuinely_constrains() {
    let mut rt = EvidentRuntime::new();
    let src = format!(
        "{VEC2_CROSS}claim S\n    a ∈ Vec2(2.0, 3.0)\n    b ∈ Vec2(5.0, 7.0)\n    c ∈ Vec2\n    c = a × b\n    c.x = 99.0\n"
    );
    rt.load_source(&src).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(!r.satisfied, "c.x = 14, so c.x = 99 must be UNSAT");
}

#[test]
fn non_overridden_plus_stays_componentwise() {
    // GREENFIELD INVARIANT: a type WITH an operator decl keeps the componentwise
    // lift for the operators it does NOT override. `+` here is still field-wise.
    let mut rt = EvidentRuntime::new();
    let src = format!(
        "{VEC2_DOT}claim S\n    a ∈ Vec2(2.0, 3.0)\n    b ∈ Vec2(5.0, 7.0)\n    c ∈ Vec2\n    c = a + b\n"
    );
    rt.load_source(&src).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    close(r.bindings.get("c.x"), 7.0, "c.x");
    close(r.bindings.get("c.y"), 10.0, "c.y");
}

// NOTE: a `·` on a type that declares no matching operator surfaces the
// standard dropped-constraint failure (the constraint can't translate to Bool).
// That policy is a hard process-abort, not a recoverable `Err`, so it can't be
// asserted from a unit test without killing the harness — it's covered at the
// CLI level instead. The point it would prove (never silently dropped) is
// already pinned by the two `*_genuinely_constrains` tests above.
