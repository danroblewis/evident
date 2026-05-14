//! Integration test for the generic `stdlib/toposort.ev` and the
//! runtime reflection seam. Load the stdlib claim, invoke
//! `Toposort<Int>` via `rt.query` with `given` pins, decode the
//! sorted Seq.
//!
//! Demonstrates the general "runtime calls a generic stdlib claim"
//! path — same plumbing reused for any future generic operation.

use evident_runtime::{EvidentRuntime, Value};
use std::collections::HashMap;
use std::path::Path;

/// Build a `Seq(Edge<Int>)` given value from `(from, to)` pairs.
fn edges_given(pairs: &[(i64, i64)]) -> Value {
    Value::SeqComposite(pairs.iter().map(|(f, t)| {
        let mut m = HashMap::new();
        m.insert("from".to_string(), Value::Int(*f));
        m.insert("to".to_string(),   Value::Int(*t));
        m
    }).collect())
}

fn run_toposort(
    items: Vec<i64>,
    edges: &[(i64, i64)],
) -> Result<Vec<i64>, String> {
    let mut rt = EvidentRuntime::new();
    rt.load_file(Path::new("../stdlib/toposort.ev")).unwrap();
    let n = items.len() as i64;
    let mut given: HashMap<String, Value> = HashMap::new();
    given.insert("n".into(),     Value::Int(n));
    given.insert("items".into(), Value::SeqInt(items));
    given.insert("edges".into(), edges_given(edges));
    let r = rt.query("Toposort<Int>", &given)
        .map_err(|e| format!("{e}"))?;
    if !r.satisfied { return Err("UNSAT".into()); }
    match r.bindings.get("sorted") {
        Some(Value::SeqInt(v)) => Ok(v.clone()),
        other => Err(format!("expected sorted as SeqInt, got {:?}", other)),
    }
}

#[test]
fn toposort_two_edges_4_nodes() {
    let sorted = run_toposort(vec![10, 20, 30, 40], &[(10, 30), (20, 40)]).unwrap();
    // The result is a permutation of [10,20,30,40] where 10 comes
    // before 30 and 20 comes before 40.
    let mut s = sorted.clone(); s.sort();
    assert_eq!(s, vec![10, 20, 30, 40], "expected a permutation of items");
    let pos_of = |x: i64| sorted.iter().position(|&v| v == x).unwrap();
    assert!(pos_of(10) < pos_of(30), "10 must precede 30: {:?}", sorted);
    assert!(pos_of(20) < pos_of(40), "20 must precede 40: {:?}", sorted);
}

#[test]
fn toposort_linear_chain_is_unique() {
    let sorted = run_toposort(vec![100, 200, 300], &[(100, 200), (200, 300)]).unwrap();
    assert_eq!(sorted, vec![100, 200, 300]);
}

#[test]
fn toposort_cycle_is_unsat() {
    let err = run_toposort(vec![1, 2, 3], &[(1, 2), (2, 3), (3, 1)]);
    assert!(matches!(err, Err(ref e) if e.contains("UNSAT")),
        "expected UNSAT, got {:?}", err);
}

#[test]
fn toposort_empty_edges() {
    let sorted = run_toposort(vec![1, 2, 3, 4, 5], &[]).unwrap();
    let mut s = sorted.clone(); s.sort();
    assert_eq!(s, vec![1, 2, 3, 4, 5]);
}

#[test]
fn toposort_eight_node_dag() {
    // Nodes by Int value (must be distinct so Z3 equality identifies vertices).
    let items: Vec<i64> = (10..18).collect();   // 10..17
    // Edges: 10→11, 11→13, 13→16, 12→14, 14→17, 13→15
    let edges: &[(i64, i64)] = &[
        (10, 11), (11, 13), (13, 16),
        (12, 14), (14, 17), (13, 15),
    ];
    let sorted = run_toposort(items.clone(), edges).unwrap();
    let mut s = sorted.clone(); s.sort();
    assert_eq!(s, items);
    let pos_of = |x: i64| sorted.iter().position(|&v| v == x).unwrap();
    for &(s_node, d_node) in edges {
        assert!(pos_of(s_node) < pos_of(d_node),
            "edge {s_node}→{d_node} violated: sorted={:?}", sorted);
    }
}
