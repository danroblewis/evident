//! Tests for the blocking-clause `sample` loop. Exercises the
//! `EvidentRuntime::sample` API.

use std::collections::HashSet;

use evident_runtime::{EvidentRuntime, Value};

/// `n ∈ Int ; n > 0 ; n < 6` has exactly five satisfying ints
/// (1..=5). Sampling with -n 5 should return all five distinct.
#[test]
fn sample_returns_distinct_int_models() {
    let mut rt = EvidentRuntime::new();
    rt.load_source("schema S\n    n ∈ Int\n    n > 0\n    n < 6\n").unwrap();
    let samples = rt.sample("S", &Default::default(), 5).unwrap();
    assert_eq!(samples.len(), 5, "expected 5 samples, got {}: {:?}", samples.len(), samples);
    let values: HashSet<i64> = samples.iter()
        .map(|s| match s.get("n") { Some(Value::Int(n)) => *n, _ => panic!("missing n in {:?}", s) })
        .collect();
    assert_eq!(values, HashSet::from([1, 2, 3, 4, 5]),
               "expected {{1..5}}, got {:?}", values);
}

/// Same schema with -n 100: blocking clauses exhaust all 5 solutions,
/// then the next check returns UNSAT and the loop terminates. The
/// function MUST NOT loop forever or return more than 5 distinct models.
#[test]
fn sample_stops_at_unsat() {
    let mut rt = EvidentRuntime::new();
    rt.load_source("schema S\n    n ∈ Int\n    n > 0\n    n < 6\n").unwrap();
    let samples = rt.sample("S", &Default::default(), 100).unwrap();
    assert_eq!(samples.len(), 5, "expected exactly 5 samples (5-solution schema), got {}",
               samples.len());
    // No duplicates.
    let values: HashSet<i64> = samples.iter()
        .map(|s| if let Some(Value::Int(n)) = s.get("n") { *n } else { unreachable!() })
        .collect();
    assert_eq!(values.len(), 5);
}

/// Pinning one variable via `given` should not block the loop from
/// returning multiple distinct models for the *un*pinned variable.
#[test]
fn sample_with_given_partially_pinned() {
    let mut rt = EvidentRuntime::new();
    // a ∈ {1,2}, b ∈ {1,2,3,4}: pinning a leaves 4 distinct b values.
    rt.load_source(
        "schema Pair\n    a ∈ Int\n    b ∈ Int\n    a > 0\n    a < 3\n    b > 0\n    b < 5\n").unwrap();
    let mut given = std::collections::HashMap::new();
    given.insert("a".to_string(), Value::Int(2));
    let samples = rt.sample("Pair", &given, 10).unwrap();
    // Should get exactly 4 models (b = 1, 2, 3, 4) all with a = 2.
    assert_eq!(samples.len(), 4, "got {}: {:?}", samples.len(), samples);
    for s in &samples {
        assert_eq!(s.get("a"), Some(&Value::Int(2)),
                   "every sample should have a = 2 (pinned), got {:?}", s);
    }
    let bs: HashSet<i64> = samples.iter()
        .map(|s| if let Some(Value::Int(n)) = s.get("b") { *n } else { unreachable!() })
        .collect();
    assert_eq!(bs, HashSet::from([1, 2, 3, 4]));
}
