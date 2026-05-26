//! Round-trip proof for the Evident → SMT-LIB → Z3 prototype (north-star slice).
//!
//! For a corpus of simple claims, this asserts that the SMT-LIB path
//! (`translate::smtlib::solve`: emit text → `Z3_solver_from_string` → check)
//! produces the SAME sat/unsat as the default C-API `EvidentRuntime::query_free`
//! path — and, where the model is forced, the same model. It also pins the
//! transpilable boundary: claims that fall outside the scalar quantifier-free
//! subset must be reported as errors, never silently mistranslated.
//!
//! This is the "it really works" evidence for
//! `docs/design/smtlib-as-compile-target.md`. The default path is untouched;
//! the prototype is only reachable from here (the dedicated test entry).

use std::collections::HashMap;
use std::time::Instant;

use evident_runtime::translate::smtlib;
use evident_runtime::{EvidentRuntime, Value};

/// Load one self-contained source, return a runtime with it loaded.
fn load(src: &str) -> EvidentRuntime {
    let mut rt = EvidentRuntime::new();
    rt.load_source(src).expect("source should load");
    rt
}

/// Assert the SMT-LIB path agrees with the C-API path on satisfiability.
fn assert_sat_parity(src: &str, claim: &str) -> smtlib::SmtSolveResult {
    let rt = load(src);
    let reference = rt
        .query_free(claim)
        .unwrap_or_else(|e| panic!("query_free({claim}) failed: {e}"));
    let schema = rt
        .get_schema(claim)
        .unwrap_or_else(|| panic!("no schema {claim}"));
    let smt = smtlib::solve(schema)
        .unwrap_or_else(|e| panic!("smtlib::solve({claim}) failed (should be in subset): {e}"));
    assert_eq!(
        smt.satisfied, reference.satisfied,
        "sat mismatch for {claim}: C-API={} SMT-LIB={}\nSMT-LIB text:\n{}",
        reference.satisfied, smt.satisfied, smt.smtlib
    );
    smt
}

// --------------------------------------------------------------------------
// In-subset corpus: sat/unsat must match the C-API path.
// --------------------------------------------------------------------------

#[test]
fn nat_gt_is_sat() {
    assert_sat_parity("claim T\n    n ∈ Nat\n    n > 5\n", "T");
}

#[test]
fn impossible_is_unsat() {
    assert_sat_parity("claim T\n    n ∈ Nat\n    n > 10\n    n < 3\n", "T");
}

#[test]
fn pair_sum_is_sat() {
    let r = assert_sat_parity(
        "claim T\n    a ∈ Nat\n    b ∈ Nat\n    a + b = 10\n    a > 0\n    b > 0\n",
        "T",
    );
    if let (Some(Value::Int(a)), Some(Value::Int(b))) =
        (r.bindings.get("a"), r.bindings.get("b"))
    {
        assert_eq!(a + b, 10);
        assert!(*a > 0 && *b > 0);
    } else {
        panic!("missing a/b bindings: {:?}", r.bindings);
    }
}

#[test]
fn bool_implies_forces_q() {
    // p = true ∧ (p ⇒ q) forces q = true — model is determined, compare it.
    let r = assert_sat_parity("claim T\n    p ∈ Bool\n    q ∈ Bool\n    p = true\n    p ⇒ q\n", "T");
    assert_eq!(r.bindings.get("q"), Some(&Value::Bool(true)));
}

#[test]
fn string_eq_forces_value() {
    let r = assert_sat_parity("claim T\n    name ∈ String\n    name = \"hello\"\n", "T");
    assert_eq!(r.bindings.get("name"), Some(&Value::Str("hello".into())));
}

#[test]
fn forced_int_model_matches() {
    let r = assert_sat_parity("claim T\n    k ∈ Int\n    k = 7\n    k > 0\n", "T");
    assert_eq!(r.bindings.get("k"), Some(&Value::Int(7)));
}

#[test]
fn neq_contradiction_is_unsat() {
    assert_sat_parity("claim T\n    a ∈ Int\n    b ∈ Int\n    a = 3\n    b = 3\n    a ≠ b\n", "T");
}

#[test]
fn negative_literal_is_sat() {
    let r = assert_sat_parity("claim T\n    x ∈ Int\n    x = 0 - 5\n    x < 0\n", "T");
    assert_eq!(r.bindings.get("x"), Some(&Value::Int(-5)));
}

#[test]
fn real_linear_is_sat() {
    // x + x = 3 → x = 1.5. Linear real arithmetic; both paths agree.
    let r = assert_sat_parity("claim T\n    x ∈ Real\n    x + x = 3.0\n", "T");
    assert_eq!(r.bindings.get("x"), Some(&Value::Real(1.5)));
}

/// DOCUMENTED DIVERGENCE (a real finding, not a parity case).
///
/// `x > 0 ∧ x*x = 2` is satisfiable (x = √2). The SMT-LIB path hands a plain
/// problem to Z3, which routes nonlinear real arithmetic to `nlsat` and decides
/// SAT correctly. The default C-API path runs a *tuned tactic chain* that returns
/// `Unknown` here, and `evaluate` maps `Unknown → satisfied = false` — so it
/// reports the problem as not-satisfiable. This pins the current behavior: if the
/// default path ever learns to solve this, update the findings doc.
#[test]
fn real_nonlinear_smtlib_decides_capi_does_not() {
    let src = "claim T\n    x ∈ Real\n    x > 0.0\n    x * x = 2.0\n";
    let rt = load(src);
    let reference = rt.query_free("T").unwrap();
    let smt = smtlib::solve(rt.get_schema("T").unwrap()).unwrap();

    // SMT-LIB path is mathematically correct.
    assert!(smt.satisfied, "SMT-LIB path should decide nonlinear reals SAT");
    // Default tactic-chain path currently gives up (Unknown → not satisfied).
    assert!(
        !reference.satisfied,
        "C-API path unexpectedly solved nonlinear reals — update findings doc"
    );
}

#[test]
fn int_division_is_sat() {
    // q = 17, r = q / 5 → r = 3 under SMT `div` (matches the C-API `Int::div`).
    let r = assert_sat_parity("claim T\n    q ∈ Int\n    r ∈ Int\n    q = 17\n    r = q / 5\n", "T");
    assert_eq!(r.bindings.get("r"), Some(&Value::Int(3)));
}

#[test]
fn set_membership_is_sat() {
    assert_sat_parity("claim T\n    m ∈ Int\n    m ∈ {2, 4, 6}\n    m > 3\n", "T");
}

#[test]
fn set_membership_can_be_unsat() {
    assert_sat_parity("claim T\n    m ∈ Int\n    m ∈ {2, 4, 6}\n    m > 10\n", "T");
}

// --------------------------------------------------------------------------
// String builtins → Z3 `str.*`. The subset grown this session; each must
// match the C-API path on sat/unsat AND on the forced model value.
// --------------------------------------------------------------------------

#[test]
fn str_substr_and_index_of_model() {
    // head = substr("Edge<Rect>", 0, indexof "<") = "Edge".
    let r = assert_sat_parity(
        "claim T\n    g ∈ String = \"Edge<Rect>\"\n    \
         head ∈ String = substr(g, 0, index_of(g, \"<\"))\n",
        "T",
    );
    assert_eq!(r.bindings.get("head"), Some(&Value::Str("Edge".into())));
}

#[test]
fn str_replace_model() {
    let r = assert_sat_parity(
        "claim T\n    mono ∈ String = replace(\"Seq(T)\", \"T\", \"Rect\")\n",
        "T",
    );
    assert_eq!(r.bindings.get("mono"), Some(&Value::Str("Seq(Rect)".into())));
}

#[test]
fn str_len_via_cardinality_model() {
    let r = assert_sat_parity(
        "claim T\n    g ∈ String = \"Edge<Rect>\"\n    n ∈ Int = #g\n",
        "T",
    );
    assert_eq!(r.bindings.get("n"), Some(&Value::Int(10)));
}

#[test]
fn str_from_int_negative_model() {
    let r = assert_sat_parity("claim T\n    s ∈ String = str_from_int(0 - 42)\n", "T");
    assert_eq!(r.bindings.get("s"), Some(&Value::Str("-42".into())));
}

#[test]
fn str_char_at_model() {
    let r = assert_sat_parity(
        "claim T\n    s ∈ String = \"abc\"\n    c ∈ String = char_at(s, 1)\n",
        "T",
    );
    assert_eq!(r.bindings.get("c"), Some(&Value::Str("b".into())));
}

#[test]
fn str_prefix_suffix_contains_sat_and_unsat() {
    // All three predicates hold for "world.pos".
    assert_sat_parity(
        "claim T\n    s ∈ String = \"world.pos\"\n    starts_with(s, \"world.\")\n    \
         ends_with(s, \".pos\")\n    str_contains(s, \"d.p\")\n",
        "T",
    );
    // A wrong prefix is unsat — proves the predicate really fires.
    assert_sat_parity(
        "claim T\n    s ∈ String = \"world.pos\"\n    starts_with(s, \"local.\")\n",
        "T",
    );
}

#[test]
fn str_contains_infix_unsat() {
    // `"xyz" ∈ s` (infix) → str.contains; absent → unsat, matching the C-API.
    assert_sat_parity("claim T\n    s ∈ String = \"abc\"\n    \"xyz\" ∈ s\n", "T");
}

// --------------------------------------------------------------------------
// Out-of-subset boundary: must be reported, not silently mistranslated.
// --------------------------------------------------------------------------

#[test]
fn seq_type_is_rejected() {
    let rt = load("claim T\n    xs ∈ Seq(Int)\n    #xs = 3\n");
    let schema = rt.get_schema("T").unwrap();
    assert!(
        smtlib::schema_to_smtlib(schema).is_err(),
        "Seq type should be rejected as out-of-subset"
    );
}

#[test]
fn quantifier_is_rejected() {
    // All declarations are scalar; the ∀ constraint is what's out of subset.
    let rt = load("claim T\n    n ∈ Int\n    ∀ i ∈ {0..2} : n > i\n");
    let schema = rt.get_schema("T").unwrap();
    assert!(
        smtlib::schema_to_smtlib(schema).is_err(),
        "∀ quantifier should be rejected (quantifier-free subset)"
    );
}

#[test]
fn enum_type_is_rejected() {
    let rt = load("enum Color = Red | Green | Blue\nclaim T\n    c ∈ Color\n    c = Red\n");
    let schema = rt.get_schema("T").unwrap();
    assert!(
        smtlib::schema_to_smtlib(schema).is_err(),
        "enum-typed var should be rejected (records/enums out of subset)"
    );
}

// --------------------------------------------------------------------------
// Cost reality (north-star gate #1): SMT-LIB gen+parse vs in-memory C-API.
// Run with `--nocapture` to see the numbers; the doc records them.
// --------------------------------------------------------------------------

#[test]
fn cost_comparison() {
    let corpus: &[(&str, &str)] = &[
        ("claim T\n    n ∈ Nat\n    n > 5\n", "T"),
        ("claim T\n    n ∈ Nat\n    n > 10\n    n < 3\n", "T"),
        ("claim T\n    a ∈ Nat\n    b ∈ Nat\n    a + b = 10\n    a > 0\n    b > 0\n", "T"),
        ("claim T\n    p ∈ Bool\n    q ∈ Bool\n    p = true\n    p ⇒ q\n", "T"),
        ("claim T\n    k ∈ Int\n    k = 7\n    k > 0\n", "T"),
        ("claim T\n    a ∈ Int\n    b ∈ Int\n    a = 3\n    b = 3\n    a ≠ b\n", "T"),
        ("claim T\n    m ∈ Int\n    m ∈ {2, 4, 6}\n    m > 3\n", "T"),
    ];

    // Pre-load runtimes + schemas so we measure solve, not parse-Evident.
    let loaded: Vec<(EvidentRuntime, &str)> =
        corpus.iter().map(|(src, c)| (load(src), *c)).collect();

    const ITERS: u32 = 200;
    let empty: HashMap<String, Value> = HashMap::new();

    // Warm up both paths (JIT / solver caches).
    for (rt, c) in &loaded {
        let _ = rt.query(c, &empty);
        let _ = smtlib::solve(rt.get_schema(c).unwrap());
    }

    // (1) C-API warm query — cached ClaimPlan + JIT + value cache (steady state).
    let t = Instant::now();
    for _ in 0..ITERS {
        for (rt, c) in &loaded {
            let _ = rt.query(c, &empty).unwrap();
        }
    }
    let capi = t.elapsed();

    // (2) emit only — Evident AST → SMT-LIB text (the "string-gen" half of gate #1).
    let t = Instant::now();
    for _ in 0..ITERS {
        for (rt, c) in &loaded {
            let _ = smtlib::schema_to_smtlib(rt.get_schema(c).unwrap()).unwrap();
        }
    }
    let emit = t.elapsed();

    // (3) full smtlib::solve — fresh Z3 Context + parse + solve every call.
    let t = Instant::now();
    for _ in 0..ITERS {
        for (rt, c) in &loaded {
            let _ = smtlib::solve(rt.get_schema(c).unwrap()).unwrap();
        }
    }
    let solve_fresh = t.elapsed();

    // (4) parse + solve on a SHARED context (isolates Z3 context-creation cost).
    let texts: Vec<String> = loaded
        .iter()
        .map(|(rt, c)| smtlib::schema_to_smtlib(rt.get_schema(c).unwrap()).unwrap())
        .collect();
    let mut cfg = z3::Config::new();
    cfg.set_model_generation(true);
    let ctx = z3::Context::new(&cfg);
    let t = Instant::now();
    for _ in 0..ITERS {
        for text in &texts {
            let s = z3::Solver::new(&ctx);
            s.from_string(text.clone());
            let _ = s.check();
        }
    }
    let parse_solve_shared = t.elapsed();

    let n = (ITERS as usize * loaded.len()) as f64;
    let per = |d: std::time::Duration| d.as_micros() as f64 / n;
    eprintln!("--- SMT-LIB prototype cost ({} solves/path) ---", n as u64);
    eprintln!("(1) C-API warm query        : {:.1} µs/solve", per(capi));
    eprintln!("(2) emit text only          : {:.1} µs/solve", per(emit));
    eprintln!("(3) solve (fresh context)   : {:.1} µs/solve", per(solve_fresh));
    eprintln!("(4) parse+solve (shared ctx): {:.1} µs/solve", per(parse_solve_shared));
    eprintln!("    context-creation cost   ≈ {:.1} µs/solve  (3 minus 4)", per(solve_fresh) - per(parse_solve_shared));
    eprintln!("    ratio (4)/(1)           : {:.2}x", parse_solve_shared.as_secs_f64() / capi.as_secs_f64());
    eprintln!("    ratio (3)/(1)           : {:.2}x", solve_fresh.as_secs_f64() / capi.as_secs_f64());
}
