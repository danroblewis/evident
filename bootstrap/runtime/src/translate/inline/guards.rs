//! Solver-assertion and guard-composition helpers for the inline walker.

use z3::{SatResult, Solver};
use z3::ast::Bool;

/// Assert `b` into the solver; use `assert_and_track` when a tracker is
/// present so the constraint participates in unsat-core extraction.
pub(super) fn track_assert(solver: &Solver<'static>, b: &Bool<'static>, tracker: Option<&Bool<'static>>) {
    match tracker {
        Some(t) => solver.assert_and_track(b, t),
        None    => solver.assert(b),
    }
}

/// True if the guard is satisfiable (or absent); prunes dead ClaimCall
/// expansions via push/check/pop, letting the depth bound cut genuine cycles.
pub(super) fn guard_is_satisfiable(
    solver: &Solver<'static>,
    guard: &Option<Bool<'static>>,
) -> bool {
    let g = match guard {
        None => return true,
        Some(g) => g,
    };
    let trace = std::env::var("EVIDENT_INLINE_TRACE").is_ok();
    let t0 = if trace { Some(std::time::Instant::now()) } else { None };
    solver.push();
    solver.assert(g);
    let result = solver.check();
    solver.pop(1);
    if let Some(t0) = t0 {
        eprintln!("[inline] sat-check {:?} in {:?}", result, t0.elapsed());
    }
    !matches!(result, SatResult::Unsat)
}

/// `guard ⇒ body` if guarded, else `body`. The guard is a pre-translated
/// Z3 Bool, so deeper recursive frames can't rebind its identifiers.
pub(super) fn guarded_bool<'ctx>(b: Bool<'ctx>, guard: &Option<Bool<'ctx>>) -> Bool<'ctx> {
    match guard {
        None => b,
        Some(g) => g.implies(&b),
    }
}

/// Compose two guards: `outer ∧ inner`.
pub(super) fn compose_guards<'ctx>(
    ctx: &'ctx z3::Context,
    outer: &Option<Bool<'ctx>>,
    inner: Bool<'ctx>,
) -> Option<Bool<'ctx>> {
    match outer {
        None => Some(inner),
        Some(o) => Some(Bool::and(ctx, &[o, &inner])),
    }
}
