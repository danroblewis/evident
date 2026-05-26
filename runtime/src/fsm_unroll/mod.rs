//! `halts_within(F, N)` via exponentiation-by-squaring FSM composition.
//! Branching bodies (ratio > 1.5 at F^8) are refused cleanly. Trace: `EVIDENT_FSM_UNROLL_TRACE=1`.

mod compose;
mod detector;

pub use compose::assert_halts_within;
// Tier-1 nested-run: halted-state expr → Z3Program → JIT. Same affine gate.
pub use compose::{collapse_run, TierOneRun};
