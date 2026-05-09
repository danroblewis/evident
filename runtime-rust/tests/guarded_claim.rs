//! Guarded claim invocation: `cond ⇒ ClaimName` inlines the claim's
//! body but wraps each constraint in `cond ⇒ …`.
//!
//! Headline use case: the per-frame state-machine init pattern in
//! anchor_collect.ev — `state.step = 0 ⇒ InitGameState` fires the
//! init constraints once on the first frame and is permanently false
//! after. Without this, the init logic would either run every frame
//! (over-constraining) or sit at the call site with all the
//! per-constraint guards inlined manually.

use evident_runtime::{EvidentRuntime, Value};

/// Smallest case: guarded invocation runs the claim's constraints
/// when the guard is true.
#[test]
fn guarded_claim_fires_when_true() {
    let mut rt = EvidentRuntime::new();
    let src = "claim Init\n    n ∈ Int\n    n = 42\nschema S\n    n ∈ Int\n    cond ∈ Bool\n    cond = true\n    cond ⇒ Init\n";
    rt.load_source(src).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    assert_eq!(r.bindings.get("n"), Some(&Value::Int(42)));
}

/// When the guard is false, the claim's constraints are vacuous —
/// other constraints can pin variables freely without conflict.
#[test]
fn guarded_claim_constraints_vacuous_when_false() {
    let mut rt = EvidentRuntime::new();
    let src = "claim Init\n    n ∈ Int\n    n = 42\nschema S\n    n ∈ Int\n    cond ∈ Bool\n    cond = false\n    cond ⇒ Init\n    n = 99\n";
    rt.load_source(src).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    assert_eq!(r.bindings.get("n"), Some(&Value::Int(99)));
}

/// When the guard is false but `n = 42` and `n = 99` are both
/// asserted unconditionally, → UNSAT. Used to verify the guard
/// actually controls Init's constraint, vs accidentally always firing.
#[test]
fn unguarded_invocation_would_conflict() {
    let mut rt = EvidentRuntime::new();
    let src = "claim Init\n    n ∈ Int\n    n = 42\nschema S\n    n ∈ Int\n    ..Init\n    n = 99\n";
    rt.load_source(src).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(!r.satisfied, "unguarded Init pins n=42, conflicting with n=99");
}

/// The claim's *declarations* fire unconditionally — only constraints
/// are guarded. Init declares `helper ∈ Int`; even when the guard is
/// false, helper is in env (just unconstrained beyond its type).
#[test]
fn guarded_claim_declarations_fire_unconditionally() {
    let mut rt = EvidentRuntime::new();
    let src = "claim Init\n    helper ∈ Nat\n    helper = 5\nschema S\n    helper ∈ Nat\n    cond ∈ Bool\n    cond = false\n    cond ⇒ Init\n    helper = 100\n";
    rt.load_source(src).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    assert_eq!(r.bindings.get("helper"), Some(&Value::Int(100)));
}

/// Multiple constraints inside the claim each get the guard wrapper.
#[test]
fn guarded_claim_multi_constraint() {
    let mut rt = EvidentRuntime::new();
    let src = "claim Init\n    a ∈ Int\n    b ∈ Int\n    a = 10\n    b = 20\nschema S\n    a ∈ Int\n    b ∈ Int\n    cond ∈ Bool\n    cond = true\n    cond ⇒ Init\n";
    rt.load_source(src).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    assert_eq!(r.bindings.get("a"), Some(&Value::Int(10)));
    assert_eq!(r.bindings.get("b"), Some(&Value::Int(20)));
}

/// Composed guards: invoking a claim from inside another claim's body
/// AND-composes the antecedents. With `outer ⇒ Outer` and Outer's body
/// containing `inner ⇒ Inner` and Inner pinning n=5, the n=5 constraint
/// only fires when both outer AND inner are true.
#[test]
fn nested_guards_compose() {
    let mut rt = EvidentRuntime::new();
    let src = "claim Inner\n    n ∈ Int\n    n = 5\nclaim Outer\n    n ∈ Int\n    inner_flag ∈ Bool\n    inner_flag ⇒ Inner\nschema S\n    n ∈ Int\n    inner_flag ∈ Bool\n    outer_flag ∈ Bool\n    outer_flag = true\n    inner_flag = true\n    outer_flag ⇒ Outer\n";
    rt.load_source(src).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    assert_eq!(r.bindings.get("n"), Some(&Value::Int(5)));
}

#[test]
fn nested_guards_inner_false_skips() {
    let mut rt = EvidentRuntime::new();
    let src = "claim Inner\n    n ∈ Int\n    n = 5\nclaim Outer\n    n ∈ Int\n    inner_flag ∈ Bool\n    inner_flag ⇒ Inner\nschema S\n    n ∈ Int\n    inner_flag ∈ Bool\n    outer_flag ∈ Bool\n    outer_flag = true\n    inner_flag = false\n    outer_flag ⇒ Outer\n    n = 99\n";
    rt.load_source(src).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied, "inner_flag = false suppresses n = 5");
    assert_eq!(r.bindings.get("n"), Some(&Value::Int(99)));
}

/// Subclaim guarded invocation — the canonical anchor_collect.ev
/// shape. Subclaim defined inside main, invoked via `step = 0 ⇒ Init`.
#[test]
fn guarded_subclaim_init_pattern() {
    let mut rt = EvidentRuntime::new();
    let src = "schema S\n    n ∈ Int\n    step ∈ Nat\n    subclaim Init\n        n = 42\n    step = 0 ⇒ Init\n    step = 0\n";
    rt.load_source(src).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    assert_eq!(r.bindings.get("n"), Some(&Value::Int(42)));
    assert_eq!(r.bindings.get("step"), Some(&Value::Int(0)));
}

#[test]
fn guarded_subclaim_skipped_when_step_nonzero() {
    let mut rt = EvidentRuntime::new();
    let src = "schema S\n    n ∈ Int\n    step ∈ Nat\n    subclaim Init\n        n = 42\n    step = 0 ⇒ Init\n    step = 5\n    n = 999\n";
    rt.load_source(src).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    assert_eq!(r.bindings.get("n"), Some(&Value::Int(999)));
}
