//! Snapshot test for the canonical "effects-producer" FSM
//! pattern.
//!
//! Mario `display` is the canonical example: a function from
//! world state to a Seq(Effect) of ordered FFI calls. NOT a
//! search problem. The runtime should compile this directly
//! to native code. The pipeline we want:
//!
//! ```text
//!   Evident AST
//!       │
//!       ├─→ STAGE 1: parse → SchemaDecl with BodyItems
//!       │
//!       ▼
//!   Z3 translation (declare + assert)
//!       │
//!       ├─→ STAGE 2: simplify + propagate-values
//!       │   Output: per-element Seq pins:
//!       │     (= effects__len N)
//!       │     (= (select effects 0) (LibCall "lib" "fn" "sig" ⟨args⟩))
//!       │     (= (select effects 1) (LibCall ...))
//!       │
//!       ▼
//!   extract_program
//!       │
//!       ├─→ STAGE 3: Z3Program with:
//!       │     steps: [Seq { var: "effects", elem_exprs: [
//!       │       (LibCall ...),
//!       │       (LibCall ...),
//!       │     ] }]
//!       │
//!       ▼
//!   Cranelift codegen
//!       │
//!       ├─→ STAGE 4: native function:
//!       │     fn(inputs: *const i64, outputs_seq: *mut Effect)
//!       │     - For each element, construct the Effect variant
//!       │     - Call Rust constructor helper (LibCall variant has
//!       │       String/Seq/ArgList payload — can't fit in i64
//!       │       slots)
//!       │
//!       ▼
//!   call(env) → Value::SeqEnum([Println("hello"), Exit(0), ...])
//! ```
//!
//! This test currently asserts what each stage SHOULD look
//! like. Where the pipeline can't produce it yet, the test
//! documents the gap with a `// TODO:` and an explicit
//! `Option::is_none()` check, so future work has a
//! red-test-to-green path.

use std::collections::HashMap;
use evident_runtime::{EvidentRuntime, Value};
use evident_runtime::z3_eval::{simplify_assertions, extract_program, Z3Step, GuardedBody};
use evident_runtime::cranelift_jit::compile_program;

// Minimal effects-producer test fixture. We use only enum
// variants WITHOUT Seq-of-enum payloads to avoid the translator's
// Seq-in-ctor-payload gap (which is its own piece of work; the
// fix lands in the auto-generated `__SeqOf_T` helper enums that
// stdlib's Effect inherits but our test fixture doesn't).
//
// Compositionally: an effects-producer takes inputs (state +
// maybe last_results) and produces a Seq(Effect) of ordered
// dispatches. The JIT codegen we want is the same as Mario's
// display: emit Vec<Effect> as a flat sequence of constructor
// calls populated from inputs.
const PROGRAM: &str = r#"
enum Effect = NoEffect | Println(String) | Exit(Int)
enum DState = Init | Done

claim display
    state ∈ DState
    state_next ∈ DState
    last_results ∈ Seq(Effect)
    effects ∈ Seq(Effect)

    state_next = Done

    eff_hello ∈ Effect = Println("hello")
    eff_world ∈ Effect = Println("world")
    eff_exit  ∈ Effect = Exit(0)

    effects = ⟨eff_hello, eff_world, eff_exit⟩
"#;

/// STAGE 2: with no given pinned, what does Z3's tactic chain
/// produce? We expect per-element Seq pins for the three Effect
/// outputs.
#[test]
fn stage_2_simplified_z3_assertions_match_per_element_pins() {
    let mut rt = EvidentRuntime::new();
    rt.load_source(PROGRAM).unwrap();

    let ctx       = rt.z3_context();
    let datatypes = rt.datatypes_registry();
    let enums     = rt.enums_registry();
    let schemas   = rt.schemas_map();
    let empty_given: HashMap<String, Value> = HashMap::new();
    let cached = evident_runtime::translate::build_cache(
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

    // We expect (in some order):
    //   - effects__len = 3
    //   - select effects 0 = LibCall(...)
    //   - select effects 1 = LibCall(...)
    //   - select effects 2 = Exit(0)
    //   - state_next = Done
    //   - plus type bounds (>= last_results__len 0, etc.)
    // Note: Z3's apply_seq_lengths pass folds the literal length
    // pin BEFORE body translation, so `effects__len` survives in
    // the simplified output only as the type-bound (>= 0). The
    // length is inferred at extract time from the consecutive
    // (select effects i) pins below.
    let has_state = formatted.iter().any(|s| s.contains("state_next") && s.contains("Done"));
    let has_hello = formatted.iter().any(|s| s.contains("select effects 0")
                                          && s.contains("Println")
                                          && s.contains("hello"));
    let has_world = formatted.iter().any(|s| s.contains("select effects 1")
                                          && s.contains("Println")
                                          && s.contains("world"));
    let has_exit  = formatted.iter().any(|s| s.contains("select effects 2")
                                          && s.contains("Exit"));

    assert!(has_state, "should pin state_next = Done");
    assert!(has_hello, "should pin select effects 0 = Println(\"hello\")");
    assert!(has_world, "should pin select effects 1 = Println(\"world\")");
    assert!(has_exit,  "should pin select effects 2 = Exit(0)");
}

/// STAGE 3: extract_program should turn those per-element pins
/// into a single Z3Step::Seq for `effects` plus a Z3Step::Scalar
/// for `state_next`.
#[test]
fn stage_3_extract_program_builds_seq_step() {
    let mut rt = EvidentRuntime::new();
    rt.load_source(PROGRAM).unwrap();

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

    // Outputs: anything declared in the body that isn't given.
    let outputs = vec![
        "state_next".to_string(),
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
        }
    }

    // We expect one Seq step (for effects) and at least one
    // Scalar step (state_next). The eff_init/eff_draw/eff_exit
    // outputs may or may not appear as separate Scalar steps —
    // Z3's simplify might fold them into the Seq elements
    // directly.
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
    assert!(scalar_steps.iter().any(|s| matches!(s, Z3Step::Scalar { var, .. } if var == "state_next")),
        "state_next should be a Scalar step");
}

/// STAGE 4: the JIT codegen should compile this program to a
/// native function that returns the same SeqEnum the AST walker
/// produces. CURRENTLY THIS FAILS — the JIT refuses Seq outputs
/// AND refuses payload-bearing enum constructors. This test
/// documents the gap.
///
/// Target output: the compiled function constructs:
///   Value::SeqEnum([
///       Value::Enum { variant: "LibCall", fields: [Str("libfoo.dylib"), Str("init"), Str("v()"), SeqEnum([])] },
///       Value::Enum { variant: "LibCall", fields: [Str("libfoo.dylib"), Str("draw"), Str("v(i)"), SeqEnum([Enum { variant: "ArgInt", fields: [Int(42)] }])] },
///       Value::Enum { variant: "Exit",    fields: [Int(0)] },
///   ])
#[test]
fn stage_4_jit_compiles_effects_producer() {
    let mut rt = EvidentRuntime::new();
    rt.load_source(PROGRAM).unwrap();

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
        "eff_hello".to_string(),
        "eff_world".to_string(),
        "eff_exit".to_string(),
    ];
    let program = extract_program(&result.formulas, &outputs).expect("extract");

    let jit = compile_program(&program, enums);
    // TODO(round-25): make this Some. Requires:
    //   1. Seq output codegen: allocate, fill elements, return.
    //   2. Payload-bearing constructor codegen: build Value::Enum
    //      with fields, either inline or via callback to Rust.
    //   3. String literal handling (intern table or static refs).
    //
    // When 1+2+3 land, this assertion flips to `is_some()` and we
    // assert on the SeqEnum content below.
    assert!(jit.is_none(),
        "JIT currently refuses Seq+payload programs — this is the gap we're closing in Round 25.");

    // Stage 4 final assertion (target):
    //
    // let jit = jit.expect("JIT should compile effects-producer");
    // let env: HashMap<String, Value> = HashMap::new();
    // let bindings = jit.call(&env).expect("jit call");
    // let effects = bindings.get("effects").expect("effects bound");
    // let Value::SeqEnum(elems) = effects else { panic!("not SeqEnum: {effects:?}"); };
    // assert_eq!(elems.len(), 3);
    // // ... assert on each element's variant + fields ...
}
