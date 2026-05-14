//! Integration test for `stdlib/toposort.ev` and the runtime
//! reflection seam: load the stdlib claim, invoke it via
//! `rt.query` from Rust with `given` pins, decode the result.
//!
//! Demonstrates the general "runtime calls a stdlib claim" path
//! — the same plumbing future features (effect-ordering, GLSL
//! transpile, codegen, …) would reuse.

use evident_runtime::{EvidentRuntime, Value};
use std::collections::HashMap;
use std::path::Path;

/// Sort 4 nodes with two independent edges (0 → 2, 1 → 3) — any
/// valid order works. Asserts the result is a permutation and
/// every edge is respected.
#[test]
fn toposort_two_edges_4_nodes() {
    let mut rt = EvidentRuntime::new();
    rt.load_file(Path::new("../stdlib/toposort.ev")).unwrap();

    let mut given: HashMap<String, Value> = HashMap::new();
    given.insert("n".into(),     Value::Int(4));
    given.insert("_edge".into(), Value::SeqInt(vec![0, 1]));
    given.insert("edge".into(),  Value::SeqInt(vec![2, 3]));

    let r = rt.query("Toposort", &given).expect("query failed");
    assert!(r.satisfied, "expected SAT, got UNSAT");

    let pos = match r.bindings.get("position") {
        Some(Value::SeqInt(v)) => v.clone(),
        other => panic!("expected position as SeqInt, got {:?}", other),
    };
    assert_eq!(pos.len(), 4, "position length must be n");
    // Permutation of {0..3}.
    let mut sorted = pos.clone(); sorted.sort();
    assert_eq!(sorted, vec![0, 1, 2, 3], "positions must be a permutation");
    // Edges respected: pos[0] < pos[2] and pos[1] < pos[3].
    assert!(pos[0] < pos[2], "edge 0→2 violated: {:?}", pos);
    assert!(pos[1] < pos[3], "edge 1→3 violated: {:?}", pos);
}

/// Linear chain 0 → 1 → 2 — the only valid order is identity.
#[test]
fn toposort_linear_chain_is_unique() {
    let mut rt = EvidentRuntime::new();
    rt.load_file(Path::new("../stdlib/toposort.ev")).unwrap();

    let mut given: HashMap<String, Value> = HashMap::new();
    given.insert("n".into(),     Value::Int(3));
    given.insert("_edge".into(), Value::SeqInt(vec![0, 1]));
    given.insert("edge".into(),  Value::SeqInt(vec![1, 2]));

    let r = rt.query("Toposort", &given).unwrap();
    assert!(r.satisfied);
    let pos = match r.bindings.get("position") {
        Some(Value::SeqInt(v)) => v.clone(),
        other => panic!("expected position as SeqInt, got {:?}", other),
    };
    assert_eq!(pos, vec![0, 1, 2]);
}

/// 3-cycle 0 → 1 → 2 → 0 is UNSAT.
#[test]
fn toposort_cycle_is_unsat() {
    let mut rt = EvidentRuntime::new();
    rt.load_file(Path::new("../stdlib/toposort.ev")).unwrap();

    let mut given: HashMap<String, Value> = HashMap::new();
    given.insert("n".into(),     Value::Int(3));
    given.insert("_edge".into(), Value::SeqInt(vec![0, 1, 2]));
    given.insert("edge".into(),  Value::SeqInt(vec![1, 2, 0]));

    let r = rt.query("Toposort", &given).unwrap();
    assert!(!r.satisfied, "cycle should be UNSAT");
}

/// No edges, 5 nodes — any permutation valid.
#[test]
fn toposort_empty_edges() {
    let mut rt = EvidentRuntime::new();
    rt.load_file(Path::new("../stdlib/toposort.ev")).unwrap();

    let mut given: HashMap<String, Value> = HashMap::new();
    given.insert("n".into(),     Value::Int(5));
    given.insert("_edge".into(), Value::SeqInt(vec![]));
    given.insert("edge".into(),  Value::SeqInt(vec![]));

    let r = rt.query("Toposort", &given).unwrap();
    assert!(r.satisfied);
    let pos = match r.bindings.get("position") {
        Some(Value::SeqInt(v)) => v.clone(),
        other => panic!("expected position as SeqInt, got {:?}", other),
    };
    assert_eq!(pos.len(), 5);
    let mut sorted = pos.clone(); sorted.sort();
    assert_eq!(sorted, vec![0, 1, 2, 3, 4]);
}

/// Larger DAG: 8 nodes, several edges. Verifies the constraint
/// model scales beyond the toy cases.
#[test]
fn toposort_eight_node_dag() {
    let mut rt = EvidentRuntime::new();
    rt.load_file(Path::new("../stdlib/toposort.ev")).unwrap();

    //   0 → 1 → 3 → 6
    //       2 → 4 → 7
    //   3 → 5
    let edges = [(0, 1), (1, 3), (3, 6), (2, 4), (4, 7), (3, 5)];
    let src: Vec<i64> = edges.iter().map(|(s, _)| *s as i64).collect();
    let dst: Vec<i64> = edges.iter().map(|(_, d)| *d as i64).collect();

    let mut given: HashMap<String, Value> = HashMap::new();
    given.insert("n".into(),     Value::Int(8));
    given.insert("_edge".into(), Value::SeqInt(src));
    given.insert("edge".into(),  Value::SeqInt(dst));

    let r = rt.query("Toposort", &given).unwrap();
    assert!(r.satisfied);
    let pos = match r.bindings.get("position") {
        Some(Value::SeqInt(v)) => v.clone(),
        other => panic!("expected position as SeqInt, got {:?}", other),
    };
    assert_eq!(pos.len(), 8);
    let mut sorted = pos.clone(); sorted.sort();
    assert_eq!(sorted, vec![0, 1, 2, 3, 4, 5, 6, 7]);
    for (s, d) in edges {
        assert!(pos[s] < pos[d], "edge {s}→{d} violated: pos={:?}", pos);
    }
}
