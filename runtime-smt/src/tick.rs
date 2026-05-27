//! N1 — the TICK: the smallest unit that makes this engine more than raw Z3.
//!
//! A tick takes an FSM's transition relation plus the previous-tick state and
//! the given inputs, pins those onto a solver, solves, and decodes the next
//! state + emitted effects. It composes the three Phase-1 components:
//!   * [`crate::assertion::pin_assertions`] — prev + given → SMT-LIB asserts
//!   * Z3 ([`crate::z3c`]) — parse the transition + pins, `check-sat`
//!   * [`crate::model::extract`] — solved model → typed [`TickModel`]
//!
//! ## Isolation by construction
//!
//! Each tick runs in a FRESH [`Z3Ctx`] that is dropped the moment the tick
//! returns. The decoded [`TickModel`] is owned Rust data (no Z3 handles), so
//! nothing escapes the context. There is therefore *zero* cross-tick Z3 state:
//! no accumulated assertions, no re-declaration clashes, no leaked contexts —
//! the class of fragility that made the legacy runtime's tests flaky simply
//! cannot arise here. (Caching a hot transition is a Phase-4 concern, layered
//! on top without giving up this property — see README.)

use std::collections::BTreeMap;

use crate::assertion::pin_assertions;
use crate::model::extract;
use crate::spec::{FsmSpec, TickModel};
use crate::z3c::{SolveOutcome, Solver, Value, Z3Ctx};

/// Why a tick could not produce a result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TickError {
    /// A value could not be lowered to SMT-LIB (e.g. trying to pin a sequence).
    Pin(String),
    /// Z3 rejected the transition or pin text.
    Z3(String),
    /// The pinned problem has no solution — the transition relation is
    /// inconsistent with the supplied prev/inputs.
    Unsat,
    /// Z3 returned `unknown` (incomplete theory / resource limit).
    Unknown,
}

impl std::fmt::Display for TickError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TickError::Pin(s) => write!(f, "pin: {s}"),
            TickError::Z3(s) => write!(f, "z3: {s}"),
            TickError::Unsat => write!(f, "transition unsat for the given prev/inputs"),
            TickError::Unknown => write!(f, "z3 returned unknown"),
        }
    }
}
impl std::error::Error for TickError {}

/// Solve one tick of `fsm`.
///
/// * `prev` maps each previous-state const name (e.g. `"_count"`) to its value.
/// * `given` maps each input const name to its value.
///
/// Returns the next state, world writes, emitted effects, and halt flag.
pub fn solve_tick(
    fsm: &FsmSpec,
    prev: &BTreeMap<String, Value>,
    given: &BTreeMap<String, Value>,
) -> Result<TickModel, TickError> {
    let pins = pin_assertions(prev, given).map_err(TickError::Pin)?;

    // Fresh context per tick — see the module-level isolation note.
    let ctx = Z3Ctx::new();
    let solver = Solver::new(&ctx);
    solver
        .from_string(&fsm.transition)
        .map_err(|e| TickError::Z3(e.0))?;
    if !pins.is_empty() {
        solver.from_string(&pins).map_err(|e| TickError::Z3(e.0))?;
    }

    match solver.check() {
        SolveOutcome::Sat(model) => Ok(extract(&model, fsm)),
        SolveOutcome::Unsat => Err(TickError::Unsat),
        SolveOutcome::Unknown => Err(TickError::Unknown),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::meta::load_str;
    use crate::spec::EffectValue;

    // A self-contained countdown FSM as an embedded fixture: count = _count - 1
    // each tick, emit a single `Tick` effect, and raise `halt` once count hits 0.
    const COUNTDOWN: &str = "\
; @meta
; {
;   \"fsms\": [
;     { \"name\": \"countdown\",
;       \"state\": [{\"prev\":\"_count\",\"next\":\"count\",\"sort\":\"Int\",\"init\":3}],
;       \"effects\": {\"var\":\"effects\"},
;       \"halt\": {\"var\":\"halt\"} }
;   ]
; }
; @end
; @transition countdown
(declare-datatypes ((Effect 0)) (((Println (msg String)) (Exit (code Int)) (Tick))))
(declare-const _count Int)
(declare-const count Int)
(declare-const effects (Seq Effect))
(declare-const halt Bool)
(assert (= count (- _count 1)))
(assert (= halt (<= count 0)))
(assert (= effects (seq.unit (as Tick Effect))))
";

    fn countdown_fsm() -> FsmSpec {
        load_str(COUNTDOWN).expect("fixture loads").fsms.pop().unwrap()
    }

    fn prev_count(n: i64) -> BTreeMap<String, Value> {
        let mut m = BTreeMap::new();
        m.insert("_count".to_string(), Value::Int(n));
        m
    }

    #[test]
    fn single_tick_decrements() {
        let fsm = countdown_fsm();
        let out = solve_tick(&fsm, &prev_count(3), &BTreeMap::new()).unwrap();
        assert_eq!(out.next_value("count"), Some(&Value::Int(2)));
        assert!(!out.halt_flag);
        assert_eq!(
            out.effects,
            vec![EffectValue { ctor: "Tick".into(), args: vec![] }]
        );
    }

    #[test]
    fn tick_to_zero_sets_halt() {
        let fsm = countdown_fsm();
        let out = solve_tick(&fsm, &prev_count(1), &BTreeMap::new()).unwrap();
        assert_eq!(out.next_value("count"), Some(&Value::Int(0)));
        assert!(out.halt_flag, "count reached 0 → halt");
    }

    #[test]
    fn ticks_are_independent_no_state_leak() {
        // Run several ticks out of order through the same code path; each is a
        // pure function of its inputs (fresh context each time).
        let fsm = countdown_fsm();
        for n in [5, 2, 9, 1, 100] {
            let out = solve_tick(&fsm, &prev_count(n), &BTreeMap::new()).unwrap();
            assert_eq!(out.next_value("count"), Some(&Value::Int(n - 1)));
        }
    }
}
