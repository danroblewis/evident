//! Integration test for Z3-AST functionizer: load test_01_hello,
//! build the solver, run simplify, extract per-output assignments,
//! evaluate against state=Init and check we get effects =
//! ⟨Println("hello from evident"), Exit(0)⟩.
//!
//! This is the proof-of-concept that we can extract function-
//! shaped components from the Z3 ASTs after preprocessing, and
//! evaluate them faster than the full SAT solver.

use std::collections::HashMap;
use evident_runtime::{EvidentRuntime, Value};
use evident_runtime::z3_eval::{simplify_assertions, extract_program, eval_program};
use z3::ast::Ast;

#[test]
fn z3_functionize_hello_state_next() {
    let mut rt = EvidentRuntime::new();
    rt.load_source(r#"
enum Result = NoResult | IntResult(Int) | StringResult(String)
enum Effect = NoEffect | Println(String) | Exit(Int)
enum HelloState = Init | Done

claim hello
    state ∈ HelloState
    state_next ∈ HelloState
    last_results ∈ Seq(Result)
    effects ∈ Seq(Effect)
    state_next = match state
        Init ⇒ Done
        Done ⇒ Done

    effects = match state
        Init ⇒ ⟨Println("hello from evident"), Exit(0)⟩
        Done ⇒ ⟨⟩
"#).unwrap();

    // Use the regular query path (slow Z3 solve) as a baseline:
    // make sure the model says state_next = Done when state = Init.
    let mut given = HashMap::new();
    given.insert("state".to_string(), Value::Enum {
        enum_name: "HelloState".into(),
        variant:   "Init".into(),
        fields:    vec![],
    });
    let r = rt.query("hello", &given).unwrap();
    assert!(r.satisfied);
    assert_eq!(r.bindings.get("state_next"), Some(&Value::Enum {
        enum_name: "HelloState".into(),
        variant:   "Done".into(),
        fields:    vec![],
    }));

    // Now exercise the new pipeline directly: build cache, simplify
    // the solver's assertions, extract per-output ASTs, evaluate.
    let ctx       = rt.z3_context();
    let datatypes = rt.datatypes_registry();
    let enums     = rt.enums_registry();
    let schemas   = rt.schemas_map();
    let arith: u32 = 2;
    let cached = evident_runtime::translate::build_cache(
        rt.get_schema("hello").unwrap(),
        schemas,
        ctx,
        datatypes,
        Some(enums),
        &given,
        arith,
    );
    // Push the given onto the cached solver so simplify sees the
    // pinned state value too.
    cached.solver.push();
    for (name, value) in &given {
        if let Some(var) = cached.env.get(name) {
            if let (
                evident_runtime::translate::Var::EnumVar { ast, .. },
                Value::Enum { .. },
            ) = (var, value)
            {
                if let Some(dt) = evident_runtime::translate::value_enum_to_datatype(
                    value, ctx, enums)
                {
                    cached.solver.assert(&ast._eq(&dt));
                }
            }
        }
    }
    let assertions = cached.solver.get_assertions();
    cached.solver.pop(1);

    let simplified = simplify_assertions(ctx, &assertions);
    eprintln!("Simplified assertions:");
    for a in &simplified {
        eprintln!("  {a}");
    }

    // Extract both `state_next` and `effects`.
    let outputs = vec!["state_next".to_string(), "effects".to_string()];
    let program = extract_program(&simplified, &outputs);
    assert!(program.is_some(), "extract_program failed: simplified={:?}", simplified);
    let program = program.unwrap();
    eprintln!("Program has {} steps, {} checks", program.steps.len(), program.checks.len());

    // Evaluate with state = Init in the input env. The output should
    // be state_next = Enum { variant: "Done" }.
    let mut input_env = HashMap::new();
    input_env.insert("state".to_string(), Value::Enum {
        enum_name: "HelloState".into(),
        variant:   "Init".into(),
        fields:    vec![],
    });
    let bindings = eval_program(&program, &input_env, Some(enums));
    assert!(bindings.is_some(), "eval_program failed");
    let bindings = bindings.unwrap();
    eprintln!("Bindings: {:?}", bindings);
    assert_eq!(bindings.get("state_next"), Some(&Value::Enum {
        enum_name: "HelloState".into(),
        variant:   "Done".into(),
        fields:    vec![],
    }));
    // effects should be the two-element seq the body builds.
    let effects = bindings.get("effects").expect("effects bound");
    eprintln!("effects = {effects:?}");
    let Value::SeqEnum(elems) = effects else {
        panic!("effects not SeqEnum: {effects:?}");
    };
    assert_eq!(elems.len(), 2);
    assert!(matches!(&elems[0], Value::Enum { variant, fields, .. }
        if variant == "Println"
           && matches!(&fields[..], [Value::Str(s)] if s == "hello from evident")));
    assert!(matches!(&elems[1], Value::Enum { variant, fields, .. }
        if variant == "Exit"
           && matches!(&fields[..], [Value::Int(0)])));
}
