//! `literal_range`: resolves `Range(lo, hi)` to a concrete `(i64, i64)` pair,
//! enabling `∀ i ∈ {0..n-1}` unrolling when `n` is pinned.

use std::collections::HashMap;
use z3::ast::Ast;
use z3::Context;

use crate::core::ast::*;
use crate::core::Var;

use super::scalar::translate_int;

/// Resolve `Range(lo, hi)` to a concrete `(i64, i64)` pair via `translate_int` + Z3 simplify.
/// Returns None if either bound stays symbolic after simplification.
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
