//! Pre-translation passes: pin literal-int variables, propagate
//! sequence lengths, fold quantifier bounds. All of these run before
//! constraint translation so the translator sees concrete integers
//! where possible (and can then unroll quantifiers, fold Cardinality,
//! etc.).

use std::collections::HashMap;
use z3::ast::{Ast, Int};
use z3::Context;

use crate::ast::*;
use super::types::{Value, Var};
use super::exprs::translate_int;

/// Pre-scan the schema body and `given` for variables that can be
/// pinned to a literal int *before* the solver runs:
///
///   - any `given` entry of value `Value::Int(n)` → `name → n`
///   - any body constraint of shape `name = literal_int_expr` (or
///     reverse) where the literal side resolves to a constant under
///     the names already pinned → `name → value`
///   - any body constraint of shape `name = #seq` where `#seq`'s
///     length itself reduces (e.g. via a sibling `#seq = N` constraint)
///     → `name → length` (length-propagation, mirrors Python's Pass 3)
///
/// Iterates to a fixed point so chains like `n = #s ∧ #s = 4 ∧ k = n - 1`
/// all resolve. The result is fed into `apply_pinned_ints` to upgrade
/// env entries to `Var::PinnedInt`, which lets `literal_range` unroll
/// quantifiers like `∀ i ∈ {0..n - 1}` even when `n` is symbolic.
pub(super) fn collect_pinned_ints(
    body: &[BodyItem],
    given: &HashMap<String, Value>,
    seq_lengths: &HashMap<String, i64>,
) -> HashMap<String, i64> {
    let mut pinned: HashMap<String, i64> = HashMap::new();
    for (k, v) in given {
        if let Value::Int(n) = v { pinned.insert(k.clone(), *n); }
    }
    let mut changed = true;
    while changed {
        changed = false;
        for item in body {
            if let BodyItem::Constraint(Expr::Binary(BinOp::Eq, lhs, rhs)) = item {
                for (a, b) in [(lhs, rhs), (rhs, lhs)] {
                    if let Expr::Identifier(name) = a.as_ref() {
                        if !pinned.contains_key(name) {
                            // Try as a pure-int expression over already-pinned
                            // names + literal Ints + #seq lengths.
                            if let Some(v) = eval_pure_int(b, &pinned, seq_lengths) {
                                pinned.insert(name.clone(), v);
                                changed = true;
                            }
                        }
                    }
                }
            }
        }
    }
    pinned
}

/// Pure constant-folding evaluator over Int expressions. Honors PinnedInt
/// names, literal Ints, arithmetic, and `#seq` references whose lengths
/// are concrete in `seq_lengths`.
fn eval_pure_int(
    e: &Expr,
    pinned: &HashMap<String, i64>,
    seq_lengths: &HashMap<String, i64>,
) -> Option<i64> {
    match e {
        Expr::Int(n) => Some(*n),
        Expr::Identifier(name) => pinned.get(name).copied(),
        Expr::Cardinality(inner) => match inner.as_ref() {
            Expr::Identifier(name) => seq_lengths.get(name).copied(),
            _ => None,
        },
        Expr::Binary(op, lhs, rhs) => {
            let l = eval_pure_int(lhs, pinned, seq_lengths)?;
            let r = eval_pure_int(rhs, pinned, seq_lengths)?;
            Some(match op {
                BinOp::Add => l.checked_add(r)?,
                BinOp::Sub => l.checked_sub(r)?,
                BinOp::Mul => l.checked_mul(r)?,
                BinOp::Div => if r == 0 { return None } else { l / r },
                _ => return None,
            })
        }
        _ => None,
    }
}

/// Pre-scan body for `#seq = literal_int` constraints. Mirrors Python's
/// "Pass 3" length propagation. The returned map is consumed by
/// `collect_pinned_ints` so e.g. `n = #s` resolves through it.
pub(super) fn collect_seq_lengths(
    body: &[BodyItem],
    given: &HashMap<String, Value>,
) -> HashMap<String, i64> {
    let mut out = HashMap::new();
    // Seq lengths from `given` Seq values are exact.
    for (k, v) in given {
        let len = match v {
            Value::SeqInt(v)  => v.len() as i64,
            Value::SeqBool(v) => v.len() as i64,
            Value::SeqStr(v)  => v.len() as i64,
            _ => continue,
        };
        out.insert(k.clone(), len);
    }
    // From body: `#seq = N` (or `N = #seq`) where N is a literal Int,
    // or `seq = ⟨…⟩` (sequence literal pins length to its arity).
    for item in body {
        if let BodyItem::Constraint(Expr::Binary(BinOp::Eq, lhs, rhs)) = item {
            for (a, b) in [(lhs, rhs), (rhs, lhs)] {
                if let Expr::Cardinality(inner) = a.as_ref() {
                    if let Expr::Identifier(name) = inner.as_ref() {
                        if let Expr::Int(n) = b.as_ref() {
                            out.insert(name.clone(), *n);
                        }
                    }
                }
                // `seq_var = ⟨e1, e2, …⟩` pins #seq_var to items.len().
                if let (Expr::Identifier(name), Expr::SeqLit(items)) =
                    (a.as_ref(), b.as_ref())
                {
                    out.insert(name.clone(), items.len() as i64);
                }
            }
        }
    }
    out
}

/// Replace env entries for pinned names with `Var::PinnedInt(value)`.
/// The replacement is a no-op for names not in env (e.g. a `n = 5`
/// constraint where `n` was never declared with `n ∈ ...`).
pub(super) fn apply_pinned_ints<'ctx>(
    env: &mut HashMap<String, Var<'ctx>>,
    pinned: &HashMap<String, i64>,
) {
    for (name, value) in pinned {
        if env.contains_key(name) {
            env.insert(name.clone(), Var::PinnedInt(*value));
        }
    }
}

/// Replace each `Var::SeqVar` / `Var::DatatypeSeqVar`'s symbolic `len`
/// with an `Int::from_i64` literal when `seq_lengths` knows the value.
/// Without this, `translate_int(Cardinality(seq))` returns the
/// solver-side free `len` symbol, so `literal_range` can't fold
/// `Range(0, #seq - 1)` to a concrete pair and the quantifier is
/// silently dropped.
///
/// Idempotent and safe to run after `apply_pinned_ints` (different
/// var kinds, no overlap).
pub(super) fn apply_seq_lengths<'ctx>(
    env: &mut HashMap<String, Var<'ctx>>,
    seq_lengths: &HashMap<String, i64>,
    ctx: &'ctx Context,
) {
    for (name, n) in seq_lengths {
        let Some(var) = env.get(name) else { continue };
        let new_len = Int::from_i64(ctx, *n);
        let new_var = match var {
            Var::SeqVar { arr, elem, .. } => {
                Var::SeqVar { arr: arr.clone(), len: new_len, elem: *elem }
            }
            Var::DatatypeSeqVar { arr, type_name, dt, fields, .. } => {
                Var::DatatypeSeqVar {
                    arr: arr.clone(),
                    len: new_len,
                    type_name: type_name.clone(),
                    dt: *dt,
                    fields: fields.clone(),
                }
            }
            _ => continue,
        };
        env.insert(name.clone(), new_var);
    }
}

/// Resolve `Range(lo, hi)` to a `(lo, hi)` literal pair.
///
/// Both bounds are evaluated through `translate_int` (so identifiers
/// bound to `Var::PinnedInt` resolve to literal `IntVal`s and arithmetic
/// over them folds), then Z3 `simplify` reduces to a literal that
/// `as_i64` can extract. Returns None if either bound stays symbolic
/// (no PinnedInt for it) or the simplified form isn't a literal.
///
/// This is what enables `∀ i ∈ {0..n - 1}` when n is bound to a
/// concrete value via `n = #seq` length propagation, `n = 4`
/// pinning, or a `given` value.
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

/// Clone an env. Var derives Clone (Z3 ast types are reference-counted)
/// so we can shadow the bound variable in quantifier unrolling.
pub(super) fn env_clone<'ctx>(env: &HashMap<String, Var<'ctx>>) -> HashMap<String, Var<'ctx>> {
    env.clone()
}
