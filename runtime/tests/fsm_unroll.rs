//! `halts_within(F, N)` — FSM halt lowered to a constraint via
//! exponentiation-by-squaring composition (the `fsm_unroll` module).
//!
//! These tests exercise the four behaviors the feature must guarantee
//! (per docs/design/fsm-halts-within.md and Z's measurement in
//! docs/perf/log-unroll-feasibility.md):
//!
//!   1. An affine counter halts within a large-enough N  → SAT.
//!   2. The same counter does NOT halt within a too-small N → UNSAT.
//!   3. A branching body is refused by the affine-step detector — no
//!      crash, no wrong answer, the claim resolves UNSAT cleanly and a
//!      "log-unroll declined" diagnostic is printed to stderr.
//!   4. A large N (≥ 10,000) completes fast — the log-spaced build is
//!      O(log N) doublings, so the affine state collapses to closed
//!      form regardless of N.
//!
//! Reproduce the refusal diagnostic:
//!   cargo test --release --test fsm_unroll branching_refused -- --nocapture

use std::time::Instant;

use evident_runtime::EvidentRuntime;

/// The affine transition under test: decrement a counter, halt at zero.
/// `count_next`/`halt` are clean equalities — the "function shape" the
/// closed-form composer extracts. `halt` reads the tick's INPUT state,
/// so starting at 50 the body first halts at tick 51.
const DECREMENT: &str = "\
fsm decrement
    count, count_next ∈ Int
    halt ∈ Bool
    count_next = count - 1
    halt = (count ≤ 0)
";

/// A branching transition: the state update forks on the carried
/// state. Z's measurement (`conditional update` shape) showed this
/// grows ~2× per doubling and never collapses — the affine-step
/// detector must refuse it.
const COND_DECREMENT: &str = "\
fsm cond_decrement
    count, count_next ∈ Int
    halt ∈ Bool
    count_next = (count > 0 ? count - 1 : count)
    halt = (count ≤ 0)
";

fn query(src: &str, claim: &str) -> bool {
    let mut rt = EvidentRuntime::new();
    rt.load_source(src).expect("load");
    rt.query_free(claim).expect("query").satisfied
}

/// 1. An affine counter starting at 50 halts within 100 ticks (it
///    reaches 0 at tick 51), so the claim is SAT.
#[test]
fn affine_counter_halts_within_n() {
    let src = format!(
        "{DECREMENT}\nclaim sat_halts\n    count ∈ Int = 50\n    halts_within(decrement, 100)\n"
    );
    assert!(query(&src, "sat_halts"),
        "counter at 50 should halt within 100 ticks");
}

/// The boundary is exact: 50 → 0 first halts at tick 51, so N = 51 is
/// SAT and N = 50 is UNSAT. This pins the `∃ k ∈ [1,N]` semantics.
#[test]
fn affine_counter_boundary_is_exact() {
    let at_51 = format!(
        "{DECREMENT}\nclaim c\n    count ∈ Int = 50\n    halts_within(decrement, 51)\n"
    );
    let by_50 = format!(
        "{DECREMENT}\nclaim c\n    count ∈ Int = 50\n    halts_within(decrement, 50)\n"
    );
    assert!(query(&at_51, "c"), "must halt by tick 51");
    assert!(!query(&by_50, "c"), "must NOT have halted by tick 50");
}

/// 2. The same counter does NOT halt within 5 ticks (50 can't reach 0
///    that fast), so the claim is UNSAT.
#[test]
fn affine_counter_does_not_halt_within_small_n() {
    let src = format!(
        "{DECREMENT}\nclaim unsat_halts\n    count ∈ Int = 50\n    halts_within(decrement, 5)\n"
    );
    assert!(!query(&src, "unsat_halts"),
        "counter at 50 cannot halt within 5 ticks");
}

/// 3. A branching body is refused by the affine-step detector. The
///    claim resolves UNSAT cleanly (an honest "couldn't prove this"),
///    no panic, and the "log-unroll declined" diagnostic is printed to
///    stderr. Run with `-- --nocapture` to see the diagnostic.
#[test]
fn branching_refused() {
    let src = format!(
        "{COND_DECREMENT}\nclaim sat_branch\n    count ∈ Int = 10\n    halts_within(cond_decrement, 100)\n"
    );
    // The branching body WOULD halt operationally (10 → 0), but the
    // detector declines to prove it via log-unroll. The contract is:
    // refuse cleanly to UNSAT rather than blow up or return a wrong
    // SAT. (The diagnostic on stderr tells the user to use a different
    // verification approach.)
    assert!(!query(&src, "sat_branch"),
        "branching body must be REFUSED (resolve UNSAT), not silently \
         proven; the affine-step detector declines it");
}

/// 4. A large N completes fast: the log-spaced build is O(log N)
///    doublings and the affine state collapses to closed form, so even
///    N = 20,000 is cheap. Budget 5s (the spec's bar); in practice the
///    unroll itself is tens of milliseconds.
#[test]
fn large_n_completes_quickly() {
    let sat = format!(
        "{DECREMENT}\nclaim c\n    count ∈ Int = 15000\n    halts_within(decrement, 20000)\n"
    );
    let unsat = format!(
        "{DECREMENT}\nclaim c\n    count ∈ Int = 25000\n    halts_within(decrement, 20000)\n"
    );
    let t0 = Instant::now();
    assert!(query(&sat, "c"), "15000 halts within 20000");
    assert!(!query(&unsat, "c"), "25000 does NOT halt within 20000");
    let elapsed = t0.elapsed();
    assert!(elapsed.as_secs() < 5,
        "log-unroll at N=20000 should be well under 5s, took {elapsed:?}");
}

/// An unknown FSM name is reported, not silently ignored — the claim
/// resolves UNSAT (the lowering asserts false on error).
#[test]
fn unknown_fsm_refused() {
    let src = "claim c\n    count ∈ Int = 5\n    halts_within(no_such_fsm, 10)\n";
    assert!(!query(src, "c"),
        "halts_within on an undefined FSM must not spuriously succeed");
}
