//! Literal-range folder. `literal_range` resolves `Range(lo, hi)` to a
//! concrete `(lo, hi)` pair by translating both bounds and simplifying
//! to literals — what enables `∀ i ∈ {0..n - 1}` unrolling when `n` is
//! pinned.

use std::collections::HashMap;
use z3::ast::Ast;
use z3::Context;

use crate::core::ast::*;
use crate::core::Var;

use super::scalar::translate_int;

/// Resolve `Range(lo, hi)` to a `(lo, hi)` literal pair.
///
/// Both bounds are evaluated through `translate_int` (so identifiers
/// bound to `Var::PinnedInt` resolve to literal `IntVal`s and arithmetic
/// over them folds), then Z3 `simplify` reduces to a literal that
/// `as_i64` can extract. Returns None if either bound stays symbolic
/// (no PinnedInt for it) or the simplified form isn't a literal.
///
/// This is what enables `∀ i ∈ {0..n - 1}` when `n` is bound to a
/// concrete value via `n = #seq` length propagation, `n = 4` pinning,
/// or a `given` value.
///
/// Lives in `exprs` because it builds Z3 expressions (calls
/// `translate_int`) — the prior home in `preprocess` was a layering
/// inversion (preprocess is AST→AST only) AND created a cycle
/// (preprocess → exprs for `translate_int`, exprs → preprocess for
/// `literal_range`).
pub(super) fn literal_range<'ctx>(
    e: &Expr,
    ctx: &'ctx Context,
    env: &HashMap<String, Var<'ctx>>,
) -> Option<(i64, i64)> {
    if let Expr::Range(lo, hi) = e {
        let lo_z3 = translate_int(lo, ctx, env)?;
        let hi_z3 = translate_int(hi, ctx, env)?;
        let lo_v = lo_z3.simplify().as_i64()?;
        let hi_v = hi_z3.simplify().as_i64()?;
        return Some((lo_v, hi_v));
    }
    None
}
