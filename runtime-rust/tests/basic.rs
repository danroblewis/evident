//! Integration tests mirroring Python's `runtime/tests/test_end_to_end.py`.

use evident_runtime::{EvidentRuntime, Value};

/// M0 — toolchain check. Just verifies z3 + cargo + linker work end-to-end.
#[test]
fn z3_hello_world() {
    use z3::{ast::{Ast, Int}, Config, Context, SatResult, Solver};
    let cfg = Config::new();
    let ctx = Context::new(&cfg);
    let s = Solver::new(&ctx);
    let n = Int::new_const(&ctx, "n");
    s.assert(&n.gt(&Int::from_i64(&ctx, 5)));
    assert!(matches!(s.check(), SatResult::Sat));
    let m = s.get_model().unwrap();
    let v = m.eval(&n, true).unwrap().as_i64().unwrap();
    assert!(v > 5);
}

/// M6 — first end-to-end test. Mirrors the Python test:
///   schema SimpleNat
///       n ∈ Nat
///       n > 5
#[test]
fn simple_nat_satisfied_with_n_gt_5() {
    let mut rt = EvidentRuntime::new();
    rt.load_source("schema SimpleNat\n    n ∈ Nat\n    n > 5\n").unwrap();
    let r = rt.query_free("SimpleNat").unwrap();
    assert!(r.satisfied);
    let n = r.bindings.get("n").expect("missing binding for n");
    match n {
        Value::Int(v) => assert!(*v > 5, "expected n > 5, got {}", v),
        other => panic!("expected Int, got {:?}", other),
    }
}

/// Mirror `test_load_source_unsat` from the Python suite:
///   schema Impossible
///       n ∈ Nat
///       n > 10
///       n < 3
#[test]
fn impossible_is_unsat() {
    let mut rt = EvidentRuntime::new();
    rt.load_source("schema Impossible\n    n ∈ Nat\n    n > 10\n    n < 3\n").unwrap();
    let r = rt.query_free("Impossible").unwrap();
    assert!(!r.satisfied);
}

/// Multiple variables + a relation.
#[test]
fn two_vars_relation() {
    let mut rt = EvidentRuntime::new();
    rt.load_source("schema Pair\n    a ∈ Nat\n    b ∈ Nat\n    a + b = 10\n    a > 0\n    b > 0\n").unwrap();
    let r = rt.query_free("Pair").unwrap();
    assert!(r.satisfied);
    if let (Some(Value::Int(a)), Some(Value::Int(b))) =
        (r.bindings.get("a"), r.bindings.get("b"))
    {
        assert_eq!(a + b, 10);
        assert!(*a > 0 && *b > 0);
    } else { panic!("missing bindings"); }
}

/// Bool variable + a logical constraint.
#[test]
fn bool_implies() {
    let mut rt = EvidentRuntime::new();
    // p ⇒ q forces q to be true when p is true.
    rt.load_source("schema P\n    p ∈ Bool\n    q ∈ Bool\n    p = true\n    p ⇒ q\n").unwrap();
    let r = rt.query_free("P").unwrap();
    assert!(r.satisfied);
    assert_eq!(r.bindings.get("q"), Some(&Value::Bool(true)));
}

/// String literal equality.
#[test]
fn string_literal_eq() {
    let mut rt = EvidentRuntime::new();
    rt.load_source("schema S\n    name ∈ String\n    name = \"hello\"\n").unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    assert_eq!(r.bindings.get("name"), Some(&Value::Str("hello".into())));
}

/// String inequality forces the solver to pick something other than the literal.
#[test]
fn string_neq_excludes_literal() {
    let mut rt = EvidentRuntime::new();
    rt.load_source("schema S\n    name ∈ String\n    name ≠ \"x\"\n    name = \"y\"\n").unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    assert_eq!(r.bindings.get("name"), Some(&Value::Str("y".into())));
}

/// `given` pre-binds a value, like Python's query(schema, given={...}).
#[test]
fn given_binds_int() {
    use std::collections::HashMap;
    let mut rt = EvidentRuntime::new();
    rt.load_source("schema S\n    n ∈ Nat\n    m ∈ Nat\n    n + m = 10\n").unwrap();
    let mut g = HashMap::new();
    g.insert("n".to_string(), Value::Int(7));
    let r = rt.query("S", &g).unwrap();
    assert!(r.satisfied);
    assert_eq!(r.bindings.get("n"), Some(&Value::Int(7)));
    assert_eq!(r.bindings.get("m"), Some(&Value::Int(3)));
}

/// `given` that violates a constraint produces UNSAT.
#[test]
fn given_violation_unsat() {
    use std::collections::HashMap;
    let mut rt = EvidentRuntime::new();
    rt.load_source("schema S\n    n ∈ Nat\n    n < 5\n").unwrap();
    let mut g = HashMap::new();
    g.insert("n".to_string(), Value::Int(10));
    let r = rt.query("S", &g).unwrap();
    assert!(!r.satisfied);
}

/// User-defined type expanded into leaf fields. `task ∈ Task` should
/// declare `task.id` and `task.duration` as Z3 consts; the constraint
/// references them with dotted syntax.
#[test]
fn sub_schema_field_expansion() {
    let mut rt = EvidentRuntime::new();
    rt.load_source(
        "type Task\n    id ∈ Nat\n    duration ∈ Nat\n\
         schema S\n    task ∈ Task\n    task.id = 7\n    task.duration > 3\n"
    ).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    assert_eq!(r.bindings.get("task.id"), Some(&Value::Int(7)));
    if let Some(Value::Int(d)) = r.bindings.get("task.duration") {
        assert!(*d > 3);
    } else { panic!("missing task.duration"); }
}

/// Nested sub-schemas: `proj ∈ Project` where Project contains
/// `lead ∈ Person` where Person has fields. Verifies recursive expansion.
#[test]
fn nested_sub_schema() {
    let mut rt = EvidentRuntime::new();
    rt.load_source(
        "type Person\n    age ∈ Nat\n\
         type Project\n    lead ∈ Person\n    budget ∈ Nat\n\
         schema S\n    proj ∈ Project\n    proj.lead.age = 30\n    proj.budget > 100\n"
    ).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    assert_eq!(r.bindings.get("proj.lead.age"), Some(&Value::Int(30)));
    if let Some(Value::Int(b)) = r.bindings.get("proj.budget") {
        assert!(*b > 100);
    } else { panic!("missing proj.budget"); }
}

/// `x ∈ {a, b, c}` — set-literal membership reduces to a disjunction.
#[test]
fn set_literal_membership() {
    let mut rt = EvidentRuntime::new();
    rt.load_source("schema S\n    n ∈ Nat\n    n ∈ {3, 5, 7}\n").unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    if let Some(Value::Int(n)) = r.bindings.get("n") {
        assert!([3, 5, 7].contains(n), "expected n ∈ {{3,5,7}}, got {}", n);
    } else { panic!(); }
}

/// Membership in a String set.
#[test]
fn set_literal_strings() {
    let mut rt = EvidentRuntime::new();
    rt.load_source("schema S\n    color ∈ String\n    color ∈ {\"red\", \"green\", \"blue\"}\n    color ≠ \"red\"\n    color ≠ \"green\"\n").unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    assert_eq!(r.bindings.get("color"), Some(&Value::Str("blue".into())));
}

/// `∀ i ∈ {0..3} : a + i > 0` — universal quantifier unrolls and asserts.
#[test]
fn forall_range_unroll() {
    let mut rt = EvidentRuntime::new();
    // Force a >= 1 by saying for every i in 0..3, a + i > 0; with i=0 → a > 0.
    rt.load_source("schema S\n    a ∈ Int\n    ∀ i ∈ {0..3} : a + i > 0\n").unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    if let Some(Value::Int(a)) = r.bindings.get("a") {
        assert!(*a > 0);
    } else { panic!(); }
}

/// `∃ i ∈ {0..5} : a = i * 3` — existential picks one i.
#[test]
fn exists_range_unroll() {
    let mut rt = EvidentRuntime::new();
    rt.load_source("schema S\n    a ∈ Nat\n    a > 6\n    ∃ i ∈ {0..5} : a = i * 3\n").unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    // a > 6 and a = i*3 for some i ∈ {0..5} → only 9, 12, or 15 work
    if let Some(Value::Int(a)) = r.bindings.get("a") {
        assert!([9, 12, 15].contains(a), "got {}", a);
    } else { panic!(); }
}

/// `..ClaimName` passthrough — the included claim's constraints fold in
/// under names-match composition.
#[test]
fn passthrough_names_match() {
    let mut rt = EvidentRuntime::new();
    rt.load_source(
        "claim positive\n    n ∈ Nat\n    n > 0\n\
         schema S\n    n ∈ Nat\n    ..positive\n    n < 10\n"
    ).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    if let Some(Value::Int(n)) = r.bindings.get("n") {
        assert!(*n > 0 && *n < 10, "got {}", n);
    } else { panic!(); }
}

/// Passthrough that introduces a new variable into the parent's scope.
#[test]
fn passthrough_introduces_var() {
    let mut rt = EvidentRuntime::new();
    rt.load_source(
        "claim has_age\n    age ∈ Nat\n    age > 18\n\
         schema Person\n    ..has_age\n    age < 100\n"
    ).unwrap();
    let r = rt.query_free("Person").unwrap();
    assert!(r.satisfied);
    if let Some(Value::Int(a)) = r.bindings.get("age") {
        assert!(*a > 18 && *a < 100, "got {}", a);
    } else { panic!(); }
}

/// Passthrough whose constraints contradict a parent constraint → UNSAT.
#[test]
fn passthrough_conflict_unsat() {
    let mut rt = EvidentRuntime::new();
    rt.load_source(
        "claim must_be_zero\n    n ∈ Nat\n    n = 0\n\
         schema S\n    n ∈ Nat\n    ..must_be_zero\n    n > 5\n"
    ).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(!r.satisfied);
}

/// `given` on a sub-schema field via dotted name.
#[test]
fn given_sub_schema_field() {
    use std::collections::HashMap;
    let mut rt = EvidentRuntime::new();
    rt.load_source(
        "type Task\n    id ∈ Nat\n    duration ∈ Nat\n\
         schema S\n    task ∈ Task\n    task.duration > task.id\n"
    ).unwrap();
    let mut g = HashMap::new();
    g.insert("task.id".to_string(), Value::Int(5));
    g.insert("task.duration".to_string(), Value::Int(10));
    let r = rt.query("S", &g).unwrap();
    assert!(r.satisfied);
    assert_eq!(r.bindings.get("task.id"), Some(&Value::Int(5)));
    assert_eq!(r.bindings.get("task.duration"), Some(&Value::Int(10)));
}
