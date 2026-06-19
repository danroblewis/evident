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

/// `Seq(Int)` declared, length and indexed access constrained.
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

/// `Seq(Bool)` round-trip.
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

/// `Seq(String)` round-trip.
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

/// `∀ i ∈ {0..2} : s[i] > 0` plus a length constraint, with elements
/// constrained per-index by other rules.
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

/// Z3 Set sort: `s ∈ Set(Int)` declared as a Z3 Set, `x ∈ s`
/// translates to `set.member(x)`. We don't extract set values into
/// bindings (Z3 sets are functions over the element domain, not
/// finite containers); we just use them for membership queries.
#[test]
fn set_var_membership_int() {
    let mut rt = EvidentRuntime::new();
    rt.load_source(
        "schema S\n    s ∈ Set(Int)\n    x ∈ Int\n    y ∈ Int\n    \
         x ∈ s\n    y ∈ s\n    x = 5\n    y = 5\n    x = y\n"
    ).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    // s itself isn't extracted (no SetVar binding); x and y are pinned.
    assert_eq!(r.bindings.get("x"), Some(&Value::Int(5)));
    assert_eq!(r.bindings.get("y"), Some(&Value::Int(5)));
}

/// Set membership of a string in a string-set.
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
/// Seq-typed enum-variant payload: `enum Bag = Empty | OfInts(Seq(Int))`
/// with `b = OfInts(⟨1, 2, 3⟩)`. Verifies the parser, multi-stage
/// datatype batching, two-accessor expansion, constructor
/// application, and extraction paths all line up.
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

/// Same shape, Seq(String) payload — confirms the String element
/// path works alongside Int.
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

/// Seq-of-enum payload via SeqLit: `enum Box = Many(Seq(Color))`
/// with `b = Many(⟨Red, Green, Blue⟩)`. translate_seq_arg_for_ctor
/// builds an Array via Array::fresh_const + successive stores of
/// the resolved enum constructor values; extract_seq_enum reads
/// back via the two-accessor expansion.
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

/// `Seq(EnumType)` extraction — declare a seq of enum-typed
/// elements, pin specific variants, query, expect Value::SeqEnum
/// with each element's variant + payload preserved.
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

/// `Set(Int)` extraction via literal pinning. `s = {1, 2, 3}` records
/// the candidates and `extract_set` walks them, asking the model for
/// membership of each.
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

/// `Set(String)` extraction via literal pinning.
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

/// `Set(Bool)` extraction via literal pinning.
#[test]
fn set_bool_literal_pinning() {
    let mut rt = EvidentRuntime::new();
    rt.load_source(
        "schema S\n    bs ∈ Set(Bool)\n    bs = {true, false}\n"
    ).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    let extracted = r.bindings.get("bs");
    // Order in the Vec mirrors literal declaration order.
    assert_eq!(extracted, Some(&Value::SetBool(vec![true, false])));
}

/// A free SetVar (declared but never pinned to a literal) should extract
/// to a missing binding — back-compat with pre-Phase-6.1 behavior.
#[test]
fn set_no_candidates_omits_binding() {
    let mut rt = EvidentRuntime::new();
    rt.load_source(
        "schema S\n    s ∈ Set(Int)\n    x ∈ Int\n    x = 5\n    x ∈ s\n"
    ).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    assert_eq!(r.bindings.get("x"), Some(&Value::Int(5)));
    // s isn't pinned to a literal → no candidates → no Value::Set* binding.
    assert!(r.bindings.get("s").is_none(),
        "free SetVar should have no extracted binding, got {:?}",
        r.bindings.get("s"));
}

/// Pinning a Set to a literal that contains a value also asserts
/// EXACT membership — the set cannot contain other elements. Verify
/// by attempting a membership constraint on a non-member; it should
/// be unsat.
#[test]
fn set_literal_is_exact_membership() {
    let mut rt = EvidentRuntime::new();
    rt.load_source(
        "schema S\n    s ∈ Set(Int)\n    s = {1, 2}\n    99 ∈ s\n"
    ).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(!r.satisfied, "99 ∈ {{1, 2}} should be unsat");
}

/// equality (`n = 4`) should unroll into 4 instances.
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

/// Length-propagation: `n = #s` and `#s = 5` together pin n.
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

/// Symbolic bound from a `given` value (the key per-step path that the
/// Python executor needs).
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

/// Length-of-sequence in arithmetic: `#s + 1 = 5` should pin length to 4.
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

/// `Seq(UserType)` — element sort is a Z3 Datatype built from the
/// user type's flat fields. Field access on indexed elements
/// (`pts[i].x`) routes through the Datatype's accessors. The model
/// extracts a `Value::SeqComposite` of per-element field maps.
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
    // y is unconstrained — just verify it appears in each element.
    assert!(elems[0].contains_key("y"));
    assert!(elems[1].contains_key("y"));
    assert!(elems[2].contains_key("y"));
}

/// Quantifier over composite-seq indices: `∀ i ∈ {0..2} : pts[i].x > 0`.
/// Verifies that field access works inside a quantifier body and that
/// the resulting model satisfies the per-element field constraint.
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

/// `Seq(UserType)` where the element type itself contains a nested
/// composite field — Color is its own struct, Rect.color references
/// Color. The Datatype builder should recurse to build Color first,
/// then build Rect with `color` as a `DatatypeAccessor::Sort(Color.sort)`.
/// Field-access chains like `rs[0].color.r` should resolve through
/// the nested accessor.
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
    // Check first element: x=10, color.r=255.
    assert_eq!(elems[0].get("x"), Some(&Value::Int(10)));
    let c0 = match elems[0].get("color") {
        Some(Value::Composite(m)) => m,
        other => panic!("elem 0 color not Composite: {:?}", other),
    };
    assert_eq!(c0.get("r"), Some(&Value::Int(255)));
    assert!(c0.contains_key("g"));
    assert!(c0.contains_key("b"));
    // Check second element: x=20, color.r=0.
    assert_eq!(elems[1].get("x"), Some(&Value::Int(20)));
    let c1 = match elems[1].get("color") {
        Some(Value::Composite(m)) => m,
        other => panic!("elem 1 color not Composite: {:?}", other),
    };
    assert_eq!(c1.get("r"), Some(&Value::Int(0)));
}

/// Quantifier ranges over composite-seq indices with nested-field
/// access in the body. `rs[i].color.r ≥ 0` should unroll to two
/// constraints (Color.r is a Nat so it's already trivially true,
/// but the test is mostly about the parse-translate-extract path
/// working end-to-end).
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

/// Sibling user types share a nested composite field type — Color is
/// referenced from both SDLRect.color and SDLOutput.bg. Both top-level
/// composite (sub-schema expansion) and seq-element composite paths
/// should produce a working program.
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
    // Top-level sub-schema expansion still applies for output.bg.*.
    assert_eq!(r.bindings.get("output.bg.r"), Some(&Value::Int(255)));
    assert_eq!(r.bindings.get("output.bg.g"), Some(&Value::Int(0)));
    assert_eq!(r.bindings.get("output.bg.b"), Some(&Value::Int(0)));
    // Seq-of-composite extracts as SeqComposite under output.rects.
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

/// `..ClaimName` (explicit passthrough) composes a claim's body into
/// the parent. This used to be testable as a bare `ClaimName` constraint
/// too, but bare-ident → passthrough is now a CLI-level desugar pass
/// (`stdlib/passes/desugar_passthrough.ev` + `commands/desugar.rs`)
/// that doesn't run on direct `load_source`. The bare-name case is
/// covered end-to-end in `tests/desugar_passthrough.rs`; this test keeps
/// the explicit-form coverage.
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

/// Negative case: a bare ident that is NOT a known claim routes through
/// the existing bool-bare-ident translation. `flag` here is a Bool
/// variable; naming it as a body item asserts `flag = true`. The CLI
/// desugar pass also leaves this shape alone (the "is name a known
/// schema" filter rejects it), so the behavior is the same with or
/// without that pass running.
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

/// `s = ⟨10, 20, 30⟩` should pin both length and per-element values.
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

/// Sequence-literal items can be arbitrary expressions, not just literals.
/// `n = 5` pins n; the literal `⟨n, n+1, n+2⟩` then becomes ⟨5, 6, 7⟩.
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

/// Empty sequence literal `⟨⟩` should pin length to 0.
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

/// `++` — string concatenation. Chained left-associative: `a ++ " " ++ b`
/// parses as `(a ++ " ") ++ b`. Pinning a and b lets Z3 derive c.
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

/// `∉` — non-membership; desugars to `¬(lhs ∈ rhs)`.
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

/// `∋` — reverse membership; `set ∋ x` means `x ∈ set`.
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

/// `Seq(UserType)` LHS with bare-Identifier items in the literal: each
/// item names a flat-expanded composite (`p1.x`, `p1.y`, `p2.x`, …) and
/// the runtime assembles a Datatype constructor application per element.
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

/// Mirrors SDL: Color nested inside Rect, Rect inside Seq(Rect). The
/// translator has to recurse into the nested FieldKind to assemble the
/// inner Color constructor before applying the outer Rect constructor.
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

/// `#seq` should fold to a literal int via `apply_seq_lengths`, so
/// quantifiers ranging over `0..#seq - 1` unroll. Regression for
/// scatter.ev's per-dot ∀ loops, which previously dropped because
/// Cardinality stayed symbolic. Also verifies the Membership
/// idempotence guard in declare_var: without it, the passthrough
/// re-declares state.dots and wipes the literal len.
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

/// Two ClaimCalls to the same claim must use distinct Z3 vars for the
/// claim's unmapped internal parameters. Regression for the
/// anchor_collect.ev black-screen bug: PlayerPhysics calls AxisPhysics
/// twice (once per axis), and the two invocations both had Memberships
/// for `intended` and `target`. Without per-call fresh Z3 names, both
/// calls' `intended` mapped to the SAME Z3 const — the x-axis branch
/// wanted `intended = 0` (no horizontal accel) and the y-axis branch
/// wanted `intended = 0` too in this specific case, but in any
/// scenario where the two axes' inputs differ, they contradicted →
/// UNSAT every step → renderer fell back to all-zero/black.
#[test]
fn claim_call_invoked_twice_uses_distinct_internals() {
    let mut rt = EvidentRuntime::new();
    rt.load_source(
        // SetVal exposes one Int output; `internal` is unmapped — each
        // call must get its own. We invoke SetVal twice with
        // different `out` slots and different desired values; without
        // the fresh-name fix, `internal` collides → UNSAT.
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

/// `seq[i] = composite_var` — single-element composite assignment into
/// a `Seq(UserType)` slot, where `composite_var` is a flat-expanded
/// sub-schema instance. Regression for the player-rect-placement line
/// `output.rects[#state.dots] = player_rect` in the dot-collect engine.
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

/// `20 ≤ x ≤ 100` desugars to `(20 ≤ x) ∧ (x ≤ 100)` — standard math
/// notation, matches Python's parser. Mixed-operator chains
/// (`a < b ≤ c`) work the same way: each adjacent pair becomes a
/// constraint, all AND-combined.
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

/// Triple chain: `a ≤ b ≤ c ≤ d` produces three pairwise constraints
/// AND-combined: (a ≤ b) ∧ (b ≤ c) ∧ (c ≤ d).
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

/// Chain that should be UNSAT: `1 ≤ x ≤ 10` with `x = 99`.
#[test]
fn chained_comparison_unsat() {
    let mut rt = EvidentRuntime::new();
    rt.load_source(
        "schema S\n    x ∈ Int\n    1 ≤ x ≤ 10\n    x = 99\n"
    ).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(!r.satisfied);
}

/// `∀ var ∈ <composite-seq>` iterates over the seq's elements, with
/// `var.field` resolving to the corresponding field of each element.
/// Same shape as `∀ i ∈ {0..#seq - 1} : seq[i].field` but reads as
/// what it does. Regression for the user's `∀ dot ∈ state.dots` ask.
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

/// `∀ x ∈ <primitive-seq>` iterates a Seq(Int)/Seq(Bool)/Seq(String);
/// the bound var holds the element directly (not a composite).
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

/// `∀`/`∃` accept an indent-block body the same way `⇒` does. Lets
/// users write multi-constraint quantifiers as
///
///   ∀ i ∈ {0..3} :
///       constraint_a
///       constraint_b
///       constraint_c
///
/// instead of repeating `∀ i ∈ {0..3} : …` per constraint.
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

/// Multi-line expressions inside `(...)`/`[...]`/`{...}`/`⟨...⟩` —
/// the lexer suppresses Newline + Indent tokens whenever bracket
/// depth > 0, so a single logical expression can span any number of
/// source lines as long as it's enclosed. Mirrors Lark's default
/// "newlines inside parens are ignored" behavior.
///
/// Without this, the parser sees `Eq` followed by `Newline` and
/// errors with "expected expression, got Newline".
#[test]
fn multi_line_expression_inside_parens() {
    let mut rt = EvidentRuntime::new();
    rt.load_source(
        // The disjunction body is split across 5 source lines but
        // lives inside one (...) group.
        "schema S\n    a ∈ Bool\n    b ∈ Bool\n    c ∈ Bool\n    \
         x ∈ Bool\n    a = true\n    b = false\n    c = false\n    \
         x = (\n        a\n        ∨ b\n        ∨ c\n    )\n"
    ).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    assert_eq!(r.bindings.get("x"), Some(&Value::Bool(true)));
}

/// Multi-line inside `⟨…⟩` — sequence literal with one element per
/// line. Same suppression rule applies to all four bracket flavors
/// (paren, square, brace, angle).
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

/// `#b = #a` chains seq-length pinning. Without this, only `#a` is
/// known and the quantifier `∀ i ∈ {0..#b - 1}` silently drops
/// because the upper bound stays symbolic. Natural shape for
/// state-forwarding: `#state_next.cells = #state.cells`.
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

/// Multi-hop chain: `#c = #b + 1` after `#b = #a` after `#a = 4`.
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


/// `∀`/`∃` are valid expressions wherever `⇒` is. Regression for the
/// rule30.ev demo: `state.step = 0 ⇒ ∀ i ∈ {0..N} : seed[i] = ...`
/// previously failed with "expected expression, got ForAll" because
/// parse_implies recursed into parse_or for the RHS, and parse_or
/// didn't know about quantifiers. Now parse_implies routes ∀/∃ at
/// the top so the consequent of `⇒` (and the body items of an
/// implies-block) accept quantifiers directly.
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
