// Indexed-element given pins (`col[0] = 1`) on a `Seq` variable — solve-for-X
// over a single cell. Before the fix the indexed key wasn't in `env` (only the
// whole Seq `col` was), so the pin was silently dropped: a witness came back
// that ignored the pin. The fix asserts `arr[idx] = value` in eval.rs.

use std::collections::HashMap;
use evident_runtime::{EvidentRuntime, Value};

// 4-queens as a Seq(Int): col[r] is the column of the queen in row r.
// Distinct columns + distinct diagonals. (Indices in the body are fine here —
// the test is about the *given*, not the model's idiom.)
const QUEENS: &str = "\
schema Queens
    col ∈ Seq(Int)
    #col = 4
    ∀ i ∈ {0..3} : 0 ≤ col[i] ∧ col[i] < 4
    ∀ i ∈ {0..3} : ∀ j ∈ {0..3} : i < j ⇒ col[i] ≠ col[j]
    ∀ i ∈ {0..3} : ∀ j ∈ {0..3} : i < j ⇒ col[i] - col[j] ≠ i - j
    ∀ i ∈ {0..3} : ∀ j ∈ {0..3} : i < j ⇒ col[i] - col[j] ≠ j - i
";

fn col_vec(r: &evident_runtime::QueryResult) -> Vec<i64> {
    match r.bindings.get("col") {
        Some(Value::SeqInt(v)) => v.clone(),
        other => panic!("expected SeqInt for col, got {:?}", other),
    }
}

#[test]
fn indexed_given_pins_a_single_seq_cell() {
    let mut rt = EvidentRuntime::new();
    rt.load_source(QUEENS).unwrap();

    let mut g = HashMap::new();
    g.insert("col[0]".to_string(), Value::Int(1));
    let r = rt.query("Queens", &g).unwrap();

    assert!(r.satisfied, "4-queens with col[0]=1 is satisfiable");
    let cols = col_vec(&r);
    assert_eq!(cols.len(), 4);
    assert_eq!(cols[0], 1, "indexed given col[0]=1 must be honored, got {cols:?}");
    // sanity: it's still a valid queens solution (distinct columns)
    let mut sorted = cols.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(sorted.len(), 4, "columns must be distinct: {cols:?}");
}

#[test]
fn two_indexed_givens_can_be_unsat() {
    let mut rt = EvidentRuntime::new();
    rt.load_source(QUEENS).unwrap();

    // col[0] and col[1] equal violates distinct-columns → UNSAT.
    let mut g = HashMap::new();
    g.insert("col[0]".to_string(), Value::Int(2));
    g.insert("col[1]".to_string(), Value::Int(2));
    let r = rt.query("Queens", &g).unwrap();
    assert!(!r.satisfied, "two equal columns must be UNSAT");
}

#[test]
fn indexed_given_changes_the_witness() {
    // Pinning a different cell yields a different solution honoring the pin —
    // proof the pin is actually applied, not ignored.
    let mut rt = EvidentRuntime::new();
    rt.load_source(QUEENS).unwrap();

    let mut g = HashMap::new();
    g.insert("col[2]".to_string(), Value::Int(0));
    let r = rt.query("Queens", &g).unwrap();
    assert!(r.satisfied);
    let cols = col_vec(&r);
    assert_eq!(cols[2], 0, "col[2]=0 must be honored, got {cols:?}");
}
