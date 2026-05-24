//! Solver-assertion + guard-composition helpers shared by the inline
//! walker and its claim-inlining helpers.

use z3::{SatResult, Solver};
use z3::ast::Bool;

/// Add `b` to the solver. With a tracker, use `assert_and_track` so
/// the constraint joins the unsat-core machinery; otherwise plain
/// `assert`. The tracker stays the same across every assertion derived
/// from one top-level body item, so the entire item shows up as one
/// entry in the core.
pub(super) fn track_assert(solver: &Solver<'static>, b: &Bool<'static>, tracker: Option<&Bool<'static>>) {
    match tracker {
        Some(t) => solver.assert_and_track(b, t),
        None    => solver.assert(b),
    }
}

/// Returns true if the active inlining guard is satisfiable (or there
/// is no guard). Used to PRUNE recursive ClaimCall expansion when the
/// guard is provably false — the body would generate only dead
/// constraints (Z3 would prove them vacuously true), so skipping the
/// inline saves the translation cost.
///
/// Without this prune, recursive transpiler-style claims (e.g.
/// `e_is_binary ⇒ emit_binary` where `emit_binary` calls `emit_expr`
/// on subexpressions) cascade unconditionally — each level multiplies
/// the inlined body count even though most branches won't fire.
///
/// Implementation: push the guard into the solver, ask Z3 if it's
/// satisfiable in the current scope, pop. Z3 prunes propositional
/// contradictions in microseconds; this lets the depth bound do its
/// real job (cutting genuine cycles) instead of bounding work-per-node.
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

/// Combine guard + body Bool: `guard ⇒ body` if guarded, else just
/// the body. Operates on already-translated Z3 Bool asts so the guard's
/// resolution is FROZEN at the point a guarded claim was entered —
/// subsequent shadowing in deeper recursive frames can't accidentally
/// rebind the guard's identifiers to fresh per-frame consts of the
/// same name. (That bug used to silently make recursive transpilers
/// emit unconstrained outputs because the depth-1 `e_is_unaryneg`
/// guard, when consumed at depth-2, resolved to depth-2's freshly
/// shadowed `e_is_unaryneg` — which Z3 had constrained to `false`
/// because the inner expression isn't a UnaryNeg.)
pub(super) fn guarded_bool<'ctx>(b: Bool<'ctx>, guard: &Option<Bool<'ctx>>) -> Bool<'ctx> {
    match guard {
        None => b,
        Some(g) => g.implies(&b),
    }
}

/// Compose two pre-translated guards: `outer ∧ inner`.
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
