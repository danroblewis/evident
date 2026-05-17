//! What does Z3's simplify produce when given values are NOT
//! pinned? This tells us whether we can extract a generic program
//! once (at load time) and walk it with different given values
//! per tick.

use std::collections::HashMap;
use evident_runtime::{EvidentRuntime, Value};
use evident_runtime::z3_eval::{simplify_assertions, extract_program, eval_program};

#[test]
fn z3_simplify_leaves_match_open_when_state_free() {
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
    eprintln!("Simplified (no given pinned):");
    for a in &simplified {
        eprintln!("  {a}");
    }

    // Try extracting against outputs={state_next, effects}
    // — state is now an INPUT (we won't pin it in given).
    let outputs = vec!["state_next".to_string(), "effects".to_string()];
    let program = extract_program(&simplified, &outputs);
    eprintln!("extract_program: {:?}", program.is_some());

    if let Some(p) = program {
        eprintln!("steps: {}", p.steps.len());
        // Now try evaluating with state=Init
        let mut input_env = HashMap::new();
        input_env.insert("state".to_string(), Value::Enum {
            enum_name: "HelloState".into(),
            variant:   "Init".into(),
            fields:    vec![],
        });
        let r = eval_program(&p, &input_env, Some(enums));
        eprintln!("eval with state=Init: {r:?}");

        // And with state=Done
        let mut input_env2 = HashMap::new();
        input_env2.insert("state".to_string(), Value::Enum {
            enum_name: "HelloState".into(),
            variant:   "Done".into(),
            fields:    vec![],
        });
        let r2 = eval_program(&p, &input_env2, Some(enums));
        eprintln!("eval with state=Done: {r2:?}");
    }
}
