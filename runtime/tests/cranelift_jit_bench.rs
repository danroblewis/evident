//! Bench: native JIT vs Z3 AST interpreter vs full Z3 solver.
//!
//! Hand-built Z3Program for a counter-style claim
//! (`next = current + 1`); calls each path 100k times and
//! compares times.

use std::collections::HashMap;
use std::time::Instant;
use evident_runtime::Value;
use evident_runtime::z3_eval::{simplify_assertions, extract_program, eval_program};
use evident_runtime::cranelift_jit::compile_program;

#[test]
fn jit_vs_walker_int_chain() {
    use z3::{Config, Context};
    use z3::ast::{Ast, Int as Z3Int};
    let cfg = Config::new();
    let ctx: &'static Context = Box::leak(Box::new(Context::new(&cfg)));

    let current = Z3Int::new_const(ctx, "current");
    let next    = Z3Int::new_const(ctx, "next");
    let doubled = Z3Int::new_const(ctx, "doubled");
    let triple  = Z3Int::new_const(ctx, "triple");

    let one = Z3Int::from_i64(ctx, 1);
    let two = Z3Int::from_i64(ctx, 2);
    let three = Z3Int::from_i64(ctx, 3);

    let assertions = vec![
        next._eq(&Z3Int::add(ctx, &[&current, &one])),
        doubled._eq(&Z3Int::mul(ctx, &[&next, &two])),
        triple._eq(&Z3Int::mul(ctx, &[&doubled, &three])),
    ];

    let simplified = simplify_assertions(ctx, &assertions).formulas;
    let outputs = vec!["next".to_string(), "doubled".to_string(), "triple".to_string()];
    let program = extract_program(&simplified, &outputs).expect("extract");

    let enums = evident_runtime::translate::EnumRegistry::default();
    let jit = compile_program(&program, &enums).expect("compile");

    const N: usize = 100_000;
    let mut env_template = HashMap::new();
    env_template.insert("current".to_string(), Value::Int(7));

    // Time JIT.
    let t0 = Instant::now();
    let mut sink_jit = 0i64;
    for i in 0..N {
        let mut env = HashMap::new();
        env.insert("current".to_string(), Value::Int(i as i64));
        let r = jit.call(&env).expect("jit call");
        if let Some(Value::Int(n)) = r.get("triple") { sink_jit ^= n; }
    }
    let jit_time = t0.elapsed();

    // Time AST walker.
    let t0 = Instant::now();
    let mut sink_walker = 0i64;
    for i in 0..N {
        let mut env = HashMap::new();
        env.insert("current".to_string(), Value::Int(i as i64));
        let r = eval_program(&program, &env, None).expect("walker eval");
        if let Some(Value::Int(n)) = r.get("triple") { sink_walker ^= n; }
    }
    let walker_time = t0.elapsed();

    eprintln!("N = {N}");
    eprintln!("JIT:    {jit_time:?} ({:>4.1}ns/call)", jit_time.as_nanos() as f64 / N as f64);
    eprintln!("Walker: {walker_time:?} ({:>4.1}ns/call)", walker_time.as_nanos() as f64 / N as f64);
    eprintln!("Speedup: {:.1}x", walker_time.as_secs_f64() / jit_time.as_secs_f64());

    let _ = env_template;
    let _ = (sink_jit, sink_walker);
}
