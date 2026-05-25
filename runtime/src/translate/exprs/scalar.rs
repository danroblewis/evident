//! Per-sort scalar translators — `translate_str`, `translate_int`,
//! `translate_real` — and the Real-literal helper `real_from_f64`. Each
//! turns an `Expr` into a Z3 value of its sort, recursing into the Bool
//! dispatcher / match folder / seq-field resolvers as needed.

use std::collections::HashMap;
use z3::ast::{Ast, Bool, Int, Real, String as Z3Str};
use z3::Context;

use crate::core::ast::*;
use crate::core::{SeqElem, Var};

use super::bool::translate_bool;
use super::match_expr::{fold_arms_to_ite, translate_match_arms};
use super::seq_field::{resolve_seq_field, resolve_seq_handle, SeqHandleRef};

pub(super) fn translate_str<'ctx>(e: &Expr, ctx: &'ctx Context, env: &HashMap<String, Var<'ctx>>) -> Option<Z3Str<'ctx>> {
    // String-producing builtins (`substr` / `replace` / `char_at`),
    // lowered to Z3 seq-theory primitives. Checked first so a Call to one
    // of these names resolves before the generic `match e` paths.
    if let Expr::Call(name, args) = e {
        if let Some(s) = super::string_ops::translate_str_call(name, args, ctx, env) {
            return Some(s);
        }
    }
    match e {
        Expr::Str(s) => Z3Str::from_str(ctx, s).ok(),
        Expr::Identifier(name) => env.get(name).and_then(|v| v.as_str().cloned()),
        // `lhs ++ rhs` — string concatenation. Both operands must translate
        // as strings; the result is a Z3 string concat.
        Expr::Binary(BinOp::Concat, lhs, rhs) => {
            let l = translate_str(lhs, ctx, env)?;
            let r = translate_str(rhs, ctx, env)?;
            Some(Z3Str::concat(ctx, &[&l, &r]))
        }
        // `seq[i]` where seq holds String elements.
        Expr::Index(seq_expr, idx_expr) => {
            let handle = resolve_seq_handle(seq_expr.as_ref(), ctx, env)?;
            let SeqHandleRef::Primitive { arr, elem, .. } = handle else { return None };
            if elem != SeqElem::Str { return None; }
            let i = translate_int(idx_expr, ctx, env)?;
            arr.select(&i).as_string()
        }
        // `pts[i].name` where pts is Seq(UserType) and `name` is a
        // String field of UserType.
        Expr::Field(_, _) => {
            let (raw, ftype) = resolve_seq_field(e, ctx, env)?;
            if ftype == "String" {
                raw.as_string()
            } else {
                None
            }
        }
        // `cond ? a : b` — String-typed branches via Z3 ITE.
        Expr::Ternary(c, a, b) => {
            let cond = translate_bool(c, ctx, env, &HashMap::new())?;
            let then_v = translate_str(a, ctx, env)?;
            let else_v = translate_str(b, ctx, env)?;
            Some(cond.ite(&then_v, &else_v))
        }
        Expr::Match(scr, arms) => {
            let compiled = translate_match_arms(scr, arms, ctx, env,
                |body, e| translate_str(body, ctx, e))?;
            fold_arms_to_ite(compiled)
        }
        _ => None,
    }
}

pub(super) fn translate_int<'ctx>(e: &Expr, ctx: &'ctx Context, env: &HashMap<String, Var<'ctx>>) -> Option<Int<'ctx>> {
    // Int-typed builtins: `min`, `max`, `abs`, `mod`, `clamp`.
    // All lower to Z3 ITE compositions over translated args, so
    // they share `translate_int`'s recursion and play with the
    // rest of integer arithmetic transparently.
    if let Expr::Call(name, args) = e {
        // Int-producing string builtins (`str_len` / `index_of`), lowered
        // to Z3 `str.len` / `str.indexof`.
        if let Some(i) = super::string_ops::translate_str_int_call(name, args, ctx, env) {
            return Some(i);
        }
        match (name.as_str(), args.len()) {
            ("min", 2) => {
                let a = translate_int(&args[0], ctx, env)?;
                let b = translate_int(&args[1], ctx, env)?;
                return Some(a.le(&b).ite(&a, &b));
            }
            ("max", 2) => {
                let a = translate_int(&args[0], ctx, env)?;
                let b = translate_int(&args[1], ctx, env)?;
                return Some(a.ge(&b).ite(&a, &b));
            }
            ("abs", 1) => {
                let x = translate_int(&args[0], ctx, env)?;
                let zero = Int::from_i64(ctx, 0);
                let neg = Int::sub(ctx, &[&zero, &x]);
                return Some(x.ge(&zero).ite(&x, &neg));
            }
            ("mod", 2) => {
                let a = translate_int(&args[0], ctx, env)?;
                let b = translate_int(&args[1], ctx, env)?;
                return Some(a.modulo(&b));
            }
            ("clamp", 3) => {
                let x  = translate_int(&args[0], ctx, env)?;
                let lo = translate_int(&args[1], ctx, env)?;
                let hi = translate_int(&args[2], ctx, env)?;
                // max(lo, min(x, hi))
                let inner = x.le(&hi).ite(&x, &hi);
                return Some(inner.ge(&lo).ite(&inner, &lo));
            }
            // `position_of(seq, x)` — index of `x` in `seq` for the
            // first match, or -1 if not present. Implemented as a
            // chained ITE over the seq's pinned-length positions:
            //
            //     seq[0] = x ? 0 : (seq[1] = x ? 1 : … : -1)
            //
            // No side effects, no fresh constants — just an
            // expression Z3 can fold. For distinct-valued seqs the
            // result is the unique position. For Seqs with the
            // element appearing multiple times, returns the lowest
            // index (well-defined; mirrors Z3 / Python semantics).
            //
            // Primitive Seq path only in v1; Datatype-Seq element
            // types fall through.
            ("position_of", 2) => {
                let Expr::Identifier(sname) = &args[0] else { return None };
                let var = env.get(sname)?;
                let (arr, len, elem) = var.as_seq()?;
                let n = len.simplify().as_i64()?;
                let mut result = Int::from_i64(ctx, -1);
                for i in (0..n).rev() {
                    let idx = Int::from_i64(ctx, i);
                    let cell = arr.select(&idx);
                    let eq = match elem {
                        SeqElem::Int => {
                            let v = translate_int(&args[1], ctx, env)?;
                            cell.as_int()?._eq(&v)
                        }
                        SeqElem::Bool => {
                            let v = match &args[1] {
                                Expr::Bool(b) => Bool::from_bool(ctx, *b),
                                Expr::Identifier(n) => env.get(n)?.as_bool()?.clone(),
                                _ => return None,
                            };
                            cell.as_bool()?._eq(&v)
                        }
                        SeqElem::Str => {
                            let v = translate_str(&args[1], ctx, env)?;
                            cell.as_string()?._eq(&v)
                        }
                    };
                    result = eq.ite(&idx, &result);
                }
                return Some(result);
            }
            _ => {}
        }
    }
    match e {
        Expr::Int(n) => Some(Int::from_i64(ctx, *n)),
        Expr::Identifier(name) => match env.get(name) {
            Some(Var::IntVar(i)) => Some(i.clone()),
            Some(Var::PinnedInt(v)) => Some(Int::from_i64(ctx, *v)),
            _ => None,
        },
        Expr::Binary(op, lhs, rhs) => {
            let l = translate_int(lhs, ctx, env)?;
            let r = translate_int(rhs, ctx, env)?;
            Some(match op {
                BinOp::Add => Int::add(ctx, &[&l, &r]),
                BinOp::Sub => Int::sub(ctx, &[&l, &r]),
                BinOp::Mul => Int::mul(ctx, &[&l, &r]),
                BinOp::Div => l.div(&r),
                _ => return None,
            })
        }
        // `#seq` → the seq's length variable. Both primitive Seq and
        // composite-element Seq (DatatypeSeqVar) expose a length.
        // For Sets (both flavors), Z3 has no native cardinality — we
        // return the recorded candidates count if the Set was pinned
        // via `S = {…}`; otherwise drop (silent, same as today for
        // unpinned Set extraction).
        //
        // Also handles `#groups[0].items` — Cardinality of a Seq-field
        // on a composite-Seq element. Routes through `resolve_seq_handle`
        // which understands both shapes.
        Expr::Cardinality(inner) => {
            if let Some(handle) = resolve_seq_handle(inner.as_ref(), ctx, env) {
                return Some(handle.len().clone());
            }
            if let Expr::Identifier(name) = inner.as_ref() {
                if let Some(var) = env.get(name) {
                    if let Some((_, _, candidates)) = var.as_set_with_candidates() {
                        if let Some(cands) = candidates.borrow().as_ref() {
                            return Some(Int::from_i64(ctx, cands.len() as i64));
                        }
                    }
                    if let Some((_, _, _, _, candidates)) = var.as_datatype_set() {
                        if let Some(cands) = candidates.borrow().as_ref() {
                            return Some(Int::from_i64(ctx, cands.len() as i64));
                        }
                    }
                }
            }
            // `#text` where `text` is a String → `str.len`. Tried last so
            // Seq/Set cardinality (above) still wins for those sorts.
            if let Some(s) = translate_str(inner, ctx, env) {
                return Some(super::string_ops::str_length(ctx, &s));
            }
            None
        }
        // `seq[i]` where seq holds Int elements → Array.select(i) → Int.
        // The seq can be a bare Identifier (top-level Seq var) OR a
        // `Field(Index(...), seq_field_name)` chain (a SeqField on a
        // composite-Seq element — unlocks `groups[0].items[0]`-style
        // nested access).
        Expr::Index(seq_expr, idx_expr) => {
            let handle = resolve_seq_handle(seq_expr.as_ref(), ctx, env)?;
            let SeqHandleRef::Primitive { arr, elem, .. } = handle else { return None };
            if elem != SeqElem::Int { return None; }
            let i = translate_int(idx_expr, ctx, env)?;
            arr.select(&i).as_int()
        }
        // `pts[i].x` where pts is Seq(UserType) and `x` is an Int field.
        Expr::Field(_, _) => {
            let (raw, ftype) = resolve_seq_field(e, ctx, env)?;
            if matches!(ftype.as_str(), "Int" | "Nat" | "Pos") {
                raw.as_int()
            } else {
                None
            }
        }
        // `cond ? a : b` — ternary conditional. Both branches must
        // translate as Int; lifted to Z3's ITE.
        Expr::Ternary(c, a, b) => {
            let cond = translate_bool(c, ctx, env, &HashMap::new())?;
            let then_v = translate_int(a, ctx, env)?;
            let else_v = translate_int(b, ctx, env)?;
            Some(cond.ite(&then_v, &else_v))
        }
        // `match scrutinee { Ctor(b) ⇒ body | _ ⇒ fallback }` with
        // Int-typed arm bodies → nested ITE.
        Expr::Match(scr, arms) => {
            let compiled = translate_match_arms(scr, arms, ctx, env,
                |body, e| translate_int(body, ctx, e))?;
            fold_arms_to_ite(compiled)
        }
        _ => None,
    }
}

/// Translate an Expr that should evaluate to a Z3 Real. Mirrors
/// `translate_int` for the Real domain. Supports:
///   - Real literals (`3.14`)
///   - Identifier resolving to `Var::RealVar`
///   - Binary arithmetic (`+`, `-`, `*`, `/`) with operands that
///     translate as Real OR can be coerced from Int (Z3 supports
///     mixed Int/Real arithmetic by lifting Int to Real).
///   - Unary minus via `0 - e` desugaring (parser does this already).
/// Returns None if the expression doesn't fit any of these patterns —
/// caller (typically `translate_bool`'s Eq/comparison arms) tries
/// other type paths.
pub(super) fn translate_real<'ctx>(e: &Expr, ctx: &'ctx Context, env: &HashMap<String, Var<'ctx>>) -> Option<Real<'ctx>> {
    match e {
        Expr::Real(f) => Some(real_from_f64(ctx, *f)),
        Expr::Int(n)  => Some(Real::from_int(&Int::from_i64(ctx, *n))),  // numeric literal coercion
        Expr::Identifier(name) => match env.get(name) {
            Some(Var::RealVar(r)) => Some(r.clone()),
            Some(Var::IntVar(i))  => Some(Real::from_int(i)),     // promote int var
            Some(Var::PinnedInt(v)) => Some(Real::from_int(&Int::from_i64(ctx, *v))),
            _ => None,
        },
        Expr::Binary(op, lhs, rhs) => {
            let l = translate_real(lhs, ctx, env)?;
            let r = translate_real(rhs, ctx, env)?;
            Some(match op {
                BinOp::Add => Real::add(ctx, &[&l, &r]),
                BinOp::Sub => Real::sub(ctx, &[&l, &r]),
                BinOp::Mul => Real::mul(ctx, &[&l, &r]),
                BinOp::Div => l.div(&r),
                _ => return None,
            })
        }
        // `cond ? a : b` — Real-typed branches via Z3 ITE. The condition
        // is a boolean expression; we don't have a `schemas` table here,
        // so claim-call conditions in ternary aren't supported in Real
        // context (use a Bool intermediate variable instead).
        Expr::Ternary(c, a, b) => {
            let cond = translate_bool(c, ctx, env, &HashMap::new())?;
            let then_v = translate_real(a, ctx, env)?;
            let else_v = translate_real(b, ctx, env)?;
            Some(cond.ite(&then_v, &else_v))
        }
        Expr::Match(scr, arms) => {
            let compiled = translate_match_arms(scr, arms, ctx, env,
                |body, e| translate_real(body, ctx, e))?;
            fold_arms_to_ite(compiled)
        }
        _ => None,
    }
}

/// Local copy of the Real-from-f64 helper. Same shape as the one in
/// `eval.rs` (private there); duplicated to avoid a cross-module
/// dependency for one tiny helper.
///
/// Splits f64's Display form (`"3.14"`) into pure-integer num/den
/// (`"314" / "100"`) so Z3's numeral parser only sees integers.
/// Z3's parser is finicky about decimals embedded in `"num/den"`.
pub(super) fn real_from_f64<'ctx>(ctx: &'ctx Context, f: f64) -> Real<'ctx> {
    if f.is_nan() || f.is_infinite() {
        return Real::from_real(ctx, 0, 1);
    }
    let s = f.to_string();
    let (num, den) = if let Some(dot) = s.find('.') {
        let (int_part, frac_with_dot) = s.split_at(dot);
        let frac = &frac_with_dot[1..];
        (format!("{}{}", int_part, frac),
         format!("1{}", "0".repeat(frac.len())))
    } else {
        (s, "1".to_string())
    };
    Real::from_real_str(ctx, &num, &den)
        .unwrap_or_else(|| Real::from_real(ctx, 0, 1))
}
