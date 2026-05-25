//! Tier-1 — symbolic-unroll → Cranelift JIT (`EvidentRuntime::tier1_run`).
//!
//! Tier 1 of the nested-FSM strategy ladder: an **affine** FSM body
//! (the regime CC's affine-step detector accepts) is composed
//! symbolically to a closed form, that collapsed `Z3Program` is handed
//! to the existing Cranelift functionizer, and the native function
//! computes `run(F, init)`'s final state in one call. See
//! `docs/design/nested-fsm-strategies.md` §7 (step 3).
//!
//! The contract these tests pin (SESSION OO acceptance criteria):
//!
//!   2. An **affine** `run` JITs: `tier1_run` returns `Some(value)`
//!      (it only does so when both the affine detector AND the
//!      Cranelift functionizer accept). Run with
//!      `EVIDENT_FSM_UNROLL_TRACE=1 -- --nocapture` to see the
//!      `comp=1/1 fn✓` compile line.
//!   3. A **branching** `run` does NOT take this path: `tier1_run`
//!      returns `None` (the detector refuses), so the caller falls
//!      through cleanly — no wrong value, no hang.
//!   4. **Equivalence:** the JIT'd result equals the tier-3 oracle
//!      (`effect_loop::run_nested`) on the same `init`, swept across
//!      several inits including the already-halted boundary.

use evident_runtime::effect_loop::run_nested;
use evident_runtime::{EvidentRuntime, Value};

/// The affine transition: decrement a counter, halt at zero. `halt`
/// reads the tick's INPUT state, so `run(decrement, 50)` returns the
/// first input ≤ 0, which is `0`. This is the shape the affine-step
/// detector accepts (state composes to closed form: `count - k`).
const DECREMENT: &str = "\
fsm decrement
    count, count_next ∈ Int
    halt ∈ Bool
    count_next = count - 1
    halt = (count ≤ 0)
";

/// A branching transition: the state update forks on the carried state.
/// Z's measurement (`conditional update` shape) showed this grows ~2×
/// per doubling and never collapses — the affine-step detector must
/// refuse it, so tier 1 falls through.
const COND_DECREMENT: &str = "\
fsm cond_decrement
    count, count_next ∈ Int
    halt ∈ Bool
    count_next = (count > 0 ? count - 1 : count)
    halt = (count ≤ 0)
";

/// A second affine shape: step by −2, halt at/under zero. Confirms the
/// closed form tracks the actual recurrence, not a hard-coded `0`.
const STEP_TWO: &str = "\
fsm step2
    count, count_next ∈ Int
    halt ∈ Bool
    count_next = count - 2
    halt = (count ≤ 0)
";

fn rt_with(src: &str) -> EvidentRuntime {
    let mut rt = EvidentRuntime::new();
    rt.load_source(src).expect("load source");
    rt
}

/// The canonical tier-3 result: drive `fsm` from `init` to halt.
fn oracle(rt: &EvidentRuntime, fsm: &str, init: i64) -> Value {
    run_nested(rt, fsm, Value::Int(init), 100_000)
        .unwrap_or_else(|e| panic!("oracle run_nested({fsm}, {init}) failed: {e}"))
}

/// Tier-1 result, or `None` if the detector / functionizer refused.
fn tier1(rt: &EvidentRuntime, fsm: &str, init: i64) -> Option<Value> {
    rt.tier1_run(fsm, &Value::Int(init))
        .unwrap_or_else(|e| panic!("tier1_run({fsm}, {init}) errored: {e}"))
}

// ── (2)+(4) the affine case JITs AND equals the tier-3 oracle ──────────

#[test]
fn affine_counter_jits_and_matches_oracle() {
    let rt = rt_with(DECREMENT);
    // Sweep inits, including the already-halted boundary (init ≤ 0).
    for init in [50, 12, 7, 3, 1, 0, -5, -100] {
        let base = oracle(&rt, "decrement", init);
        let jit = tier1(&rt, "decrement", init)
            .unwrap_or_else(|| panic!(
                "affine counter must JIT (tier1_run = Some) for init={init}; \
                 got None (fell through)"));
        assert_eq!(jit, base,
            "tier-1 JIT result must equal the tier-3 oracle for init={init}: \
             jit={jit:?} oracle={base:?}");
    }
}

#[test]
fn affine_counter_known_values() {
    // The concrete closed-form answers, independent of the oracle.
    let rt = rt_with(DECREMENT);
    assert_eq!(tier1(&rt, "decrement", 50), Some(Value::Int(0)));
    assert_eq!(tier1(&rt, "decrement", 1),  Some(Value::Int(0)));
    assert_eq!(tier1(&rt, "decrement", 0),  Some(Value::Int(0)));
    // Already halted at the seed: returns the seed unchanged.
    assert_eq!(tier1(&rt, "decrement", -5), Some(Value::Int(-5)));
}

#[test]
fn affine_step_two_matches_oracle() {
    // A −2 step lands on 0 from even inits and on −1 from odd inits —
    // the closed form must reproduce that, not assume the halt value is
    // always 0.
    let rt = rt_with(STEP_TWO);
    for init in [10, 9, 8, 5, 2, 1, 0, -3] {
        let base = oracle(&rt, "step2", init);
        let jit = tier1(&rt, "step2", init)
            .unwrap_or_else(|| panic!("step2 must JIT for init={init}"));
        assert_eq!(jit, base,
            "step2 tier-1 must equal oracle for init={init}: jit={jit:?} oracle={base:?}");
    }
    // Spot-check the parity: 9 → 7 → 5 → 3 → 1 → -1 (first ≤ 0 is -1).
    assert_eq!(tier1(&rt, "step2", 9), Some(Value::Int(-1)));
    // 10 → 8 → ... → 2 → 0 (first ≤ 0 is 0).
    assert_eq!(tier1(&rt, "step2", 10), Some(Value::Int(0)));
}

// ── (3) the branching case falls through cleanly ──────────────────────

#[test]
fn branching_body_falls_through() {
    let rt = rt_with(COND_DECREMENT);
    // The detector refuses the branching body → tier1_run = None. The
    // run is still *operationally* halting (10 → 0), so the tier-3
    // oracle gives the right answer; tier 1 just declines to JIT it.
    for init in [10, 3, 1] {
        assert_eq!(tier1(&rt, "cond_decrement", init), None,
            "branching body must fall through (tier1_run = None) for init={init}");
        // The fall-through target (tier 3) still computes correctly.
        assert_eq!(oracle(&rt, "cond_decrement", init), Value::Int(0),
            "tier-3 oracle must still drive the branching body to 0");
    }
}

// ── refusals that fall through, never error ───────────────────────────

#[test]
fn enum_state_falls_through() {
    // Enum-state FSMs are tiers 2/3, not tier 1 (the affine class is
    // Int recurrences). tier1_run must decline, not crash.
    let rt = rt_with(
        "enum Acc = Acc(Int)\n\
         fsm accumulate\n    state, state_next ∈ Acc\n    halt ∈ Bool\n\
         \u{20}\u{20}\u{20}\u{20}n ∈ Int = match state\n        Acc(v) ⇒ v\n\
         \u{20}\u{20}\u{20}\u{20}state_next = Acc(n + 1)\n\
         \u{20}\u{20}\u{20}\u{20}halt = (n ≥ 5)\n",
    );
    // Bare Int init, but the state type is the enum Acc → not Int → fall through.
    assert_eq!(tier1(&rt, "accumulate", 0), None,
        "enum-state FSM must fall through tier 1");
}

#[test]
fn unknown_fsm_errors() {
    let rt = rt_with(DECREMENT);
    let err = rt.tier1_run("no_such_fsm", &Value::Int(5))
        .expect_err("an unknown FSM must error, not silently return None");
    assert!(err.to_string().contains("no_such_fsm"),
        "error should name the missing FSM, got: {err}");
}

#[test]
fn non_int_init_falls_through() {
    // A non-Int init can't seed an Int affine counter via tier 1 — fall
    // through (the caller's tier-3 path handles coercion / rejection).
    let rt = rt_with(DECREMENT);
    assert_eq!(
        rt.tier1_run("decrement", &Value::Bool(true)).expect("no error"),
        None,
        "non-Int init must fall through, not JIT");
}
