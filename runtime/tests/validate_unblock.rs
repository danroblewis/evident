//! Gap B "prove the unblock" (#18): the construct `validate.ev` was
//! forced to stub — pin `e ∈ Expr` (the REAL stdlib/ast.ev Expr, whose
//! `ECall` is `ECall(String, Seq(Expr))`), destructure `ECall(nm, _)`,
//! and decide on `nm = "LibCall"` — now returns the right answer
//! through the `given` ⇄ match-extraction path.
//!
//! This is the shape `stdlib/passes/validate.ev` had to side-step by
//! pinning `nm ∈ String` on the Rust side. With Gap B closed, a later
//! session can pin `e ∈ Expr` directly. We don't rewrite the pass
//! here — just demonstrate the gap is closed.

use std::collections::HashMap;
use std::path::Path;
use evident_runtime::{EvidentRuntime, Value};

const STDLIB_AST: &str = "../stdlib/ast.ev";

/// Build `ECall(name, ⟨⟩)` as a `given`-injectable Value over the real
/// stdlib/ast.ev shape: the second field is the empty `Seq(Expr)`,
/// represented by the runtime's internal Cons helper `__SeqOf_Expr`'s
/// nullary `__Empty_Expr` terminator.
fn ecall_value(name: &str) -> Value {
    Value::Enum {
        enum_name: "Expr".into(),
        variant: "ECall".into(),
        fields: vec![
            Value::Str(name.into()),
            Value::Enum {
                enum_name: "__SeqOf_Expr".into(),
                variant: "__Empty_Expr".into(),
                fields: vec![],
            },
        ],
    }
}

fn rt_with_validate() -> EvidentRuntime {
    let mut rt = EvidentRuntime::new();
    rt.load_file(Path::new(STDLIB_AST)).expect("load stdlib/ast.ev");
    // The exact decision the canonical validate walker needs: classify
    // an ECall's name against the banned FFI-construction primitives.
    rt.load_source("\
claim ValidateExpr
    e ∈ Expr
    out ∈ String
    out = match e
        ECall(nm, _) ⇒ ((nm = \"FFICall\" ∨ nm = \"FFIOpen\" ∨ nm = \"FFILookup\" ∨ nm = \"LibCall\") ? nm : \"\")
        _            ⇒ \"\"
").unwrap();
    rt
}

#[test]
fn validate_expr_flags_banned_call_through_given() {
    let rt = rt_with_validate();
    let given = HashMap::from([("e".to_string(), ecall_value("LibCall"))]);
    let r = rt.query("ValidateExpr", &given).unwrap();
    assert!(r.satisfied);
    assert_eq!(r.bindings.get("out"), Some(&Value::Str("LibCall".into())),
        "ECall(\"LibCall\", _) must be flagged — this is the decision \
         validate.ev had to stub out (#18)");
}

#[test]
fn validate_expr_passes_benign_call_through_given() {
    let rt = rt_with_validate();
    let given = HashMap::from([("e".to_string(), ecall_value("Println"))]);
    let r = rt.query("ValidateExpr", &given).unwrap();
    assert!(r.satisfied);
    assert_eq!(r.bindings.get("out"), Some(&Value::Str("".into())),
        "a benign call name must NOT be flagged");
}
