//! N2 — the LOOP. The serial spine that threads the tick into a run:
//! initialize state → solve a tick → dispatch its effects → decide halt →
//! thread next-state into prev → repeat.
//!
//! This is single-FSM (one `Problem.fsms[0]`). Phase 3 generalizes the same
//! shape to multiple coordinated FSMs sharing world state; this loop is the
//! degenerate one-FSM case of that scheduler.

use std::collections::BTreeMap;
use std::io::Write;

use crate::effect::dispatch_all;
use crate::halt::{decide, HaltReason};
use crate::spec::{FsmSpec, Problem};
use crate::tick::{solve_tick, TickError};
use crate::z3c::Value;

/// What a finished run reports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunReport {
    /// Number of ticks executed.
    pub ticks: u64,
    /// Process exit code (0 unless an `Exit(code)` effect fired).
    pub exit_code: i32,
    /// Why the loop stopped.
    pub reason: HaltReason,
}

/// Default safety cap so a non-terminating FSM can't spin forever.
pub const DEFAULT_MAX_TICKS: u64 = 100_000;

/// Run the problem's single FSM to halt, dispatching effects to `out`.
pub fn run(problem: &Problem, out: &mut dyn Write, max_ticks: u64) -> Result<RunReport, TickError> {
    let fsm = problem
        .fsms
        .first()
        .ok_or_else(|| TickError::Z3("problem has no FSMs".into()))?;
    run_fsm(fsm, out, max_ticks)
}

/// Run a single FSM to halt.
pub fn run_fsm(fsm: &FsmSpec, out: &mut dyn Write, max_ticks: u64) -> Result<RunReport, TickError> {
    // Seed prev-state from each StateVar's declared init. A state var with no
    // init starts unpinned (Z3 picks tick-0's prev freely).
    let mut prev: BTreeMap<String, Value> = BTreeMap::new();
    for sv in &fsm.state {
        if let Some(lit) = &sv.init {
            prev.insert(sv.prev.clone(), lit.to_value(&sv.sort));
        }
    }
    // No external inputs in the single-FSM loop.
    let given: BTreeMap<String, Value> = BTreeMap::new();

    let mut ticks: u64 = 0;
    loop {
        if ticks >= max_ticks {
            return Ok(RunReport { ticks, exit_code: 0, reason: HaltReason::MaxTicks });
        }

        let model = solve_tick(fsm, &prev, &given)?;
        ticks += 1;

        // Dispatch this tick's effects (Exit is graceful — all run).
        let exit_code = dispatch_all(&model.effects, out)
            .map_err(|e| TickError::Z3(format!("effect IO failed: {e}")))?;

        // Thread next-state into prev for the following tick.
        let mut next_prev = prev.clone();
        for sv in &fsm.state {
            if let Some(v) = model.next_value(&sv.next) {
                next_prev.insert(sv.prev.clone(), v.clone());
            }
        }
        let progressed = next_prev != prev || !model.effects.is_empty();

        let halt = decide(exit_code, model.halt_flag, progressed);
        prev = next_prev;

        if let Some(reason) = halt {
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
    use crate::meta::load_str;

    // Countdown that prints "tick" while the prev count is positive, then
    // "done" + Exit(0) once it hits 0. State threads count → _count.
    const COUNTDOWN: &str = include_str!("../fixtures/countdown.smt2");

    #[test]
    fn countdown_runs_to_exit() {
        let problem = load_str(COUNTDOWN).expect("fixture loads");
        let (stdout, report) = run_to_string(&problem, DEFAULT_MAX_TICKS).unwrap();
        assert_eq!(stdout, "tick\ntick\ntick\ndone\n", "stdout was:\n{stdout}");
        assert_eq!(report.reason, HaltReason::Exit(0));
        assert_eq!(report.exit_code, 0);
        assert_eq!(report.ticks, 4);
    }

    #[test]
    fn max_ticks_caps_a_runaway() {
        // A trivial always-progressing FSM with no halt: count keeps growing,
        // an effect every tick → never NoProgress, no Exit. The cap stops it.
        const SPIN: &str = "\
; @meta
; { \"fsms\": [ { \"name\": \"spin\",
;   \"state\": [{\"prev\":\"_n\",\"next\":\"n\",\"sort\":\"Int\",\"init\":0}],
;   \"effects\": {\"var\":\"effects\"} } ] }
; @end
; @transition spin
(declare-datatypes ((Effect 0)) (((Println (msg String)) (Exit (code Int)))))
(declare-const _n Int)
(declare-const n Int)
(declare-const effects (Seq Effect))
(assert (= n (+ _n 1)))
(assert (= effects (seq.unit (Println \"x\"))))
";
        let problem = load_str(SPIN).unwrap();
        let (_stdout, report) = run_to_string(&problem, 5).unwrap();
        assert_eq!(report.reason, HaltReason::MaxTicks);
        assert_eq!(report.ticks, 5);
    }

    #[test]
    fn no_progress_halts() {
        // Absorbing FSM: n stays equal to _n and emits nothing → NoProgress
        // after the first tick.
        const STILL: &str = "\
; @meta
; { \"fsms\": [ { \"name\": \"still\",
;   \"state\": [{\"prev\":\"_n\",\"next\":\"n\",\"sort\":\"Int\",\"init\":7}] } ] }
; @end
; @transition still
(declare-const _n Int)
(declare-const n Int)
(assert (= n _n))
";
        let problem = load_str(STILL).unwrap();
        let (stdout, report) = run_to_string(&problem, DEFAULT_MAX_TICKS).unwrap();
        assert_eq!(stdout, "");
        assert_eq!(report.reason, HaltReason::NoProgress);
        assert_eq!(report.ticks, 1);
    }
}
