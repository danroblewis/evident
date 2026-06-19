use evident_runtime::{EvidentRuntime, Value};

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

#[test]
fn impossible_is_unsat() {
    let mut rt = EvidentRuntime::new();
    rt.load_source("schema Impossible\n    n ∈ Nat\n    n > 10\n    n < 3\n").unwrap();
    let r = rt.query_free("Impossible").unwrap();
    assert!(!r.satisfied);
}

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

#[test]
fn bool_implies() {
    let mut rt = EvidentRuntime::new();

    rt.load_source("schema P\n    p ∈ Bool\n    q ∈ Bool\n    p = true\n    p ⇒ q\n").unwrap();
    let r = rt.query_free("P").unwrap();
    assert!(r.satisfied);
    assert_eq!(r.bindings.get("q"), Some(&Value::Bool(true)));
}

#[test]
fn string_literal_eq() {
    let mut rt = EvidentRuntime::new();
    rt.load_source("schema S\n    name ∈ String\n    name = \"hello\"\n").unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    assert_eq!(r.bindings.get("name"), Some(&Value::Str("hello".into())));
}

#[test]
fn string_neq_excludes_literal() {
    let mut rt = EvidentRuntime::new();
    rt.load_source("schema S\n    name ∈ String\n    name ≠ \"x\"\n    name = \"y\"\n").unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    assert_eq!(r.bindings.get("name"), Some(&Value::Str("y".into())));
}

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

#[test]
fn set_literal_strings() {
    let mut rt = EvidentRuntime::new();
    rt.load_source("schema S\n    color ∈ String\n    color ∈ {\"red\", \"green\", \"blue\"}\n    color ≠ \"red\"\n    color ≠ \"green\"\n").unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    assert_eq!(r.bindings.get("color"), Some(&Value::Str("blue".into())));
}

#[test]
fn forall_range_unroll() {
    let mut rt = EvidentRuntime::new();

    rt.load_source("schema S\n    a ∈ Int\n    ∀ i ∈ {0..3} : a + i > 0\n").unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    if let Some(Value::Int(a)) = r.bindings.get("a") {
        assert!(*a > 0);
    } else { panic!(); }
}

#[test]
fn exists_range_unroll() {
    let mut rt = EvidentRuntime::new();
    rt.load_source("schema S\n    a ∈ Nat\n    a > 6\n    ∃ i ∈ {0..5} : a = i * 3\n").unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);

    if let Some(Value::Int(a)) = r.bindings.get("a") {
        assert!([9, 12, 15].contains(a), "got {}", a);
    } else { panic!(); }
}

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

#[test]
fn claim_call_sub_schema_mapping() {
    let mut rt = EvidentRuntime::new();
    rt.load_source(

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

#[test]
fn seq_int_basic() {
    let mut rt = EvidentRuntime::new();
    rt.load_source(
        "schema S\n    s ∈ Seq(Int)\n    #s = 3\n    s[0] = 10\n    s[1] = 20\n    s[2] = 30\n"
    ).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    assert_eq!(r.bindings.get("s"), Some(&Value::SeqInt(vec![10, 20, 30])));
}

#[test]
fn seq_bool_basic() {
    let mut rt = EvidentRuntime::new();
    rt.load_source(
        "schema S\n    s ∈ Seq(Bool)\n    #s = 2\n    s[0] = true\n    s[1] = false\n"
    ).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    assert_eq!(r.bindings.get("s"), Some(&Value::SeqBool(vec![true, false])));
}

#[test]
fn seq_string_basic() {
    let mut rt = EvidentRuntime::new();
    rt.load_source(
        "schema S\n    names ∈ Seq(String)\n    #names = 2\n    names[0] = \"alice\"\n    names[1] = \"bob\"\n"
    ).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    assert_eq!(r.bindings.get("names"),
        Some(&Value::SeqStr(vec!["alice".into(), "bob".into()])));
}

#[test]
fn seq_with_quantifier() {
    let mut rt = EvidentRuntime::new();
    rt.load_source(
        "schema S\n    s ∈ Seq(Int)\n    #s = 3\n    \
         s[0] = 5\n    s[1] = 7\n    s[2] = 9\n    \
         ∀ i ∈ {0..2} : s[i] > 0\n"
    ).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    assert_eq!(r.bindings.get("s"), Some(&Value::SeqInt(vec![5, 7, 9])));
}

#[test]
fn set_var_membership_int() {
    let mut rt = EvidentRuntime::new();
    rt.load_source(
        "schema S\n    s ∈ Set(Int)\n    x ∈ Int\n    y ∈ Int\n    \
         x ∈ s\n    y ∈ s\n    x = 5\n    y = 5\n    x = y\n"
    ).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);

    assert_eq!(r.bindings.get("x"), Some(&Value::Int(5)));
    assert_eq!(r.bindings.get("y"), Some(&Value::Int(5)));
}

#[test]
fn set_var_membership_string() {
    let mut rt = EvidentRuntime::new();
    rt.load_source(
        "schema S\n    names ∈ Set(String)\n    name ∈ String\n    \
         name ∈ names\n    name = \"alice\"\n"
    ).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    assert_eq!(r.bindings.get("name"), Some(&Value::Str("alice".into())));
}

#[test]
fn enum_payload_with_seq_int() {
    let mut rt = EvidentRuntime::new();
    rt.load_source(
        "enum Bag = Empty | OfInts(Seq(Int))\n\
         schema S\n    \
         b ∈ Bag\n    \
         b = OfInts(⟨1, 2, 3⟩)\n"
    ).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    match r.bindings.get("b") {
        Some(Value::Enum { variant, fields, .. }) => {
            assert_eq!(variant, "OfInts");
            assert_eq!(fields.len(), 1);
            assert_eq!(fields[0], Value::SeqInt(vec![1, 2, 3]));
        }
        other => panic!("expected OfInts(SeqInt[1,2,3]), got {:?}", other),
    }
}

#[test]
fn enum_payload_with_seq_string() {
    let mut rt = EvidentRuntime::new();
    rt.load_source(
        "enum Pile = Empty | OfStrs(Seq(String))\n\
         schema S\n    \
         p ∈ Pile\n    \
         p = OfStrs(⟨\"a\", \"b\"⟩)\n"
    ).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    match r.bindings.get("p") {
        Some(Value::Enum { variant, fields, .. }) => {
            assert_eq!(variant, "OfStrs");
            assert_eq!(fields.len(), 1);
            assert_eq!(fields[0], Value::SeqStr(vec!["a".into(), "b".into()]));
        }
        other => panic!("expected OfStrs(SeqStr[a,b]), got {:?}", other),
    }
}

#[test]
fn enum_payload_with_seq_enum() {
    let mut rt = EvidentRuntime::new();
    rt.load_source(
        "enum Color = Red | Green | Blue\n\
         enum BoxOfColors = Empty | Many(Seq(Color))\n\
         schema S\n    \
         b ∈ BoxOfColors\n    \
         b = Many(⟨Red, Green, Blue⟩)\n"
    ).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    match r.bindings.get("b") {
        Some(Value::Enum { variant, fields, .. }) => {
            assert_eq!(variant, "Many");
            assert_eq!(fields.len(), 1);
            match &fields[0] {
                Value::SeqEnum(elems) => {
                    assert_eq!(elems.len(), 3);
                    let names: Vec<String> = elems.iter().map(|e| match e {
                        Value::Enum { variant, .. } => variant.clone(),
                        _ => "?".into(),
                    }).collect();
                    assert_eq!(names, vec!["Red", "Green", "Blue"]);
                }
                other => panic!("expected SeqEnum, got {:?}", other),
            }
        }
        other => panic!("expected Many(SeqEnum), got {:?}", other),
    }
}

#[test]
fn seq_enum_extraction() {
    let mut rt = EvidentRuntime::new();
    rt.load_source(
        "enum Color = Red | Green | Blue(Int)\n\
         schema S\n    \
         cs ∈ Seq(Color)\n    \
         #cs = 3\n    \
         cs[0] = Red\n    \
         cs[1] = Green\n    \
         cs[2] = Blue(42)\n"
    ).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    let cs = r.bindings.get("cs").expect("cs should be extracted");
    match cs {
        Value::SeqEnum(elems) => {
            assert_eq!(elems.len(), 3);
            match &elems[0] {
                Value::Enum { variant, fields, .. } => {
                    assert_eq!(variant, "Red");
                    assert!(fields.is_empty());
                }
                other => panic!("expected Red enum at [0], got {:?}", other),
            }
            match &elems[1] {
                Value::Enum { variant, fields, .. } => {
                    assert_eq!(variant, "Green");
                    assert!(fields.is_empty());
                }
                other => panic!("expected Green enum at [1], got {:?}", other),
            }
            match &elems[2] {
                Value::Enum { variant, fields, .. } => {
                    assert_eq!(variant, "Blue");
                    assert_eq!(fields.len(), 1);
                    assert_eq!(fields[0], Value::Int(42));
                }
                other => panic!("expected Blue(42) at [2], got {:?}", other),
            }
        }
        other => panic!("expected SeqEnum, got {:?}", other),
    }
}

#[test]
fn set_int_literal_pinning() {
    let mut rt = EvidentRuntime::new();
    rt.load_source(
        "schema S\n    s ∈ Set(Int)\n    s = {1, 2, 3}\n"
    ).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    assert_eq!(r.bindings.get("s"), Some(&Value::SetInt(vec![1, 2, 3])));
}

#[test]
fn set_string_literal_pinning() {
    let mut rt = EvidentRuntime::new();
    rt.load_source(
        "schema S\n    flags ∈ Set(String)\n    flags = {\"a\", \"b\"}\n"
    ).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    assert_eq!(r.bindings.get("flags"),
        Some(&Value::SetStr(vec!["a".into(), "b".into()])));
}

#[test]
fn set_bool_literal_pinning() {
    let mut rt = EvidentRuntime::new();
    rt.load_source(
        "schema S\n    bs ∈ Set(Bool)\n    bs = {true, false}\n"
    ).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    let extracted = r.bindings.get("bs");

    assert_eq!(extracted, Some(&Value::SetBool(vec![true, false])));
}

#[test]
fn set_no_candidates_omits_binding() {
    let mut rt = EvidentRuntime::new();
    rt.load_source(
        "schema S\n    s ∈ Set(Int)\n    x ∈ Int\n    x = 5\n    x ∈ s\n"
    ).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    assert_eq!(r.bindings.get("x"), Some(&Value::Int(5)));

    assert!(r.bindings.get("s").is_none(),
        "free SetVar should have no extracted binding, got {:?}",
        r.bindings.get("s"));
}

#[test]
fn set_literal_is_exact_membership() {
    let mut rt = EvidentRuntime::new();
    rt.load_source(
        "schema S\n    s ∈ Set(Int)\n    s = {1, 2}\n    99 ∈ s\n"
    ).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(!r.satisfied, "99 ∈ {{1, 2}} should be unsat");
}

#[test]
fn forall_symbolic_bound_via_pinned_var() {
    let mut rt = EvidentRuntime::new();
    rt.load_source(
        "schema S\n    n ∈ Nat\n    n = 4\n    s ∈ Seq(Int)\n    #s = n\n    \
         ∀ i ∈ {0..n - 1} : s[i] = i + 10\n"
    ).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    assert_eq!(r.bindings.get("n"), Some(&Value::Int(4)));
    assert_eq!(r.bindings.get("s"), Some(&Value::SeqInt(vec![10, 11, 12, 13])));
}

#[test]
fn forall_symbolic_bound_via_length_propagation() {
    let mut rt = EvidentRuntime::new();
    rt.load_source(
        "schema S\n    n ∈ Nat\n    s ∈ Seq(Int)\n    #s = 3\n    n = #s\n    \
         ∀ i ∈ {0..n - 1} : s[i] = 100\n"
    ).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    assert_eq!(r.bindings.get("n"), Some(&Value::Int(3)));
    assert_eq!(r.bindings.get("s"), Some(&Value::SeqInt(vec![100, 100, 100])));
}

#[test]
fn forall_symbolic_bound_from_given() {
    use std::collections::HashMap;
    let mut rt = EvidentRuntime::new();
    rt.load_source(
        "schema S\n    n ∈ Nat\n    s ∈ Seq(Int)\n    #s = n\n    \
         ∀ i ∈ {0..n - 1} : s[i] = i * 2\n"
    ).unwrap();
    let mut g = HashMap::new();
    g.insert("n".to_string(), Value::Int(5));
    let r = rt.query("S", &g).unwrap();
    assert!(r.satisfied);
    assert_eq!(r.bindings.get("s"), Some(&Value::SeqInt(vec![0, 2, 4, 6, 8])));
}

#[test]
fn seq_cardinality_in_arithmetic() {
    let mut rt = EvidentRuntime::new();
    rt.load_source(
        "schema S\n    s ∈ Seq(Int)\n    #s + 1 = 5\n    s[0] = 100\n"
    ).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    if let Some(Value::SeqInt(v)) = r.bindings.get("s") {
        assert_eq!(v.len(), 4);
        assert_eq!(v[0], 100);
    } else { panic!(); }
}

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

#[test]
fn seq_composite_field_access() {
    let mut rt = EvidentRuntime::new();
    rt.load_source(
        "type Point\n    x ∈ Int\n    y ∈ Int\n\
         schema S\n    pts ∈ Seq(Point)\n    #pts = 3\n    \
         pts[0].x = 10\n    pts[1].x = 20\n    pts[2].x = 30\n"
    ).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied, "expected SAT, got UNSAT");
    let pts = r.bindings.get("pts").expect("missing pts binding");
    let elems = match pts {
        Value::SeqComposite(v) => v,
        other => panic!("expected SeqComposite, got {:?}", other),
    };
    assert_eq!(elems.len(), 3, "expected 3 elements, got {}", elems.len());
    assert_eq!(elems[0].get("x"), Some(&Value::Int(10)));
    assert_eq!(elems[1].get("x"), Some(&Value::Int(20)));
    assert_eq!(elems[2].get("x"), Some(&Value::Int(30)));

    assert!(elems[0].contains_key("y"));
    assert!(elems[1].contains_key("y"));
    assert!(elems[2].contains_key("y"));
}

#[test]
fn seq_composite_with_quantifier() {
    let mut rt = EvidentRuntime::new();
    rt.load_source(
        "type Point\n    x ∈ Int\n    y ∈ Int\n\
         schema S\n    pts ∈ Seq(Point)\n    #pts = 3\n    \
         ∀ i ∈ {0..2} : pts[i].x > 0\n    \
         pts[0].x = 5\n    pts[1].x = 7\n    pts[2].x = 9\n"
    ).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    let pts = r.bindings.get("pts").expect("missing pts binding");
    let elems = match pts {
        Value::SeqComposite(v) => v,
        other => panic!("expected SeqComposite, got {:?}", other),
    };
    assert_eq!(elems.len(), 3);
    for (i, expected) in [5, 7, 9].iter().enumerate() {
        match elems[i].get("x") {
            Some(Value::Int(n)) => {
                assert!(*n > 0, "elem {} x not positive: {}", i, n);
                assert_eq!(*n, *expected, "elem {} x mismatch", i);
            }
            other => panic!("elem {} missing/typed x: {:?}", i, other),
        }
    }
}

#[test]
fn seq_nested_composite_extracts() {
    let mut rt = EvidentRuntime::new();
    rt.load_source(
        "type Color\n    r ∈ Nat\n    g ∈ Nat\n    b ∈ Nat\n\
         type Rect\n    x ∈ Int\n    y ∈ Int\n    color ∈ Color\n\
         schema S\n    rs ∈ Seq(Rect)\n    #rs = 2\n    \
         rs[0].x = 10\n    rs[0].color.r = 255\n    \
         rs[1].x = 20\n    rs[1].color.r = 0\n"
    ).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied, "expected SAT, got UNSAT");
    let rs = r.bindings.get("rs").expect("missing rs binding");
    let elems = match rs {
        Value::SeqComposite(v) => v,
        other => panic!("expected SeqComposite, got {:?}", other),
    };
    assert_eq!(elems.len(), 2);

    assert_eq!(elems[0].get("x"), Some(&Value::Int(10)));
    let c0 = match elems[0].get("color") {
        Some(Value::Composite(m)) => m,
        other => panic!("elem 0 color not Composite: {:?}", other),
    };
    assert_eq!(c0.get("r"), Some(&Value::Int(255)));
    assert!(c0.contains_key("g"));
    assert!(c0.contains_key("b"));

    assert_eq!(elems[1].get("x"), Some(&Value::Int(20)));
    let c1 = match elems[1].get("color") {
        Some(Value::Composite(m)) => m,
        other => panic!("elem 1 color not Composite: {:?}", other),
    };
    assert_eq!(c1.get("r"), Some(&Value::Int(0)));
}

#[test]
fn seq_nested_composite_with_quantifier() {
    let mut rt = EvidentRuntime::new();
    rt.load_source(
        "type Color\n    r ∈ Nat\n    g ∈ Nat\n    b ∈ Nat\n\
         type Rect\n    x ∈ Int\n    y ∈ Int\n    color ∈ Color\n\
         schema S\n    rs ∈ Seq(Rect)\n    #rs = 2\n    \
         ∀ i ∈ {0..1} : rs[i].color.r ≥ 0\n    \
         rs[0].color.r = 100\n    rs[1].color.r = 200\n"
    ).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    let rs = r.bindings.get("rs").expect("missing rs binding");
    let elems = match rs {
        Value::SeqComposite(v) => v,
        other => panic!("expected SeqComposite, got {:?}", other),
    };
    assert_eq!(elems.len(), 2);
    let r0 = match elems[0].get("color") {
        Some(Value::Composite(m)) => m.get("r").cloned(),
        _ => panic!("elem 0 color"),
    };
    let r1 = match elems[1].get("color") {
        Some(Value::Composite(m)) => m.get("r").cloned(),
        _ => panic!("elem 1 color"),
    };
    assert_eq!(r0, Some(Value::Int(100)));
    assert_eq!(r1, Some(Value::Int(200)));
}

#[test]
fn nested_composite_shared_across_siblings() {
    let mut rt = EvidentRuntime::new();
    rt.load_source(
        "type Color\n    r ∈ Nat\n    g ∈ Nat\n    b ∈ Nat\n\
         type SDLRect\n    x ∈ Int\n    y ∈ Int\n    w ∈ Nat\n    h ∈ Nat\n    color ∈ Color\n\
         type SDLOutput\n    bg ∈ Color\n    rects ∈ Seq(SDLRect)\n\
         schema S\n    output ∈ SDLOutput\n    \
         output.bg.r = 255\n    output.bg.g = 0\n    output.bg.b = 0\n    \
         #output.rects = 1\n    \
         output.rects[0].x = 5\n    output.rects[0].color.r = 128\n"
    ).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied, "expected SAT, got UNSAT");

    assert_eq!(r.bindings.get("output.bg.r"), Some(&Value::Int(255)));
    assert_eq!(r.bindings.get("output.bg.g"), Some(&Value::Int(0)));
    assert_eq!(r.bindings.get("output.bg.b"), Some(&Value::Int(0)));

    let rects = r.bindings.get("output.rects").expect("missing output.rects");
    let elems = match rects {
        Value::SeqComposite(v) => v,
        other => panic!("expected SeqComposite, got {:?}", other),
    };
    assert_eq!(elems.len(), 1);
    assert_eq!(elems[0].get("x"), Some(&Value::Int(5)));
    let color = match elems[0].get("color") {
        Some(Value::Composite(m)) => m,
        other => panic!("elem 0 color: {:?}", other),
    };
    assert_eq!(color.get("r"), Some(&Value::Int(128)));
}

#[test]
fn passthrough_composes_claim_body() {
    let mut rt = EvidentRuntime::new();
    rt.load_source(
        "claim is_positive\n    n ∈ Nat\n    n > 0\n\
         schema S\n    n ∈ Nat\n    ..is_positive\n    n < 10\n"
    ).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    if let Some(Value::Int(v)) = r.bindings.get("n") {
        assert!(*v > 0 && *v < 10);
    } else { panic!(); }
}

#[test]
fn bare_bool_var_still_works_after_passthrough_change() {
    let mut rt = EvidentRuntime::new();
    rt.load_source("schema S\n    flag ∈ Bool\n    flag\n").unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    match r.bindings.get("flag") {
        Some(Value::Bool(true)) => {}
        other => panic!("expected flag=true, got {:?}", other),
    }
}

#[test]
fn seq_literal_int_assignment() {
    let mut rt = EvidentRuntime::new();
    rt.load_source(
        "schema S\n    s ∈ Seq(Int)\n    s = ⟨10, 20, 30⟩\n"
    ).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    assert_eq!(r.bindings.get("s"), Some(&Value::SeqInt(vec![10, 20, 30])));
}

#[test]
fn seq_literal_with_arithmetic() {
    let mut rt = EvidentRuntime::new();
    rt.load_source(
        "schema S\n    s ∈ Seq(Int)\n    n ∈ Nat\n    n = 5\n    \
         s = ⟨n, n + 1, n + 2⟩\n"
    ).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    assert_eq!(r.bindings.get("s"), Some(&Value::SeqInt(vec![5, 6, 7])));
}

#[test]
fn seq_literal_empty() {
    let mut rt = EvidentRuntime::new();
    rt.load_source(
        "schema S\n    s ∈ Seq(Int)\n    s = ⟨⟩\n"
    ).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    assert_eq!(r.bindings.get("s"), Some(&Value::SeqInt(vec![])));
}

#[test]
fn string_concat_basic() {
    let mut rt = EvidentRuntime::new();
    rt.load_source(
        "schema S\n    a ∈ String\n    b ∈ String\n    c ∈ String\n    \
         a = \"hello\"\n    b = \"world\"\n    c = a ++ \" \" ++ b\n"
    ).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    assert_eq!(r.bindings.get("c"), Some(&Value::Str("hello world".into())));
}

#[test]
fn not_in_set_literal() {
    let mut rt = EvidentRuntime::new();
    rt.load_source(
        "schema S\n    n ∈ Nat\n    n ∉ {1, 2, 3}\n    n < 6\n"
    ).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    if let Some(Value::Int(n)) = r.bindings.get("n") {
        assert!(![1, 2, 3].contains(n) && *n < 6, "got {}", n);
    } else { panic!(); }
}

#[test]
fn contains_rev_set_literal() {
    let mut rt = EvidentRuntime::new();
    rt.load_source(
        "schema S\n    n ∈ Nat\n    {1, 2, 3} ∋ n\n    n > 1\n"
    ).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    if let Some(Value::Int(n)) = r.bindings.get("n") {
        assert!([2, 3].contains(n));
    } else { panic!(); }
}

#[test]
fn seq_literal_composite_assignment() {
    let mut rt = EvidentRuntime::new();
    rt.load_source(
        "type Point\n    x ∈ Int\n    y ∈ Int\n\
         schema S\n    pts ∈ Seq(Point)\n    p1 ∈ Point\n    p2 ∈ Point\n    \
         p1.x = 10\n    p1.y = 20\n    p2.x = 30\n    p2.y = 40\n    \
         pts = ⟨p1, p2⟩\n"
    ).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    let pts = match r.bindings.get("pts") {
        Some(Value::SeqComposite(v)) => v,
        other => panic!("expected SeqComposite, got {:?}", other),
    };
    assert_eq!(pts.len(), 2);
    assert_eq!(pts[0].get("x"), Some(&Value::Int(10)));
    assert_eq!(pts[0].get("y"), Some(&Value::Int(20)));
    assert_eq!(pts[1].get("x"), Some(&Value::Int(30)));
    assert_eq!(pts[1].get("y"), Some(&Value::Int(40)));
}

#[test]
fn seq_literal_nested_composite_assignment() {
    let mut rt = EvidentRuntime::new();
    rt.load_source(
        "type Color\n    r ∈ Nat\n    g ∈ Nat\n    b ∈ Nat\n\
         type Rect\n    x ∈ Int\n    color ∈ Color\n\
         schema S\n    rs ∈ Seq(Rect)\n    rect ∈ Rect\n    \
         rect.x = 10\n    rect.color.r = 255\n    rect.color.g = 0\n    rect.color.b = 0\n    \
         rs = ⟨rect⟩\n"
    ).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    let rs = match r.bindings.get("rs") {
        Some(Value::SeqComposite(v)) => v,
        other => panic!("expected SeqComposite, got {:?}", other),
    };
    assert_eq!(rs.len(), 1);
    assert_eq!(rs[0].get("x"), Some(&Value::Int(10)));
    let color = match rs[0].get("color") {
        Some(Value::Composite(m)) => m,
        other => panic!("expected nested Color Composite, got {:?}", other),
    };
    assert_eq!(color.get("r"), Some(&Value::Int(255)));
}

#[test]
fn forall_over_cardinality_of_composite_seq() {
    let mut rt = EvidentRuntime::new();
    rt.load_source(
        "type Item\n    val ∈ Int\n\
         schema S\n    items ∈ Seq(Item)\n    #items = 4\n    \
         ∀ i ∈ {0..#items - 1} : items[i].val = i * 10\n"
    ).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    let items = match r.bindings.get("items") {
        Some(Value::SeqComposite(v)) => v,
        other => panic!("expected SeqComposite, got {:?}", other),
    };
    assert_eq!(items.len(), 4);
    for i in 0..4 {
        assert_eq!(items[i].get("val"), Some(&Value::Int(i as i64 * 10)));
    }
}

#[test]
fn claim_call_invoked_twice_uses_distinct_internals() {
    let mut rt = EvidentRuntime::new();
    rt.load_source(

        "claim SetVal\n    out ∈ Int\n    target ∈ Int\n    internal ∈ Int\n    \
         internal = target * 2\n    out = internal\n\
         schema S\n    a ∈ Int\n    b ∈ Int\n    \
         SetVal (out mapsto a, target mapsto 5)\n    \
         SetVal (out mapsto b, target mapsto 9)\n"
    ).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    assert_eq!(r.bindings.get("a"), Some(&Value::Int(10)));
    assert_eq!(r.bindings.get("b"), Some(&Value::Int(18)));
}

#[test]
fn seq_index_assign_composite_var() {
    let mut rt = EvidentRuntime::new();
    rt.load_source(
        "type Point\n    x ∈ Int\n    y ∈ Int\n\
         schema S\n    pts ∈ Seq(Point)\n    p ∈ Point\n    \
         #pts = 3\n    p.x = 99\n    p.y = 100\n    pts[2] = p\n"
    ).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    let pts = match r.bindings.get("pts") {
        Some(Value::SeqComposite(v)) => v,
        other => panic!("expected SeqComposite, got {:?}", other),
    };
    assert_eq!(pts.len(), 3);
    assert_eq!(pts[2].get("x"), Some(&Value::Int(99)));
    assert_eq!(pts[2].get("y"), Some(&Value::Int(100)));
}

#[test]
fn chained_comparisons_basic() {
    let mut rt = EvidentRuntime::new();
    rt.load_source(
        "schema S\n    x ∈ Int\n    y ∈ Int\n    \
         20 ≤ x ≤ 100\n    -5 < y ≤ 5\n    x = 50\n    y = 0\n"
    ).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    assert_eq!(r.bindings.get("x"), Some(&Value::Int(50)));
    assert_eq!(r.bindings.get("y"), Some(&Value::Int(0)));
}

#[test]
fn chained_comparisons_triple() {
    let mut rt = EvidentRuntime::new();
    rt.load_source(
        "schema S\n    a ∈ Int\n    b ∈ Int\n    c ∈ Int\n    d ∈ Int\n    \
         a ≤ b ≤ c ≤ d\n    a = 1\n    b = 2\n    c = 3\n    d = 4\n"
    ).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
}

#[test]
fn chained_comparison_unsat() {
    let mut rt = EvidentRuntime::new();
    rt.load_source(
        "schema S\n    x ∈ Int\n    1 ≤ x ≤ 10\n    x = 99\n"
    ).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(!r.satisfied);
}

#[test]
fn forall_iter_composite_seq() {
    let mut rt = EvidentRuntime::new();
    rt.load_source(
        "type Dot\n    pos_x ∈ Int\n    pos_y ∈ Int\n\
         schema S\n    dots ∈ Seq(Dot)\n    #dots = 3\n    \
         ∀ dot ∈ dots :\n        dot.pos_x ≥ 0\n        dot.pos_x ≤ 100\n        dot.pos_y = dot.pos_x * 2\n"
    ).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    let dots = match r.bindings.get("dots") {
        Some(Value::SeqComposite(v)) => v,
        other => panic!("expected SeqComposite, got {:?}", other),
    };
    assert_eq!(dots.len(), 3);
    for d in dots {
        let x = match d.get("pos_x") { Some(Value::Int(n)) => *n, _ => panic!() };
        let y = match d.get("pos_y") { Some(Value::Int(n)) => *n, _ => panic!() };
        assert!(x >= 0 && x <= 100);
        assert_eq!(y, x * 2);
    }
}

#[test]
fn forall_iter_primitive_seq() {
    let mut rt = EvidentRuntime::new();
    rt.load_source(
        "schema S\n    s ∈ Seq(Int)\n    #s = 4\n    \
         ∀ x ∈ s : x ≥ 10 ∧ x ≤ 20\n"
    ).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    let s = match r.bindings.get("s") {
        Some(Value::SeqInt(v)) => v,
        other => panic!("expected SeqInt, got {:?}", other),
    };
    assert_eq!(s.len(), 4);
    for x in s { assert!(*x >= 10 && *x <= 20); }
}

#[test]
fn forall_indent_block_body() {
    let mut rt = EvidentRuntime::new();
    rt.load_source(
        "schema S\n    s ∈ Seq(Int)\n    #s = 4\n    \
         ∀ i ∈ {0..3} :\n        s[i] ≥ 0\n        s[i] ≤ 100\n        s[i] = i * 5\n"
    ).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    assert_eq!(r.bindings.get("s"), Some(&Value::SeqInt(vec![0, 5, 10, 15])));
}

#[test]
fn multi_line_expression_inside_parens() {
    let mut rt = EvidentRuntime::new();
    rt.load_source(

        "schema S\n    a ∈ Bool\n    b ∈ Bool\n    c ∈ Bool\n    \
         x ∈ Bool\n    a = true\n    b = false\n    c = false\n    \
         x = (\n        a\n        ∨ b\n        ∨ c\n    )\n"
    ).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    assert_eq!(r.bindings.get("x"), Some(&Value::Bool(true)));
}

#[test]
fn multi_line_seq_literal() {
    let mut rt = EvidentRuntime::new();
    rt.load_source(
        "schema S\n    s ∈ Seq(Int)\n    \
         s = ⟨\n        10,\n        20,\n        30\n    ⟩\n"
    ).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    assert_eq!(r.bindings.get("s"), Some(&Value::SeqInt(vec![10, 20, 30])));
}

#[test]
fn seq_length_chain_via_cardinality_eq() {
    let mut rt = EvidentRuntime::new();
    rt.load_source(
        "schema S\n    a ∈ Seq(Int)\n    b ∈ Seq(Int)\n    \
         #a = 5\n    #b = #a\n    \
         ∀ i ∈ {0..#b - 1} : b[i] = i * 10\n"
    ).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    assert_eq!(r.bindings.get("b"), Some(&Value::SeqInt(vec![0, 10, 20, 30, 40])));
}

#[test]
fn seq_length_chain_arithmetic() {
    let mut rt = EvidentRuntime::new();
    rt.load_source(
        "schema S\n    a ∈ Seq(Int)\n    b ∈ Seq(Int)\n    c ∈ Seq(Int)\n    \
         #a = 4\n    #b = #a\n    #c = #b + 1\n    \
         ∀ i ∈ {0..#c - 1} : c[i] = i\n"
    ).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    assert_eq!(r.bindings.get("c"), Some(&Value::SeqInt(vec![0, 1, 2, 3, 4])));
}

#[test]
fn implies_consequent_can_be_forall() {
    let mut rt = EvidentRuntime::new();
    rt.load_source(
        "schema S\n    n ∈ Nat\n    s ∈ Seq(Int)\n    \
         #s = 4\n    n = 1\n    \
         n = 1 ⇒ ∀ i ∈ {0..3} : s[i] = i + 10\n"
    ).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    assert_eq!(r.bindings.get("s"), Some(&Value::SeqInt(vec![10, 11, 12, 13])));
}
