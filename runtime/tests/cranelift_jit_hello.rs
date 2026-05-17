//! End-to-end Cranelift JIT codegen test.
//!
//! Build a Z3Program for hello.ev with unpinned state, then
//! compile it to native code via Cranelift. Call the compiled
//! function with state=Init and state=Done; verify the outputs
//! match the slow-path Z3 result.

use std::collections::HashMap;
use evident_runtime::{EvidentRuntime, Value};
use evident_runtime::z3_eval::{simplify_assertions, extract_program};
use evident_runtime::cranelift_jit::compile_program;

#[test]
fn jit_compiles_hello_state_next() {
    let mut rt = EvidentRuntime::new();
    rt.load_source(r#"
enum Result = NoResult | IntResult(Int) | StringResult(String)
enum Effect = NoEffect | Println(String) | Exit(Int)
enum HelloState = Init | Done

claim hello
    state ∈ HelloState
    state_next ∈ HelloState
    state_next = match state
        Init ⇒ Done
        Done ⇒ Done
"#).unwrap();

    let ctx       = rt.z3_context();
    let datatypes = rt.datatypes_registry();
    let enums     = rt.enums_registry();
    let schemas   = rt.schemas_map();
    let arith: u32 = 2;
    let empty_given: HashMap<String, Value> = HashMap::new();
    let cached = evident_runtime::translate::build_cache(
        rt.get_schema("hello").unwrap(),
        schemas, ctx, datatypes, Some(enums), &empty_given, arith);
    let assertions = cached.solver.get_assertions();
    let simplified = simplify_assertions(ctx, &assertions).formulas;

    let outputs = vec!["state_next".to_string()];
    let program = extract_program(&simplified, &outputs).expect("extract");
    eprintln!("program has {} steps, {} predicates", program.steps.len(), program.predicates.len());

    // Compile to native.
    let jit = compile_program(&program, enums).expect("compile_program");
    eprintln!("inputs: {:?}", jit.input_offsets.keys().collect::<Vec<_>>());
    eprintln!("outputs: {:?}", jit.output_offsets.keys().collect::<Vec<_>>());

    // Call with state=Init.
    let mut env = HashMap::new();
    env.insert("state".to_string(), Value::Enum {
        enum_name: "HelloState".into(),
        variant:   "Init".into(),
        fields:    vec![],
    });
    let result = jit.call(&env).expect("jit call");
    eprintln!("result for state=Init: {result:?}");
    assert_eq!(result.get("state_next"), Some(&Value::Enum {
        enum_name: "HelloState".into(),
        variant:   "Done".into(),
        fields:    vec![],
    }));

    // Call with state=Done.
    let mut env2 = HashMap::new();
    env2.insert("state".to_string(), Value::Enum {
        enum_name: "HelloState".into(),
        variant:   "Done".into(),
        fields:    vec![],
    });
    let result2 = jit.call(&env2).expect("jit call 2");
    assert_eq!(result2.get("state_next"), Some(&Value::Enum {
        enum_name: "HelloState".into(),
        variant:   "Done".into(),
        fields:    vec![],
    }));
}

#[test]
fn jit_compiles_int_arithmetic() {
    // Build a Z3Program by hand for a simple int claim:
    //   sum = a + b
    //   prod = a * b
    use z3::{Config, Context};
    use z3::ast::{Ast, Int as Z3Int, Dynamic};
    let cfg = Config::new();
    let ctx: &'static Context = Box::leak(Box::new(Context::new(&cfg)));
    let a = Z3Int::new_const(ctx, "a");
    let b = Z3Int::new_const(ctx, "b");
    let sum  = Z3Int::new_const(ctx, "sum");
    let prod = Z3Int::new_const(ctx, "prod");

    let a_plus_b  = Z3Int::add(ctx, &[&a, &b]);
    let a_times_b = Z3Int::mul(ctx, &[&a, &b]);

    let assertions = vec![
        sum._eq(&a_plus_b),
        prod._eq(&a_times_b),
    ];

    let simplified = simplify_assertions(ctx, &assertions).formulas;
    let outputs = vec!["sum".to_string(), "prod".to_string()];
    let program = extract_program(&simplified, &outputs).expect("extract");

    // Need an EnumRegistry — empty one is fine.
    let enums = evident_runtime::translate::EnumRegistry::default();
    let jit = compile_program(&program, &enums).expect("compile_program");

    let mut env = HashMap::new();
    env.insert("a".to_string(), Value::Int(5));
    env.insert("b".to_string(), Value::Int(7));
    let result = jit.call(&env).expect("jit call");
    assert_eq!(result.get("sum"),  Some(&Value::Int(12)));
    assert_eq!(result.get("prod"), Some(&Value::Int(35)));

    // Different inputs, same compiled function.
    let mut env2 = HashMap::new();
    env2.insert("a".to_string(), Value::Int(10));
    env2.insert("b".to_string(), Value::Int(3));
    let result2 = jit.call(&env2).expect("jit call 2");
    assert_eq!(result2.get("sum"),  Some(&Value::Int(13)));
    assert_eq!(result2.get("prod"), Some(&Value::Int(30)));
}
