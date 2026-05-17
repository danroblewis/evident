//! Round-3 integration test: the function-izer compiles claims with
//! user-record-typed memberships (IVec2 / Point style).

use evident_runtime::{EvidentRuntime, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Instant;

fn tmp(tag: &str) -> PathBuf {
    std::env::temp_dir().join(format!("fz_rec_{}_{}.ev", std::process::id(), tag))
}

#[test]
fn record_typed_membership_compiles() {
    // `pos ∈ IVec2` declares pos.x and pos.y as Z3 consts (declare_and_assert).
    // Each is referenced as `pos.x` / `pos.y` in body — dotted identifier.
    let src = r#"type IVec2(x, y ∈ Int)

claim MoveRight
    pos ∈ IVec2
    next ∈ IVec2
    next.x = pos.x + 1
    next.y = pos.y
"#;
    let path = tmp("moveright");
    std::fs::write(&path, src).unwrap();
    let mut rt = EvidentRuntime::new();
    rt.load_file(&path).unwrap();

    let mut given = HashMap::new();
    given.insert("pos.x".into(), Value::Int(5));
    given.insert("pos.y".into(), Value::Int(7));

    // Z3 baseline.
    std::env::remove_var("EVIDENT_FUNCTIONIZE");
    let z3_r = rt.query("MoveRight", &given).unwrap();
    assert!(z3_r.satisfied);
    assert_eq!(z3_r.bindings.get("next.x"), Some(&Value::Int(6)));
    assert_eq!(z3_r.bindings.get("next.y"), Some(&Value::Int(7)));

    // Native via function-izer.
    std::env::set_var("EVIDENT_FUNCTIONIZE", "1");
    let native_r = rt.query("MoveRight", &given).unwrap();
    assert!(native_r.satisfied);
    assert_eq!(native_r.bindings.get("next.x"), z3_r.bindings.get("next.x"));
    assert_eq!(native_r.bindings.get("next.y"), z3_r.bindings.get("next.y"));
    let _ = std::fs::remove_file(&path);
}

#[test]
fn record_typed_bench() {
    let src = r#"type IVec2(x, y ∈ Int)

claim Step
    pos ∈ IVec2
    vel ∈ IVec2
    nxt ∈ IVec2
    nxt.x = pos.x + vel.x
    nxt.y = pos.y + vel.y
"#;
    let path = tmp("stepbench");
    std::fs::write(&path, src).unwrap();
    let mut rt = EvidentRuntime::new();
    rt.load_file(&path).unwrap();

    let mut given = HashMap::new();
    given.insert("pos.x".into(), Value::Int(100));
    given.insert("pos.y".into(), Value::Int(200));
    given.insert("vel.x".into(), Value::Int(5));
    given.insert("vel.y".into(), Value::Int(-3));

    // Warm-up.
    for _ in 0..50 {
        std::env::remove_var("EVIDENT_FUNCTIONIZE");
        let _ = rt.query("Step", &given);
        std::env::set_var("EVIDENT_FUNCTIONIZE", "1");
        let _ = rt.query("Step", &given);
    }
    const N: usize = 2_000;

    std::env::remove_var("EVIDENT_FUNCTIONIZE");
    let t0 = Instant::now();
    for _ in 0..N { let _ = rt.query("Step", &given); }
    let z3_per = t0.elapsed().as_secs_f64() * 1_000_000.0 / N as f64;

    std::env::set_var("EVIDENT_FUNCTIONIZE", "1");
    let t0 = Instant::now();
    for _ in 0..N { let _ = rt.query("Step", &given); }
    let native_per = t0.elapsed().as_secs_f64() * 1_000_000.0 / N as f64;

    println!("\nRecord-typed bench ({} iter):", N);
    println!("  Z3 query:     {:>10.2} μs/call", z3_per);
    println!("  Native (fz):  {:>10.2} μs/call", native_per);
    println!("  Speedup:      {:.0}×", z3_per / native_per);
    let _ = std::fs::remove_file(&path);

    assert!(native_per < z3_per,
        "function-izer ({:.2}μs) should beat Z3 ({:.2}μs) for record-typed claim",
        native_per, z3_per);
}
