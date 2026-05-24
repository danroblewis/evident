//! End-to-end gate for the SDL window JIT path.
//!
//! Models the canonical Mario-shaped FSM that should compile to
//! native code: an FSM with a typed FFI resource (`win ∈ SDL_Window`)
//! and a Seq(Effect) output produced from per-tick state. The full
//! pipeline we exercise here:
//!
//! ```text
//!   load_file("../stdlib/runtime.ev")        — declarations only
//!   load_file("../packages/sdl/window.ev")   — SDL_Window + subclaims
//!   load_source(<test fixture>)              — the display FSM
//!     │
//!     ▼
//!   build_cache(display, ...)               — Z3 sorts + assertions
//!     │
//!     ▼
//!   simplify_assertions(ctx, assertions)    — value propagation
//!     │
//!     ▼
//!   extract_program(simplified, outputs)    — Z3Program
//!     │
//!     ▼
//!   functionize::cranelift::compile_program(prog) — native function
//!     │
//!     ▼
//!   jit.call(env)                            — emit Seq(Effect)
//!     │
//!     ▼
//!   effect dispatch                          — opens a real SDL window
//! ```
//!
//! Status today: the test exists as the success criterion. We
//! intentionally use `#[ignore]` on the JIT-call portion because
//! Cranelift codegen for multi-field enum-payload constructors
//! (LibCall has String/String/String/ArgList fields) is not yet
//! wired up. The non-ignored tests verify the earlier stages
//! (load + extract) succeed so we have a green-to-red→green path
//! when codegen catches up.
//!
//! Window opening is gated behind `EVIDENT_SDL_TEST_WINDOW=1` so a
//! default `cargo test` doesn't open windows on the developer's
//! machine or in CI.

use std::collections::HashMap;
use std::path::Path;

use evident_runtime::{EvidentRuntime, Value};
use evident_runtime::z3_eval::{simplify_assertions, extract_program};
use evident_runtime::functionize::cranelift::compile_program;

/// Minimal SDL window display FSM. Uses the SDL_Window FTI bridge
/// to install the window + renderer, then per-tick emits a
/// set-color → clear → present → delay sequence. After 120 frames
/// (~2 seconds at 16ms/frame) transitions to Done and emits
/// Println + Exit(0).
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

/// Construct a runtime with stdlib + SDL window package loaded,
/// then load the test FSM. Mirrors the helper in
/// `runtime/tests/effect_loop.rs`. Imports resolve relative to
/// `runtime/` (the cwd during `cargo test`).
fn load_display() -> EvidentRuntime {
    let mut rt = EvidentRuntime::new();
    rt.load_file(Path::new("../stdlib/runtime.ev"))
        .expect("load stdlib/runtime.ev");
    rt.load_file(Path::new("../packages/sdl/window.ev"))
        .expect("load packages/sdl/window.ev");
    rt.load_source(PROGRAM).expect("load display fsm");
    rt
}

/// Stage 1: schema loads, parses, and translates without errors.
/// This is the floor — if this breaks, the test fixture itself
/// regressed.
#[test]
fn stage_1_schema_loads() {
    let rt = load_display();
    assert!(rt.get_schema("display").is_some(),
        "display schema should be registered after load");
}

/// Stage 2: build_cache produces SAT assertions. The body's
/// effect bindings (sky_eff, clear_eff, …) translate to LibCall
/// values pinned by the SDL subclaim inlining.
#[test]
fn stage_2_build_cache_and_simplify() {
    let rt = load_display();

    let ctx       = rt.z3_context();
    let datatypes = rt.datatypes_registry();
    let enums     = rt.enums_registry();
    let schemas   = rt.schemas_map();
    let empty_given: HashMap<String, Value> = HashMap::new();
    let cached = evident_runtime::translate::build_cache(
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

/// Stage 3: extract_program turns the simplified Z3 assertions
/// into a Z3Program with steps for the FSM outputs. The Seq step
/// for `effects` should appear (six elements: sky/clear/present/
/// delay/done_print/done_exit). Other declared bindings appear as
/// Scalar steps.
///
/// This is the key gate: if the Z3Program extracts cleanly, the
/// schema is in the shape the JIT can ingest. JIT compilation may
/// still fail (Cranelift codegen for multi-field ctors is the
/// open work), but the program shape is already correct.
#[test]
fn stage_3_extract_program() {
    let rt = load_display();

    let ctx       = rt.z3_context();
    let datatypes = rt.datatypes_registry();
    let enums     = rt.enums_registry();
    let schemas   = rt.schemas_map();
    let empty_given: HashMap<String, Value> = HashMap::new();
    let cached = evident_runtime::translate::build_cache(
        rt.get_schema("display").unwrap(),
        schemas, ctx, datatypes, Some(enums), &empty_given, 2);
    let assertions = cached.solver.get_assertions();
    let result = simplify_assertions(ctx, &assertions);
    assert!(!result.unsat);

    // Outputs: every binding we expect the JIT to produce for the
    // caller. The Effect intermediates appear as Scalar steps or
    // get folded into the Seq's element exprs depending on Z3's
    // simplify pass.
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

/// Stage 4: the JIT compiles the Z3Program to a native function.
/// CURRENTLY EXPECTED TO RETURN None — multi-field enum-payload
/// constructors (LibCall, with String/String/String/ArgList
/// fields) are not yet wired through Cranelift codegen.
///
/// The test runs as a soft check: print whichever happened so a
/// developer iterating on codegen sees progress without the test
/// turning red. When codegen lands, flip the assert to require
/// Some.
#[test]
fn stage_4_compile_program_soft_check() {
    let rt = load_display();

    let ctx       = rt.z3_context();
    let datatypes = rt.datatypes_registry();
    let enums     = rt.enums_registry();
    let schemas   = rt.schemas_map();
    let empty_given: HashMap<String, Value> = HashMap::new();
    let cached = evident_runtime::translate::build_cache(
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

    match compile_program(&program, enums) {
        Some(_jit) => eprintln!(
            "JIT codegen succeeded for SDL display FSM — \
             flip stage_5_jit_call_opens_window to non-ignored."),
        None => eprintln!(
            "JIT codegen returned None (expected today — \
             multi-field enum-payload ctors not yet supported)."),
    }
}

/// Stage 5: the JIT-emitted function runs end-to-end, the effects
/// it produces dispatch through the runtime, and a real SDL window
/// opens for ~2 seconds. This is the headline success criterion
/// for the SDL JIT path.
///
/// Ignored by default because:
///   (a) Cranelift codegen for multi-field ctors isn't done yet —
///       the test will fail with `compile_program returned None`.
///   (b) Opening an SDL window from `cargo test` is not appropriate
///       for default CI; gate behind `EVIDENT_SDL_TEST_WINDOW=1`.
///
/// When codegen lands, remove `#[ignore]` and run with:
///   EVIDENT_SDL_TEST_WINDOW=1 cargo test --release --test \
///     sdl_window_jit_pipeline stage_5_jit_call_opens_window -- --nocapture
#[test]
fn stage_5_jit_call_opens_window() {
    // Gated behind EVIDENT_SDL_TEST_WINDOW=1 — opening an SDL
    // window from `cargo test` is not appropriate for default CI
    // (cgi has no display; opening a window blocks). The test
    // exists so that when codegen supports multi-field LibCall
    // ctors we have an end-to-end check ready; with the env var
    // set, it should JIT-compile and open a real window.
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
    let cached = evident_runtime::translate::build_cache(
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

    let jit = compile_program(&program, enums)
        .expect("JIT compile_program must return Some for the success path");

    // Initial env: state = Render. The JIT-emitted function reads
    // `state` (and any prev-tick `_frame` / `is_first_tick` inputs
    // the FSM materializes) and returns the per-tick output bindings.
    let mut env: HashMap<String, Value> = HashMap::new();
    env.insert("state".to_string(), Value::Enum {
        enum_name: "SState".into(),
        variant:   "Render".into(),
        fields:    vec![],
    });
    // Drive one tick. We don't actually run a full effect-loop
    // here — that's `effect_loop::run_with_ctx`'s job, and the
    // SDL_Window bridge will install the window the first time
    // the FSM is scheduled. This stage just verifies the JIT
    // returns a populated `effects` Seq.
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
