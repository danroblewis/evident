//! Gap coverage (session T): integer division + modulo as the
//! top-level expression of a Scalar output.
//!
//! Before the fix, `emit_compute_i64` emitted `sdiv`/`srem` for div/mod
//! *operands*, but `emit_write_value`'s arithmetic arm only listed
//! `ADD | SUB | MUL | UMINUS` — so a Scalar step whose outermost decl
//! was `(div x 2)` / `(mod x 3)` fell through to `_ => None` and bailed
//! the whole program to the slow Z3 solve. The fix lists IDIV/DIV/MOD/REM
//! in that arm too. These tests construct a `Z3Program` whose single
//! step IS the division (and the modulo) and assert the JIT compiles it
//! and computes the right value — i.e. the claim is fully JIT'd (all
//! components compiled), not routed to Z3.

use std::collections::HashMap;
use evident_runtime::{EvidentRuntime, Value};
use evident_runtime::z3_eval::{simplify_assertions, extract_program};
use evident_runtime::functionize::cranelift::compile_program;

/// Compile `claim`'s `output` and call the JIT with `given`, returning
/// the produced value. Panics if extraction or compilation bails — which
/// is exactly the regression these tests guard against.
fn jit_eval_scalar(src: &str, claim: &str, output: &str,
                   given: HashMap<String, Value>) -> Value {
    let mut rt = EvidentRuntime::new();
    rt.load_source(src).unwrap();
    let ctx       = rt.z3_context();
    let datatypes = rt.datatypes_registry();
    let enums     = rt.enums_registry();
    let schemas   = rt.schemas_map();
    let empty: HashMap<String, Value> = HashMap::new();
    let cached = evident_runtime::translate::build_cache(
        rt.get_schema(claim).unwrap(),
        schemas, ctx, datatypes, Some(enums), &empty, 2);
    let assertions = cached.solver.get_assertions();
    let result = simplify_assertions(ctx, &assertions);
    let program = extract_program(&result.formulas, &vec![output.to_string()])
        .expect("extraction should produce a clean Z3Program");
    let jit = compile_program(&program, enums, datatypes)
        .expect("div/mod top-level Scalar output must JIT-compile (comp=N/N)");
    let bindings = jit.call(&given).expect("compiled fn call");
    bindings.get(output).cloned().expect("output binding present")
}

#[test]
fn jit_div_top_level_scalar() {
    // `q = x / 2` — div is the outermost decl of the Scalar step.
    let src = r#"
claim divtest
    x ∈ Int
    q ∈ Int = x / 2
"#;
    let mut given = HashMap::new();
    given.insert("x".to_string(), Value::Int(10));
    assert_eq!(jit_eval_scalar(src, "divtest", "q", given), Value::Int(5));
}

#[test]
fn jit_div_inside_ite_branch() {
    // div nested in an ITE branch is also written via emit_write_value
    // (the then/else branches recurse through it), so this exercises the
    // same arm. `(x > 0 ? x / 2 : 0)`.
    let src = r#"
claim divite
    x ∈ Int
    q ∈ Int = (x > 0 ? x / 2 : 0)
"#;
    let mut given = HashMap::new();
    given.insert("x".to_string(), Value::Int(20));
    assert_eq!(jit_eval_scalar(src, "divite", "q", given), Value::Int(10));
}

#[test]
fn jit_mod_top_level_scalar() {
    // `r = mod(x, 3)` — modulo is the outermost decl of the Scalar step.
    let src = r#"
claim modtest
    x ∈ Int
    r ∈ Int = mod(x, 3)
"#;
    let mut given = HashMap::new();
    given.insert("x".to_string(), Value::Int(10));
    assert_eq!(jit_eval_scalar(src, "modtest", "r", given), Value::Int(1));
}
