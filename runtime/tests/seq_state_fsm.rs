//! Seq-state FSM regression (the "Seq-state export gap").
//!
//! An fsm that carries a `Seq` var across ticks and advances it with
//! `coindexed(_xs, xs)` used to SILENTLY DROP the ongoing transition: the
//! prev-tick twin `_xs` got injected as a Seq decl, but its length was never
//! pinned (only `#xs = 3` is in source, never `#_xs = 3`), so the encoder's
//! `coindexed` translator couldn't resolve `_xs`'s length and dropped the
//! whole `¬is_first_tick ⇒ ∀ (cur,nxt) ∈ coindexed(_xs, xs) : nxt = cur+1`.
//!
//! The fix propagates the base Seq's pinned length to its `_`-prefixed twins
//! in `apply_seq_lengths`. These tests pin `_xs` (as the trampoline would at
//! runtime) and assert the transition actually fires.

use evident_runtime::{EvidentRuntime, Value};
use std::collections::HashMap;

const SHIFT: &str = "fsm shift\n    \
    xs ∈ Seq(Int)\n    \
    #xs = 3\n    \
    is_first_tick ⇒ xs = ⟨1, 2, 3⟩\n    \
    ¬is_first_tick ⇒ ∀ (cur, nxt) ∈ coindexed(_xs, xs) : nxt = cur + 1\n";

fn ints(v: Option<&Value>) -> Vec<i64> {
    match v {
        Some(Value::SeqInt(xs)) => xs.clone(),
        other => panic!("expected SeqInt, got {:?}", other),
    }
}

#[test]
fn first_tick_seeds_the_seq() {
    let mut rt = EvidentRuntime::new();
    rt.load_source(SHIFT).unwrap();
    let mut given = HashMap::new();
    given.insert("is_first_tick".to_string(), Value::Bool(true));
    let r = rt.query("shift", &given).unwrap();
    assert!(r.satisfied, "first tick must be SAT");
    assert_eq!(ints(r.bindings.get("xs")), vec![1, 2, 3]);
}

#[test]
fn ongoing_transition_advances_the_carried_seq() {
    // The transition reads the previous tick's Seq via `_xs` and must produce
    // each element + 1. Before the fix this constraint was dropped, so `xs`
    // came back free (any length-3 Seq) instead of [3, 4, 5].
    let mut rt = EvidentRuntime::new();
    rt.load_source(SHIFT).unwrap();
    let mut given = HashMap::new();
    given.insert("is_first_tick".to_string(), Value::Bool(false));
    given.insert("_xs".to_string(), Value::SeqInt(vec![2, 3, 4]));
    let r = rt.query("shift", &given).unwrap();
    assert!(r.satisfied, "ongoing transition must be SAT");
    assert_eq!(
        ints(r.bindings.get("xs")),
        vec![3, 4, 5],
        "coindexed(_xs, xs): nxt = cur + 1 must advance every element"
    );
}

#[test]
fn export_state_includes_the_carried_seq() {
    // The export's `state[]` must surface the carried Seq var (it reported
    // `state: []` before the fix, so the viz layer saw no carried vars).
    let mut rt = EvidentRuntime::new();
    rt.load_source(SHIFT).unwrap();
    let (_smt2, json) = rt.export_transition("shift").unwrap();
    assert!(json.contains("\"name\": \"xs\""), "carried seq must appear:\n{json}");
    assert!(json.contains("\"kind\": \"seq\""), "kind must be seq:\n{json}");
    assert!(json.contains("\"elem\": \"int\""), "elem must be int:\n{json}");
    assert!(json.contains("\"len\": 3"), "pinned length must be 3:\n{json}");
    assert!(json.contains("\"prev\": \"_xs\""), "prev twin must be _xs:\n{json}");
}

#[test]
fn exported_smt2_pins_the_seq_length() {
    // The standalone exported SMT-LIB must be SELF-CONTAINED: the carried Seq's
    // length const `<seq>__len` has to be ASSERTED `= N`, not left at the bare
    // `>= 0` from declaration. Before the fix the source's `#xs = N` lowered
    // against the in-memory literal `len` to the no-op `(= N N)`, so a z3 user
    // pasting the export could pick any `xs__len` and a `∀ i` property would read
    // out-of-range cells. Both the base `xs` and its prev-twin `_xs` must pin.
    let mut rt = EvidentRuntime::new();
    rt.load_source(SHIFT).unwrap();
    let (smt2, _json) = rt.export_transition("shift").unwrap();
    assert!(
        smt2.contains("(= xs__len 3)"),
        "exported smt2 must pin the carried Seq length (xs__len = 3), not leave \
         it at `>= 0` with a tautological `(= 3 3)`:\n{smt2}"
    );
    assert!(
        smt2.contains("(= _xs__len 3)"),
        "the prev-tick twin's length must pin too (_xs__len = 3) so the ongoing \
         transition's `coindexed(_xs, xs)` is length-bounded:\n{smt2}"
    );
}

#[test]
fn ongoing_transition_is_not_dropped() {
    // Direct exercise of the dropped-constraint bug: if the transition were
    // dropped, pinning `_xs` would leave `xs` unconstrained and a contradictory
    // pin (xs ≠ _xs+1) would still be SAT. With the transition present, pinning
    // both `_xs` and an INCORRECT `xs` must be UNSAT.
    let mut rt = EvidentRuntime::new();
    rt.load_source(SHIFT).unwrap();
    let mut given = HashMap::new();
    given.insert("is_first_tick".to_string(), Value::Bool(false));
    given.insert("_xs".to_string(), Value::SeqInt(vec![2, 3, 4]));
    given.insert("xs".to_string(), Value::SeqInt(vec![9, 9, 9])); // wrong on purpose
    let r = rt.query("shift", &given).unwrap();
    assert!(
        !r.satisfied,
        "transition must constrain xs = _xs + 1; a wrong xs must be UNSAT \
         (was SAT when the constraint was silently dropped)"
    );
}
