//! `halts_within(F, N)` — FSM halt as a constraint, lowered via
//! exponentiation-by-squaring composition of the FSM body.
//!
//! Given an FSM body `F` (a claim with one or more `name, name_next ∈ T`
//! state pairs plus a `halt ∈ Bool`), the lowering builds F^N — the
//! composition of F applied N times — and asserts that the cumulative
//! halt (true iff halt fired at any tick in 1..=N) holds at the end of
//! F^N. Equivalent to `∃ k ∈ [1, N] : halt_k`.
//!
//! Implementation: cached powers F^1, F^2, F^4, ..., F^(2^p) are built
//! incrementally by Z3-substituting the previous power into itself.
//! Each doubling runs a `simplify, solve-eqs, propagate-values,
//! simplify` tactic chain — `solve-eqs` is critical to fold out the
//! bridge state vars between the two halves, which is what makes
//! affine bodies collapse to closed form. For arbitrary N, F^N is
//! assembled by chaining the cached powers picked out by N's binary
//! expansion.
//!
//! Gating: after F^2 is built, the [`detector`] compares the unique-
//! AST-node count of F^2 to F^1. If the ratio exceeds 1.5, the body
//! is data-dependent / branching (Z's measurement showed this regime
//! plateaus at ratio ~2× regardless of more doublings) and the
//! technique refuses cleanly — stderr diagnostic, outer solver gets
//! `assert false` so the enclosing claim resolves UNSAT (an honest
//! "I can't prove this" rather than a wrong answer).
//!
//! Trace via `EVIDENT_FSM_UNROLL_TRACE=1`. See
//! [`docs/design/fsm-halts-within.md`] for the halt convention, the
//! affine-step rationale, and the worked counter example.

mod compose;
mod detector;

// `HaltsWithinError` is surfaced only via its Display impl by the
// caller (the inline walker prints `eprintln!("[halts_within] {e}")`),
// so the type itself doesn't need re-exporting.
pub use compose::assert_halts_within;

// Tier-1 nested-run wiring: `collapse_run` reads the composer's
// *halted-state* expression (rather than the halt Bool) into a
// function-shaped `Z3Program`, which `runtime/src/runtime/query.rs`
// JITs via the Cranelift functionizer. Gated by the same affine
// detector that gates `halts_within`. See
// `docs/design/nested-fsm-strategies.md` §7 (step 3).
pub use compose::{collapse_run, TierOneRun};
