//! Cross-tick value-keyed memoization (session C).
//!
//! `try_functionize_z3`'s result is a pure function of `(claim, given
//! values)` while a program is loaded. An idle FSM (e.g. Mario with no
//! input pressed) feeds a claim byte-identical inputs frame after frame;
//! the value cache memoizes the last result keyed by `hash(given)` and
//! returns it without re-running the compiled function.
//!
//! These tests pin the contract:
//!   * identical inputs N× → 1 analysis + (N-1) value-cache hits,
//!   * a changed input is a value-cache miss (recomputes the right
//!     answer), and a return to a prior input hits again,
//!   * the memoized bindings equal a fresh (uncached) solve, and
//!   * a reload invalidates the cache.

use std::collections::HashMap;
use evident_runtime::{EvidentRuntime, Value};

/// One JIT-able component: `y = a + 1`, a pure scalar function of the
/// single given `a`. Compiles fully (no slow part), so a repeat call
/// with the same `a` is a clean value-cache hit.
const SRC: &str = r#"
claim plus_one
    a ∈ Int
    y ∈ Int = a + 1
"#;

fn given(a: i64) -> HashMap<String, Value> {
    let mut g = HashMap::new();
    g.insert("a".to_string(), Value::Int(a));
    g
}

#[test]
fn identical_inputs_hit_value_cache() {
    let mut rt = EvidentRuntime::new();
    rt.load_source(SRC).unwrap();

    // 10 calls with the SAME given: the first is a miss (analyzes +
    // compiles), the next 9 are value-cache hits.
    for _ in 0..10 {
        let r = rt.query("plus_one", &given(5)).unwrap();
        assert!(r.satisfied);
        assert_eq!(r.bindings.get("y"), Some(&Value::Int(6)));
    }

    let stats = rt.functionize_stats();
    let per = stats.claims.get("plus_one")
        .expect("plus_one should have been functionize-analyzed");
    assert_eq!(per.analyses, 1, "compiled-fn analysis runs exactly once");
    assert_eq!(per.value_cache_hits, 9,
        "9 of the 10 identical-input calls skip the compiled fn");
    assert_eq!(per.cache_hits, 0,
        "value-cache hits never reach the fn_cache plan-rerun path");
}

#[test]
fn changed_input_misses_then_return_hits() {
    let mut rt = EvidentRuntime::new();
    rt.load_source(SRC).unwrap();

    // a=5 (miss, analyze), a=6 (value miss → fn_cache plan rerun),
    // a=5 again (value HIT — the a=5 result is still memoized).
    assert_eq!(rt.query("plus_one", &given(5)).unwrap().bindings.get("y"),
        Some(&Value::Int(6)));
    assert_eq!(rt.query("plus_one", &given(6)).unwrap().bindings.get("y"),
        Some(&Value::Int(7)), "changed input recomputes the right answer");
    assert_eq!(rt.query("plus_one", &given(5)).unwrap().bindings.get("y"),
        Some(&Value::Int(6)), "returning to a prior input hits the value cache");

    let per_stats = rt.functionize_stats();
    let per = per_stats.claims.get("plus_one").unwrap();
    assert_eq!(per.analyses, 1, "only the first call analyzes");
    assert_eq!(per.value_cache_hits, 1, "exactly the a=5 return is a value hit");
    assert_eq!(per.cache_hits, 1, "the a=6 call reruns the cached plan");
}

#[test]
fn memoized_result_matches_uncached_solve() {
    // The cached value must equal what a from-scratch solve produces.
    let mut rt = EvidentRuntime::new();
    rt.load_source(SRC).unwrap();
    let fresh = rt.query("plus_one", &given(41)).unwrap();   // miss
    let cached = rt.query("plus_one", &given(41)).unwrap();  // hit
    assert_eq!(fresh.satisfied, cached.satisfied);
    assert_eq!(fresh.bindings, cached.bindings,
        "value-cache hit reproduces the exact bindings of the miss");
}

#[test]
fn reload_invalidates_value_cache() {
    let mut rt = EvidentRuntime::new();
    rt.load_source(SRC).unwrap();

    rt.query("plus_one", &given(5)).unwrap();   // miss, analyses = 1
    rt.query("plus_one", &given(5)).unwrap();   // value hit
    assert_eq!(rt.functionize_stats().claims.get("plus_one").unwrap().analyses, 1);

    // Reloading clears the value cache (load.rs), so the next identical
    // call must re-analyze rather than serve a stale memo.
    rt.load_source(SRC).unwrap();
    let r = rt.query("plus_one", &given(5)).unwrap();
    assert_eq!(r.bindings.get("y"), Some(&Value::Int(6)));
    assert_eq!(rt.functionize_stats().claims.get("plus_one").unwrap().analyses, 2,
        "reload invalidated the value cache → the call re-analyzed");
}
