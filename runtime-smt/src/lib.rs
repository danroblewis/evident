//! `runtime-smt` — a greenfield SMT-LIB-input FSM execution engine.
//!
//! The mission (see `docs/plans/new-runtime.md`): a clean Rust runtime whose
//! INPUT is SMT-LIB text + metadata, not Evident syntax. Z3 parses the
//! SMT-LIB; this crate is the EXECUTION ENGINE — per-tick solve, state
//! threading, effect dispatch, halt, scheduling. Additive: never touches the
//! legacy `runtime/`.
//!
//! Module map (built milestone by milestone):
//!   * [`z3c`]   — N0: the Z3 floor. RAII context, solve, model decode.
//!   * (further milestones add: metadata, tick, effects, driver, scheduler)

pub mod z3c;

pub use z3c::{solve_smtlib, Model, SolveOutcome, Value, Z3Ctx, Z3Error};
