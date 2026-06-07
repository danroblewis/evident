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
                let r = env.get(&name).cloned();
                if r.is_none() && std::env::var("EVIDENT_FUNCTIONIZE_TRACE").is_ok() {
                    eprintln!("[fz/eval] missing env entry: {name}");
                }
                r
            } else {
                None
            }
        }
        // SMT-LIB Int division/modulo are EUCLIDEAN (Boute): the remainder is
        // always non-negative. Rust's div_euclid/rem_euclid match exactly.
        // Division by zero is an underspecified function in SMT — refuse the
        // tick (None → Z3 fallback) rather than guess.
        DeclKind::IDIV => {
            let a = as_int(eval_scalar(ctx, ch[0], env)?)?;
            let b = as_int(eval_scalar(ctx, ch[1], env)?)?;
            if b == 0 { return None; }
            Some(Sv::Int(a.div_euclid(b)))
        }
        DeclKind::MOD => {
            let a = as_int(eval_scalar(ctx, ch[0], env)?)?;
            let b = as_int(eval_scalar(ctx, ch[1], env)?)?;
            if b == 0 { return None; }
            Some(Sv::Int(a.rem_euclid(b)))
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
        // Out-of-range returns None — caller falls through to Z3, preserving
        // soundness on the kernel's `last_results` pattern (an attempt to
        // sentinel-return here diverged from Z3 on programs with strict
        // length pins; commit c420fe6 reverted).
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
        // `(str.len s)` — Z3 SEQ_LENGTH on a string returns the number of
        // unicode code points.
        DeclKind::SEQ_LENGTH => {
            let v = eval_scalar(ctx, ch[0], env)?;
            match v {
                Sv::Str(s) => Some(Sv::Int(s.chars().count() as i64)),
                _ => None,
            }
        }
        // `(str.++ a b ...)` — string concatenation.
        DeclKind::SEQ_CONCAT => {
            let mut out = String::new();
            for &c in &ch {
                let v = eval_scalar(ctx, c, env)?;
                let Sv::Str(s) = v else { return None };
                out.push_str(&s);
            }
            Some(Sv::Str(out))
        }
        // `(str.substr s offset len)` — Z3 SEQ_EXTRACT on a string returns the
        // substring starting at `offset` of length `len` (counted in code points).
        // Out-of-range slices clamp to empty per SMT-LIB semantics.
        DeclKind::SEQ_EXTRACT => {
            let s = match eval_scalar(ctx, ch[0], env)? { Sv::Str(s) => s, _ => return None };
            let off = as_int(eval_scalar(ctx, ch[1], env)?)?;
            let len = as_int(eval_scalar(ctx, ch[2], env)?)?;
            if off < 0 || len < 0 { return Some(Sv::Str(String::new())); }
            let chars: Vec<char> = s.chars().collect();
            let n = chars.len() as i64;
            if off >= n { return Some(Sv::Str(String::new())); }
            let end = (off + len).min(n) as usize;
            let out: String = chars[off as usize..end].iter().collect();
            Some(Sv::Str(out))
        }
        // `(str.indexof s sub off)` — first position of `sub` in `s` starting
        // search at `off`; -1 if not found.
        DeclKind::SEQ_INDEX => {
            let s = match eval_scalar(ctx, ch[0], env)? { Sv::Str(s) => s, _ => return None };
            let sub = match eval_scalar(ctx, ch[1], env)? { Sv::Str(s) => s, _ => return None };
            let off = as_int(eval_scalar(ctx, ch[2], env)?)?;
            if off < 0 { return Some(Sv::Int(-1)); }
            // Convert codepoint offset to byte offset.
            let mut byte_off = 0usize;
            let mut cp_count = 0i64;
            for (i, _) in s.char_indices() {
                if cp_count == off { byte_off = i; break; }
                cp_count += 1;
                byte_off = s.len(); // sentinel if loop falls off
            }
            if cp_count < off { return Some(Sv::Int(-1)); }
            match s[byte_off..].find(&sub) {
                Some(b) => {
                    // Return codepoint position of (byte_off + b).
                    let target = byte_off + b;
                    let pos = s[..target].chars().count() as i64;
                    Some(Sv::Int(pos))
                }
                None => Some(Sv::Int(-1)),
            }
        }
        // `(str.contains s sub)` — Z3 SEQ_CONTAINS.
        DeclKind::SEQ_CONTAINS => {
            let s = match eval_scalar(ctx, ch[0], env)? { Sv::Str(s) => s, _ => return None };
            let sub = match eval_scalar(ctx, ch[1], env)? { Sv::Str(s) => s, _ => return None };
            Some(Sv::Bool(s.contains(&sub)))
        }
        // `(str.prefixof a b)` — true iff `a` is a prefix of `b`.
        DeclKind::SEQ_PREFIX => {
            let a = match eval_scalar(ctx, ch[0], env)? { Sv::Str(s) => s, _ => return None };
            let b = match eval_scalar(ctx, ch[1], env)? { Sv::Str(s) => s, _ => return None };
            Some(Sv::Bool(b.starts_with(&a)))
        }
        // `(str.suffixof a b)` — true iff `a` is a suffix of `b`.
        DeclKind::SEQ_SUFFIX => {
            let a = match eval_scalar(ctx, ch[0], env)? { Sv::Str(s) => s, _ => return None };
            let b = match eval_scalar(ctx, ch[1], env)? { Sv::Str(s) => s, _ => return None };
            Some(Sv::Bool(b.ends_with(&a)))
        }
        // `(str.at s i)` — codepoint at index i as a 1-char string, or "" if oor.
        DeclKind::SEQ_AT => {
            let s = match eval_scalar(ctx, ch[0], env)? { Sv::Str(s) => s, _ => return None };
            let i = as_int(eval_scalar(ctx, ch[1], env)?)?;
            if i < 0 { return Some(Sv::Str(String::new())); }
            match s.chars().nth(i as usize) {
                Some(c) => Some(Sv::Str(c.to_string())),
                None => Some(Sv::Str(String::new())),
            }
        }
        // `(str.replace s from to)` — replace FIRST occurrence of `from` in `s`
        // with `to` (matching Z3 SMT-LIB semantics, not Rust's replace-all).
        DeclKind::SEQ_REPLACE => {
            let s = match eval_scalar(ctx, ch[0], env)? { Sv::Str(s) => s, _ => return None };
            let from = match eval_scalar(ctx, ch[1], env)? { Sv::Str(s) => s, _ => return None };
            let to = match eval_scalar(ctx, ch[2], env)? { Sv::Str(s) => s, _ => return None };
            Some(Sv::Str(s.replacen(&from, &to, 1)))
        }
        // `(seq.unit x)` — 1-element Seq.
        DeclKind::SEQ_UNIT => {
            let v = eval_scalar(ctx, ch[0], env)?;
            Some(Sv::Seq(vec![v]))
        }
        // `(seq.empty)` — empty Seq.
        DeclKind::SEQ_EMPTY => Some(Sv::Seq(Vec::new())),
        // `(str.to-int s)` — parse non-negative int; -1 on bad input.
        DeclKind::STR_TO_INT => {
            let s = match eval_scalar(ctx, ch[0], env)? { Sv::Str(s) => s, _ => return None };
            Some(Sv::Int(s.parse::<i64>().ok().filter(|&n| n >= 0).unwrap_or(-1)))
        }
        // `(int.to-str i)` — base-10 digits for non-negative i; "" for negative.
        DeclKind::INT_TO_STR => {
            let i = as_int(eval_scalar(ctx, ch[0], env)?)?;
            Some(Sv::Str(if i < 0 { String::new() } else { i.to_string() }))
        }
        // `(store arr i v)` — Z3 array store. Returns a Seq with element i set
        // to v; extends with default-pad if needed (Z3 arrays are total but our
        // Seq sentinel is empty-pad; this is fine for the OOB read convention).
        DeclKind::STORE => {
            let mut elems = match eval_scalar(ctx, ch[0], env)? {
                Sv::Seq(es) => es,
                _ => return None,
            };
            let i = as_int(eval_scalar(ctx, ch[1], env)?)?;
            let v = eval_scalar(ctx, ch[2], env)?;
            if i < 0 { return None; }
            let idx = i as usize;
            while elems.len() <= idx { elems.push(Sv::Int(0)); }
            elems[idx] = v;
            Some(Sv::Seq(elems))
        }
        // `((_ is Variant) val)` — Z3's datatype-variant recognizer.
        // True iff `val` is built from the named constructor.
        // Two enum variants exist in z3-sys; the parametric `(_ is …)` form
        // surfaces as DT_IS, while the standalone "is-Foo" decl surfaces as
        // DT_RECOGNISER. Treat them the same.
        DeclKind::DT_IS | DeclKind::DT_RECOGNISER => {
            let v = eval_scalar(ctx, ch[0], env)?;
            let Sv::Datatype(actual, _) = v else {
                if std::env::var("EVIDENT_FUNCTIONIZE_TRACE").is_ok() {
                    eprintln!("[fz/eval] DT_IS: argument is not a Datatype Sv");
                }
                return None;
            };
            let want = super::recognizer_target(ctx, a)?;
            if std::env::var("EVIDENT_FUNCTIONIZE_TRACE").is_ok() {
                eprintln!("[fz/eval] DT_IS: actual={} want={}", actual, want);
            }
            Some(Sv::Bool(actual == want))
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
