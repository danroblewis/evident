//! `runtime-smt` — a greenfield SMT-LIB-input FSM execution engine.
//!
//! The mission (see `docs/plans/new-runtime.md`): a clean Rust runtime whose
//! INPUT is SMT-LIB text + metadata, not Evident syntax. Z3 parses the
//! SMT-LIB; this crate is the EXECUTION ENGINE — per-tick solve, state
//! threading, effect dispatch, halt, scheduling. Additive: never touches the
//! legacy `runtime/`.
//!
//! Module map:
//!   * [`z3c`]       — N0: the Z3 floor. RAII context, solve, model decode.
//!   * [`spec`]      — the metadata model + typed tick result (frozen contract).
//!   * [`meta`]      — N1: load a fixture (embedded metadata + transition).
//!   * [`assertion`] — N1: build per-tick pin assertions (prev state + given).
//!   * [`model`]     — N1: extract typed next-state + effects from a model.
//!   * (further milestones add: tick, effects, driver, scheduler)

pub mod z3c;

pub mod spec;

pub mod assertion;
pub mod driver;
pub mod effect;
pub mod halt;
pub mod meta;
pub mod model;
pub mod schedule;
pub mod scheduler;
pub mod tick;
pub mod world;

pub use driver::{run, run_fsm, run_to_string, RunReport, DEFAULT_MAX_TICKS};
pub use effect::{dispatch_all, DispatchOutcome};
pub use halt::{decide as halt_decide, HaltReason};
pub use tick::{solve_tick, TickError};
pub use spec::{
    EffectSpec, EffectValue, FsmSpec, GivenVar, HaltSpec, Lit, Problem, Sort, StateVar, TickModel,
    WorldVar,
};
pub use z3c::{solve_smtlib, Model, SolveOutcome, Value, Z3Ctx, Z3Error};
