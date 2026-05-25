//! Gap B (#18): a value bound by destructuring an enum payload in a
//! `match` must read its REAL value when computed on (compared with
//! `=`, used in boolean ops), not just when embedded into a new
//! constructed node.
//!
//! These repros pin the scrutinee enum via `given` (the AST-inspector
//! shape that forced `validate.ev`'s stub) and assert the
//! comparison/boolean-op result is correct.

use std::collections::HashMap;
use evident_runtime::{EvidentRuntime, Value};

fn ecall(name: &str) -> Value {
    Value::Enum {
        enum_name: "Expr".into(),
        variant: "ECall".into(),
        fields: vec![Value::Str(name.into()), Value::Int(0)],
    }
}

const SRC: &str = "\
enum Expr = ECall(String, Int) | EOther

claim ValidateExpr
    e ∈ Expr
    out ∈ String
    out = match e
        ECall(nm, _) ⇒ (nm = \"FFICall\" ? nm : \"\")
        _            ⇒ \"\"
";

/// The keystone: destructured String payload `nm` compared `= "FFICall"`
/// must be TRUE when the bytes match.
#[test]
fn destructured_string_eq_is_true_when_matching() {
    let mut rt = EvidentRuntime::new();
    rt.load_source(SRC).unwrap();
    let given = HashMap::from([("e".to_string(), ecall("FFICall"))]);
    let r = rt.query("ValidateExpr", &given).unwrap();
    assert!(r.satisfied);
    assert_eq!(r.bindings.get("out"), Some(&Value::Str("FFICall".into())),
        "nm = \"FFICall\" should be TRUE for a given-pinned ECall(\"FFICall\", _)");
}

/// The negative case: a different name compares false → out = "".
#[test]
fn destructured_string_eq_is_false_when_not_matching() {
    let mut rt = EvidentRuntime::new();
    rt.load_source(SRC).unwrap();
    let given = HashMap::from([("e".to_string(), ecall("Println"))]);
    let r = rt.query("ValidateExpr", &given).unwrap();
    assert!(r.satisfied);
    assert_eq!(r.bindings.get("out"), Some(&Value::Str("".into())));
}

/// The bare round-trip (control): returning `nm` directly already works
/// per #18; this guards against a regression in that direction.
#[test]
fn destructured_string_bare_round_trips() {
    let mut rt = EvidentRuntime::new();
    rt.load_source("\
enum Expr = ECall(String, Int) | EOther

claim Bare
    e ∈ Expr
    out ∈ String
    out = match e
        ECall(nm, _) ⇒ nm
        _            ⇒ \"\"
").unwrap();
    let given = HashMap::from([("e".to_string(), ecall("FFICall"))]);
    let r = rt.query("Bare", &given).unwrap();
    assert!(r.satisfied);
    assert_eq!(r.bindings.get("out"), Some(&Value::Str("FFICall".into())));
}

/// Second shape from the gap: a destructured Bool payload used in a
/// boolean computation `(b ∧ ¬other)` must read its real value.
#[test]
fn destructured_bool_in_boolean_op() {
    let mut rt = EvidentRuntime::new();
    rt.load_source("\
enum Decision = Decide(Bool, Bool) | Skip

claim DecideExpr
    d ∈ Decision
    out ∈ Bool
    out = match d
        Decide(rsn, hsn) ⇒ (rsn ∧ ¬hsn)
        _                ⇒ false
").unwrap();
    let given = HashMap::from([(
        "d".to_string(),
        Value::Enum {
            enum_name: "Decision".into(),
            variant: "Decide".into(),
            fields: vec![Value::Bool(true), Value::Bool(false)],
        },
    )]);
    let r = rt.query("DecideExpr", &given).unwrap();
    assert!(r.satisfied);
    assert_eq!(r.bindings.get("out"), Some(&Value::Bool(true)),
        "(true ∧ ¬false) should be TRUE");
}

/// #17's exact shape: a destructured Bool payload used as an ITE
/// condition. `match e { EBool(b) ⇒ (b ? "true" : "false") }` returned
/// "false" for both values under the JIT (Bool accessor read via
/// load_int → 0 → always-else). Same root cause as the boolean-op case.
#[test]
fn destructured_bool_as_ite_condition() {
    let src = "\
enum E = EBool(Bool) | EOther

claim Render
    e ∈ E
    out ∈ String
    out = match e
        EBool(b) ⇒ (b ? \"true\" : \"false\")
        _        ⇒ \"?\"
";
    let mk = |b: bool| std::collections::HashMap::from([(
        "e".to_string(),
        Value::Enum { enum_name: "E".into(), variant: "EBool".into(), fields: vec![Value::Bool(b)] },
    )]);

    let mut rt = EvidentRuntime::new();
    rt.load_source(src).unwrap();
    let r = rt.query("Render", &mk(true)).unwrap();
    assert_eq!(r.bindings.get("out"), Some(&Value::Str("true".into())),
        "EBool(true) ⇒ (b ? .. : ..) must pick the `b` branch");

    let mut rt2 = EvidentRuntime::new();
    rt2.load_source(src).unwrap();
    let r2 = rt2.query("Render", &mk(false)).unwrap();
    assert_eq!(r2.bindings.get("out"), Some(&Value::Str("false".into())));
}

/// Destructured Int payload compared `> 0` (the #6 restatement).
#[test]
fn destructured_int_compared_gt() {
    let mut rt = EvidentRuntime::new();
    rt.load_source("\
enum Result = IntResult(Int) | NoResult

claim Positive
    r ∈ Result
    out ∈ Bool
    out = match r
        IntResult(n) ⇒ (n > 0)
        _            ⇒ false
").unwrap();
    let given = HashMap::from([(
        "r".to_string(),
        Value::Enum {
            enum_name: "Result".into(),
            variant: "IntResult".into(),
            fields: vec![Value::Int(42)],
        },
    )]);
    let r = rt.query("Positive", &given).unwrap();
    assert!(r.satisfied);
    assert_eq!(r.bindings.get("out"), Some(&Value::Bool(true)));
}
