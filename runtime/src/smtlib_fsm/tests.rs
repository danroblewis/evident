//! Unit tests for the SMT-LIB FSM path: metadata parsing, synthetic-schema
//! shape, and the single-tick solve (the Phase-2 gate at unit level).

use std::collections::HashMap;

use super::*;
use crate::core::ast::{BodyItem, Keyword};
use crate::core::Value;
use crate::runtime::EvidentRuntime;

/// A scalar countdown FSM: `count = is_first_tick ? 3 : _count - 1`, prints
/// "tick" while counting and "liftoff" + Exit(0) at zero.
fn countdown_fsm() -> SmtLibFsm {
    let json = r#"{
        "fsm": "countdown",
        "vars": [
            {"name": "count", "sort": "Int"},
            {"name": "_count", "sort": "Int"},
            {"name": "is_first_tick", "sort": "Bool"},
            {"name": "more", "sort": "Bool"},
            {"name": "done", "sort": "Bool"}
        ],
        "outputs": ["count"],
        "effects_var": "effects",
        "effects": [
            {"guard": "more", "variant": "Println", "args": [{"lit_str": "tick"}]},
            {"guard": "done", "variant": "Println", "args": [{"lit_str": "liftoff"}]},
            {"guard": "done", "variant": "Exit", "args": [{"lit_int": 0}]}
        ]
    }"#;
    let meta = parse_meta(&serde_json::from_str(json).unwrap()).unwrap();
    let smtlib = "\
(declare-const count Int)
(declare-const _count Int)
(declare-const is_first_tick Bool)
(declare-const more Bool)
(declare-const done Bool)
(assert (= count (ite is_first_tick 3 (- _count 1))))
(assert (= more (> count 0)))
(assert (= done (<= count 0)))
"
    .to_string();
    SmtLibFsm { meta, smtlib }
}

#[test]
fn parses_meta_and_effects() {
    let fsm = countdown_fsm();
    assert_eq!(fsm.meta.fsm, "countdown");
    assert_eq!(fsm.meta.vars.len(), 5);
    assert_eq!(fsm.meta.outputs, vec!["count".to_string()]);
    assert_eq!(fsm.meta.effects_var.as_deref(), Some("effects"));
    assert_eq!(fsm.meta.effects.len(), 3);
    assert_eq!(fsm.meta.effects[0].guard.as_deref(), Some("more"));
    assert_eq!(fsm.meta.effects[0].variant, "Println");
}

#[test]
fn synthetic_schema_has_fsm_shape() {
    let fsm = countdown_fsm();
    let schema = fsm.synthetic_schema();
    assert_eq!(schema.keyword, Keyword::Fsm);
    assert_eq!(schema.name, "countdown");
    // `is_first_tick` is engine-provided and intentionally not a Membership.
    let mem_names: Vec<&str> = schema
        .body
        .iter()
        .filter_map(|b| match b {
            BodyItem::Membership { name, .. } => Some(name.as_str()),
            _ => None,
        })
        .collect();
    assert!(mem_names.contains(&"count"));
    assert!(mem_names.contains(&"_count"), "needed for the _var time-shift scan");
    assert!(mem_names.contains(&"effects"), "Seq(Effect) slot drives effects_var");
    assert!(!mem_names.contains(&"is_first_tick"));
    // The `effects` membership must be typed Seq(Effect) so resolve_fsm picks it.
    assert!(schema.body.iter().any(|b| matches!(
        b, BodyItem::Membership { name, type_name, .. }
        if name == "effects" && type_name == "Seq(Effect)"
    )));
}

#[test]
fn tick_zero_starts_at_three_and_prints_tick() {
    let rt = EvidentRuntime::new();
    let fsm = countdown_fsm();
    // First tick: is_first_tick = true, _count absent.
    let mut given: HashMap<String, Value> = HashMap::new();
    given.insert("is_first_tick".to_string(), Value::Bool(true));
    let r = solve_tick(&rt, &fsm, &[], &given);
    assert!(r.satisfied);
    assert_eq!(r.bindings.get("count"), Some(&Value::Int(3)));
    let effects = r.bindings.get("effects").expect("effects bound");
    assert_eq!(
        effects,
        &Value::SeqEnum(vec![Value::Enum {
            enum_name: "Effect".to_string(),
            variant: "Println".to_string(),
            fields: vec![Value::Str("tick".to_string())],
        }])
    );
}

#[test]
fn ongoing_tick_decrements() {
    let rt = EvidentRuntime::new();
    let fsm = countdown_fsm();
    let mut given: HashMap<String, Value> = HashMap::new();
    given.insert("is_first_tick".to_string(), Value::Bool(false));
    given.insert("_count".to_string(), Value::Int(3));
    let r = solve_tick(&rt, &fsm, &[], &given);
    assert!(r.satisfied);
    assert_eq!(r.bindings.get("count"), Some(&Value::Int(2)));
}

#[test]
fn terminal_tick_prints_liftoff_and_exits() {
    let rt = EvidentRuntime::new();
    let fsm = countdown_fsm();
    let mut given: HashMap<String, Value> = HashMap::new();
    given.insert("is_first_tick".to_string(), Value::Bool(false));
    given.insert("_count".to_string(), Value::Int(1)); // count -> 0
    let r = solve_tick(&rt, &fsm, &[], &given);
    assert!(r.satisfied);
    assert_eq!(r.bindings.get("count"), Some(&Value::Int(0)));
    let effects = r.bindings.get("effects").unwrap();
    assert_eq!(
        effects,
        &Value::SeqEnum(vec![
            Value::Enum {
                enum_name: "Effect".to_string(),
                variant: "Println".to_string(),
                fields: vec![Value::Str("liftoff".to_string())],
            },
            Value::Enum {
                enum_name: "Effect".to_string(),
                variant: "Exit".to_string(),
                fields: vec![Value::Int(0)],
            },
        ])
    );
}

#[test]
fn var_arg_pulls_model_value() {
    // Effect arg `{var: count}` formats the live count into an IntToStr effect.
    let json = r#"{
        "fsm": "fmt",
        "vars": [{"name": "count", "sort": "Int"}, {"name": "is_first_tick", "sort": "Bool"}],
        "outputs": ["count"],
        "effects_var": "effects",
        "effects": [{"variant": "IntToStr", "args": [{"var": "count"}]}]
    }"#;
    let meta = parse_meta(&serde_json::from_str(json).unwrap()).unwrap();
    let smtlib = "(declare-const count Int)\n(declare-const is_first_tick Bool)\n\
                  (assert (= count 7))\n"
        .to_string();
    let fsm = SmtLibFsm { meta, smtlib };
    let rt = EvidentRuntime::new();
    let r = solve_tick(&rt, &fsm, &[], &HashMap::new());
    assert!(r.satisfied);
    assert_eq!(
        r.bindings.get("effects"),
        Some(&Value::SeqEnum(vec![Value::Enum {
            enum_name: "Effect".to_string(),
            variant: "IntToStr".to_string(),
            fields: vec![Value::Int(7)],
        }]))
    );
}

#[test]
fn malformed_smtlib_is_unsat_not_panic() {
    let meta = parse_meta(&serde_json::from_str(r#"{"fsm":"bad","vars":[]}"#).unwrap()).unwrap();
    let fsm = SmtLibFsm { meta, smtlib: "(this is not smtlib))".to_string() };
    let rt = EvidentRuntime::new();
    let r = solve_tick(&rt, &fsm, &[], &HashMap::new());
    assert!(!r.satisfied);
}

#[test]
fn register_injects_synthetic_schema() {
    let mut rt = EvidentRuntime::new();
    rt.register_smtlib_fsm(countdown_fsm());
    // resolve_fsm sees the synthetic fsm-keyword schema.
    let shape = crate::effect_loop::resolve_fsm(&rt, "countdown")
        .expect("countdown resolves as an fsm");
    assert_eq!(shape.claim_name, "countdown");
    assert_eq!(shape.effects_var.as_deref(), Some("effects"));
    // Scalar FSM: no enum state_var.
    assert_eq!(shape.state_var, None);
}
