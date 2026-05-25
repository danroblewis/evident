//! Gap coverage (session T): String concatenation `(str.++ a b)`.
//!
//! `emit_write_value` had no `DeclKind::SEQ_CONCAT` arm, so any Scalar
//! output whose value was a string concat (e.g. `world_next.trail =
//! "." ++ world.trail`, or a `Println("count = " ++ s)` payload) bailed
//! the whole program. The fix builds each operand into a temp slot and
//! calls the existing `ev_str_concat` helper. Operands reach this via
//! the String-literal short-circuit / UNINTERPRETED clone-from-env at
//! the top of emit_write_value, so a String input works as an operand.

use std::collections::HashMap;
use evident_runtime::{EvidentRuntime, Value};
use evident_runtime::z3_eval::{simplify_assertions, extract_program};
use evident_runtime::functionize::cranelift::compile_program;

fn jit_eval_scalar(src: &str, claim: &str, output: &str,
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
        .expect("str.++ Scalar output must JIT-compile");
    let bindings = jit.call(&given).expect("compiled fn call");
    bindings.get(output).cloned().expect("output binding present")
}

#[test]
fn jit_str_concat_literal_and_input() {
    // `out = "hi " ++ name` — literal prefix + String input.
    let src = r#"
claim greet
    name ∈ String
    out ∈ String = "hi " ++ name
"#;
    let mut given = HashMap::new();
    given.insert("name".to_string(), Value::Str("bob".to_string()));
    assert_eq!(jit_eval_scalar(src, "greet", "out", given),
               Value::Str("hi bob".to_string()));
}

#[test]
fn jit_str_concat_three_way() {
    let src = r#"
claim greet3
    a ∈ String
    b ∈ String
    out ∈ String = a ++ "-" ++ b
"#;
    let mut given = HashMap::new();
    given.insert("a".to_string(), Value::Str("x".to_string()));
    given.insert("b".to_string(), Value::Str("y".to_string()));
    assert_eq!(jit_eval_scalar(src, "greet3", "out", given),
               Value::Str("x-y".to_string()));
}
