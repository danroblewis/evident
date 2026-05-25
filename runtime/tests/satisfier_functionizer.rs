//! SatisfierFunctionizer integration tests.
//!
//! Each test loads a real Evident claim, runs it through the actual
//! translate → simplify → extract pipeline (so the Z3 assertions are
//! exactly what production produces), recovers `Sample*` steps, and
//! compiles via `SatisfierFunctionizer`. We then assert:
//!   * the drawn assignment satisfies all constraints (cross-validated
//!     by re-asserting the body + the assignment in a fresh Z3 solver
//!     — must be SAT);
//!   * repeated calls with the same inputs return the same assignment
//!     (determinism — the value cache depends on it);
//!   * sampler shapes outside the v1 scope refuse cleanly (→ slow Z3).

use std::collections::HashMap;
use std::rc::Rc;

use evident_runtime::{EvidentRuntime, Value};
use evident_runtime::translate::{build_cache, Var};
use evident_runtime::z3_eval::{simplify_assertions, extract_program_with_samplers};
use evident_runtime::functionize::{CompiledFunction, Functionizer};
use evident_runtime::functionize::satisfier::SatisfierFunctionizer;

use z3::ast::{Ast, Bool, Int};
use z3::{SatResult, Solver};

/// Build a claim's `Sample`-augmented program and compile it with the
/// SatisfierFunctionizer. Returns `None` when extraction or compilation
/// refuses (the production fall-through to a slow Z3 solve).
fn compile_satisfier(
    rt: &EvidentRuntime,
    claim: &str,
    outputs: &[&str],
) -> Option<Rc<dyn CompiledFunction>> {
    let ctx       = rt.z3_context();
    let datatypes = rt.datatypes_registry();
    let enums     = rt.enums_registry();
    let schemas   = rt.schemas_map();
    let empty: HashMap<String, Value> = HashMap::new();
    let cached = build_cache(
        rt.get_schema(claim).unwrap(),
        schemas, ctx, datatypes, Some(enums), &empty, 2);
    let assertions = cached.solver.get_assertions();
    let result = simplify_assertions(ctx, &assertions);
    let outs: Vec<String> = outputs.iter().map(|s| s.to_string()).collect();
    let program = extract_program_with_samplers(&result.formulas, &outs)?;
    SatisfierFunctionizer::new().compile(&program, enums, datatypes)
}

/// Cross-validate against Z3: re-assert the claim's simplified body
/// plus the produced assignment in a fresh solver. SAT ⟺ the
/// assignment genuinely satisfies every constraint.
fn satisfies_in_fresh_z3(
    rt: &EvidentRuntime,
    claim: &str,
    bindings: &HashMap<String, Value>,
) -> bool {
    let ctx       = rt.z3_context();
    let datatypes = rt.datatypes_registry();
    let enums     = rt.enums_registry();
    let schemas   = rt.schemas_map();
    let empty: HashMap<String, Value> = HashMap::new();
    let cached = build_cache(
        rt.get_schema(claim).unwrap(),
        schemas, ctx, datatypes, Some(enums), &empty, 2);
    let assertions = cached.solver.get_assertions();
    let result = simplify_assertions(ctx, &assertions);

    let solver = Solver::new(ctx);
    for f in &result.formulas {
        solver.assert(f);
    }
    // Pin each binding to its drawn value, using the SAME Z3 const the
    // body references (Z3 interns consts by name+sort, so the env's
    // handle and a fresh `new_const` of the same name unify).
    for (name, val) in bindings {
        match (cached.env.get(name), val) {
            (Some(Var::IntVar(c)), Value::Int(i)) => {
                solver.assert(&c._eq(&Int::from_i64(ctx, *i)));
            }
            (Some(Var::BoolVar(c)), Value::Bool(b)) => {
                solver.assert(&c._eq(&Bool::from_bool(ctx, *b)));
            }
            (Some(Var::EnumVar { ast, .. }), Value::Enum { enum_name, variant, .. }) => {
                let by_name = enums.by_name.borrow();
                let (dt, variants) = by_name.get(enum_name).expect("enum registered");
                let idx = variants.iter().position(|v| &v.name == variant)
                    .expect("variant exists");
                let vv = dt.variants[idx].constructor.apply(&[]);
                solver.assert(&ast._eq(&vv.as_datatype().unwrap()));
            }
            // Bindings for vars not in env (or shape mismatch) just
            // aren't pinned — the body still constrains them.
            _ => {}
        }
    }
    matches!(solver.check(), SatResult::Sat)
}

// ── 1. Range scalar: x ∈ [0,10], y = x*3 + 5 ─────────────────────

#[test]
fn range_scalar_then_computed() {
    let mut rt = EvidentRuntime::new();
    rt.load_source(r#"
claim setup
    x ∈ Int
    x ≥ 0
    x ≤ 10
    y ∈ Int = x * 3 + 5
"#).unwrap();

    let f = compile_satisfier(&rt, "setup", &["x", "y"]).expect("compiles");
    let out = f.call(&HashMap::new()).expect("call");

    let x = match out.get("x") { Some(Value::Int(i)) => *i, o => panic!("x = {o:?}") };
    let y = match out.get("y") { Some(Value::Int(i)) => *i, o => panic!("y = {o:?}") };
    assert!((0..=10).contains(&x), "x={x} out of [0,10]");
    assert_eq!(y, x * 3 + 5, "y must be computed from sampled x");
    assert!(satisfies_in_fresh_z3(&rt, "setup", &out), "Z3 must accept the assignment");
}

// ── 2. Enum: c ∈ Color, used (not constrained) ───────────────────

#[test]
fn enum_sample() {
    let mut rt = EvidentRuntime::new();
    rt.load_source(r#"
enum Color = Red | Green | Blue

claim setup
    c ∈ Color
    chosen ∈ Int = (c = Red ? 1 : (c = Green ? 2 : 3))
"#).unwrap();

    let f = compile_satisfier(&rt, "setup", &["c", "chosen"]).expect("compiles");
    let out = f.call(&HashMap::new()).expect("call");

    match out.get("c") {
        Some(Value::Enum { enum_name, variant, fields }) => {
            assert_eq!(enum_name, "Color");
            assert!(["Red", "Green", "Blue"].contains(&variant.as_str()), "variant={variant}");
            assert!(fields.is_empty());
        }
        o => panic!("c = {o:?}"),
    }
    assert!(satisfies_in_fresh_z3(&rt, "setup", &out), "Z3 must accept the assignment");
}

// ── 3. Finite set: x ∈ {1, 3, 5} ─────────────────────────────────

#[test]
fn set_sample() {
    let mut rt = EvidentRuntime::new();
    rt.load_source(r#"
claim setup
    x ∈ Int
    x ∈ {1, 3, 5}
    doubled ∈ Int = x * 2
"#).unwrap();

    let f = compile_satisfier(&rt, "setup", &["x", "doubled"]).expect("compiles");
    let out = f.call(&HashMap::new()).expect("call");

    let x = match out.get("x") { Some(Value::Int(i)) => *i, o => panic!("x = {o:?}") };
    assert!([1, 3, 5].contains(&x), "x={x} not in {{1,3,5}}");
    assert_eq!(out.get("doubled"), Some(&Value::Int(x * 2)));
    assert!(satisfies_in_fresh_z3(&rt, "setup", &out));
}

// ── 4. Nat lower bound + computed (mixed) ────────────────────────

#[test]
fn nat_range_mixed() {
    let mut rt = EvidentRuntime::new();
    // Nat auto-asserts x ≥ 0; the explicit bound gives the upper end.
    rt.load_source(r#"
claim setup
    x ∈ Nat
    x ≤ 7
    half ∈ Int = x / 2
    label ∈ Int = (x ≥ 4 ? 1 : 0)
"#).unwrap();

    let f = compile_satisfier(&rt, "setup", &["x", "half", "label"]).expect("compiles");
    let out = f.call(&HashMap::new()).expect("call");

    let x = match out.get("x") { Some(Value::Int(i)) => *i, o => panic!("x = {o:?}") };
    assert!((0..=7).contains(&x), "x={x} out of [0,7]");
    assert_eq!(out.get("half"), Some(&Value::Int(x / 2)));
    assert_eq!(out.get("label"), Some(&Value::Int(if x >= 4 { 1 } else { 0 })));
    assert!(satisfies_in_fresh_z3(&rt, "setup", &out));
}

// ── 5. Range with a tight window pinned to a single value ────────

#[test]
fn range_singleton() {
    let mut rt = EvidentRuntime::new();
    rt.load_source(r#"
claim setup
    n ∈ Int
    n ≥ 42
    n ≤ 42
    m ∈ Int = n + 1
"#).unwrap();

    let f = compile_satisfier(&rt, "setup", &["n", "m"]).expect("compiles");
    let out = f.call(&HashMap::new()).expect("call");
    assert_eq!(out.get("n"), Some(&Value::Int(42)));
    assert_eq!(out.get("m"), Some(&Value::Int(43)));
    assert!(satisfies_in_fresh_z3(&rt, "setup", &out));
}

// ── 6. Determinism: same query → same assignment ─────────────────

#[test]
fn determinism_same_query_same_result() {
    let mut rt = EvidentRuntime::new();
    rt.load_source(r#"
claim setup
    x ∈ Int
    x ≥ 0
    x ≤ 1000000
    y ∈ Int = x + 1
"#).unwrap();

    let f = compile_satisfier(&rt, "setup", &["x", "y"]).expect("compiles");
    let a = f.call(&HashMap::new()).expect("call");
    let b = f.call(&HashMap::new()).expect("call");
    let c = f.call(&HashMap::new()).expect("call");
    assert_eq!(a, b, "repeated calls must agree");
    assert_eq!(b, c);

    // A freshly-compiled function (same seed env) must also agree —
    // determinism is a property of (program, given, seed), not of the
    // compiled instance.
    let f2 = compile_satisfier(&rt, "setup", &["x", "y"]).expect("compiles");
    let d = f2.call(&HashMap::new()).expect("call");
    assert_eq!(a, d, "a fresh compile must draw the same value");
}

// ── 7. Determinism varies with given values, stays per-value stable ─

#[test]
fn determinism_given_sensitivity() {
    let mut rt = EvidentRuntime::new();
    rt.load_source(r#"
claim setup
    seed_in ∈ Int
    x ∈ Int
    x ≥ 0
    x ≤ 1000000
    y ∈ Int = x + seed_in
"#).unwrap();

    let f = compile_satisfier(&rt, "setup", &["x", "y"]).expect("compiles");
    let g1: HashMap<String, Value> = [("seed_in".to_string(), Value::Int(1))].into();
    let g2: HashMap<String, Value> = [("seed_in".to_string(), Value::Int(2))].into();

    let a1 = f.call(&g1).expect("call");
    let a1b = f.call(&g1).expect("call");
    let a2 = f.call(&g2).expect("call");
    assert_eq!(a1, a1b, "same given → same draw");
    // Different given values almost certainly draw a different x (the
    // seed is folded from given). Not a hard guarantee, but with a
    // million-wide range a collision is astronomically unlikely.
    let x1 = a1.get("x");
    let x2 = a2.get("x");
    assert_ne!(x1, x2, "different given should reseed the draw");
    // y must always be consistent with the drawn x + given seed.
    for (a, g) in [(&a1, &g1), (&a2, &g2)] {
        assert!(satisfies_in_fresh_z3_with_given(&rt, "setup", a, g));
    }
}

/// Variant of cross-validation that also pins the `given` inputs.
fn satisfies_in_fresh_z3_with_given(
    rt: &EvidentRuntime,
    claim: &str,
    bindings: &HashMap<String, Value>,
    given: &HashMap<String, Value>,
) -> bool {
    let mut all = bindings.clone();
    for (k, v) in given { all.insert(k.clone(), v.clone()); }
    satisfies_in_fresh_z3(rt, claim, &all)
}

// ── 8. Refusal: range mixed with a free relation (x < y, y free) ──

#[test]
fn refuses_free_relation() {
    let mut rt = EvidentRuntime::new();
    rt.load_source(r#"
claim setup
    x ∈ Int
    y ∈ Int
    x ≥ 0
    x < y
"#).unwrap();

    // x's `x < y` mentions a free y → not a closed range. y is wholly
    // unbounded. Neither is a clean sampler → refuse to the slow path.
    let f = compile_satisfier(&rt, "setup", &["x", "y"]);
    assert!(f.is_none(), "free relation must refuse (fall through to Z3)");
}

// ── 9. Refusal: residual predicate on a derived var ──────────────

#[test]
fn refuses_residual_predicate() {
    let mut rt = EvidentRuntime::new();
    rt.load_source(r#"
claim setup
    x ∈ Int
    x ≥ 0
    x ≤ 10
    y ∈ Int = x * 2
    y ≥ 100
"#).unwrap();

    // x samples cleanly, but `y ≥ 100` is a real constraint on a
    // computed var that the sampler can't honor by construction — the
    // compile step must refuse so the slow Z3 solve validates it.
    let f = compile_satisfier(&rt, "setup", &["x", "y"]);
    assert!(f.is_none(), "residual predicate on derived var must refuse");
}

// ── 10. No sampler steps → delegate to Cranelift unchanged ───────

#[test]
fn no_sampler_delegates_to_cranelift() {
    let mut rt = EvidentRuntime::new();
    rt.load_source(r#"
claim setup
    a ∈ Int
    b ∈ Int = a + 7
"#).unwrap();

    // `a` is a free input (given), `b` is computed — no sampler shape.
    // The satisfier must compile this exactly like Cranelift would.
    let f = compile_satisfier(&rt, "setup", &["b"]).expect("compiles (delegated)");
    let given: HashMap<String, Value> = [("a".to_string(), Value::Int(35))].into();
    let out = f.call(&given).expect("call");
    assert_eq!(out.get("b"), Some(&Value::Int(42)));
}
