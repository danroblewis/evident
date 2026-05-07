//! `Seq(UserType)` round-trip through `given` when the user type has
//! nested record fields.
//!
//! Regression: `assert_seq_given`'s `FieldKind::Nested` arm used to
//! return None ("skip for v1"), so the SDL executor's
//! "extract state from frame N → re-inject as given for frame N+1"
//! loop silently dropped the entire `state.dots` constraint when
//! `BouncingDot` had `pos ∈ IVec2` instead of flat `pos_x` / `pos_y`
//! fields. Symptom: warnings spammed and the next frame ran
//! unconstrained → state went "crazy".
//!
//! This pins the symmetric behavior: model extraction writes a
//! `Value::SeqComposite` containing `Value::Composite` for each nested
//! field; injection has to read it back in.

use std::collections::HashMap;
use evident_runtime::{EvidentRuntime, Value};

const SRC: &str = "type IVec2\n    x ∈ Int\n    y ∈ Int\ntype Dot\n    pos       ∈ IVec2\n    vel       ∈ IVec2\n    eff_vy    ∈ Int\n    collected ∈ Bool\ntype S\n    dots ∈ Seq(Dot)\n    #dots = 2\n";

/// Free query produces a model populated for a `Seq(Dot)` whose
/// elements have nested IVec2 fields. The output values are arbitrary;
/// what we care about is the shape: `Value::SeqComposite` of maps,
/// each containing `Value::Composite` for `pos` and `vel`.
#[test]
fn extract_seq_with_nested_record_fields() {
    let mut rt = EvidentRuntime::new();
    rt.load_source(SRC).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    let dots = r.bindings.get("dots").expect("missing dots binding");
    let Value::SeqComposite(items) = dots else {
        panic!("expected SeqComposite, got {:?}", dots);
    };
    assert_eq!(items.len(), 2);
    // Every dot has a Composite pos with x and y leaves.
    for (i, m) in items.iter().enumerate() {
        let pos = m.get("pos").unwrap_or_else(|| panic!("dot[{i}] missing pos"));
        let Value::Composite(pos_map) = pos else {
            panic!("dot[{i}].pos: expected Composite, got {:?}", pos);
        };
        assert!(matches!(pos_map.get("x"), Some(Value::Int(_))));
        assert!(matches!(pos_map.get("y"), Some(Value::Int(_))));
    }
}

/// The fix: a `SeqComposite` with nested `Composite` fields can be
/// passed back as a `given` and the solver's bindings reflect every
/// pinned leaf. Before this, `assert_seq_given` returned None and the
/// caller printed "type mismatch for given" — the dots stayed free.
#[test]
fn given_seq_with_nested_record_fields_round_trips() {
    let mut rt = EvidentRuntime::new();
    rt.load_source(SRC).unwrap();

    // Build a Value::SeqComposite by hand — same shape `extract`
    // produces, same shape the executor passes as `given`.
    let dot0 = HashMap::from([
        (
            "pos".to_string(),
            Value::Composite(HashMap::from([
                ("x".to_string(), Value::Int(11)),
                ("y".to_string(), Value::Int(22)),
            ])),
        ),
        (
            "vel".to_string(),
            Value::Composite(HashMap::from([
                ("x".to_string(), Value::Int(-3)),
                ("y".to_string(), Value::Int(7)),
            ])),
        ),
        ("eff_vy".to_string(), Value::Int(100)),
        ("collected".to_string(), Value::Bool(false)),
    ]);
    let dot1 = HashMap::from([
        (
            "pos".to_string(),
            Value::Composite(HashMap::from([
                ("x".to_string(), Value::Int(33)),
                ("y".to_string(), Value::Int(44)),
            ])),
        ),
        (
            "vel".to_string(),
            Value::Composite(HashMap::from([
                ("x".to_string(), Value::Int(5)),
                ("y".to_string(), Value::Int(-9)),
            ])),
        ),
        ("eff_vy".to_string(), Value::Int(-200)),
        ("collected".to_string(), Value::Bool(true)),
    ]);
    let mut given = HashMap::new();
    given.insert(
        "dots".to_string(),
        Value::SeqComposite(vec![dot0, dot1]),
    );

    let r = rt.query("S", &given).unwrap();
    assert!(r.satisfied, "SeqComposite given with nested fields must inject cleanly");
    let Some(Value::SeqComposite(items)) = r.bindings.get("dots") else {
        panic!("expected SeqComposite back, got {:?}", r.bindings.get("dots"));
    };
    assert_eq!(items.len(), 2);

    // Verify every leaf round-tripped: nested pos.x / pos.y / vel.x /
    // vel.y plus the flat eff_vy / collected.
    let p0 = match items[0].get("pos") {
        Some(Value::Composite(m)) => m,
        other => panic!("dot[0].pos: {:?}", other),
    };
    assert_eq!(p0.get("x"), Some(&Value::Int(11)));
    assert_eq!(p0.get("y"), Some(&Value::Int(22)));
    let v0 = match items[0].get("vel") {
        Some(Value::Composite(m)) => m,
        other => panic!("dot[0].vel: {:?}", other),
    };
    assert_eq!(v0.get("x"), Some(&Value::Int(-3)));
    assert_eq!(v0.get("y"), Some(&Value::Int(7)));
    assert_eq!(items[0].get("eff_vy"), Some(&Value::Int(100)));
    assert_eq!(items[0].get("collected"), Some(&Value::Bool(false)));

    let p1 = match items[1].get("pos") {
        Some(Value::Composite(m)) => m,
        other => panic!("dot[1].pos: {:?}", other),
    };
    assert_eq!(p1.get("x"), Some(&Value::Int(33)));
    assert_eq!(p1.get("y"), Some(&Value::Int(44)));
    let v1 = match items[1].get("vel") {
        Some(Value::Composite(m)) => m,
        other => panic!("dot[1].vel: {:?}", other),
    };
    assert_eq!(v1.get("x"), Some(&Value::Int(5)));
    assert_eq!(v1.get("y"), Some(&Value::Int(-9)));
    assert_eq!(items[1].get("eff_vy"), Some(&Value::Int(-200)));
    assert_eq!(items[1].get("collected"), Some(&Value::Bool(true)));
}

/// Full round-trip: extract a model, pass the extraction back as
/// given, get the same shape back. Mirrors the executor's per-frame
/// state-handoff loop.
#[test]
fn extract_then_inject_round_trip() {
    let mut rt = EvidentRuntime::new();
    rt.load_source(SRC).unwrap();
    let first = rt.query_free("S").unwrap();
    assert!(first.satisfied);
    let dots = first.bindings.get("dots").cloned().expect("first.dots");

    let mut given = HashMap::new();
    given.insert("dots".to_string(), dots.clone());
    let second = rt.query("S", &given).unwrap();
    assert!(second.satisfied, "re-injecting a freshly-extracted model must succeed");
    assert_eq!(second.bindings.get("dots"), Some(&dots));
}
