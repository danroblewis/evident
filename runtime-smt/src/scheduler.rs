//! N3 — multi-FSM scheduling over shared world state.
//!
//! The serial spine that generalizes the [`crate::driver`] single-FSM loop to
//! several FSMs that coordinate through a shared world. Each tick:
//!   1. run the FSMs in writer-first order ([`crate::schedule::order`]) so a
//!      reader sees the value a writer produced THIS tick;
//!   2. for each FSM: pin its world reads ([`crate::world::build_given`]) and
//!      its private prev-state, solve the tick, dispatch its effects, and fold
//!      its world writes back into the shared world ([`crate::world::record_writes`]);
//!   3. thread each FSM's private next-state into its prev, and the shared
//!      world into the next tick's prev-world;
//!   4. decide halt (any `Exit` → graceful stop; else any halt flag; else stop
//!      when no FSM made progress).
//!
//! A single-FSM problem is the degenerate case (empty world, trivial order), so
//! this path subsumes the N2 loop; the CLI routes every `run` through here.

use std::collections::BTreeMap;
use std::io::Write;

use crate::cache::TickCache;
use crate::driver::RunReport;
use crate::effect::dispatch_all_with_results;
use crate::halt::{decide, HaltReason};
use crate::schedule::order;
use crate::spec::{FsmSpec, Problem, TickModel};
use crate::tick::{solve_tick, TickError};
use crate::world::{build_given, init_world, record_writes};
use crate::z3c::Value;

/// Run a (possibly multi-FSM) problem to halt, dispatching effects to `out`.
/// Solves every tick freshly (no caching).
pub fn run(problem: &Problem, out: &mut dyn Write, max_ticks: u64) -> Result<RunReport, TickError> {
    run_with(problem, out, max_ticks, |fsm, prev, given| solve_tick(fsm, prev, given))
}

/// Like [`run`], but memoize tick solves through `cache` (N4a). Re-entering an
/// already-seen (FSM, prev-state, given) returns the stored result without
/// calling Z3 — useful for FSMs that cycle through a bounded set of states.
/// The cache is the caller's, so its hit/miss stats survive the call.
pub fn run_cached(
    problem: &Problem,
    out: &mut dyn Write,
    max_ticks: u64,
    cache: &mut TickCache,
) -> Result<RunReport, TickError> {
    run_with(problem, out, max_ticks, |fsm, prev, given| cache.solve(fsm, prev, given))
}

/// The shared multi-FSM loop, parameterized over how a single tick is solved
/// (fresh vs cached). Keeps the two entry points byte-for-byte identical in
/// behavior — caching is transparent.
fn run_with<F>(
    problem: &Problem,
    out: &mut dyn Write,
    max_ticks: u64,
    mut solve: F,
) -> Result<RunReport, TickError>
where
    F: FnMut(&FsmSpec, &BTreeMap<String, Value>, &BTreeMap<String, Value>) -> Result<TickModel, TickError>,
{
    if problem.fsms.is_empty() {
        return Err(TickError::Z3("problem has no FSMs".into()));
    }
    let order = order(&problem.fsms).map_err(TickError::Z3)?;

    // Per-FSM private prev-state, seeded from each StateVar's init.
    let mut fsm_prev: Vec<BTreeMap<String, Value>> = problem
        .fsms
        .iter()
        .map(|fsm| {
            let mut m = BTreeMap::new();
            for sv in &fsm.state {
                if let Some(lit) = &sv.init {
                    m.insert(sv.prev.clone(), lit.to_value(&sv.sort));
                }
            }
            m
        })
        .collect();

    // Shared world as of end of last tick (init before tick 0).
    let mut world_prev = init_world(&problem.world);

    // Per-FSM `last_results` carry: the ordered `Result`-enum Seq produced by
    // THIS FSM's effects last tick, threaded into next tick's given. Empty on
    // tick 0 (no prior effects). Only FSMs that declare a `last_results` spec
    // participate; the rest carry an unused empty Seq.
    let mut fsm_last_results: Vec<Value> = problem
        .fsms
        .iter()
        .map(|_| Value::Seq(Vec::new()))
        .collect();

    let mut ticks: u64 = 0;
    loop {
        if ticks >= max_ticks {
            return Ok(RunReport { ticks, exit_code: 0, reason: HaltReason::MaxTicks });
        }

        let mut world_current: BTreeMap<String, Value> = BTreeMap::new();
        let mut exit_code: Option<i32> = None;
        let mut any_progress = false;
        let mut any_halt_flag = false;

        for &idx in &order {
            let fsm = &problem.fsms[idx];
            let mut given = build_given(fsm, &world_current, &world_prev);
            // Pin the previous tick's effect results under the declared var.
            if let Some(lr) = &fsm.last_results {
                given.insert(lr.var.clone(), fsm_last_results[idx].clone());
            }
            let model = solve(fsm, &fsm_prev[idx], &given)?;

            let (exit, results) = dispatch_all_with_results(&model.effects, out)
                .map_err(|e| TickError::Z3(format!("effect IO failed: {e}")))?;
            if let Some(c) = exit {
                if exit_code.is_none() {
                    exit_code = Some(c);
                }
            }
            // Carry this tick's results forward as next tick's last_results.
            fsm_last_results[idx] = Value::Seq(results);

            record_writes(&model, &mut world_current);

            // Thread this FSM's private next-state into its prev.
            let prev = &mut fsm_prev[idx];
            let mut changed = false;
            for sv in &fsm.state {
                if let Some(v) = model.next_value(&sv.next) {
                    if prev.get(&sv.prev) != Some(v) {
                        changed = true;
                    }
                    prev.insert(sv.prev.clone(), v.clone());
                }
            }
            any_progress |= changed || !model.effects.is_empty();
            any_halt_flag |= model.halt_flag;
        }

        // Thread the shared world into the next tick (writes overlay carried-forward values).
        for (k, v) in world_current {
            world_prev.insert(k, v);
        }

        ticks += 1;
        if let Some(reason) = decide(exit_code, any_halt_flag, any_progress) {
            let code = match reason {
                HaltReason::Exit(c) => c,
                _ => 0,
            };
            return Ok(RunReport { ticks, exit_code: code, reason });
        }
    }
}

/// Convenience for tests / cross-check: run into a captured String.
pub fn run_to_string(problem: &Problem, max_ticks: u64) -> Result<(String, RunReport), TickError> {
    let mut buf: Vec<u8> = Vec::new();
    let report = run(problem, &mut buf, max_ticks)?;
    Ok((String::from_utf8_lossy(&buf).into_owned(), report))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::driver::DEFAULT_MAX_TICKS;
    use crate::meta::load_str;

    const TWO_FSMS: &str = include_str!("../fixtures/two_fsms.smt2");
    const COUNTDOWN: &str = include_str!("../fixtures/countdown.smt2");
    const PINGPONG: &str = include_str!("../fixtures/pingpong.smt2");
    const FEEDBACK: &str = include_str!("../fixtures/feedback_loop.smt2");

    #[test]
    fn two_fsms_producer_consumer() {
        let problem = load_str(TWO_FSMS).expect("fixture loads");
        let (stdout, report) = run_to_string(&problem, DEFAULT_MAX_TICKS).unwrap();
        assert_eq!(
            stdout, "consumed\nconsumed\nconsumed\nproducer done\n",
            "stdout was:\n{stdout}"
        );
        assert_eq!(report.reason, HaltReason::Exit(0));
        assert_eq!(report.exit_code, 0);
        assert_eq!(report.ticks, 4);
    }

    #[test]
    fn cached_run_collapses_revisited_states() {
        // pingpong revisits only 2 prev-state values; over a 10-tick capped run
        // the cache solves Z3 twice and serves the other 8 ticks from cache.
        use crate::cache::TickCache;
        let problem = load_str(PINGPONG).expect("fixture loads");
        let mut cache = TickCache::new();
        let mut buf: Vec<u8> = Vec::new();
        let report = run_cached(&problem, &mut buf, 10, &mut cache).unwrap();
        assert_eq!(report.reason, HaltReason::MaxTicks);
        assert_eq!(report.ticks, 10);
        assert_eq!(cache.misses(), 2, "only 2 distinct prev states");
        assert_eq!(cache.hits(), 8, "remaining ticks served from cache");
    }

    #[test]
    fn cached_and_uncached_runs_agree() {
        // Caching is transparent: same observable result either way.
        use crate::cache::TickCache;
        let problem = load_str(COUNTDOWN).expect("fixture loads");
        let (plain, r1) = run_to_string(&problem, DEFAULT_MAX_TICKS).unwrap();
        let mut cache = TickCache::new();
        let mut buf: Vec<u8> = Vec::new();
        let r2 = run_cached(&problem, &mut buf, DEFAULT_MAX_TICKS, &mut cache).unwrap();
        assert_eq!(plain, String::from_utf8_lossy(&buf));
        assert_eq!(r1, r2);
    }

    #[test]
    fn scheduler_subsumes_single_fsm() {
        // The same countdown the N2 driver runs, routed through the multi-FSM
        // scheduler: identical observable behavior.
        let problem = load_str(COUNTDOWN).expect("fixture loads");
        let (stdout, report) = run_to_string(&problem, DEFAULT_MAX_TICKS).unwrap();
        assert_eq!(stdout, "tick\ntick\ntick\ndone\n");
        assert_eq!(report.reason, HaltReason::Exit(0));
        assert_eq!(report.ticks, 4);
    }

    #[test]
    fn feedback_loop_threads_last_results_across_ticks() {
        // The proof that effect RESULTS thread across ticks:
        //   tick 0 emits IntToStr(42) -> dispatcher produces StringResult("42")
        //   tick 1 reads last_results[0] = StringResult("42") -> Println("42")
        //   tick 2 Println("bye") + Exit(0)
        // If threading were broken, tick 1 would print "" (the StringResult
        // payload would never reach it), so "42" on the first line is the
        // load-bearing assertion.
        let problem = load_str(FEEDBACK).expect("fixture loads");
        let (stdout, report) = run_to_string(&problem, DEFAULT_MAX_TICKS).unwrap();
        assert_eq!(stdout, "42\nbye\n", "stdout was:\n{stdout}");
        assert_eq!(report.reason, HaltReason::Exit(0));
        assert_eq!(report.exit_code, 0);
        assert_eq!(report.ticks, 3);
    }
}
