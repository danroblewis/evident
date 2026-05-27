//! `runtime-contract` — the portable behavior-oracle harness.
//!
//! This crate is the engine-neutral half of the behavior contract: the
//! [`FsmEngine`] trait a replacement Evident FSM engine implements, the fixture
//! loader that reads the captured `runtime-contract/fixtures/`, and the
//! [`run_matrix`] runner that classifies each (engine × fixture) into one honest
//! [`Verdict`] and renders the pass/fail matrix.
//!
//! It depends on neither Z3 nor `evident_runtime` — the golden is held as the
//! engine-neutral [`CVal`]. Each engine lives in its own crate's tests and
//! converts its native model values into `CVal` (or compares via
//! [`CVal::canonical`]):
//!
//!   * `runtime/tests/contract_evolve.rs` — strategy 2 (the existing runtime
//!     driven by SMT-LIB text, via `evident_runtime::smtlib_fsm::solve_tick`).
//!   * `runtime-smt/tests/contract.rs` — strategy 1 (the greenfield
//!     `runtime_smt::solve_tick` engine).
//!
//! See `runtime-contract/README.md` and `FORMAT.md`. The pre-existing
//! `runtime/tests/behavior_contract.rs` (CurrentRuntime + pure-Z3 SmtLib
//! engines) predates this crate and keeps its own inline copy; this lib is what
//! NEW engines plug into.

pub mod engine;
pub mod fixture;
pub mod value;

pub use engine::{classify, run_matrix, EngineColumn, FsmEngine, MatrixReport, Outcome, Verdict};
pub use fixture::{fixtures_dir_from_manifest, load_fixtures, Fixture, Meta};
pub use value::CVal;
