//! Exponentiation-by-squaring FSM composition. The closed-form unroller
//! (`build_f1`/`double`/`series`) backs the tier-1 `collapse_run` JIT path and
//! is retained for the §6.2 BMC discharge of `F(seed, fsm_state)`. Branching
//! bodies (ratio > 1.5 at F^8) are refused cleanly. Trace: `EVIDENT_FSM_UNROLL_TRACE=1`.

mod compose;
mod detector;

// Tier-1 nested-run: halted-state expr → Z3Program → JIT. Same affine gate.
pub use compose::{collapse_run, TierOneRun};
