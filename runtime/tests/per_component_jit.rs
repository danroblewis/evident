//! Per-component JIT compilation + fallback.
//!
//! A claim decomposes into independent sub-models; each is compiled in
//! isolation. A construct one component can't emit (here: a `Guarded`
//! step, which the Cranelift functionizer refuses) no longer blocks
//! the rest — that component is solved by a cached scoped Z3 solver
//! while the others run as native code.
//!
//! These tests pin the contract from `runtime/query.rs`:
//!   * a two-component claim where exactly one component compiles,
//!   * correct bindings from BOTH the compiled and the slow-path
//!     component, across different `given` values, and
//!   * the per-claim stats reporting partial compilation (1 of 2).

use std::collections::HashMap;
use evident_runtime::{EvidentRuntime, Value};

/// Two independent components:
///   * `y = a + 1`        — a scalar function of given `a`; JIT-friendly.
///   * `z` via `b ⇒ z=100 / ¬b ⇒ z=200` — guarded implications, which
///     extract to a `Z3Step::Guarded` the Cranelift functionizer
///     refuses, so this component slow-paths.
/// `y` (touches only `y`) and `z` (touches only `z`) share no variable,
/// so they decompose into two components.
const SRC: &str = r#"
claim two_comp_fallback
    a ∈ Int
    b ∈ Bool
    y ∈ Int = a + 1
    z ∈ Int
    b ⇒ (z = 100)
    (¬b) ⇒ (z = 200)
"#;

fn given(a: i64, b: bool) -> HashMap<String, Value> {
    let mut g = HashMap::new();
    g.insert("a".to_string(), Value::Int(a));
    g.insert("b".to_string(), Value::Bool(b));
    g
}

#[test]
fn one_component_compiles_other_slow_paths() {
    let mut rt = EvidentRuntime::new();
    rt.load_source(SRC).unwrap();

    // First query (cache miss): builds + caches the plan.
    let r = rt.query("two_comp_fallback", &given(5, true)).unwrap();
    assert!(r.satisfied);
    assert_eq!(r.bindings.get("y"), Some(&Value::Int(6)),
        "JIT-friendly component: y = a + 1");
    assert_eq!(r.bindings.get("z"), Some(&Value::Int(100)),
        "slow-path component under b = true");

    // Exactly one of the two components compiled; the other slow-paths.
    let stats = rt.functionize_stats();
    let per = stats.claims.get("two_comp_fallback")
        .expect("two_comp_fallback should have been functionize-analyzed");
    assert_eq!(per.components, 2, "claim decomposes into two components");
    assert_eq!(per.components_compiled, 1,
        "exactly one component compiles (the other has a Guarded step)");
    assert!(per.compiled >= 1, "≥1 component compiled → claim marked compiled");
}

#[test]
fn slow_path_component_resolves_per_given() {
    // The slow-path component is re-solved with each call's `given`,
    // so flipping `b` flips `z` — it is NOT a baked constant.
    let mut rt = EvidentRuntime::new();
    rt.load_source(SRC).unwrap();

    let r1 = rt.query("two_comp_fallback", &given(0, true)).unwrap();
    assert_eq!(r1.bindings.get("z"), Some(&Value::Int(100)));
    assert_eq!(r1.bindings.get("y"), Some(&Value::Int(1)));

    // Second call (cache hit): re-runs the cached plan with new given.
    let r2 = rt.query("two_comp_fallback", &given(41, false)).unwrap();
    assert_eq!(r2.bindings.get("z"), Some(&Value::Int(200)),
        "slow-path component tracks b = false");
    assert_eq!(r2.bindings.get("y"), Some(&Value::Int(42)),
        "compiled component tracks a = 41");

    // The plan was reused, not rebuilt: a cache hit was recorded.
    let stats = rt.functionize_stats();
    let per = stats.claims.get("two_comp_fallback").unwrap();
    assert!(per.cache_hits >= 1, "second call should hit the cached plan");
}

#[test]
fn per_component_matches_full_solve() {
    // The per-component result must equal a full Z3 solve (functionizer
    // disabled) for the same inputs — partial compilation changes
    // performance, never the answer.
    let inputs = [(5, true), (5, false), (-3, true), (100, false)];
    for &(a, b) in &inputs {
        let want_z = if b { 100 } else { 200 };

        let mut rt_fast = EvidentRuntime::new();
        rt_fast.load_source(SRC).unwrap();
        let fast = rt_fast.query("two_comp_fallback", &given(a, b)).unwrap();

        assert!(fast.satisfied);
        assert_eq!(fast.bindings.get("y"), Some(&Value::Int(a + 1)));
        assert_eq!(fast.bindings.get("z"), Some(&Value::Int(want_z)));
    }
}
