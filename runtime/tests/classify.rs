//! Integration tests for the 2-copy uniqueness check (component
//! functionality classification). The smoking gun: a claim that is
//! non-functional under empty `given` should become functional once
//! its inputs are pinned.

use evident_runtime::{EvidentRuntime, Value};
use std::collections::HashMap;
use std::path::PathBuf;

fn tmp_path(tag: &str) -> PathBuf {
    std::env::temp_dir().join(
        format!("classify_integration_{}_{}.ev", std::process::id(), tag))
}

#[test]
fn pair_becomes_functional_when_inputs_pinned() {
    // Pair(a, b) → (sum, prod). Without pinning a and b, there are
    // infinitely many models. With them pinned, sum and prod are
    // uniquely determined.
    let source = r#"claim Pair
    a ∈ Int
    b ∈ Int
    sum ∈ Int
    prod ∈ Int
    sum = a + b
    prod = a * b
"#;
    let path = tmp_path("pair");
    std::fs::write(&path, source).unwrap();

    let mut rt = EvidentRuntime::new();
    rt.load_file(&path).unwrap();

    // Empty given: nothing pinned. Z3 picks arbitrary a, b — sum
    // and prod follow but can still take many different values (any
    // valid (a, b, sum=a+b, prod=a*b) tuple satisfies the body).
    // So components SHOULD be non-functional.
    let no_given = rt.classify_components("Pair", &HashMap::new()).unwrap();
    let any_functional_multi = no_given.iter()
        .any(|c| c.functional && c.component.vars.len() > 1);
    // Note: depending on how decomposition merges, sum + prod might
    // be in one or two components. In either case, none should be
    // "functional" without inputs.
    assert!(!any_functional_multi,
        "expected no functional multi-var components without given; got {:?}",
        no_given);

    // Pin a and b. Now sum = 8 and prod = 15 are unique answers.
    let mut given = HashMap::new();
    given.insert("a".to_string(), Value::Int(5));
    given.insert("b".to_string(), Value::Int(3));
    let with_given = rt.classify_components("Pair", &given).unwrap();
    let _ = std::fs::remove_file(&path);

    // With inputs pinned, every component should be functional —
    // sum and prod are uniquely determined by a and b.
    let non_functional: Vec<&_> = with_given.iter()
        .filter(|c| !c.functional)
        .collect();
    assert!(non_functional.is_empty(),
        "expected all components functional with given; non-functional: {:?}",
        non_functional);
    // And we should have actual components (not just emptied).
    assert!(!with_given.is_empty(),
        "expected non-empty component list");
}

#[test]
fn coin_flip_is_non_functional() {
    // A claim where the output is genuinely free even with inputs:
    // result is a Bool not constrained to anything specific. Multiple
    // valid models exist. Should classify non-functional.
    let source = r#"claim CoinFlip
    seed ∈ Int
    result ∈ Bool
"#;
    let path = tmp_path("coin");
    std::fs::write(&path, source).unwrap();
    let mut rt = EvidentRuntime::new();
    rt.load_file(&path).unwrap();

    let mut given = HashMap::new();
    given.insert("seed".to_string(), Value::Int(42));
    let comps = rt.classify_components("CoinFlip", &given).unwrap();
    let _ = std::fs::remove_file(&path);

    // `result` is unconstrained — both true and false are valid. The
    // component containing it should be classified non-functional.
    let result_comp = comps.iter()
        .find(|c| c.component.vars.contains(&"result".to_string()))
        .expect("result var should be in some component");
    assert!(!result_comp.functional,
        "expected `result` component non-functional (true or false both valid); got functional={}",
        result_comp.functional);
}
