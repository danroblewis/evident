//! Minimal isolation test: just JIT-compile a single 0-arity
//! enum output (state_next = Done) and verify it returns the
//! right Value. Helps isolate which helper call breaks.

use std::collections::HashMap;
use evident_runtime::{EvidentRuntime, Value};
use evident_runtime::z3_eval::{simplify_assertions, extract_program};
use evident_runtime::functionize::cranelift::compile_program;

#[test]
fn jit_minimal_nullary_enum() {
    let mut rt = EvidentRuntime::new();
    rt.load_source(r#"
enum DState = Init | Done

claim display
    state ∈ DState
    state_next ∈ DState
    state_next = Done
"#).unwrap();

    let ctx       = rt.z3_context();
    let datatypes = rt.datatypes_registry();
    let enums     = rt.enums_registry();
    let schemas   = rt.schemas_map();
    let empty: HashMap<String, Value> = HashMap::new();
    let cached = evident_runtime::translate::build_cache(
        rt.get_schema("display").unwrap(),
        schemas, ctx, datatypes, Some(enums), &empty, 2);
    let assertions = cached.solver.get_assertions();
    let result = simplify_assertions(ctx, &assertions);
    let program = extract_program(&result.formulas,
        &vec!["state_next".to_string()]).expect("extract");
    eprintln!("program steps: {}", program.steps.len());
    for step in &program.steps {
        eprintln!("  {step:?}");
    }
    let jit = compile_program(&program, enums).expect("compile");
    eprintln!("jit compiled. outputs = {:?}", jit.output_offsets);
    let bindings = jit.call(&HashMap::new()).expect("call");
    eprintln!("result: {:?}", bindings);
    assert_eq!(bindings.get("state_next"), Some(&Value::Enum {
        enum_name: "DState".into(),
        variant:   "Done".into(),
        fields:    vec![],
    }));
}
