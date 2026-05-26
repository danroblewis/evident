//! String builtins lowered to Z3 seq theory: `str_len`, `index_of`, `substr`,
//! `replace`, `char_at`, `str_contains`, `starts_with`, `ends_with`.

use std::collections::HashMap;

use z3::ast::{Ast, Bool, Int, String as Z3Str};
use z3::Context;

use crate::core::ast::Expr;
use crate::core::Var;

use super::scalar::{translate_int, translate_str};

// `z3::Context` is a single-field newtype around `Z3_context`; we reach the
// raw pointer for unwrapped seq builders. The const_assert guards the layout.
const _: () = {
    assert!(
        std::mem::size_of::<Context>() == std::mem::size_of::<z3_sys::Z3_context>(),
        "z3::Context is no longer a single-pointer newtype; raw_ctx is unsound"
    );
};

#[inline]
fn raw_ctx(ctx: &Context) -> z3_sys::Z3_context {
    // SAFETY: `Context` is a single-field newtype around `Z3_context`; the
    // `size_of` assertion above verifies the layout hasn't changed.
    unsafe { *(ctx as *const Context as *const z3_sys::Z3_context) }
}

/// `str.len text` — length of `text` as a non-negative Int.
pub(super) fn str_length<'ctx>(ctx: &'ctx Context, s: &Z3Str<'ctx>) -> Int<'ctx> {
    unsafe { Int::wrap(ctx, z3_sys::Z3_mk_seq_length(raw_ctx(ctx), s.get_z3_ast())) }
}

/// `str.substr text off len` — SMT-LIB semantics: negative/OOB off or len
/// yields ""; len past end is clamped.
pub(super) fn str_substr<'ctx>(
    ctx: &'ctx Context,
    s: &Z3Str<'ctx>,
    off: &Int<'ctx>,
    len: &Int<'ctx>,
) -> Z3Str<'ctx> {
    unsafe {
        Z3Str::wrap(
            ctx,
            z3_sys::Z3_mk_seq_extract(raw_ctx(ctx), s.get_z3_ast(), off.get_z3_ast(), len.get_z3_ast()),
        )
    }
}

/// `str.replace text src dst` — replace FIRST occurrence of `src` with `dst`;
/// returns `text` unchanged if `src` is absent.
pub(super) fn str_replace<'ctx>(
    ctx: &'ctx Context,
    s: &Z3Str<'ctx>,
    src: &Z3Str<'ctx>,
    dst: &Z3Str<'ctx>,
) -> Z3Str<'ctx> {
    unsafe {
        Z3Str::wrap(
            ctx,
            z3_sys::Z3_mk_seq_replace(raw_ctx(ctx), s.get_z3_ast(), src.get_z3_ast(), dst.get_z3_ast()),
        )
    }
}

/// `str.indexof text sub off` — first occurrence of `sub` at or after `off`, or -1.
pub(super) fn str_index_of<'ctx>(
    ctx: &'ctx Context,
    s: &Z3Str<'ctx>,
    sub: &Z3Str<'ctx>,
    off: &Int<'ctx>,
) -> Int<'ctx> {
    unsafe {
        Int::wrap(
            ctx,
            z3_sys::Z3_mk_seq_index(raw_ctx(ctx), s.get_z3_ast(), sub.get_z3_ast(), off.get_z3_ast()),
        )
    }
}

/// `str.at text i` — length-1 string at index `i`, or "" if out of range.
pub(super) fn str_char_at<'ctx>(ctx: &'ctx Context, s: &Z3Str<'ctx>, i: &Int<'ctx>) -> Z3Str<'ctx> {
    unsafe { Z3Str::wrap(ctx, z3_sys::Z3_mk_seq_at(raw_ctx(ctx), s.get_z3_ast(), i.get_z3_ast())) }
}

/// String-producing builtins: `substr`, `replace`, `char_at`.
/// Returns None if name/arity doesn't match (caller falls through).
pub(super) fn translate_str_call<'ctx>(
    name: &str,
    args: &[Expr],
    ctx: &'ctx Context,
    env: &HashMap<String, Var<'ctx>>,
) -> Option<Z3Str<'ctx>> {
    match (name, args.len()) {
        ("substr", 3) => {
            let s = translate_str(&args[0], ctx, env)?;
            let off = translate_int(&args[1], ctx, env)?;
            let len = translate_int(&args[2], ctx, env)?;
            Some(str_substr(ctx, &s, &off, &len))
        }
        ("replace", 3) => {
            let s = translate_str(&args[0], ctx, env)?;
            let src = translate_str(&args[1], ctx, env)?;
            let dst = translate_str(&args[2], ctx, env)?;
            Some(str_replace(ctx, &s, &src, &dst))
        }
        ("char_at", 2) => {
            let s = translate_str(&args[0], ctx, env)?;
            let i = translate_int(&args[1], ctx, env)?;
            Some(str_char_at(ctx, &s, &i))
        }
        _ => None,
    }
}

/// Int-producing string builtins: `str_len`, `index_of` (2- or 3-arg form).
pub(super) fn translate_str_int_call<'ctx>(
    name: &str,
    args: &[Expr],
    ctx: &'ctx Context,
    env: &HashMap<String, Var<'ctx>>,
) -> Option<Int<'ctx>> {
    match (name, args.len()) {
        ("str_len", 1) => {
            let s = translate_str(&args[0], ctx, env)?;
            Some(str_length(ctx, &s))
        }
        ("index_of", 2) => {
            let s = translate_str(&args[0], ctx, env)?;
            let sub = translate_str(&args[1], ctx, env)?;
            let zero = Int::from_i64(ctx, 0);
            Some(str_index_of(ctx, &s, &sub, &zero))
        }
        ("index_of", 3) => {
            let s = translate_str(&args[0], ctx, env)?;
            let sub = translate_str(&args[1], ctx, env)?;
            let off = translate_int(&args[2], ctx, env)?;
            Some(str_index_of(ctx, &s, &sub, &off))
        }
        _ => None,
    }
}

/// Bool-producing string builtins: `str_contains`, `starts_with`, `ends_with`.
/// `sub ∈ text` infix is handled in bool.rs; this is the explicit-call form.
pub(super) fn translate_str_bool<'ctx>(
    name: &str,
    args: &[Expr],
    ctx: &'ctx Context,
    env: &HashMap<String, Var<'ctx>>,
) -> Option<Bool<'ctx>> {
    match (name, args.len()) {
        ("str_contains", 2) => {
            let hay = translate_str(&args[0], ctx, env)?;
            let needle = translate_str(&args[1], ctx, env)?;
            Some(hay.contains(&needle))
        }
        ("starts_with", 2) => {
            let text = translate_str(&args[0], ctx, env)?;
            let pre = translate_str(&args[1], ctx, env)?;
            Some(pre.prefix(&text))
        }
        ("ends_with", 2) => {
            let text = translate_str(&args[0], ctx, env)?;
            let suf = translate_str(&args[1], ctx, env)?;
            Some(suf.suffix(&text))
        }
        _ => None,
    }
}
