//! Concurrency stress: many threads each build a fresh `EvidentRuntime` and
//! solve the same claim-with-unmapped-internal, asserting the answer stays in
//! range. Targets the `basic.rs::claim_call_unmapped_internal` flake (a
//! silently-wrong `n` under the full-suite's parallel test threads). A wrong
//! answer here means a constraint was dropped — i.e. Z3 / translator state was
//! corrupted by concurrent runtime construction or solving.

use evident_runtime::{EvidentRuntime, Value};

const SRC: &str = "claim pick\n    picked ∈ Nat\n    out ∈ Nat\n    out = picked + 1\n    picked > 5\n\
                     schema S\n    n ∈ Nat\n    pick (out mapsto n)\n    n < 20\n";

fn solve_once() -> i64 {
    let mut rt = EvidentRuntime::new();
    rt.load_source(SRC).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied, "S should be satisfiable");
    match r.bindings.get("n") {
        Some(Value::Int(n)) => *n,
        other => panic!("expected Int n, got {other:?}"),
    }
}

#[test]
fn concurrent_runtime_build_and_solve_keeps_constraints() {
    // 8×20 = 160 fresh runtimes: enough concurrency to race context creation
    // and enough claim-call-id variation to flip the functionizer's
    // compile-vs-slow decision for `S` (the regression vectors), while keeping
    // leaked-context memory modest (each EvidentRuntime leaks a Z3 context).
    let threads = 8;
    let iters = 20;
    let handles: Vec<_> = (0..threads)
        .map(|_| {
            std::thread::spawn(move || {
                for _ in 0..iters {
                    let n = solve_once();
                    // picked > 5 (Nat) ⇒ picked ≥ 6 ⇒ out = picked+1 ≥ 7, and n < 20.
                    assert!(
                        n > 6 && n < 20,
                        "constraint dropped under concurrency: got n={n} (want 7..=19)"
                    );
                }
            })
        })
        .collect();
    for h in handles {
        h.join().expect("worker thread panicked (constraint dropped or crash)");
    }
}
