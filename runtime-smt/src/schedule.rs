//! N3 scheduler ordering — writer-first topological sort.
//!
//! For each world variable `v`, every FSM that writes `v` must execute before
//! every FSM that reads `v` (same-tick visibility). This module computes a
//! valid ordering via Kahn's algorithm, breaking ties by original declaration
//! index (stable/deterministic).

use crate::spec::FsmSpec;

/// Return an ordering of FSM indices (into `fsms`) such that for every world
/// variable `v`, every FSM that writes `v` appears before every FSM that reads
/// `v`. Among FSMs with no ordering constraint between them, preserve their
/// original declaration order (stable). Returns `Err` if the
/// writer→reader dependencies form a cycle (no valid writer-first order).
pub fn order(fsms: &[FsmSpec]) -> Result<Vec<usize>, String> {
    let n = fsms.len();

    // Build adjacency list and in-degree counts.
    // Edge w -> r means: FSM w must run before FSM r.
    // Use a set to deduplicate (w, r) pairs so in-degree is counted once per distinct pair.
    let mut edges: std::collections::HashSet<(usize, usize)> = std::collections::HashSet::new();

    for (w, writer) in fsms.iter().enumerate() {
        for var in &writer.world_writes {
            for (r, reader) in fsms.iter().enumerate() {
                // Skip self-edges: an FSM that both writes and reads the same var
                // doesn't depend on itself.
                if w == r {
                    continue;
                }
                if reader.world_reads.contains(var) {
                    edges.insert((w, r));
                }
            }
        }
    }

    // Build adjacency list and in-degree from deduplicated edge set.
    let mut adj: Vec<Vec<usize>> = vec![vec![]; n];
    let mut in_degree: Vec<usize> = vec![0; n];

    for (w, r) in &edges {
        adj[*w].push(*r);
        in_degree[*r] += 1;
    }

    // Kahn's algorithm with min-index tie-breaking (stable, deterministic).
    // Use a sorted structure: repeatedly pick the smallest-index node with in-degree 0.
    // We use a BinaryHeap (min-heap via Reverse) for O(n log n) overall.
    use std::collections::BinaryHeap;
    use std::cmp::Reverse;

    let mut heap: BinaryHeap<Reverse<usize>> = BinaryHeap::new();
    for i in 0..n {
        if in_degree[i] == 0 {
            heap.push(Reverse(i));
        }
    }

    let mut result = Vec::with_capacity(n);

    while let Some(Reverse(node)) = heap.pop() {
        result.push(node);
        for &neighbor in &adj[node] {
            in_degree[neighbor] -= 1;
            if in_degree[neighbor] == 0 {
                heap.push(Reverse(neighbor));
            }
        }
    }

    if result.len() != n {
        // Some nodes were never emitted — they are part of a cycle.
        return Err(
            "writer/reader cycle detected: no valid writer-first ordering exists".to_string(),
        );
    }

    Ok(result)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal FsmSpec with only name, world_writes, and world_reads set.
    fn fsm(name: &str, writes: &[&str], reads: &[&str]) -> FsmSpec {
        FsmSpec {
            name: name.to_string(),
            transition: String::new(),
            state: vec![],
            given: vec![],
            effects: None,
            halt: None,
            world_writes: writes.iter().map(|s| s.to_string()).collect(),
            world_reads: reads.iter().map(|s| s.to_string()).collect(),
        }
    }

    /// Producer declared before consumer — natural order preserved.
    #[test]
    fn producer_before_consumer_natural_order() {
        let fsms = vec![
            fsm("producer", &["n"], &[]),
            fsm("consumer", &[], &["n"]),
        ];
        let order = order(&fsms).unwrap();
        assert_eq!(order, vec![0, 1]);
    }

    /// Consumer declared first (index 0), producer is index 1 — order must flip
    /// so producer (idx 1) runs before consumer (idx 0).
    #[test]
    fn producer_after_consumer_in_declaration_order_flips() {
        let fsms = vec![
            fsm("consumer", &[], &["n"]),  // index 0
            fsm("producer", &["n"], &[]), // index 1
        ];
        let order = order(&fsms).unwrap();
        // producer (1) must come before consumer (0)
        assert_eq!(order, vec![1, 0]);
    }

    /// Independent FSMs (no shared world vars) preserve declaration order.
    #[test]
    fn independent_fsms_preserve_declaration_order() {
        let fsms = vec![
            fsm("a", &[], &[]),
            fsm("b", &[], &[]),
            fsm("c", &[], &[]),
        ];
        let order = order(&fsms).unwrap();
        assert_eq!(order, vec![0, 1, 2]);
    }

    /// Chain: A writes x (read by B), B writes y (read by C).
    /// Regardless of declaration order, result must be [A, B, C].
    #[test]
    fn chain_a_b_c() {
        // Declared in forward order
        let fsms = vec![
            fsm("a", &["x"], &[]),
            fsm("b", &["y"], &["x"]),
            fsm("c", &[], &["y"]),
        ];
        let order = order(&fsms).unwrap();
        assert_eq!(order, vec![0, 1, 2]);
    }

    /// Chain declared in reverse order — must still come out A, B, C by constraint.
    #[test]
    fn chain_declared_in_reverse() {
        // Declared as C(idx 0), B(idx 1), A(idx 2)
        let fsms = vec![
            fsm("c", &[], &["y"]),       // index 0
            fsm("b", &["y"], &["x"]),    // index 1
            fsm("a", &["x"], &[]),       // index 2
        ];
        let order = order(&fsms).unwrap();
        // a(2) must before b(1) must before c(0)
        assert_eq!(order, vec![2, 1, 0]);
    }

    /// An FSM that both writes and reads the same var does NOT cause a cycle.
    #[test]
    fn self_read_write_no_cycle() {
        let fsms = vec![
            fsm("self_rw", &["x"], &["x"]),
        ];
        let result = order(&fsms);
        assert!(result.is_ok(), "self read/write should not cause a cycle");
        assert_eq!(result.unwrap(), vec![0]);
    }

    /// Self read/write FSM alongside others — self-edge skipped, others ordered normally.
    #[test]
    fn self_read_write_with_others() {
        let fsms = vec![
            fsm("self_rw", &["x"], &["x"]), // index 0, writes and reads x
            fsm("consumer", &[], &["x"]),   // index 1, reads x
        ];
        // self_rw(0) writes x which consumer(1) reads, so 0 before 1.
        // self_rw does NOT have a self-edge.
        let order = order(&fsms).unwrap();
        assert_eq!(order, vec![0, 1]);
    }

    /// True cycle: A writes x read by B, B writes y read by A → Err.
    #[test]
    fn true_cycle_returns_err() {
        let fsms = vec![
            fsm("a", &["x"], &["y"]),
            fsm("b", &["y"], &["x"]),
        ];
        let result = order(&fsms);
        assert!(result.is_err(), "expected cycle error, got {:?}", result);
        let msg = result.unwrap_err();
        assert!(msg.contains("cycle"), "error message should mention 'cycle': {msg}");
    }

    /// Stable tie-break: three independent FSMs stay in declaration order [0,1,2].
    #[test]
    fn stable_tie_break_independent() {
        let fsms = vec![
            fsm("x", &[], &[]),
            fsm("y", &[], &[]),
            fsm("z", &[], &[]),
        ];
        let order = order(&fsms).unwrap();
        assert_eq!(order, vec![0, 1, 2]);
    }

    /// Stable tie-break with partial ordering: A -> C (A before C), B independent.
    /// Min-index tie-breaking: B(1) and A(0) both start with in-degree 0.
    /// A(0) < B(1), so A comes first. Then both B and C are available; B(1) < C(2).
    #[test]
    fn stable_tie_break_partial_order() {
        let fsms = vec![
            fsm("a", &["v"], &[]),   // index 0, writes v
            fsm("b", &[], &[]),      // index 1, independent
            fsm("c", &[], &["v"]),   // index 2, reads v
        ];
        let order = order(&fsms).unwrap();
        // a(0) must before c(2); b(1) is free.
        // Round 1: in-degree 0 = {a(0), b(1)} → pick a(0).
        // After a: in-degree of c drops to 0. Ready = {b(1), c(2)} → pick b(1).
        // After b: Ready = {c(2)} → pick c(2).
        assert_eq!(order, vec![0, 1, 2]);
    }

    /// Duplicate edges don't inflate in-degree: two vars both written by same writer,
    /// both read by same reader — still just one edge w->r, no double-count.
    #[test]
    fn duplicate_edge_deduplication() {
        let fsms = vec![
            fsm("writer", &["x", "y"], &[]),  // index 0
            fsm("reader", &[], &["x", "y"]),  // index 1
        ];
        // Two vars would create two w->r edges without dedup; dedup collapses to one.
        // Either way the order is the same [0, 1], but we also test no panic/wrong count.
        let order = order(&fsms).unwrap();
        assert_eq!(order, vec![0, 1]);
    }
}
