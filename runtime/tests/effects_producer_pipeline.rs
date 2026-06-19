use std::collections::HashMap;
use evident_runtime::{EvidentRuntime, Value};
use evident_runtime::functionize::extract_program::{simplify_assertions, extract_program, Z3Step, GuardedBody};
use evident_runtime::functionize::cranelift::compile_program;

const PROGRAM: &str = r#"
enum Effect = NoEffect | Println(String) | Exit(Int)
enum DState = Init | Done

claim display
    state ∈ DState
    last_results ∈ Seq(Effect)
    effects ∈ Seq(Effect)

    state = Done

    eff_hello ∈ Effect = Println("hello")
    eff_world ∈ Effect = Println("world")
    eff_exit  ∈ Effect = Exit(0)

    effects = ⟨eff_hello, eff_world, eff_exit⟩
"#;

#[test]
fn stage_2_simplified_z3_assertions_match_per_element_pins() {
    let mut rt = EvidentRuntime::new();
    rt.load_source(PROGRAM).unwrap();

    let ctx       = rt.z3_context();
    let datatypes = rt.datatypes_registry();
    let enums     = rt.enums_registry();
    let schemas   = rt.schemas_map();
    let empty_given: HashMap<String, Value> = HashMap::new();
    let cached = evident_runtime::encode::build_cache(
        rt.get_schema("display").unwrap(),
        schemas, ctx, datatypes, Some(enums), &empty_given, 2);
    let assertions = cached.solver.get_assertions();
    eprintln!("RAW assertions before simplify ({} total):", assertions.len());
    for a in &assertions { eprintln!("  {a}"); }
    let result = simplify_assertions(ctx, &assertions);
    assert!(!result.unsat, "body should be SAT");

    let formatted: Vec<String> = result.formulas.iter().map(|f| format!("{f}")).collect();
    eprintln!("Simplified assertions ({} total):", formatted.len());
    for f in &formatted { eprintln!("  {f}"); }

    let has_state = formatted.iter().any(|s| s.contains("state") && s.contains("Done"));
    let has_hello = formatted.iter().any(|s| s.contains("select effects 0")
                                          && s.contains("Println")
                                          && s.contains("hello"));
    let has_world = formatted.iter().any(|s| s.contains("select effects 1")
                                          && s.contains("Println")
                                          && s.contains("world"));
    let has_exit  = formatted.iter().any(|s| s.contains("select effects 2")
                                          && s.contains("Exit"));

    assert!(has_state, "should pin state = Done");
    assert!(has_hello, "should pin select effects 0 = Println(\"hello\")");
    assert!(has_world, "should pin select effects 1 = Println(\"world\")");
    assert!(has_exit,  "should pin select effects 2 = Exit(0)");
}

#[test]
fn stage_3_extract_program_builds_seq_step() {
    let mut rt = EvidentRuntime::new();
    rt.load_source(PROGRAM).unwrap();

    let ctx       = rt.z3_context();
    let datatypes = rt.datatypes_registry();
    let enums     = rt.enums_registry();
    let schemas   = rt.schemas_map();
    let empty_given: HashMap<String, Value> = HashMap::new();
    let cached = evident_runtime::encode::build_cache(
        rt.get_schema("display").unwrap(),
        schemas, ctx, datatypes, Some(enums), &empty_given, 2);
    let assertions = cached.solver.get_assertions();
    let result = simplify_assertions(ctx, &assertions);

    let outputs = vec![
        "state".to_string(),
        "effects".to_string(),
        "eff_hello".to_string(),
        "eff_world".to_string(),
        "eff_exit".to_string(),
    ];
    let program = extract_program(&result.formulas, &outputs).expect("extract");

    eprintln!("Program: {} steps, {} checks, {} predicates",
        program.steps.len(), program.checks.len(), program.predicates.len());
    for (i, step) in program.steps.iter().enumerate() {
        match step {
            Z3Step::Scalar { var, expr }    => eprintln!("  step {i}: Scalar  {var} = {expr}"),
            Z3Step::Seq    { var, elem_exprs } => {
                eprintln!("  step {i}: Seq     {var} = ⟨{} elems⟩", elem_exprs.len());
                for (j, e) in elem_exprs.iter().enumerate() {
                    eprintln!("    [{j}] = {e}");
                }
            }
            Z3Step::Guarded { var, branches } =>
                eprintln!("  step {i}: Guarded {var} (with {} branches)", branches.len()),
            Z3Step::PreBaked { var, value } =>
                eprintln!("  step {i}: PreBaked {var} = {value:?}"),
        }
    }

    let seq_steps:   Vec<&Z3Step> = program.steps.iter()
        .filter(|s| matches!(s, Z3Step::Seq { .. })).collect();
    let scalar_steps: Vec<&Z3Step> = program.steps.iter()
        .filter(|s| matches!(s, Z3Step::Scalar { .. })).collect();

    assert_eq!(seq_steps.len(), 1, "exactly one Seq step (for `effects`)");
    match seq_steps[0] {
        Z3Step::Seq { var, elem_exprs } => {
            assert_eq!(var, "effects");
            assert_eq!(elem_exprs.len(), 3, "three effect elements");
        }
        _ => unreachable!(),
    }
    assert!(scalar_steps.iter().any(|s| matches!(s, Z3Step::Scalar { var, .. } if var == "state")),
        "state should be a Scalar step");
}

#[test]
fn stage_4_jit_compiles_effects_producer() {
    let mut rt = EvidentRuntime::new();
    rt.load_source(PROGRAM).unwrap();

    let ctx       = rt.z3_context();
    let datatypes = rt.datatypes_registry();
    let enums     = rt.enums_registry();
    let schemas   = rt.schemas_map();
    let empty_given: HashMap<String, Value> = HashMap::new();
    let cached = evident_runtime::encode::build_cache(
        rt.get_schema("display").unwrap(),
        schemas, ctx, datatypes, Some(enums), &empty_given, 2);
    let assertions = cached.solver.get_assertions();
    let result = simplify_assertions(ctx, &assertions);

    let outputs = vec![
        "state".to_string(),
        "effects".to_string(),
        "eff_hello".to_string(),
        "eff_world".to_string(),
        "eff_exit".to_string(),
    ];
    let program = extract_program(&result.formulas, &outputs).expect("extract");

    let jit = compile_program(&program, enums, datatypes);
    let jit = jit.expect("Round 26: JIT should compile Seq output + payload-bearing constructors");
    let env: HashMap<String, Value> = HashMap::new();
    let bindings = jit.call(&env).expect("jit call");
    let effects = bindings.get("effects").expect("effects bound");
    eprintln!("JIT output for effects = {effects:?}");
    let Value::SeqEnum(elems) = effects else {
        panic!("effects not SeqEnum: {effects:?}");
    };
    assert_eq!(elems.len(), 3, "three effect elements");

    assert!(matches!(&elems[0], Value::Enum { variant, fields, .. }
        if variant == "Println"
           && matches!(&fields[..], [Value::Str(s)] if s == "hello")),
        "elem 0 should be Println(\"hello\"), got: {:?}", elems[0]);

    assert!(matches!(&elems[1], Value::Enum { variant, fields, .. }
        if variant == "Println"
           && matches!(&fields[..], [Value::Str(s)] if s == "world")),
        "elem 1 should be Println(\"world\"), got: {:?}", elems[1]);

    assert!(matches!(&elems[2], Value::Enum { variant, fields, .. }
        if variant == "Exit"
           && matches!(&fields[..], [Value::Int(0)])),
        "elem 2 should be Exit(0), got: {:?}", elems[2]);

    assert!(matches!(bindings.get("state"),
        Some(Value::Enum { variant, .. }) if variant == "Done"),
        "state should be Done, got: {:?}", bindings.get("state"));
}
