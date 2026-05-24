//! Per-claim constraint inlining — the recursive walker that turns a
//! schema's body items into Z3 assertions on the solver.
//!
//! Split across this directory by concern:
//!
//!   * [`walk`]       — the public entry points (`inline_body_items`,
//!     `inline_body_items_tracked`) + the body-item dispatch loop.
//!   * [`membership`] — the `Membership` arm: declare-if-new, type-use
//!     pins, and type-body / `Seq(T)`-element invariant inheritance.
//!   * [`calls`]      — top-level claim invocation inlining
//!     (positional, tuple-in-claim, guarded `⇒`, `ClaimCall`).
//!   * [`subschema`]  — subclaim-of-type invocation inlining
//!     (`recv.subclaim(args)` and the `∀`-unrolled form).
//!   * [`dispatch`]   — call-name resolution (`CallDispatch`) +
//!     the static `∀`-unroll analysis.
//!   * [`rewrite`]    — pure AST identifier rewriters (prefix
//!     injection for inherited constraints, bound-var substitution).
//!   * [`recursion`]  — the inlining depth bound + helper-local
//!     Z3-const isolation.
//!   * [`guards`]     — solver-assertion + guard-composition helpers.
//!
//! Only `inline_body_items` and `inline_body_items_tracked` are
//! visible outside this module (re-exported below); everything else
//! is internal to the inline pass.

mod calls;
mod dispatch;
mod guards;
mod membership;
mod recursion;
mod rewrite;
mod subschema;
mod walk;

pub(in crate::translate) use walk::{inline_body_items, inline_body_items_tracked};
