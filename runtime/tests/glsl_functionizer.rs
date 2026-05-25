//! Integration tests for the GLSL functionizer (`functionize/glsl.rs`).
//!
//! macOS-only: `GlslFunctionizer` compiles a `Z3Program` to a GLSL
//! fragment shader and runs it on a headless CGL context. Each test
//! hand-builds a `Z3Program`, compiles it with BOTH the GLSL strategy
//! and the default Cranelift strategy, and asserts the two produce
//! identical bindings for the same inputs (acceptance criterion #4:
//! output verified against Cranelift across 5 claim shapes).
//!
//! If no headless GL context can be created (a hypothetical GL-less
//! environment), the GLSL tests print a skip notice and pass — the
//! headless-OK gate. On the macOS dev machine the context is created
//! via CGL and the cross-validation runs for real.

#![cfg(target_os = "macos")]

use std::cell::RefCell;
use std::collections::HashMap;

use z3::ast::{Ast, Dynamic, Int};
use z3::{Config, Context};

use evident_runtime::functionize::cranelift::CraneliftFunctionizer;
use evident_runtime::functionize::glsl::GlslFunctionizer;
use evident_runtime::functionize::{CompiledFunction, Functionizer};
use evident_runtime::translate::{DatatypeRegistry, EnumRegistry};
use evident_runtime::z3_eval::{Z3Program, Z3Step};
use evident_runtime::Value;

// ── helpers ──────────────────────────────────────────────────────────

fn empty_registries() -> (EnumRegistry, DatatypeRegistry) {
    (EnumRegistry::new(), RefCell::new(HashMap::new()))
}

fn gi(name: &str, v: i64) -> (String, Value) {
    (name.to_string(), Value::Int(v))
}

fn given(pairs: &[(&str, i64)]) -> HashMap<String, Value> {
    pairs.iter().map(|(n, v)| gi(n, *v)).collect()
}

/// Compile `program` with the GLSL functionizer. Returns `None` (with a
/// skip notice) if no headless GL context exists, so the suite stays
/// green in a GL-less environment.
fn glsl_compiled<'c>(
    program: &Z3Program<'c>,
) -> Option<std::rc::Rc<dyn CompiledFunction>> {
    let fz = match GlslFunctionizer::new() {
        Ok(f) => f,
        Err(e) => {
            eprintln!("[glsl test] skipping — no headless GL context: {e}");
            return None;
        }
    };
    let (enums, dts) = empty_registries();
    Some(
        fz.compile(program, &enums, &dts)
            .expect("GLSL should compile this scalar Int/Bool program"),
    )
}

fn cranelift_compiled<'c>(program: &Z3Program<'c>) -> std::rc::Rc<dyn CompiledFunction> {
    let (enums, dts) = empty_registries();
    CraneliftFunctionizer
        .compile(program, &enums, &dts)
        .expect("Cranelift should compile this scalar Int/Bool program")
}

/// Cross-validate: GLSL output must equal Cranelift output for every
/// given. Skips (returns) if no GL context.
fn cross_validate(program: &Z3Program, givens: &[HashMap<String, Value>]) {
    let Some(glsl) = glsl_compiled(program) else { return };
    let cl = cranelift_compiled(program);
    for g in givens {
        let gout = glsl.call(g).expect("glsl call");
        let cout = cl.call(g).expect("cranelift call");
        assert_eq!(
            gout, cout,
            "GLSL vs Cranelift mismatch for given={g:?}\n  glsl={gout:?}\n  cl  ={cout:?}"
        );
    }
}

fn scalar<'c>(var: &str, expr: impl Ast<'c>) -> Z3Step<'c> {
    Z3Step::Scalar {
        var: var.to_string(),
        expr: Dynamic::from_ast(&expr),
    }
}

fn program(steps: Vec<Z3Step>) -> Z3Program {
    Z3Program {
        steps,
        checks: vec![],
        predicates: vec![],
        label: None,
    }
}

// ── Test 1: scalar Int arithmetic (acceptance criterion #3) ──────────

/// `output ∈ Int = 3 * input + 5`. The minimal acceptance case:
/// input=7 → output=26, matching Cranelift.
#[test]
fn scalar_affine() {
    let cfg = Config::new();
    let ctx = Context::new(&cfg);
    let input = Int::new_const(&ctx, "input");
    let three = Int::from_i64(&ctx, 3);
    let five = Int::from_i64(&ctx, 5);
    let expr = Int::add(&ctx, &[&Int::mul(&ctx, &[&input, &three]), &five]);
    let prog = program(vec![scalar("output", expr)]);

    // Explicit acceptance check: input=7 → output=26.
    if let Some(glsl) = glsl_compiled(&prog) {
        let out = glsl.call(&given(&[("input", 7)])).expect("call");
        assert_eq!(out.get("output"), Some(&Value::Int(26)));
    }

    cross_validate(
        &prog,
        &[
            given(&[("input", 7)]),
            given(&[("input", 0)]),
            given(&[("input", -3)]),
            given(&[("input", 100)]),
            given(&[("input", -100)]),
        ],
    );
}

// ── Test 2: Bool comparison ──────────────────────────────────────────

/// `out ∈ Bool = (a > b)`. Both true and false cases.
#[test]
fn bool_comparison() {
    let cfg = Config::new();
    let ctx = Context::new(&cfg);
    let a = Int::new_const(&ctx, "a");
    let b = Int::new_const(&ctx, "b");
    let expr = a.gt(&b);
    let prog = program(vec![scalar("out", expr)]);

    if let Some(glsl) = glsl_compiled(&prog) {
        assert_eq!(
            glsl.call(&given(&[("a", 5), ("b", 3)])).unwrap().get("out"),
            Some(&Value::Bool(true))
        );
        assert_eq!(
            glsl.call(&given(&[("a", 2), ("b", 9)])).unwrap().get("out"),
            Some(&Value::Bool(false))
        );
    }

    cross_validate(
        &prog,
        &[
            given(&[("a", 5), ("b", 3)]),
            given(&[("a", 2), ("b", 9)]),
            given(&[("a", 4), ("b", 4)]),
            given(&[("a", -1), ("b", -8)]),
        ],
    );
}

// ── Test 3: ternary / ITE with unary minus ──────────────────────────

/// `out ∈ Int = (a > 0 ? a * 2 : -a)`. Positive, negative, zero.
#[test]
fn ternary_ite() {
    let cfg = Config::new();
    let ctx = Context::new(&cfg);
    let a = Int::new_const(&ctx, "a");
    let zero = Int::from_i64(&ctx, 0);
    let two = Int::from_i64(&ctx, 2);
    let then_branch = Int::mul(&ctx, &[&a, &two]);
    let else_branch = a.unary_minus();
    let expr = a.gt(&zero).ite(&then_branch, &else_branch);
    let prog = program(vec![scalar("out", expr)]);

    if let Some(glsl) = glsl_compiled(&prog) {
        assert_eq!(
            glsl.call(&given(&[("a", 5)])).unwrap().get("out"),
            Some(&Value::Int(10))
        );
        assert_eq!(
            glsl.call(&given(&[("a", -4)])).unwrap().get("out"),
            Some(&Value::Int(4))
        );
        assert_eq!(
            glsl.call(&given(&[("a", 0)])).unwrap().get("out"),
            Some(&Value::Int(0))
        );
    }

    cross_validate(
        &prog,
        &[
            given(&[("a", 5)]),
            given(&[("a", -4)]),
            given(&[("a", 0)]),
            given(&[("a", 37)]),
            given(&[("a", -99)]),
        ],
    );
}

// ── Test 4: chained Int conditional updates (test_29 shape) ──────────

/// `b = (a > 10 ? a - 5 : a + 7); c = (b > 20 ? b * 2 : b)`. The second
/// step references the first output — exercises the topo-ordered
/// output-references-output path. Verified against Cranelift on 10
/// inputs.
#[test]
fn chained_int_update() {
    let cfg = Config::new();
    let ctx = Context::new(&cfg);
    let a = Int::new_const(&ctx, "a");
    let b_const = Int::new_const(&ctx, "b");

    let b_expr = a.gt(&Int::from_i64(&ctx, 10)).ite(
        &Int::sub(&ctx, &[&a, &Int::from_i64(&ctx, 5)]),
        &Int::add(&ctx, &[&a, &Int::from_i64(&ctx, 7)]),
    );
    let c_expr = b_const.gt(&Int::from_i64(&ctx, 20)).ite(
        &Int::mul(&ctx, &[&b_const, &Int::from_i64(&ctx, 2)]),
        &b_const,
    );
    let prog = program(vec![scalar("b", b_expr), scalar("c", c_expr)]);

    let inputs = [-30, -7, 0, 5, 10, 11, 12, 25, 26, 100];
    let givens: Vec<_> = inputs.iter().map(|a| given(&[("a", *a)])).collect();
    cross_validate(&prog, &givens);
}

// ── Test 5: multi-output ─────────────────────────────────────────────

/// `out_a = x + y; out_b = x - y; out_c = (x > y)`. All three outputs
/// read back from one 1×3 draw and verified against Cranelift.
#[test]
fn multi_output() {
    let cfg = Config::new();
    let ctx = Context::new(&cfg);
    let x = Int::new_const(&ctx, "x");
    let y = Int::new_const(&ctx, "y");
    let sum = Int::add(&ctx, &[&x, &y]);
    let diff = Int::sub(&ctx, &[&x, &y]);
    let gt = x.gt(&y);
    let prog = program(vec![
        scalar("out_a", sum),
        scalar("out_b", diff),
        scalar("out_c", gt),
    ]);

    if let Some(glsl) = glsl_compiled(&prog) {
        let out = glsl.call(&given(&[("x", 12), ("y", 5)])).unwrap();
        assert_eq!(out.get("out_a"), Some(&Value::Int(17)));
        assert_eq!(out.get("out_b"), Some(&Value::Int(7)));
        assert_eq!(out.get("out_c"), Some(&Value::Bool(true)));
    }

    cross_validate(
        &prog,
        &[
            given(&[("x", 12), ("y", 5)]),
            given(&[("x", -3), ("y", 8)]),
            given(&[("x", 0), ("y", 0)]),
            given(&[("x", 1000), ("y", -1000)]),
        ],
    );
}

// ── Test 6: graceful refusal of an unsupported shape ─────────────────

/// A String-valued output is outside GLSL's scalar Int/Bool scope, so
/// `compile` returns `None` (the runtime would fall through to Z3).
#[test]
fn refuses_string_output() {
    let cfg = Config::new();
    let ctx = Context::new(&cfg);
    let greeting = z3::ast::String::from_str(&ctx, "hi").unwrap();
    let prog = program(vec![scalar("msg", greeting)]);

    let fz = match GlslFunctionizer::new() {
        Ok(f) => f,
        Err(e) => {
            eprintln!("[glsl test] skipping — no headless GL context: {e}");
            return;
        }
    };
    let (enums, dts) = empty_registries();
    assert!(
        fz.compile(&prog, &enums, &dts).is_none(),
        "GLSL must refuse a String-valued output (→ Z3 fallback)"
    );
}

// ── Benchmark: GLSL vs Cranelift on test_29's 30-step chain ──────────

/// Build test_29's chain-A shape: `seed_a` then `n` conditional-update
/// steps, each applying one of four branch-dependent arithmetic rules
/// to the previous value. This is the workload where the JIT genuinely
/// wins over Z3 (per the test_29 docstring); here it's a pure
/// per-call-cost comparison between the two functionizers.
fn build_chain<'c>(ctx: &'c Context, n: usize) -> Z3Program<'c> {
    let mut steps: Vec<Z3Step<'c>> = Vec::new();
    let mut prev_name = "seed_a".to_string();
    for i in 1..=n {
        let prev = Int::new_const(ctx, prev_name.as_str());
        let k = |v: i64| Int::from_i64(ctx, v);
        // Four repeating rules (threshold, then-op, else-op).
        let expr = match (i - 1) % 4 {
            0 => prev.gt(&k(50)).ite(
                &Int::sub(ctx, &[&prev, &k(7)]),
                &Int::add(ctx, &[&Int::mul(ctx, &[&prev, &k(2)]), &k(11)]),
            ),
            1 => prev.gt(&k(100)).ite(
                &Int::sub(ctx, &[&prev, &k(13)]),
                &Int::add(ctx, &[&Int::mul(ctx, &[&prev, &k(3)]), &k(7)]),
            ),
            2 => prev.gt(&k(75)).ite(
                &Int::add(ctx, &[&prev, &k(19)]),
                &Int::sub(ctx, &[&prev, &k(5)]),
            ),
            _ => prev.gt(&k(200)).ite(
                &Int::sub(ctx, &[&prev, &k(100)]),
                &Int::add(ctx, &[&prev, &k(23)]),
            ),
        };
        let var = format!("a{i:02}");
        steps.push(scalar(&var, expr));
        prev_name = var;
    }
    program(steps)
}

/// Correctness on the 30-step chain (the test_29 shape): GLSL must match
/// Cranelift on the final value `a30` across 64 inputs. When
/// `EVIDENT_GLSL_BENCH=1` is set, also runs a 5000-call single-shot
/// timing loop and prints per-call cost for both functionizers — the
/// benchmark reported in the GLSL functionizer doc. (Env-gated rather
/// than `#[ignore]`d so the correctness check always runs; lint AP-005
/// forbids `#[ignore]` in tests, and rightly so.)
#[test]
fn chain_matches_cranelift_and_bench() {
    use std::time::Instant;

    let cfg = Config::new();
    let ctx = Context::new(&cfg);
    let prog = build_chain(&ctx, 30);

    let Some(glsl) = glsl_compiled(&prog) else { return };
    let cl = cranelift_compiled(&prog);

    // Correctness: GLSL must match Cranelift on the chain's final value.
    for i in 0..64i64 {
        let g = given(&[("seed_a", (i * 7) % 300)]);
        assert_eq!(
            glsl.call(&g).unwrap().get("a30"),
            cl.call(&g).unwrap().get("a30"),
            "chain mismatch at {g:?}"
        );
    }

    if std::env::var("EVIDENT_GLSL_BENCH").is_err() {
        return;
    }

    let n = 5000usize;
    let g0 = Instant::now();
    for i in 0..n {
        let g = given(&[("seed_a", (i as i64 * 7) % 300)]);
        std::hint::black_box(glsl.call(&g));
    }
    let glsl_each = g0.elapsed().as_secs_f64() * 1e6 / n as f64;

    let c0 = Instant::now();
    for i in 0..n {
        let g = given(&[("seed_a", (i as i64 * 7) % 300)]);
        std::hint::black_box(cl.call(&g));
    }
    let cl_each = c0.elapsed().as_secs_f64() * 1e6 / n as f64;

    eprintln!("\n=== bench: 30-step chain, {n} single-shot calls ===");
    eprintln!("  GLSL      : {glsl_each:8.2} µs/call");
    eprintln!("  Cranelift : {cl_each:8.2} µs/call");
    eprintln!("  ratio     : {:.1}× (GLSL / Cranelift)\n", glsl_each / cl_each);
}
