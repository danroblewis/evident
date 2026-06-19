use std::collections::HashMap;
use evident_runtime::{EvidentRuntime, Value};

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

    let r = rt.query("two_comp_fallback", &given(5, true)).unwrap();
    assert!(r.satisfied);
    assert_eq!(r.bindings.get("y"), Some(&Value::Int(6)),
        "JIT-friendly component: y = a + 1");
    assert_eq!(r.bindings.get("z"), Some(&Value::Int(100)),
        "slow-path component under b = true");

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

    let mut rt = EvidentRuntime::new();
    rt.load_source(SRC).unwrap();

    let r1 = rt.query("two_comp_fallback", &given(0, true)).unwrap();
    assert_eq!(r1.bindings.get("z"), Some(&Value::Int(100)));
    assert_eq!(r1.bindings.get("y"), Some(&Value::Int(1)));

    let r2 = rt.query("two_comp_fallback", &given(41, false)).unwrap();
    assert_eq!(r2.bindings.get("z"), Some(&Value::Int(200)),
        "slow-path component tracks b = false");
    assert_eq!(r2.bindings.get("y"), Some(&Value::Int(42)),
        "compiled component tracks a = 41");

    let stats = rt.functionize_stats();
    let per = stats.claims.get("two_comp_fallback").unwrap();
    assert!(per.cache_hits >= 1, "second call should hit the cached plan");
}

#[test]
fn per_component_matches_full_solve() {

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
