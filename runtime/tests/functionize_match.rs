//! Round-2 integration test: the function-izer can now compile a
//! claim that uses `match` over an enum.

use evident_runtime::{EvidentRuntime, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Instant;

fn tmp(tag: &str) -> PathBuf {
    std::env::temp_dir().join(format!("fz_match_{}_{}.ev", std::process::id(), tag))
}

#[test]
fn match_dispatch_compiles_natively() {
    // A two-state machine claim. With state pinned, state_next is a
    // pure function of state via match dispatch.
    let src = r#"enum HelloState = Init | Done

claim HelloStep
    state ∈ HelloState
    state_next ∈ HelloState
    state_next = match state
        Init ⇒ Done
        Done ⇒ Done
"#;
    let path = tmp("hello");
    std::fs::write(&path, src).unwrap();
    let mut rt = EvidentRuntime::new();
    rt.load_file(&path).unwrap();

    // Pin state = Init. With the function-izer wired in, this should
    // route through native eval (Match dispatch).
    let mut given = HashMap::new();
    given.insert("state".into(), Value::Enum {
        enum_name: "HelloState".into(),
        variant: "Init".into(),
        fields: vec![],
    });

    // Run via rt.query — try BOTH paths and check they agree.
    std::env::set_var("EVIDENT_FUNCTIONIZE", "0");
    let z3_r = rt.query("HelloStep", &given).unwrap();
    assert!(z3_r.satisfied, "Z3 baseline must be SAT");
    assert_eq!(z3_r.bindings.get("state_next"), Some(&Value::Enum {
        enum_name: "HelloState".into(),
        variant: "Done".into(),
        fields: vec![],
    }), "expected state_next = Done; got {:?}", z3_r.bindings.get("state_next"));

    std::env::set_var("EVIDENT_FUNCTIONIZE", "1");
    let native_r = rt.query("HelloStep", &given).unwrap();
    assert!(native_r.satisfied, "functionize path must agree on SAT");
    assert_eq!(native_r.bindings.get("state_next"),
               z3_r.bindings.get("state_next"),
               "Z3 and native disagree on state_next");

    let _ = std::fs::remove_file(&path);
}

#[test]
fn match_dispatch_state_done_stays_done() {
    let src = r#"enum HelloState = Init | Done

claim HelloStep
    state ∈ HelloState
    state_next ∈ HelloState
    state_next = match state
        Init ⇒ Done
        Done ⇒ Done
"#;
    let path = tmp("hellostep");
    std::fs::write(&path, src).unwrap();
    let mut rt = EvidentRuntime::new();
    rt.load_file(&path).unwrap();

    let mut given = HashMap::new();
    given.insert("state".into(), Value::Enum {
        enum_name: "HelloState".into(),
        variant: "Done".into(),
        fields: vec![],
    });

    std::env::set_var("EVIDENT_FUNCTIONIZE", "1");
    let r = rt.query("HelloStep", &given).unwrap();
    assert!(r.satisfied);
    assert_eq!(r.bindings.get("state_next"), Some(&Value::Enum {
        enum_name: "HelloState".into(),
        variant: "Done".into(),
        fields: vec![],
    }));
    let _ = std::fs::remove_file(&path);
}

#[test]
fn match_dispatch_bench() {
    // Confirm the function-izer is actually faster than Z3 on this
    // Match-shaped claim.
    let src = r#"enum HelloState = Init | Done

claim HelloStep
    state ∈ HelloState
    state_next ∈ HelloState
    state_next = match state
        Init ⇒ Done
        Done ⇒ Done
"#;
    let path = tmp("bench");
    std::fs::write(&path, src).unwrap();
    let mut rt = EvidentRuntime::new();
    rt.load_file(&path).unwrap();

    let mut given = HashMap::new();
    given.insert("state".into(), Value::Enum {
        enum_name: "HelloState".into(),
        variant: "Init".into(),
        fields: vec![],
    });

    // Warm-up both paths.
    for _ in 0..50 {
        std::env::set_var("EVIDENT_FUNCTIONIZE", "0");
        let _ = rt.query("HelloStep", &given);
        std::env::set_var("EVIDENT_FUNCTIONIZE", "1");
        let _ = rt.query("HelloStep", &given);
    }
    const N: usize = 2_000;

    std::env::set_var("EVIDENT_FUNCTIONIZE", "0");
    let t0 = Instant::now();
    for _ in 0..N { let _ = rt.query("HelloStep", &given); }
    let z3_per = t0.elapsed().as_secs_f64() * 1_000_000.0 / N as f64;

    std::env::set_var("EVIDENT_FUNCTIONIZE", "1");
    let t0 = Instant::now();
    for _ in 0..N { let _ = rt.query("HelloStep", &given); }
    let native_per = t0.elapsed().as_secs_f64() * 1_000_000.0 / N as f64;

    println!("\nMatch-dispatch bench ({} iter):", N);
    println!("  Z3 query:     {:>10.2} μs/call", z3_per);
    println!("  Native (fz):  {:>10.2} μs/call", native_per);
    println!("  Speedup:      {:.0}×", z3_per / native_per);
    let _ = std::fs::remove_file(&path);

    // Speedup must be positive (function-izer should beat Z3 here).
    assert!(native_per < z3_per,
        "function-izer ({:.2}μs) should beat Z3 ({:.2}μs) for Match dispatch",
        native_per, z3_per);
}
