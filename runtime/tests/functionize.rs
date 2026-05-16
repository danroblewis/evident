//! Integration test: extract a substitution chain from a real claim
//! and evaluate it natively. Confirms the function-izer pipeline
//! works end-to-end against an actual `.ev` source file (not just
//! hand-rolled AST).

use evident_runtime::functionize::{evaluate_chain, extract_chain, SubstitutionChain};
use evident_runtime::{EvidentRuntime, Value};
use std::collections::HashMap;
use std::path::PathBuf;

fn tmp(tag: &str) -> PathBuf {
    std::env::temp_dir().join(format!("functionize_int_{}_{}.ev", std::process::id(), tag))
}

#[test]
fn pair_native_evaluation_matches_z3() {
    let src = r#"claim Pair
    a ∈ Int
    b ∈ Int
    sum ∈ Int
    prod ∈ Int
    sum = a + b
    prod = a * b
"#;
    let path = tmp("pair");
    std::fs::write(&path, src).unwrap();
    let mut rt = EvidentRuntime::new();
    rt.load_file(&path).unwrap();

    let mut given = HashMap::new();
    given.insert("a".to_string(), Value::Int(7));
    given.insert("b".to_string(), Value::Int(4));

    // Z3 path
    let z3_r = rt.query("Pair", &given).unwrap();
    assert!(z3_r.satisfied);

    // Native path
    let comps = rt.classify_components("Pair", &given).unwrap();
    let schema = rt.get_schema("Pair").unwrap().clone();
    let mut steps = Vec::new();
    for c in comps.iter().filter(|c| c.functional) {
        if let Some(ch) = extract_chain(&schema, &c.component) {
            steps.extend(ch.steps);
        }
    }
    assert!(!steps.is_empty(), "expected functional components for Pair");
    let chain = SubstitutionChain { steps };
    let native = evaluate_chain(&chain, &given).unwrap();
    let _ = std::fs::remove_file(&path);

    // Outputs match.
    assert_eq!(native.get("sum"),  z3_r.bindings.get("sum"));
    assert_eq!(native.get("prod"), z3_r.bindings.get("prod"));
    assert_eq!(native.get("sum"),  Some(&Value::Int(11)));
    assert_eq!(native.get("prod"), Some(&Value::Int(28)));
}
