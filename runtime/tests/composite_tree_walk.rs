//! Focused tests for the four COUNTEREXAMPLES #19 composite-state gaps
//! that let the kernel scheduler tree-walk ANY recursive enum:
//!
//!   19b  nested constructor patterns deep-match (`Node(Leaf(n), r)`),
//!   19c  enum equality vs a literal carrying nested enum fields,
//!   19d  `run`'s init can be a composite (tree / enum literal),
//!   (ret) `run` returns a composite final state (nested enum / Seq).
//!
//! These are kernel/slow-path capabilities — nothing AST-specific. The
//! end-to-end proof is `examples/test_37_tree_walk.ev`; these pin the
//! individual gaps so a regression points at the exact one.

use std::collections::HashMap;

use evident_runtime::effect_loop::run_nested;
use evident_runtime::{EvidentRuntime, Value};

/// Build a runtime with stdlib + `src` loaded (stdlib resolves from
/// `../stdlib`, matching the `runtime/` cwd used by `cargo test`).
fn rt_with(src: &str) -> EvidentRuntime {
    let mut rt = EvidentRuntime::new();
    rt.load_file(std::path::Path::new("../stdlib/runtime.ev"))
        .expect("load stdlib/runtime.ev");
    let body: String = src.lines()
        .filter(|l| !l.trim_start().starts_with("import "))
        .collect::<Vec<_>>().join("\n");
    rt.load_source(&body).expect("load source");
    rt
}

fn sat(rt: &EvidentRuntime, claim: &str) -> bool {
    rt.query(claim, &HashMap::new())
        .unwrap_or_else(|e| panic!("query {claim} failed: {e}"))
        .satisfied
}

// ── 19b: nested constructor patterns deep-match ─────────────────────

const TREES: &str = r#"
enum Tree = Leaf(Int) | Node(Tree, Tree)
"#;

#[test]
fn nested_pattern_matches_only_when_inner_ctor_matches() {
    let rt = rt_with(&format!("{TREES}
claim sat_both_leaves
    t ∈ Tree = Node(Leaf(3), Leaf(4))
    s ∈ Int = match t
        Node(Leaf(a), Leaf(b)) ⇒ a + b
        _                      ⇒ 0
    s = 7
"));
    assert!(sat(&rt, "sat_both_leaves"),
        "Node(Leaf(3), Leaf(4)) must deep-match Node(Leaf(a), Leaf(b)) and bind a=3,b=4");
}

#[test]
fn nested_pattern_falls_through_when_inner_ctor_differs() {
    // The left child is a Node, NOT a Leaf, so Node(Leaf(_),_) must NOT
    // fire — it falls to the wildcard. (Pre-fix, the outer Node tester
    // matched regardless of the inner pattern → wrong dispatch.)
    let rt = rt_with(&format!("{TREES}
claim sat_left_is_node
    t ∈ Tree = Node(Node(Leaf(1), Leaf(2)), Leaf(9))
    s ∈ Int = match t
        Node(Leaf(a), Leaf(b)) ⇒ a + b
        _                      ⇒ 99
    s = 99
"));
    assert!(sat(&rt, "sat_left_is_node"),
        "left child is a Node, so the Leaf-pattern arm must not fire");
}

#[test]
fn nested_pattern_binds_through_two_levels() {
    let rt = rt_with(&format!("{TREES}
claim sat_deep_bind
    t ∈ Tree = Node(Node(Leaf(10), Leaf(20)), Leaf(5))
    s ∈ Int = match t
        Node(Node(Leaf(a), Leaf(b)), Leaf(c)) ⇒ a + b + c
        _                                     ⇒ 0
    s = 35
"));
    assert!(sat(&rt, "sat_deep_bind"),
        "three-deep nested pattern must bind a=10,b=20,c=5");
}

// ── 19c: enum equality vs a literal carrying nested enum fields ─────

#[test]
fn enum_eq_against_nested_literal() {
    let rt = rt_with(&format!("{TREES}
claim sat_nested_eq
    x ∈ Tree = Node(Leaf(1), Leaf(2))
    x = Node(Leaf(1), Leaf(2))
claim unsat_nested_eq
    x ∈ Tree = Node(Leaf(1), Leaf(2))
    x = Node(Leaf(1), Leaf(3))
"));
    assert!(sat(&rt, "sat_nested_eq"), "equality vs the matching nested literal holds");
    assert!(!sat(&rt, "unsat_nested_eq"), "a differing nested payload is UNSAT");
}

#[test]
fn enum_eq_with_nullary_nested_field() {
    // The reported 19c shape: a literal whose payload contains a NULLARY
    // enum value (`Empty`). Must translate, not silently drop.
    let rt = rt_with(r#"
enum Tree  = Leaf(Int) | Node(Tree, Tree)
enum Stack = Empty | Push(Tree, Stack)
enum Walk  = Seed(Int) | Step(Stack, Int) | Done(Int)
claim sat_nullary_nested
    x ∈ Walk = Step(Push(Leaf(7), Empty), 0)
    x = Step(Push(Leaf(7), Empty), 0)
"#);
    assert!(sat(&rt, "sat_nullary_nested"),
        "equality vs Step(Push(Leaf(7), Empty), 0) (nullary Empty nested) must hold");
}

// ── 19d: composite run() init ───────────────────────────────────────

const SUM1: &str = r#"
enum Tree = Leaf(Int) | Node(Tree, Tree)
enum W = WSeed(Tree) | WDone(Int)
fsm sum1(state ∈ W, state_next ∈ W, halt ∈ Bool)
    state_next = match state
        WSeed(t) ⇒ match t
            Leaf(v)    ⇒ WDone(v)
            Node(a, b) ⇒ WDone(0)
        WDone(d) ⇒ WDone(d)
    halt = match state
        WDone(_) ⇒ true
        _        ⇒ false
"#;

#[test]
fn composite_init_enum_literal_seeds_first_variant() {
    // run's init is a composite enum literal `Leaf(42)`; coerce_init
    // wraps it into the state enum's first variant `WSeed(Leaf(42))`.
    let rt = rt_with(SUM1);
    let final_state = run_nested(&rt, "sum1",
        Value::Enum { enum_name: "Tree".into(), variant: "Leaf".into(),
                      fields: vec![Value::Int(42)] }, 10_000)
        .expect("run sum1");
    assert_eq!(final_state, Value::Enum {
        enum_name: "W".into(), variant: "WDone".into(), fields: vec![Value::Int(42)] });
}

#[test]
fn composite_init_through_outer_query() {
    // The whole chain via `run(...)` in source: init carries a nested
    // enum literal, the run resolves before the outer solve, the result
    // pins the outer constraint.
    let mut rt = rt_with(SUM1);
    rt.load_source(
        "claim sat_init\n    final ∈ W\n    sum1(Leaf(42), final)\n    final = WDone(42)\n",
    ).expect("load outer claim");
    assert!(sat(&rt, "sat_init"), "sum1(Leaf(42), final) should pin final = WDone(42)");
}

// ── composite final-state return ────────────────────────────────────

const BUILD_STACK: &str = r#"
enum Stack = Empty | Push(Int, Stack)
enum W1 = S1(Int) | D1(Stack)
fsm build_stack(state ∈ W1, state_next ∈ W1, halt ∈ Bool)
    state_next = match state
        S1(n) ⇒ D1(Push(n, Push(n + 1, Empty)))
        D1(s) ⇒ D1(s)
    halt = match state
        D1(_) ⇒ true
        _     ⇒ false
"#;

#[test]
fn composite_return_nested_enum_with_nullary_terminator() {
    // The run returns a nested-enum final state whose spine ends in the
    // NULLARY `Empty`. value_to_literal_expr must emit `Empty` as a bare
    // identifier (not a zero-arg call) so the outer equality translates.
    let mut rt = rt_with(BUILD_STACK);
    rt.load_source(
        "claim sat_ret\n    final ∈ W1\n    build_stack(5, final)\n    \
         final = D1(Push(5, Push(6, Empty)))\n",
    ).expect("load outer claim");
    assert!(sat(&rt, "sat_ret"),
        "build_stack(5, final) should return D1(Push(5, Push(6, Empty))) and pin it");

    // And the run_nested value itself is the structured composite.
    let v = run_nested(&rt, "build_stack", Value::Int(5), 10_000).expect("run");
    let inner = |n: i64, tail: Value| Value::Enum {
        enum_name: "Stack".into(), variant: "Push".into(), fields: vec![Value::Int(n), tail] };
    let empty = Value::Enum { enum_name: "Stack".into(), variant: "Empty".into(), fields: vec![] };
    assert_eq!(v, Value::Enum {
        enum_name: "W1".into(), variant: "D1".into(),
        fields: vec![inner(5, inner(6, empty))] });
}
