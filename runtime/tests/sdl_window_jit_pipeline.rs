use std::collections::HashMap;
use std::path::Path;

use evident_runtime::{EvidentRuntime, Value};
use evident_runtime::z3_eval::{simplify_assertions, extract_program};
use evident_runtime::functionize::cranelift::compile_program;

const PROGRAM: &str = r#"
enum SState = Render | Done

fsm display(state ∈ SState)
    win ∈ SDL_Window (title ↦ "JIT Test", width ↦ 320, height ↦ 240)

    frame ∈ Int = (is_first_tick ? 0 : _frame + 1)

    state_next = (frame ≥ 120 ? Done : Render)

    win.set_draw_color((120, 40, 200, 255), sky_eff)
    win.render_clear(clear_eff)
    win.render_present(present_eff)
    sdl_delay(16, delay_eff)

    done_print ∈ Effect = (state_next = Done ? Println("jit test done") : NoEffect)
    done_exit  ∈ Effect = (state_next = Done ? Exit(0) : NoEffect)

    effects ∈ Seq(Effect) = ⟨sky_eff, clear_eff, present_eff, delay_eff,
                              done_print, done_exit⟩
"#;

fn load_display() -> EvidentRuntime {
    let mut rt = EvidentRuntime::new();
    rt.load_file(Path::new("../stdlib/runtime.ev"))
        .expect("load stdlib/runtime.ev");
    rt.load_file(Path::new("../packages/sdl/window.ev"))
        .expect("load packages/sdl/window.ev");
    rt.load_source(PROGRAM).expect("load display fsm");
    rt
}

#[test]
fn stage_1_schema_loads() {
    let rt = load_display();
    assert!(rt.get_schema("display").is_some(),
        "display schema should be registered after load");
}

#[test]
fn stage_2_build_cache_and_simplify() {
    let rt = load_display();

    let ctx       = rt.z3_context();
    let datatypes = rt.datatypes_registry();
    let enums     = rt.enums_registry();
    let schemas   = rt.schemas_map();
    let empty_given: HashMap<String, Value> = HashMap::new();
    let cached = evident_runtime::encode::build_cache(
        rt.get_schema("display").unwrap(),
        schemas, ctx, datatypes, Some(enums), &empty_given, 2);
    let assertions = cached.solver.get_assertions();
    eprintln!("RAW assertions ({} total):", assertions.len());
    let result = simplify_assertions(ctx, &assertions);
    assert!(!result.unsat,
        "display body should be SAT (got unsat after simplify)");

    eprintln!("Simplified assertions ({} total):", result.formulas.len());
    for f in &result.formulas {
        eprintln!("  {f}");
    }
}

#[test]
fn stage_3_extract_program() {
    let rt = load_display();

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
    assert!(!result.unsat);

    let outputs = vec![
        "state_next".to_string(),
        "effects".to_string(),
        "frame".to_string(),
        "sky_eff".to_string(),
        "clear_eff".to_string(),
        "present_eff".to_string(),
        "delay_eff".to_string(),
        "done_print".to_string(),
        "done_exit".to_string(),
    ];
    let program = extract_program(&result.formulas, &outputs);
    assert!(program.is_some(),
        "extract_program should produce a Z3Program for the SDL display FSM");
    let program = program.unwrap();

    eprintln!("Z3Program: {} steps, {} checks, {} predicates",
        program.steps.len(), program.checks.len(), program.predicates.len());
    for (i, step) in program.steps.iter().enumerate() {
        use evident_runtime::z3_eval::Z3Step;
        match step {
            Z3Step::Scalar { var, expr } =>
                eprintln!("  step {i}: Scalar  {var} = {expr}"),
            Z3Step::Seq { var, elem_exprs } => {
                eprintln!("  step {i}: Seq     {var} = ⟨{} elems⟩", elem_exprs.len());
                for (j, e) in elem_exprs.iter().enumerate() {
                    eprintln!("    [{j}] = {e}");
                }
            }
            Z3Step::Guarded { var, branches } =>
                eprintln!("  step {i}: Guarded {var} ({} branches)", branches.len()),
            Z3Step::PreBaked { var, value } =>
                eprintln!("  step {i}: PreBaked {var} = {value:?}"),
        }
    }
}

#[test]
fn stage_4_compile_program_soft_check() {
    let rt = load_display();

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
        "state_next".to_string(),
        "effects".to_string(),
        "frame".to_string(),
        "sky_eff".to_string(),
        "clear_eff".to_string(),
        "present_eff".to_string(),
        "delay_eff".to_string(),
        "done_print".to_string(),
        "done_exit".to_string(),
    ];
    let program = extract_program(&result.formulas, &outputs)
        .expect("extract_program should succeed");

    match compile_program(&program, enums, datatypes) {
        Some(_jit) => eprintln!(
            "JIT codegen succeeded for SDL display FSM — \
             flip stage_5_jit_call_opens_window to non-ignored."),
        None => eprintln!(
            "JIT codegen returned None (expected today — \
             multi-field enum-payload ctors not yet supported)."),
    }
}

#[test]
fn stage_5_jit_call_opens_window() {

    if std::env::var("EVIDENT_SDL_TEST_WINDOW").ok().as_deref() != Some("1") {
        eprintln!("EVIDENT_SDL_TEST_WINDOW != 1; skipping window-open phase.");
        return;
    }

    let rt = load_display();

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
        "state_next".to_string(),
        "effects".to_string(),
        "frame".to_string(),
        "sky_eff".to_string(),
        "clear_eff".to_string(),
        "present_eff".to_string(),
        "delay_eff".to_string(),
        "done_print".to_string(),
        "done_exit".to_string(),
    ];
    let program = extract_program(&result.formulas, &outputs)
        .expect("extract_program should succeed");

    let jit = compile_program(&program, enums, datatypes)
        .expect("JIT compile_program must return Some for the success path");

    let mut env: HashMap<String, Value> = HashMap::new();
    env.insert("state".to_string(), Value::Enum {
        enum_name: "SState".into(),
        variant:   "Render".into(),
        fields:    vec![],
    });

    let bindings = jit.call(&env)
        .expect("JIT-compiled function call returned None");
    let effects = bindings.get("effects")
        .expect("effects binding should be present in JIT output");
    eprintln!("JIT output effects = {effects:?}");
    let Value::SeqEnum(elems) = effects else {
        panic!("effects not a SeqEnum: {effects:?}");
    };
    assert_eq!(elems.len(), 6,
        "expected 6 effects (sky/clear/present/delay/done_print/done_exit)");
}
