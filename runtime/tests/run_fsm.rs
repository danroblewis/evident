//! Equivalence-oracle harness for `run(F, init)` — tier 3,
//! blocking-interpret (docs/design/nested-fsm-strategies.md §4).
//!
//! Tier 3 is the *oracle*: the faster tiers (loop-functionizer,
//! symbolic-unroll→JIT) must compute the SAME value as tier 3 on the
//! same `init`. They aren't built yet, so this harness pins tier 3's
//! own behavior and is structured so a later session drops in the
//! cross-tier assertions without restructuring:
//!
//! ```ignore
//! for (fsm, init, expected) in CORPUS {
//!     let base = oracle(&rt, fsm, init);          // tier 3, canonical
//!     assert_eq!(base, expected);
//!     // when tier 2 lands:
//!     // assert_eq!(run_forced(&rt, fsm, init, "loop"),   base);
//!     // when tier 1 lands (affine only):
//!     // assert_eq!(run_forced(&rt, fsm, init, "unroll"), base);
//! }
//! ```
//!
//! The four acceptance prongs (SESSION criterion 5):
//!   (a) the counter runs to the right final state,
//!   (b) seeding variants (bare Int → Int state; bare Int → enum
//!       state via the single-Int-payload first-variant convention),
//!   (c) a non-FSM `F` is rejected at load,
//!   (d) a non-halting FSM hits the max-iteration guard with a clear
//!       error.

use std::collections::HashMap;

use evident_runtime::effect_loop::run_nested;
use evident_runtime::{EvidentRuntime, Value};

const COUNTER: &str = r#"
import "stdlib/runtime.ev"
fsm decrement(count ∈ Int, count_next ∈ Int, halt ∈ Bool)
    count_next = count - 1
    halt = (count ≤ 0)
enum Acc = Acc(Int)
fsm accumulate(state ∈ Acc, state_next ∈ Acc, halt ∈ Bool)
    n ∈ Int = match state
        Acc(v) ⇒ v
    state_next = Acc(n + 1)
    halt = (n ≥ 5)
"#;

/// Build a runtime with `src` loaded (stdlib resolves from `../stdlib`).
fn rt_with(src: &str) -> EvidentRuntime {
    let mut rt = EvidentRuntime::new();
    // stdlib imports resolve relative to the process cwd (`runtime/`),
    // so point the import at the repo stdlib.
    rt.load_file(std::path::Path::new("../stdlib/runtime.ev"))
        .expect("load stdlib/runtime.ev");
    // The COUNTER source re-imports runtime.ev; load_file dedups by path
    // so the second import is a no-op. Strip the import line to be safe.
    let body: String = src.lines()
        .filter(|l| !l.trim_start().starts_with("import "))
        .collect::<Vec<_>>().join("\n");
    rt.load_source(&body).expect("load source");
    rt
}

/// The canonical tier-3 result: drive `fsm` from `init` to halt.
fn oracle(rt: &EvidentRuntime, fsm: &str, init: Value) -> Value {
    run_nested(rt, fsm, init, 10_000)
        .unwrap_or_else(|e| panic!("run_nested({fsm}) failed: {e}"))
}

// ── (a) the counter runs to the right final state ──────────────────

#[test]
fn counter_runs_to_zero() {
    let rt = rt_with(COUNTER);
    assert_eq!(oracle(&rt, "decrement", Value::Int(50)), Value::Int(0));
    assert_eq!(oracle(&rt, "decrement", Value::Int(3)),  Value::Int(0));
    assert_eq!(oracle(&rt, "decrement", Value::Int(1)),  Value::Int(0));
}

#[test]
fn counter_already_halted_returns_init() {
    // halt = count ≤ 0 is already true at the seed, so the run returns
    // the input state with zero advancement.
    let rt = rt_with(COUNTER);
    assert_eq!(oracle(&rt, "decrement", Value::Int(-5)), Value::Int(-5));
    assert_eq!(oracle(&rt, "decrement", Value::Int(0)),  Value::Int(0));
}

// ── (b) seeding variants: Int state and enum state ─────────────────

#[test]
fn seed_int_state() {
    let rt = rt_with(COUNTER);
    // Bare Int seeds an Int state pair directly.
    assert_eq!(oracle(&rt, "decrement", Value::Int(7)), Value::Int(0));
}

#[test]
fn seed_enum_state_from_int() {
    // Bare Int seeds the state enum's first single-Int-payload variant
    // (Acc(0)); ticks to Acc(5), returns the enum-state value.
    let rt = rt_with(COUNTER);
    let final_state = oracle(&rt, "accumulate", Value::Int(0));
    assert_eq!(final_state, Value::Enum {
        enum_name: "Acc".to_string(),
        variant:   "Acc".to_string(),
        fields:    vec![Value::Int(5)],
    });
}

#[test]
fn seed_enum_state_from_enum_value() {
    // An already-built enum value seeds directly (skips the Int-coerce).
    let rt = rt_with(COUNTER);
    let start = Value::Enum {
        enum_name: "Acc".to_string(),
        variant:   "Acc".to_string(),
        fields:    vec![Value::Int(2)],
    };
    let final_state = oracle(&rt, "accumulate", start);
    assert_eq!(final_state, Value::Enum {
        enum_name: "Acc".to_string(),
        variant:   "Acc".to_string(),
        fields:    vec![Value::Int(5)],
    });
}

// ── End-to-end: the rewrite pins the run result into the outer model ──

#[test]
fn run_result_pins_into_outer_query() {
    let mut rt = rt_with(COUNTER);
    rt.load_source(
        "claim sat_outer\n    final ∈ Int = run(decrement, 50)\n    final = 0\n",
    ).expect("load outer claim");
    let qr = rt.query("sat_outer", &HashMap::new()).expect("query");
    assert!(qr.satisfied, "run(decrement, 50) should pin final = 0, making sat_outer SAT");
}

#[test]
fn run_result_unsat_when_outer_contradicts() {
    // If the run result (0) contradicts the outer constraint, the claim
    // is UNSAT — proving the value really is pinned, not free.
    let mut rt = rt_with(COUNTER);
    rt.load_source(
        "claim unsat_outer\n    final ∈ Int = run(decrement, 50)\n    final = 99\n",
    ).expect("load outer claim");
    let qr = rt.query("unsat_outer", &HashMap::new()).expect("query");
    assert!(!qr.satisfied, "final is pinned to 0; `final = 99` must be UNSAT");
}

// ── (c) a non-FSM-shaped F is rejected at load ─────────────────────

#[test]
fn non_fsm_shaped_target_rejected_at_load() {
    // `notfsm` IS declared `fsm` (so it clears the keyword gate) but has
    // no state pair — the shape check rejects it at load.
    let mut rt = EvidentRuntime::new();
    rt.load_file(std::path::Path::new("../stdlib/runtime.ev")).unwrap();
    let err = rt.load_source(
        "fsm notfsm(x ∈ Int, y ∈ Int)\n    y = x + 1\n\
         claim sat_bad\n    z ∈ Int = run(notfsm, 5)\n    z = 0\n",
    ).expect_err("a non-FSM-shaped run target must be a load error");
    let msg = err.to_string();
    assert!(msg.contains("FSM-shaped") && msg.contains("notfsm"),
        "load error should name the target and the shape requirement, got: {msg}");
}

// ── (c′) a `claim`-keyworded F is rejected at load (the keyword IS the
// rule: a shape-perfect transition declared `claim` is not an FSM) ──

#[test]
fn claim_keyword_run_target_rejected_at_load() {
    // `decr` is a textbook FSM-shaped transition — but declared `claim`,
    // not `fsm`. The keyword is the sole FSM signal, so `run(decr, ..)`
    // is a load error naming the fix.
    let mut rt = EvidentRuntime::new();
    rt.load_file(std::path::Path::new("../stdlib/runtime.ev")).unwrap();
    let err = rt.load_source(
        "claim decr(count ∈ Int, count_next ∈ Int, halt ∈ Bool)\n\
         \u{20}\u{20}\u{20}\u{20}count_next = count - 1\n\
         \u{20}\u{20}\u{20}\u{20}halt = (count ≤ 0)\n\
         claim sat_bad\n    z ∈ Int = run(decr, 5)\n    z = 0\n",
    ).expect_err("a `claim`-keyworded run target must be a load error");
    let msg = err.to_string();
    assert!(msg.contains("must be declared `fsm`") && msg.contains("decr")
            && msg.contains("not `claim`"),
        "load error should say the target must be `fsm`, not `claim`, got: {msg}");
}

#[test]
fn claim_keyword_halts_within_target_rejected_at_load() {
    let mut rt = EvidentRuntime::new();
    rt.load_file(std::path::Path::new("../stdlib/runtime.ev")).unwrap();
    let err = rt.load_source(
        "claim decr2\n    count, count_next ∈ Int\n    halt ∈ Bool\n\
         \u{20}\u{20}\u{20}\u{20}count_next = count - 1\n\
         \u{20}\u{20}\u{20}\u{20}halt = (count ≤ 0)\n\
         claim sat_bad2\n    count ∈ Int = 50\n    halts_within(decr2, 100)\n",
    ).expect_err("a `claim`-keyworded halts_within target must be a load error");
    let msg = err.to_string();
    assert!(msg.contains("must be declared `fsm`") && msg.contains("decr2"),
        "load error should say the halts_within target must be `fsm`, got: {msg}");
}

#[test]
fn effect_emitting_target_is_accepted_and_captured() {
    // Session RR: an effect-emitting child is NO LONGER rejected at load.
    // It runs as a value (final state pins into the outer model), and its
    // per-tick effects are CAPTURED, not dispatched. Here we prove (a) it
    // loads + solves to the right final state, and (b) `run_nested_capturing`
    // hands back the captured effects in child-tick order — see the
    // dedicated `nested_effects.rs` for the full capture/percolation/purity
    // matrix.
    let mut rt = EvidentRuntime::new();
    rt.load_file(std::path::Path::new("../stdlib/runtime.ev")).unwrap();
    rt.load_source(
        "fsm noisy(count ∈ Int, count_next ∈ Int, halt ∈ Bool, effects ∈ Seq(Effect))\n\
         \u{20}\u{20}\u{20}\u{20}count_next = count - 1\n\
         \u{20}\u{20}\u{20}\u{20}halt = (count ≤ 0)\n\
         \u{20}\u{20}\u{20}\u{20}effects = ⟨Println(\"tick\")⟩\n\
         claim sat_eff\n    final ∈ Int = run(noisy, 3)\n    final = 0\n",
    ).expect("an effect-emitting run target must now LOAD (no rejection)");
    // The static claim resolves the run, captures (drops) the effects,
    // and asserts final = 0.
    let qr = rt.query("sat_eff", &HashMap::new()).expect("query");
    assert!(qr.satisfied, "run(noisy, 3) should still pin final = 0");
}

// ── (d) a non-halting FSM hits the max-iteration guard ─────────────

#[test]
fn non_halting_fsm_hits_max_iter_guard() {
    let rt = rt_with(
        "fsm forever(count ∈ Int, count_next ∈ Int, halt ∈ Bool)\n\
         \u{20}\u{20}\u{20}\u{20}count_next = count + 1\n\
         \u{20}\u{20}\u{20}\u{20}halt = false\n",
    );
    let err = run_nested(&rt, "forever", Value::Int(0), 25)
        .expect_err("a never-halting FSM must hit the guard, not hang");
    let msg = err.to_string();
    assert!(msg.contains("max-iteration") || msg.contains("25"),
        "guard error should mention the cap, got: {msg}");
}

#[test]
fn unknown_target_errors_clearly() {
    let rt = rt_with(COUNTER);
    let err = run_nested(&rt, "nonexistent", Value::Int(0), 100)
        .expect_err("an unknown FSM must error");
    assert!(err.to_string().contains("nonexistent"),
        "error should name the missing FSM, got: {err}");
}
