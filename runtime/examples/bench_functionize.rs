//! Bench: native substitution-chain evaluator vs. Z3 solve on a
//! function-shaped claim. Confirms the function-izer's value when
//! the constraints actually compile to a chain.
//!
//! Run:  cargo run --release --example bench_functionize

use evident_runtime::functionize::{evaluate_chain, extract_chain};
use evident_runtime::{EvidentRuntime, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Instant;

fn write_pair_source() -> PathBuf {
    let path = std::env::temp_dir().join(format!("bench_func_pair_{}.ev", std::process::id()));
    let src = r#"claim Pair
    a ∈ Int
    b ∈ Int
    sum ∈ Int
    prod ∈ Int
    diff ∈ Int
    neg_a ∈ Int
    sum = a + b
    prod = a * b
    diff = a - b
    neg_a = 0 - a
"#;
    std::fs::write(&path, src).unwrap();
    path
}

fn main() {
    let path = write_pair_source();
    let mut rt = EvidentRuntime::new();
    rt.load_file(&path).unwrap();

    // Pin a=5, b=3. Expected: sum=8, prod=15, diff=2, neg_a=-5.
    let mut given = HashMap::new();
    given.insert("a".to_string(), Value::Int(5));
    given.insert("b".to_string(), Value::Int(3));

    // ── 1. Verify both paths produce the same answer ──
    let z3_result = rt.query("Pair", &given).unwrap();
    assert!(z3_result.satisfied);
    println!("Z3 model:");
    for var in &["a", "b", "sum", "prod", "diff", "neg_a"] {
        println!("  {} = {:?}", var, z3_result.bindings.get(*var));
    }

    let comps = rt.classify_components("Pair", &given).unwrap();
    let func_comps: Vec<_> = comps.iter().filter(|c| c.functional).collect();
    println!("\nFunctional components: {} (of {} total)",
        func_comps.len(), comps.len());
    let schema = rt.get_schema("Pair").expect("Pair schema").clone();

    // Extract chains for each functional component; merge into one
    // big chain (each substitution refers only to inputs + prior steps).
    let mut all_steps = Vec::new();
    for fc in &func_comps {
        if let Some(chain) = extract_chain(&schema, &fc.component) {
            all_steps.extend(chain.steps);
        }
    }
    let merged = evident_runtime::functionize::SubstitutionChain { steps: all_steps, checks: vec![] };
    println!("Extracted {} substitution steps", merged.steps.len());

    let native_result = evaluate_chain(&merged, &given).unwrap();
    println!("Native model:");
    for var in &["a", "b", "sum", "prod", "diff", "neg_a"] {
        println!("  {} = {:?}", var, native_result.get(*var));
    }
    // Verify against Z3.
    for var in &["sum", "prod", "diff", "neg_a"] {
        let z3v = z3_result.bindings.get(*var);
        let nv = native_result.get(*var);
        assert_eq!(z3v, nv, "Z3 and native differ on {}: z3={:?} native={:?}",
            var, z3v, nv);
    }
    println!("✓ Z3 and native agree on all outputs");

    // ── 2. Bench: native vs Z3 ──
    const N: usize = 10_000;

    // Warm-up.
    for _ in 0..100 {
        let _ = rt.query("Pair", &given);
    }

    let t0 = Instant::now();
    for _ in 0..N {
        let _ = rt.query("Pair", &given);
    }
    let z3_total = t0.elapsed();
    let z3_per = z3_total.as_secs_f64() * 1_000_000.0 / N as f64;

    // Pre-extract the chain once (compile cost amortized).
    for _ in 0..100 {
        let _ = evaluate_chain(&merged, &given);
    }
    let t1 = Instant::now();
    for _ in 0..N {
        let _ = evaluate_chain(&merged, &given);
    }
    let native_total = t1.elapsed();
    let native_per = native_total.as_secs_f64() * 1_000_000.0 / N as f64;

    println!("\nBench ({} iterations, μs per call):", N);
    println!("  Z3 query:        {:>10.2} μs/call  (total {:?})", z3_per, z3_total);
    println!("  Native chain:    {:>10.2} μs/call  (total {:?})", native_per, native_total);
    println!("  Speedup:         {:.0}×", z3_per / native_per);

    let _ = std::fs::remove_file(&path);
}
