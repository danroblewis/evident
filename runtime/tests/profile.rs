//! Integration tests for the analysis tooling:
//!   * `given_vars` / `solved_for_vars` (cheap, AST-only)
//!   * `bottleneck_vars` (one solve per candidate)
//!
//! Uses small synthetic claims so the suite stays fast — the heavy
//! end-to-end check on Mario's `display` lives in the CLI conformance
//! tests.

use evident_runtime::EvidentRuntime;
use std::collections::HashMap;

fn rt_with(src: &str) -> EvidentRuntime {
    let mut rt = EvidentRuntime::new();
    rt.load_source(src).expect("load");
    rt
}

const ADDER: &str = r#"claim Adder(x ∈ Int, y ∈ Int)
    sum ∈ Int
    diff ∈ Int
    sum = x + y
    diff = x - y
"#;

#[test]
fn given_vars_are_the_claim_line_params() {
    let rt = rt_with(ADDER);
    let given = rt.given_vars("Adder").unwrap();
    assert_eq!(given, vec!["x".to_string(), "y".to_string()],
        "claim-line params should be the given vars");
}

#[test]
fn solved_for_excludes_params_and_given_keys() {
    let rt = rt_with(ADDER);

    // No given: outputs are the two body memberships.
    let solved = rt.solved_for_vars("Adder", &[]).unwrap();
    assert_eq!(solved, vec!["sum".to_string(), "diff".to_string()]);

    // Pinning `sum` removes it from the solved-for set.
    let solved = rt.solved_for_vars("Adder", &["sum".to_string()]).unwrap();
    assert_eq!(solved, vec!["diff".to_string()]);

    // Params are never solved-for even if named in given_keys.
    let solved = rt.solved_for_vars("Adder", &["x".to_string()]).unwrap();
    assert_eq!(solved, vec!["sum".to_string(), "diff".to_string()]);
}

#[test]
fn solved_for_flattens_passthrough() {
    let src = r#"claim Base
    p ∈ Int
    q ∈ Int

claim Composed(x ∈ Int)
    ..Base
    r ∈ Int
    r = x + p + q
"#;
    let rt = rt_with(src);
    let given = rt.given_vars("Composed").unwrap();
    assert_eq!(given, vec!["x".to_string()]);

    let solved = rt.solved_for_vars("Composed", &[]).unwrap();
    // p, q come from the `..Base` mixin; r is local. x is a param.
    assert!(solved.contains(&"p".to_string()), "got {solved:?}");
    assert!(solved.contains(&"q".to_string()), "got {solved:?}");
    assert!(solved.contains(&"r".to_string()), "got {solved:?}");
    assert!(!solved.contains(&"x".to_string()), "param must not be solved-for: {solved:?}");
}

#[test]
fn unknown_schema_is_an_error() {
    let rt = rt_with(ADDER);
    assert!(rt.given_vars("Nope").is_err());
    assert!(rt.solved_for_vars("Nope", &[]).is_err());
    assert!(rt.bottleneck_vars("Nope", &HashMap::new(), 5).is_err());
}

#[test]
fn bottleneck_ranks_candidate_leaves() {
    let rt = rt_with(ADDER);
    let entries = rt.bottleneck_vars("Adder", &HashMap::new(), 10).unwrap();

    // Candidates are the solved-for scalar leaves: sum and diff.
    assert!(!entries.is_empty(), "expected at least one candidate");
    let names: Vec<&str> = entries.iter().map(|e| e.var_name.as_str()).collect();
    for n in &names {
        assert!(*n == "sum" || *n == "diff",
            "candidate {n:?} should be a solved-for leaf (sum/diff)");
    }
    // No param leaf (x/y) is ever a candidate.
    assert!(!names.contains(&"x") && !names.contains(&"y"),
        "params must not be candidates: {names:?}");

    // Every entry's arithmetic is internally consistent.
    for e in &entries {
        assert_eq!(e.savings_us, e.baseline_solve_us as i128 - e.pinned_solve_us as i128,
            "savings must equal baseline - pinned for {:?}", e.var_name);
    }
}

#[test]
fn bottleneck_respects_top_n() {
    let rt = rt_with(ADDER);
    let entries = rt.bottleneck_vars("Adder", &HashMap::new(), 1).unwrap();
    assert!(entries.len() <= 1, "top_n=1 should cap the ranking");
}

#[test]
fn bottleneck_errors_on_unsat_baseline() {
    // y can't equal both x+1 and x+2 → UNSAT for any x.
    let src = r#"claim Contradiction(x ∈ Int)
    y ∈ Int
    y = x + 1
    y = x + 2
"#;
    let rt = rt_with(src);
    let err = rt.bottleneck_vars("Contradiction", &HashMap::new(), 5);
    assert!(err.is_err(), "UNSAT baseline must produce an error, got {err:?}");
}

#[test]
fn bottleneck_pins_given_and_excludes_it() {
    // With `y` given, the solved-for set is still {sum, diff}; the
    // given value participates in the baseline solve.
    let rt = rt_with(ADDER);
    let mut given = HashMap::new();
    given.insert("y".to_string(), evident_runtime::Value::Int(3));
    let entries = rt.bottleneck_vars("Adder", &given, 10).unwrap();
    let names: Vec<&str> = entries.iter().map(|e| e.var_name.as_str()).collect();
    assert!(!names.contains(&"y"), "given var must not be a candidate: {names:?}");
}
