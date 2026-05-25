//! Session RR — child-FSM effects percolate to the parent (capture, not
//! dispatch, during the child run; parent dispatches once, in order).
//!
//! Covers the five SESSION acceptance prongs:
//!   - **capture-not-dispatch**: `run_nested_capturing` RETURNS the
//!     child's per-tick effects as data; it has no `DispatchContext` and
//!     performs no IO during the run.
//!   - **percolation to the parent**: end-to-end via the scheduler, the
//!     child's effects appear in the parent's dispatched stdout.
//!   - **order**: child-tick order is preserved (three → two → one).
//!   - **single-dispatch**: each child effect appears exactly once, and
//!     only after the parent dispatches (the run prints nothing itself).
//!   - **purity**: same `init` → same `(final_state, captured_effects)`.

use std::collections::HashMap;
use std::io::Write;
use std::sync::{Arc, Mutex};

use evident_runtime::ast::Effect;
use evident_runtime::effect_dispatch::DispatchContext;
use evident_runtime::effect_loop::{run_nested_capturing, run_with_ctx, LoopOpts};
use evident_runtime::{EvidentRuntime, Value};

/// A child that emits one distinct Println per ADVANCING tick (three →
/// two → one) and halts at 0, plus a parent `main` that runs it and
/// exits. The msg lookup is complete (a sentinel row for any other
/// count) so every per-tick solve pins msg — no Z3 free-var noise.
const SRC: &str = r#"
fsm countdown(count ∈ Int, count_next ∈ Int, halt ∈ Bool, effects ∈ Seq(Effect))
    count_next = count - 1
    halt = (count ≤ 0)
    msg ∈ String
    (count = 3) ⇒ (msg = "three")
    (count = 2) ⇒ (msg = "two")
    (count = 1) ⇒ (msg = "one")
    (count ≠ 3 ∧ count ≠ 2 ∧ count ≠ 1) ⇒ (msg = "tick")
    effects = ⟨Println(msg)⟩

fsm main
    result ∈ Int = run(countdown, 3)
    reached_zero ∈ Bool = (result ≤ 0)
    effects = (reached_zero
        ? ⟨Println("PARENT-DONE"), Exit(0)⟩
        : ⟨Println("PARENT-BUG"), Exit(1)⟩)
"#;

fn rt_with(src: &str) -> EvidentRuntime {
    let mut rt = EvidentRuntime::new();
    rt.load_file(std::path::Path::new("../stdlib/runtime.ev"))
        .expect("load stdlib/runtime.ev");
    rt.load_source(src).expect("load source");
    rt
}

/// Pull the Println payloads out of a captured-effect list (panics on any
/// non-Println — the corpus only emits Printlns).
fn println_msgs(effects: &[Effect]) -> Vec<String> {
    effects.iter().map(|e| match e {
        Effect::Println(s) => s.clone(),
        other => panic!("expected Println, got {other:?}"),
    }).collect()
}

// ── capture-not-dispatch + order ───────────────────────────────────

#[test]
fn capture_returns_child_effects_in_order() {
    let rt = rt_with(SRC);
    // No DispatchContext: the run performs no IO. It hands the per-tick
    // effects back as DATA, in child-tick order.
    let (state, effects) =
        run_nested_capturing(&rt, "countdown", Value::Int(3), 10_000).unwrap();
    assert_eq!(state, Value::Int(0), "countdown(3) halts at 0");
    assert_eq!(println_msgs(&effects), vec!["three", "two", "one"],
        "captured effects must be in child-tick (advancing) order");
}

#[test]
fn already_halted_captures_nothing() {
    // halt is already true at the seed, so the run returns immediately
    // with ZERO advancing ticks → no effects to percolate.
    let rt = rt_with(SRC);
    let (state, effects) =
        run_nested_capturing(&rt, "countdown", Value::Int(0), 10_000).unwrap();
    assert_eq!(state, Value::Int(0));
    assert!(effects.is_empty(),
        "an immediately-halted run advances zero ticks; got {effects:?}");
}

// ── purity: same init → same (state, effects) ──────────────────────

#[test]
fn purity_same_init_same_state_and_effects() {
    let rt = rt_with(SRC);
    let (s1, e1) = run_nested_capturing(&rt, "countdown", Value::Int(3), 10_000).unwrap();
    let (s2, e2) = run_nested_capturing(&rt, "countdown", Value::Int(3), 10_000).unwrap();
    assert_eq!(s1, s2, "same init → same final state");
    assert_eq!(println_msgs(&e1), println_msgs(&e2),
        "same init → IDENTICAL captured effects (referential transparency)");
}

#[test]
fn purity_different_init_different_capture() {
    // A different seed yields a different (deterministic) capture — the
    // run is a function OF init, not a constant.
    let rt = rt_with(SRC);
    let (_, e1) = run_nested_capturing(&rt, "countdown", Value::Int(1), 10_000).unwrap();
    let (_, e3) = run_nested_capturing(&rt, "countdown", Value::Int(3), 10_000).unwrap();
    assert_eq!(println_msgs(&e1), vec!["one"]);
    assert_eq!(println_msgs(&e3), vec!["three", "two", "one"]);
}

// ── percolation + single-dispatch, end-to-end via the scheduler ────

/// A `Write` sink backed by a shared buffer the test can inspect after
/// the run (the scheduler's `DispatchContext::stdout` is a boxed trait
/// object that can't be downcast).
#[derive(Clone)]
struct SharedBuf(Arc<Mutex<Vec<u8>>>);
impl Write for SharedBuf {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> { Ok(()) }
}

#[test]
fn percolates_to_parent_dispatched_once_in_order() {
    let rt = rt_with(SRC);
    let buf = Arc::new(Mutex::new(Vec::<u8>::new()));
    let mut ctx = DispatchContext::with_streams(
        Box::new(std::io::BufReader::new(std::io::Cursor::new(Vec::<u8>::new()))),
        Box::new(SharedBuf(buf.clone())),
    );
    let r = run_with_ctx(&rt, &LoopOpts { max_steps: 10 }, &mut ctx)
        .expect("scheduler run");
    assert_eq!(r.exit_code, Some(0), "parent exits 0");

    let out = String::from_utf8(buf.lock().unwrap().clone()).unwrap();
    let lines: Vec<&str> = out.lines().collect();
    // The three child effects, dispatched in order by the PARENT, then
    // the parent's own summary. Exactly four lines: each child effect
    // appears EXACTLY ONCE (no double-dispatch), the child run printed
    // nothing itself (no dispatch-during-child-run — those lines are here
    // only because the parent dispatched the percolated effects), and the
    // order is child-tick order followed by the parent's line.
    assert_eq!(lines, vec!["three", "two", "one", "PARENT-DONE"],
        "child effects percolate, dispatched once, in order, before the parent's own");
}

#[test]
fn run_result_still_pins_into_outer_query() {
    // The percolation doesn't disturb the value contract: the run's final
    // state still pins into the outer model as before.
    let mut rt = rt_with(SRC);
    rt.load_source(
        "claim sat_outer\n    final ∈ Int = run(countdown, 3)\n    final = 0\n",
    ).expect("load outer claim");
    let qr = rt.query("sat_outer", &HashMap::new()).expect("query");
    assert!(qr.satisfied, "run(countdown, 3) must pin final = 0");
}
