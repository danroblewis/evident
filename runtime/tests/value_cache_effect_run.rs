//! Cross-tick value cache, end-to-end through the multi-FSM scheduler.
//!
//! This is the `effect-run`-path analogue of `value_cache.rs` (which
//! drives `query()` directly): it proves `value_cache_hits` climbs when
//! a scheduled FSM is fed inputs it has already seen.
//!
//! The session's motivating case ("idle Mario → identical display
//! inputs frame after frame") does NOT actually arise in the current
//! Mario demo: `display`/`game`/`keyboard` each carry a monotonic
//! per-frame counter (`_frame`, `_game_clock`, `_kb_frame`) in their
//! `given`, and the enemies patrol every tick — so no gameplay FSM ever
//! sees identical cross-tick inputs. The value cache still pays off
//! whenever an FSM's input *repeats*, which the oscillating FSM below
//! exercises directly: its `state` toggles A → B → A → B …, so from the
//! third tick on every input has been seen before and hits the cache.

use std::io::{BufReader, Cursor};
use std::path::Path;

use evident_runtime::{EvidentRuntime, effect_loop};
use evident_runtime::effect_dispatch::DispatchContext;

#[test]
fn repeated_inputs_hit_value_cache_under_scheduler() {
    let mut rt = EvidentRuntime::new();
    rt.load_file(Path::new("../stdlib/runtime.ev")).unwrap();
    // `state` oscillates between two values, so the per-tick `given`
    // the scheduler feeds `try_functionize_z3` repeats from tick 2 on.
    rt.load_source("\
enum Phase = A | B
fsm main
    state ∈ Phase
    state = A ⇒ (state_next = B ∧ effects = ⟨⟩)
    state = B ⇒ (state_next = A ∧ effects = ⟨⟩)
").unwrap();

    let mut ctx = DispatchContext::with_streams(
        Box::new(BufReader::new(Cursor::new(Vec::<u8>::new()))),
        Box::new(std::io::sink()),
    );
    let r = effect_loop::run_with_ctx(
        &rt, &effect_loop::LoopOpts { max_steps: 12 }, &mut ctx).unwrap();
    // The FSM never stops transitioning, so it runs to the step cap.
    assert_eq!(r.steps, 12, "oscillating FSM should run to max_steps");

    let stats = rt.functionize_stats();
    let per = stats.claims.get("main")
        .expect("main FSM should have been functionize-analyzed");
    // Only two distinct inputs (state = A, state = B) exist, so the
    // compiled fn runs at most twice; every later tick is a value hit.
    assert!(per.value_cache_hits >= 5,
        "repeated inputs across ticks should hit the value cache; got vh={}",
        per.value_cache_hits);
}
