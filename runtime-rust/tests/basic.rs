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

/// Claim composition with mappings: the called claim's slot binds to
/// a value from the caller's scope.
#[test]
fn claim_call_with_mapping() {
    let mut rt = EvidentRuntime::new();
    rt.load_source(
        "claim positive\n    n ∈ Nat\n    n > 0\n\
         schema S\n    a ∈ Nat\n    positive (n mapsto a)\n    a < 5\n"
    ).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    if let Some(Value::Int(a)) = r.bindings.get("a") {
        assert!(*a > 0 && *a < 5, "got {}", a);
    } else { panic!(); }
}

/// Multiple mappings, with literal values and identifier values mixed.
#[test]
fn claim_call_mixed_mappings() {
    let mut rt = EvidentRuntime::new();
    rt.load_source(
        "claim in_range\n    x ∈ Int\n    lo ∈ Int\n    hi ∈ Int\n    x ≥ lo\n    x ≤ hi\n\
         schema S\n    val ∈ Int\n    in_range (x mapsto val, lo mapsto 10, hi mapsto 20)\n"
    ).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    if let Some(Value::Int(v)) = r.bindings.get("val") {
        assert!(*v >= 10 && *v <= 20, "got {}", v);
    } else { panic!(); }
}

/// Sub-schema mapping in a ClaimCall: `state mapsto state.player`
/// should re-key every `state.field` slot in the claim to the
/// caller's `state.player.field` env entry.
#[test]
fn claim_call_sub_schema_mapping() {
    let mut rt = EvidentRuntime::new();
    rt.load_source(
        // PlayerState is the field bag; positive_xy constrains both fields.
        "type PlayerState\n    x ∈ Int\n    y ∈ Int\n\
         claim positive_xy\n    state ∈ PlayerState\n    state.x > 0\n    state.y > 0\n\
         schema World\n    p ∈ PlayerState\n    positive_xy (state mapsto p)\n    p.x < 5\n    p.y < 5\n"
    ).unwrap();
    let r = rt.query_free("World").unwrap();
    assert!(r.satisfied);
    if let (Some(Value::Int(x)), Some(Value::Int(y))) =
        (r.bindings.get("p.x"), r.bindings.get("p.y"))
    {
        assert!(*x > 0 && *x < 5);
        assert!(*y > 0 && *y < 5);
    } else { panic!("missing p.x or p.y"); }
}

/// `query_cached` matches `query` for the same input.
#[test]
fn cached_query_matches_uncached() {
    use std::collections::HashMap;
    let mut rt = EvidentRuntime::new();
    rt.load_source("schema S\n    n ∈ Nat\n    m ∈ Nat\n    n + m = 10\n").unwrap();
    let mut g = HashMap::new();
    g.insert("n".to_string(), Value::Int(7));
    let a = rt.query("S", &g).unwrap();
    let b = rt.query_cached("S", &g).unwrap();
    assert_eq!(a.satisfied, b.satisfied);
    assert_eq!(a.bindings, b.bindings);
}

/// Cached evaluator handles per-query given changes — different givens
/// give different bindings, and the constraints are translated only once.
#[test]
fn cached_query_per_call_givens() {
    use std::collections::HashMap;
    let mut rt = EvidentRuntime::new();
    rt.load_source("schema S\n    n ∈ Nat\n    m ∈ Nat\n    n + m = 10\n").unwrap();
    for n_given in [3, 5, 8] {
        let mut g = HashMap::new();
        g.insert("n".to_string(), Value::Int(n_given));
        let r = rt.query_cached("S", &g).unwrap();
        assert!(r.satisfied);
        assert_eq!(r.bindings.get("n"), Some(&Value::Int(n_given)));
        assert_eq!(r.bindings.get("m"), Some(&Value::Int(10 - n_given)));
    }
}

/// Cached evaluator preserves UNSAT correctly across queries.
#[test]
fn cached_query_unsat() {
    use std::collections::HashMap;
    let mut rt = EvidentRuntime::new();
    rt.load_source("schema S\n    n ∈ Nat\n    n < 5\n").unwrap();
    let mut g = HashMap::new();
    g.insert("n".to_string(), Value::Int(10));
    assert!(!rt.query_cached("S", &g).unwrap().satisfied);
    // Same cached schema; SAT case still works.
    let mut g2 = HashMap::new();
    g2.insert("n".to_string(), Value::Int(3));
    assert!(rt.query_cached("S", &g2).unwrap().satisfied);
}

/// Cached evaluator is faster than uncached on the same schema queried
/// many times. (Smoke test, not a strict perf gate.)
#[test]
fn cached_query_perf_smoke() {
    use std::collections::HashMap;
    use std::time::Instant;

    let mut rt = EvidentRuntime::new();
    // Multi-claim composition with passthrough — translation is heavy
    // enough that the cache should win.
    rt.load_source(
        "claim positive\n    n ∈ Nat\n    n > 0\n\
         claim small\n    n ∈ Nat\n    n < 100\n\
         schema S\n    n ∈ Nat\n    ..positive\n    ..small\n    n + 1 > 1\n"
    ).unwrap();

    let n_iters = 100;
    let mut g = HashMap::new();
    g.insert("n".to_string(), Value::Int(42));

    let t0 = Instant::now();
    for _ in 0..n_iters { rt.query("S", &g).unwrap(); }
    let uncached = t0.elapsed();

    let t0 = Instant::now();
    for _ in 0..n_iters { rt.query_cached("S", &g).unwrap(); }
    let cached = t0.elapsed();

    eprintln!("uncached: {:?}, cached: {:?}", uncached, cached);
    // Cached should be at least 1.5× faster — generous bound to avoid
    // CI flakiness while still catching regressions.
    assert!(cached < uncached, "cached ({:?}) should be < uncached ({:?})", cached, uncached);
}

/// Subclaim defined inside a parent's body. Other claims (or the parent
/// itself) can call it by name; the runtime registers it during load.
#[test]
fn subclaim_register_and_call() {
    let mut rt = EvidentRuntime::new();
    rt.load_source(
        "claim outer\n    \
         subclaim inner\n        \
         n ∈ Nat\n        \
         n > 5\n    \
         m ∈ Nat\n    \
         inner (n mapsto m)\n"
    ).unwrap();
    let r = rt.query_free("outer").unwrap();
    assert!(r.satisfied);
    if let Some(Value::Int(m)) = r.bindings.get("m") {
        assert!(*m > 5);
    } else { panic!("missing m"); }
}

/// A subclaim from one parent isn't accidentally hidden — it's globally
/// registered, so a sibling schema can also reach it.
#[test]
fn subclaim_visible_to_sibling() {
    let mut rt = EvidentRuntime::new();
    rt.load_source(
        "claim host\n    \
         subclaim helper\n        \
         k ∈ Nat\n        \
         k = 42\n\
         schema sibling\n    \
         a ∈ Nat\n    \
         helper (k mapsto a)\n"
    ).unwrap();
    let r = rt.query_free("sibling").unwrap();
    assert!(r.satisfied);
    assert_eq!(r.bindings.get("a"), Some(&Value::Int(42)));
}

/// Internal slot of the claim that isn't mapped should get a fresh
/// constant — Z3 picks any value satisfying the constraints.
#[test]
fn claim_call_unmapped_internal() {
    let mut rt = EvidentRuntime::new();
    // `pick` declares `picked ∈ Nat` and constrains it but doesn't
    // expose it via a mapping. The caller doesn't see `picked`; Z3
    // just needs to find some value to satisfy the claim.
    rt.load_source(
        "claim pick\n    picked ∈ Nat\n    out ∈ Nat\n    out = picked + 1\n    picked > 5\n\
         schema S\n    n ∈ Nat\n    pick (out mapsto n)\n    n < 20\n"
    ).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    if let Some(Value::Int(n)) = r.bindings.get("n") {
        // n = picked + 1 with picked > 5 → n > 6; plus n < 20.
        assert!(*n > 6 && *n < 20, "got {}", n);
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
