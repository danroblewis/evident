//! Field-wise operator lifting on record-typed values.
//!
//! `a = b` between two values of the same record type expands to a
//! conjunction over fields (`a.x = b.x ∧ a.y = b.y`). Same idea for
//! `<`, `≤`, `>`, `≥` (componentwise) and `≠` (some field differs,
//! disjunction). This frees users from hand-writing field equations
//! when a single comparison already says it.
//!
//! Pinning the behavior here so the hook in `translate_bool` doesn't
//! regress when the comparison arms get reshuffled.

use evident_runtime::{EvidentRuntime, Value};

const VEC2: &str = "type Vec2\n    x ∈ Real\n    y ∈ Real\n";
const VEC3: &str = "type Vec3\n    x ∈ Real\n    y ∈ Real\n    z ∈ Real\n";

fn close(v: Option<&Value>, expected: f64, label: &str) {
    match v {
        Some(Value::Real(f)) => assert!(
            (f - expected).abs() < 1e-9,
            "{label}: expected ≈ {expected}, got {f}"
        ),
        other => panic!("{label}: expected Real, got {:?}", other),
    }
}

/// Baseline: `a = b` lifts componentwise. Pinning a's fields forces
/// b's to match, proving the expansion produced equality on each.
#[test]
fn record_eq_two_vec2() {
    let mut rt = EvidentRuntime::new();
    let src = format!("{VEC2}schema S\n    a ∈ Vec2\n    b ∈ Vec2\n    a = b\n    a.x = 3.14\n    a.y = 2.71\n");
    rt.load_source(&src).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    close(r.bindings.get("b.x"), 3.14, "b.x");
    close(r.bindings.get("b.y"), 2.71, "b.y");
}

/// `a ≠ b` is *disjunctive* — only one field needs to differ.
#[test]
fn record_neq_disjunctive_some_field_differs() {
    let mut rt = EvidentRuntime::new();
    let src = format!("{VEC2}schema S\n    a ∈ Vec2\n    b ∈ Vec2\n    a ≠ b\n    a.x = 1.0\n    a.y = 2.0\n    b.x = 1.0\n    b.y = 9.0\n");
    rt.load_source(&src).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied, "y differs, so a ≠ b should hold");
}

#[test]
fn record_neq_unsat_when_all_fields_equal() {
    let mut rt = EvidentRuntime::new();
    let src = format!("{VEC2}schema S\n    a ∈ Vec2\n    b ∈ Vec2\n    a ≠ b\n    a.x = 1.0\n    a.y = 2.0\n    b.x = 1.0\n    b.y = 2.0\n");
    rt.load_source(&src).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(!r.satisfied, "all fields equal → a ≠ b must be UNSAT");
}

/// `a < b` is *componentwise* — every field must be strictly less.
/// Equal-on-one-axis case must be UNSAT.
#[test]
fn record_lt_componentwise_unsat_when_one_axis_equal() {
    let mut rt = EvidentRuntime::new();
    let src = format!("{VEC2}schema S\n    a ∈ Vec2\n    b ∈ Vec2\n    a < b\n    a.x = 1.0\n    a.y = 2.0\n    b.x = 5.0\n    b.y = 2.0\n");
    rt.load_source(&src).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(!r.satisfied, "a.y = b.y violates strict componentwise <");
}

#[test]
fn record_lt_sat_when_all_axes_strict() {
    let mut rt = EvidentRuntime::new();
    let src = format!("{VEC2}schema S\n    a ∈ Vec2\n    b ∈ Vec2\n    a < b\n    a.x = 1.0\n    a.y = 2.0\n    b.x = 5.0\n    b.y = 7.0\n");
    rt.load_source(&src).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
}

/// The headline use case: `min ≤ pos ≤ max`. Verifies chained
/// comparison composes correctly with record lift.
#[test]
fn record_chained_in_box() {
    let mut rt = EvidentRuntime::new();
    let src = format!("{VEC2}schema InBox\n    pos ∈ Vec2\n    min ∈ Vec2\n    max ∈ Vec2\n    min ≤ pos ≤ max\n    min.x = 0.0\n    min.y = 0.0\n    max.x = 10.0\n    max.y = 10.0\n    pos.x = 5.0\n    pos.y = 7.5\n");
    rt.load_source(&src).unwrap();
    let r = rt.query_free("InBox").unwrap();
    assert!(r.satisfied);
    close(r.bindings.get("pos.x"), 5.0, "pos.x");
    close(r.bindings.get("pos.y"), 7.5, "pos.y");
}

/// Out-of-box must be UNSAT. Verifies the chain isn't just folding
/// to `true` somewhere.
#[test]
fn record_chained_in_box_violation() {
    let mut rt = EvidentRuntime::new();
    let src = format!("{VEC2}schema InBox\n    pos ∈ Vec2\n    min ∈ Vec2\n    max ∈ Vec2\n    min ≤ pos ≤ max\n    min.x = 0.0\n    min.y = 0.0\n    max.x = 10.0\n    max.y = 10.0\n    pos.x = 5.0\n    pos.y = 99.0\n");
    rt.load_source(&src).unwrap();
    let r = rt.query_free("InBox").unwrap();
    assert!(!r.satisfied, "pos.y = 99 is outside box max.y = 10");
}

/// Vec3 lift pins three fields per comparison instead of two.
#[test]
fn record_eq_two_vec3() {
    let mut rt = EvidentRuntime::new();
    let src = format!("{VEC3}schema S\n    a ∈ Vec3\n    b ∈ Vec3\n    a = b\n    a.x = 1.0\n    a.y = 2.0\n    a.z = 3.0\n");
    rt.load_source(&src).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    close(r.bindings.get("b.x"), 1.0, "b.x");
    close(r.bindings.get("b.y"), 2.0, "b.y");
    close(r.bindings.get("b.z"), 3.0, "b.z");
}

/// Nested records: `outer1 = outer2` where Outer contains a Vec2.
/// The flat-leaf encoding makes nested lift fall out for free —
/// `outer.inner.x` and `outer.inner.y` are already top-level keys
/// under the `outer.` prefix scan.
#[test]
fn record_lift_nested_struct() {
    let mut rt = EvidentRuntime::new();
    let src = format!("{VEC2}type Outer\n    inner ∈ Vec2\n    tag ∈ Real\nschema S\n    a ∈ Outer\n    b ∈ Outer\n    a = b\n    a.inner.x = 1.5\n    a.inner.y = 2.5\n    a.tag = 9.0\n");
    rt.load_source(&src).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    close(r.bindings.get("b.inner.x"), 1.5, "b.inner.x");
    close(r.bindings.get("b.inner.y"), 2.5, "b.inner.y");
    close(r.bindings.get("b.tag"), 9.0, "b.tag");
}

/// Cross-type comparison (Vec2 vs Vec3) is shape-mismatched. The lift
/// must return `None` rather than silently lifting only the overlap
/// {x, y} and ignoring Vec3's extra `z`. With the runtime's default
/// dropped-constraint policy, that None propagates to a fatal error;
/// in lenient mode it warns. We test via subprocess so we can observe
/// the exit code and stderr — `process::exit` would tank the test
/// harness if invoked in-process.
#[test]
fn record_lift_shape_mismatch_is_an_error() {
    use std::io::Write;
    use std::process::Command;
    let mut path = std::env::temp_dir();
    path.push(format!("evident-record-lift-mismatch-{}.ev", std::process::id()));
    let mut f = std::fs::File::create(&path).unwrap();
    let src = format!("{VEC2}{VEC3}schema S\n    a ∈ Vec2\n    b ∈ Vec3\n    a = b\n");
    f.write_all(src.as_bytes()).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_evident"))
        .args(["sample", path.to_str().unwrap(), "S", "-n", "1"])
        .output().unwrap();
    assert!(!out.status.success(), "shape mismatch should be fatal");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("dropped constraint"),
        "expected dropped-constraint error, stderr was:\n{stderr}"
    );
}

/// Lift mixed with explicit per-field constraints. The lift's
/// conjunction must AND with the rest of the body, not replace it.
#[test]
fn record_lift_combined_with_field_constraints() {
    let mut rt = EvidentRuntime::new();
    let src = format!("{VEC2}schema S\n    a ∈ Vec2\n    b ∈ Vec2\n    a ≤ b\n    a.x = 1.0\n    a.y = 2.0\n    b.y = 5.0\n    b.x ≥ 3.0\n");
    rt.load_source(&src).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    close(r.bindings.get("a.x"), 1.0, "a.x");
    close(r.bindings.get("a.y"), 2.0, "a.y");
    let bx = match r.bindings.get("b.x") {
        Some(Value::Real(f)) => *f,
        other => panic!("b.x: expected Real, got {:?}", other),
    };
    assert!(bx >= 3.0 && bx >= 1.0, "b.x must satisfy both ≥ 3.0 and ≥ a.x; got {bx}");
}

/// Lift over Int-typed fields too — the per-leaf recursion goes
/// through `translate_int`, not just Real. Pins both axes via the
/// lifted `=`.
#[test]
fn record_lift_with_int_fields() {
    let mut rt = EvidentRuntime::new();
    rt.load_source(
        "type IPair\n    a ∈ Int\n    b ∈ Int\nschema S\n    p ∈ IPair\n    q ∈ IPair\n    p = q\n    p.a = 7\n    p.b = -3\n",
    ).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    assert_eq!(r.bindings.get("q.a"), Some(&Value::Int(7)));
    assert_eq!(r.bindings.get("q.b"), Some(&Value::Int(-3)));
}
