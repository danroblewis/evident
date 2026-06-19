//! Scalar translators: `Expr` → Z3 `String` / `Int` / `Real`.

use std::collections::HashMap;
use z3::ast::{Bool, Int, Real, String as Z3Str};
use z3::Context;

use crate::core::{SeqElem, Var};

use super::*;

pub(super) fn translate_str<'ctx>(e: &Expr, ctx: &'ctx Context, env: &HashMap<String, Var<'ctx>>) -> Option<Z3Str<'ctx>> {
    match e {
        Expr::Str(s) => Z3Str::from_str(ctx, s).ok(),
        Expr::Identifier(name) => env.get(name).and_then(|v| v.as_str().cloned()),

        Expr::Binary(BinOp::Concat, lhs, rhs) => {
            let l = translate_str(lhs, ctx, env)?;
            let r = translate_str(rhs, ctx, env)?;
            Some(Z3Str::concat(ctx, &[&l, &r]))
        }

        Expr::Index(seq_expr, idx_expr) => {
            let handle = resolve_seq_handle(seq_expr.as_ref(), ctx, env)?;
            let SeqHandleRef::Primitive { arr, elem, .. } = handle else { return None };
            if elem != SeqElem::Str { return None; }
            let i = translate_int(idx_expr, ctx, env)?;
            arr.select(&i).as_string()
        }

        Expr::Field(_, _) => {
            let (raw, ftype) = resolve_seq_field(e, ctx, env)?;
            if ftype == "String" {
                raw.as_string()
            } else {
                None
            }
        }

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

    if let Expr::Call(name, args) = e {
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

                let inner = x.le(&hi).ite(&x, &hi);
                return Some(inner.ge(&lo).ite(&inner, &lo));
            }

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
            None
        }

        Expr::Index(seq_expr, idx_expr) => {
            let handle = resolve_seq_handle(seq_expr.as_ref(), ctx, env)?;
            let SeqHandleRef::Primitive { arr, elem, .. } = handle else { return None };
            if elem != SeqElem::Int { return None; }
            let i = translate_int(idx_expr, ctx, env)?;
            arr.select(&i).as_int()
        }

        Expr::Field(_, _) => {
            let (raw, ftype) = resolve_seq_field(e, ctx, env)?;
            if matches!(ftype.as_str(), "Int" | "Nat" | "Pos") {
                raw.as_int()
            } else {
                None
            }
        }

        Expr::Ternary(c, a, b) => {
            let cond = translate_bool(c, ctx, env, &HashMap::new())?;
            let then_v = translate_int(a, ctx, env)?;
            let else_v = translate_int(b, ctx, env)?;
            Some(cond.ite(&then_v, &else_v))
        }

        Expr::Match(scr, arms) => {
            let compiled = translate_match_arms(scr, arms, ctx, env,
                |body, e| translate_int(body, ctx, e))?;
            fold_arms_to_ite(compiled)
        }
        _ => None,
    }
}

pub(super) fn translate_real<'ctx>(e: &Expr, ctx: &'ctx Context, env: &HashMap<String, Var<'ctx>>) -> Option<Real<'ctx>> {
    match e {
        Expr::Real(f) => Some(real_from_f64(ctx, *f)),
        Expr::Int(n)  => Some(Real::from_int(&Int::from_i64(ctx, *n))),
        Expr::Identifier(name) => match env.get(name) {
            Some(Var::RealVar(r)) => Some(r.clone()),
            Some(Var::IntVar(i))  => Some(Real::from_int(i)),
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
