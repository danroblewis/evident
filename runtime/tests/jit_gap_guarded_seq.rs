//! Gap coverage (session T): Seq-bodied `Guarded` steps now compile.
//!
//! `compile_program` used to refuse ANY program containing a
//! `Z3Step::Guarded` (the `effects = match state ⇒ ⟨…⟩` shape that
//! 24/27 demos have), falling through to the slow Z3 solve. The fix
//! compiles Seq-bodied Guarded steps via the existing branch-chain
//! codegen, plus a runtime bail flag: if no guard matches at run time
//! the JIT sets the flag and `JitProgram::call` returns `None`, so the
//! caller falls through to the slow solve — the same None-style bailout
//! the slow path always provided. (Scalar-bodied Guarded steps — a
//! match-to-scalar — stay refused; see docs/jit-codegen-gaps.md.)

use std::collections::HashMap;
use evident_runtime::{EvidentRuntime, Value};
use evident_runtime::z3_eval::{simplify_assertions, extract_program};
use evident_runtime::functionize::cranelift::compile_program;

fn jit_eval(src: &str, claim: &str, output: &str,
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
        .expect("Seq-bodied Guarded step must JIT-compile");
    let bindings = jit.call(&given).expect("compiled fn call");
    bindings.get(output).cloned().expect("output binding present")
}

#[test]
fn jit_guarded_seq_match_picks_right_branch() {
    // `out = match state { A ⇒ ⟨1,2⟩ | B ⇒ ⟨3,4⟩ }` — a Guarded step
    // with Seq bodies. The matching guard's body is the result.
    let src = r#"
enum St = A | B
claim g
    state ∈ St
    out ∈ Seq(Int)
    out = match state
        A ⇒ ⟨1, 2⟩
        B ⇒ ⟨3, 4⟩
"#;
    let mut given_a = HashMap::new();
    given_a.insert("state".to_string(), Value::Enum {
        enum_name: "St".into(), variant: "A".into(), fields: vec![] });
    assert_eq!(jit_eval(src, "g", "out", given_a),
               Value::SeqInt(vec![1, 2]));

    let mut given_b = HashMap::new();
    given_b.insert("state".to_string(), Value::Enum {
        enum_name: "St".into(), variant: "B".into(), fields: vec![] });
    assert_eq!(jit_eval(src, "g", "out", given_b),
               Value::SeqInt(vec![3, 4]));
}
