//! Stage 9 integration tests: cross-body-item `=` propagation.
//!
//! The pass finds two body items that, together, imply a type for
//! some variable. Single-rule iteration over `body` can't do this
//! — needs the nested `∃ i, j` pattern in propagation.ev.

use std::path::Path;
use evident_runtime::{EvidentRuntime, Value};

const STDLIB_AST:  &str = "../stdlib/ast.ev";
const PROPAGATION: &str = "../stdlib/passes/propagation.ev";

fn fresh_rt_with_propagation() -> EvidentRuntime {
    let mut rt = EvidentRuntime::new();
    rt.load_file(Path::new(STDLIB_AST)).unwrap();
    rt.load_file(Path::new(PROPAGATION)).unwrap();
    rt.mark_system_loads_complete();
    rt
}

#[test]
fn propagate_string_from_indirect_assignment() {
    // x = y; y = "hello" → x ∈ String
    let mut rt = fresh_rt_with_propagation();
    rt.load_source("\
claim t
    x = y
    y = \"hello\"
").unwrap();
    let r = rt.query_with_program_and_body(
        "propagate_string", "program", "body",
    ).unwrap();
    assert!(r.satisfied,
        "propagation should bind x ∈ String via y");
    assert_eq!(r.bindings.get("target_var"),
               Some(&Value::Str("x".to_string())));
    assert_eq!(r.bindings.get("via_var"),
               Some(&Value::Str("y".to_string())));
    assert_eq!(r.bindings.get("string_lit"),
               Some(&Value::Str("hello".to_string())));
}

#[test]
fn propagate_int_from_indirect_assignment() {
    let mut rt = fresh_rt_with_propagation();
    rt.load_source("\
claim t
    a = b
    b = 42
").unwrap();
    let r = rt.query_with_program_and_body(
        "propagate_int", "program", "body",
    ).unwrap();
    assert!(r.satisfied);
    assert_eq!(r.bindings.get("target_var"),
               Some(&Value::Str("a".to_string())));
    assert_eq!(r.bindings.get("int_lit"),
               Some(&Value::Int(42)));
}

#[test]
fn propagate_bool_from_indirect_assignment() {
    let mut rt = fresh_rt_with_propagation();
    rt.load_source("\
claim t
    flag = base
    base = true
").unwrap();
    let r = rt.query_with_program_and_body(
        "propagate_bool", "program", "body",
    ).unwrap();
    assert!(r.satisfied);
    assert_eq!(r.bindings.get("target_var"),
               Some(&Value::Str("flag".to_string())));
    assert_eq!(r.bindings.get("bool_lit"),
               Some(&Value::Bool(true)));
}

#[test]
fn propagate_string_unsat_when_via_var_unassigned() {
    // x = y but y is never assigned to a String literal.
    let mut rt = fresh_rt_with_propagation();
    rt.load_source("\
claim t
    x = y
    z = \"unrelated\"
").unwrap();
    let r = rt.query_with_program_and_body(
        "propagate_string", "program", "body",
    ).unwrap();
    assert!(!r.satisfied,
        "no y = literal binding → propagation can't fire");
}

#[test]
fn propagate_string_unsat_when_via_assigned_to_int() {
    // x = y, but y = 42 (Int, not String). String rule must reject.
    let mut rt = fresh_rt_with_propagation();
    rt.load_source("\
claim t
    x = y
    y = 42
").unwrap();
    let r = rt.query_with_program_and_body(
        "propagate_string", "program", "body",
    ).unwrap();
    assert!(!r.satisfied,
        "y = Int doesn't trigger String propagation");
}

#[test]
fn propagate_works_when_assignments_are_in_swapped_order() {
    // y = "hello" first, then x = y. Order shouldn't matter.
    let mut rt = fresh_rt_with_propagation();
    rt.load_source("\
claim t
    y = \"hello\"
    x = y
").unwrap();
    let r = rt.query_with_program_and_body(
        "propagate_string", "program", "body",
    ).unwrap();
    assert!(r.satisfied,
        "swapped order should still satisfy ∃ i, j");
    assert_eq!(r.bindings.get("target_var"),
               Some(&Value::Str("x".to_string())));
}

#[test]
fn propagate_unsat_for_single_body_program() {
    // Only one body item — there's no `via_var` chain to follow.
    let mut rt = fresh_rt_with_propagation();
    rt.load_source("claim t\n    x = \"hello\"\n").unwrap();
    let r = rt.query_with_program_and_body(
        "propagate_string", "program", "body",
    ).unwrap();
    // The two existentials need DIFFERENT body[i] and body[j], but
    // here body[0] = body[0] would have to satisfy two structurally
    // different patterns → UNSAT.
    assert!(!r.satisfied,
        "single-body programs can't satisfy x = y AND y = literal");
}

#[test]
fn propagate_works_with_other_intervening_body_items() {
    // The two relevant items (x = y, y = "h") are buried among
    // unrelated stuff. Iteration finds them.
    let mut rt = fresh_rt_with_propagation();
    rt.load_source("\
claim t
    other ∈ Int
    x = y
    other = 5
    y = \"hello\"
    final ∈ Bool
").unwrap();
    let r = rt.query_with_program_and_body(
        "propagate_string", "program", "body",
    ).unwrap();
    assert!(r.satisfied);
    assert_eq!(r.bindings.get("target_var"),
               Some(&Value::Str("x".to_string())));
}
