//! Correctness pins for the self-hosted effect-dispatch toposort
//! (`portable::toposort`), the sole ordering impl since session
//! PORT-toposort. This replaces the old `EVIDENT_TOPOSORT_IMPL` env-gated
//! cross-check (Rust vs Evident) — the Rust Kahn's algorithm is deleted, so
//! there is nothing to cross-validate against. Instead we pin the contract
//! the dispatcher relies on:
//!
//!   * **Forced orders** — a linear chain has exactly one valid topological
//!     order; pin it sequence-identical.
//!   * **Edge-respecting validity** — for graphs with multiple valid
//!     linearizations, every declared edge `from → to` has `from` earlier
//!     than `to`, and the output is a permutation of the input nodes.
//!   * **Cycle → None** — a cyclic dependency graph is UNSAT; the pass
//!     returns `None` (the dispatcher recovers via `cycle_recovery`).
//!
//! Driven through the production entry point `portable::toposort::toposort`
//! — the exact call the effect scheduler's `collect` makes — and the engine
//! struct directly, both backed by the integer-rank `ToposortRanks` claim.

use evident_runtime::portable::toposort::{toposort, EvidentToposort, ToposortImpl};
use evident_runtime::portable::Portable;
use std::path::PathBuf;

/// Repo `stdlib/` dir via the same resolver the runtime uses.
fn stdlib_dir() -> PathBuf {
    evident_runtime::stdlib_path::stdlib_dir().expect("locate stdlib/")
}

fn engine() -> EvidentToposort {
    EvidentToposort::new(&stdlib_dir()).expect("load stdlib/toposort.ev")
}

/// Helper: node names `n0..n{k-1}`.
fn nodes(k: usize) -> Vec<String> {
    (0..k).map(|i| format!("n{i}")).collect()
}

fn edge(f: usize, t: usize) -> (String, String) {
    (format!("n{f}"), format!("n{t}"))
}

/// Position of a node in an ordering.
fn pos(order: &[String], node: usize) -> usize {
    order.iter().position(|s| s == &format!("n{node}")).expect("node present")
}

#[test]
fn linear_chain_forces_unique_order() {
    let ev = engine();
    // n0 → n1 → n2 → n3: the only valid order is the chain itself.
    let edges = vec![edge(0, 1), edge(1, 2), edge(2, 3)];
    let out = ev.toposort(&nodes(4), &edges).expect("acyclic → Some");
    assert_eq!(out, vec!["n0", "n1", "n2", "n3"], "chain order is forced");
}

#[test]
fn dag_output_is_a_valid_topological_order() {
    let ev = engine();
    // Two independent chains: 0→2, 1→3. Multiple valid linearizations;
    // assert the invariant, not a specific permutation.
    let edges = vec![edge(0, 2), edge(1, 3)];
    let out = ev.toposort(&nodes(4), &edges).expect("acyclic → Some");
    let mut sorted = out.clone();
    sorted.sort();
    assert_eq!(sorted, vec!["n0", "n1", "n2", "n3"], "output is a permutation of nodes");
    assert!(pos(&out, 0) < pos(&out, 2), "edge 0→2: {out:?}");
    assert!(pos(&out, 1) < pos(&out, 3), "edge 1→3: {out:?}");
}

#[test]
fn eight_node_dag_respects_every_edge() {
    let ev = engine();
    let edges = vec![
        edge(0, 1), edge(1, 3), edge(3, 6),
        edge(2, 4), edge(4, 7), edge(3, 5),
    ];
    let out = ev.toposort(&nodes(8), &edges).expect("acyclic → Some");
    let mut sorted = out.clone();
    sorted.sort();
    assert_eq!(sorted, nodes(8), "permutation of all 8 nodes");
    for (f, t) in &edges {
        let (fi, ti) = (
            out.iter().position(|s| s == f).unwrap(),
            out.iter().position(|s| s == t).unwrap(),
        );
        assert!(fi < ti, "edge {f}→{t} violated: {out:?}");
    }
}

#[test]
fn empty_edges_returns_all_nodes() {
    let ev = engine();
    let out = ev.toposort(&nodes(5), &[]).expect("no edges → Some");
    let mut sorted = out.clone();
    sorted.sort();
    assert_eq!(sorted, nodes(5), "all nodes present, any order");
}

#[test]
fn cycle_is_none() {
    let ev = engine();
    // n0 → n1 → n2 → n0 has no topological order.
    let edges = vec![edge(0, 1), edge(1, 2), edge(2, 0)];
    assert_eq!(ev.toposort(&nodes(3), &edges), None, "cyclic graph → None");
}

#[test]
fn empty_graph_is_empty() {
    let ev = engine();
    assert_eq!(ev.toposort(&[], &[]), Some(Vec::new()));
}

#[test]
fn production_entry_matches_engine() {
    // `portable::toposort::toposort` is what `effect_loop::collect` calls.
    // It must agree with a directly-built engine on a forced order.
    let edges = vec![edge(0, 1), edge(1, 2)];
    let prod = toposort(&nodes(3), &edges).expect("acyclic → Some");
    assert_eq!(prod, vec!["n0", "n1", "n2"]);
    // And recover from a cycle the same way (None).
    assert_eq!(toposort(&nodes(3), &vec![edge(0, 1), edge(1, 0)]), None);
}

#[test]
fn impl_name_is_evident() {
    assert_eq!(engine().impl_name(), "evident");
}
