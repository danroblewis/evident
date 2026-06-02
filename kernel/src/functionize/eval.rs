//! Native (non-JIT) interpreter over the simplified Z3 body ASTs.
//!
//! `eval_scalar` walks a `Z3_ast` and computes its concrete value given an
//! environment of input/earlier-output bindings (`name → Sv`). This is the
//! always-available slow path the functionizer falls back to when the
//! Cranelift JIT can't compile a shape (strings, datatypes, Seqs) or when
//! `EVIDENT_FUNCTIONIZE_JIT=0`.
//!
//! Soundness: every operator implemented here matches Z3's semantics exactly
//! (standard two's-complement-free big-int arithmetic on the values we
//! actually see, lazy ITE, structural datatype construction). Anything not
//! handled returns `None`, which makes the whole tick fall through to a real
//! Z3 solve. Integer division / modulo are deliberately NOT implemented (Z3's
//! Euclidean `div`/`mod` differ from machine truncation), so a body using them
//! refuses cleanly rather than risk a silent divergence.

use std::collections::HashMap;
use z3_sys::*;

use crate::tick::Sv;
use super::{children, decl_kind, ast_app_name, app_decl_name, accessor_field_index};

/// Evaluate a scalar (Int / Bool / String / Datatype) AST under `env`.
pub unsafe fn eval_scalar(ctx: Z3_context, a: Z3_ast, env: &HashMap<String, Sv>) -> Option<Sv> {
    let kind = Z3_get_ast_kind(ctx, a);
    if kind == AstKind::Numeral {
        let mut n: i64 = 0;
        if Z3_get_numeral_int64(ctx, a, &mut n) {
            return Some(Sv::Int(n));
        }
        return None;
    }
    if kind != AstKind::App {
        return None;
    }

    // String literal (a 0-arity, non-uninterpreted app of String sort).
    if Z3_get_app_num_args(ctx, Z3_to_app(ctx, a)) == 0 {
        if Z3_is_string(ctx, a) {
            let p = Z3_get_string(ctx, a);
            if !p.is_null() {
                let raw = std::ffi::CStr::from_ptr(p).to_string_lossy().into_owned();
                return Some(Sv::Str(crate::tick::unescape_z3_pub(&raw)));
            }
        }
    }

    let dk = decl_kind(ctx, a)?;
    let ch = children(ctx, a);
    match dk {
        DeclKind::TRUE => Some(Sv::Bool(true)),
        DeclKind::FALSE => Some(Sv::Bool(false)),
        DeclKind::UNINTERPRETED => {
            if ch.is_empty() {
                let name = ast_app_name(ctx, a)?;
                env.get(&name).cloned()
            } else {
                None
            }
        }
        DeclKind::ADD | DeclKind::MUL | DeclKind::SUB => {
            let mut it = ch.iter();
            let first = as_int(eval_scalar(ctx, *it.next()?, env)?)?;
            let mut acc = first;
            for &c in it {
                let v = as_int(eval_scalar(ctx, c, env)?)?;
                acc = match dk {
                    DeclKind::ADD => acc.checked_add(v)?,
                    DeclKind::MUL => acc.checked_mul(v)?,
                    DeclKind::SUB => acc.checked_sub(v)?,
                    _ => unreachable!(),
                };
            }
            Some(Sv::Int(acc))
        }
        DeclKind::UMINUS => {
            let v = as_int(eval_scalar(ctx, ch[0], env)?)?;
            Some(Sv::Int(v.checked_neg()?))
        }
        DeclKind::LE | DeclKind::LT | DeclKind::GE | DeclKind::GT => {
            let l = as_int(eval_scalar(ctx, ch[0], env)?)?;
            let r = as_int(eval_scalar(ctx, ch[1], env)?)?;
            Some(Sv::Bool(match dk {
                DeclKind::LE => l <= r,
                DeclKind::LT => l < r,
                DeclKind::GE => l >= r,
                DeclKind::GT => l > r,
                _ => unreachable!(),
            }))
        }
        DeclKind::EQ | DeclKind::IFF => {
            let l = eval_scalar(ctx, ch[0], env)?;
            let r = eval_scalar(ctx, ch[1], env)?;
            Some(Sv::Bool(crate::tick::compare_sv_pub(&l, &r)))
        }
        DeclKind::DISTINCT => {
            let l = eval_scalar(ctx, ch[0], env)?;
            let r = eval_scalar(ctx, ch[1], env)?;
            Some(Sv::Bool(!crate::tick::compare_sv_pub(&l, &r)))
        }
        DeclKind::NOT => {
            let v = as_bool(eval_scalar(ctx, ch[0], env)?)?;
            Some(Sv::Bool(!v))
        }
        DeclKind::AND => {
            for &c in &ch {
                if !as_bool(eval_scalar(ctx, c, env)?)? {
                    return Some(Sv::Bool(false));
                }
            }
            Some(Sv::Bool(true))
        }
        DeclKind::OR => {
            for &c in &ch {
                if as_bool(eval_scalar(ctx, c, env)?)? {
                    return Some(Sv::Bool(true));
                }
            }
            Some(Sv::Bool(false))
        }
        DeclKind::IMPLIES => {
            let p = as_bool(eval_scalar(ctx, ch[0], env)?)?;
            if !p { return Some(Sv::Bool(true)); }
            Some(Sv::Bool(as_bool(eval_scalar(ctx, ch[1], env)?)?))
        }
        DeclKind::ITE => {
            // Lazy: only the taken branch is evaluated, so the untaken branch
            // may reference inputs absent on this tick.
            let c = as_bool(eval_scalar(ctx, ch[0], env)?)?;
            if c { eval_scalar(ctx, ch[1], env) } else { eval_scalar(ctx, ch[2], env) }
        }
        DeclKind::DT_CONSTRUCTOR => {
            let name = app_decl_name(ctx, a)?;
            let mut fields = Vec::with_capacity(ch.len());
            for &c in &ch {
                fields.push(eval_scalar(ctx, c, env)?);
            }
            Some(Sv::Datatype(name, fields))
        }
        // `(select rs i)` — index a record-Seq intermediate bound in env.
        DeclKind::SELECT => {
            let arr = eval_scalar(ctx, ch[0], env)?;
            let idx = as_int(eval_scalar(ctx, ch[1], env)?)?;
            match arr {
                Sv::Seq(elems) if idx >= 0 => elems.into_iter().nth(idx as usize),
                _ => None,
            }
        }
        // `(w r)` — read one field of a Datatype value (a record element).
        DeclKind::DT_ACCESSOR => {
            let v = eval_scalar(ctx, ch[0], env)?;
            let Sv::Datatype(_, fields) = v else { return None };
            let fi = accessor_field_index(ctx, a)?;
            fields.into_iter().nth(fi)
        }
        other => {
            if std::env::var("EVIDENT_FUNCTIONIZE_TRACE").is_ok() {
                let p = Z3_ast_to_string(ctx, a);
                let s = if p.is_null() { String::new() }
                        else { std::ffi::CStr::from_ptr(p).to_string_lossy().into_owned() };
                eprintln!("[fz/eval] unsupported op {other:?}: {s}");
            }
            None
        }
    }
}

fn as_int(v: Sv) -> Option<i64> {
    match v { Sv::Int(n) => Some(n), Sv::Bool(b) => Some(b as i64), _ => None }
}
fn as_bool(v: Sv) -> Option<bool> {
    match v { Sv::Bool(b) => Some(b), Sv::Int(n) => Some(n != 0), _ => None }
}
