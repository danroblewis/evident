use std::collections::HashMap;
use evident_runtime::{EvidentRuntime, Value};
use evident_runtime::z3_eval::{simplify_assertions, extract_program};
use evident_runtime::functionize::cranelift::compile_program;

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

    let jit = compile_program(&program, enums, datatypes).expect("compile_program");
    eprintln!("inputs: {:?}", jit.input_offsets.keys().collect::<Vec<_>>());
    eprintln!("outputs: {:?}", jit.output_offsets.keys().collect::<Vec<_>>());

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
