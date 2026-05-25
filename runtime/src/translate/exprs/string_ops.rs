//! String-manipulation builtins lowered to Z3's string (seq) theory.
//!
//! Evident already had string concat (`++`) and equality, but no way to
//! *slice* or *tokenize* a string — so any load-time pass that has to
//! pick a string apart (generics' `split_generic_head` / `substitute_idents`,
//! subscriptions' `world.`-prefix test) kept its logic in Rust. This module
//! adds the missing operations:
//!
//! | Evident surface            | Z3 primitive          | result |
//! |----------------------------|-----------------------|--------|
//! | `#text` / `str_len(text)`  | `str.len`             | Int    |
//! | `index_of(text, sub[, k])` | `str.indexof`         | Int    |
//! | `substr(text, off, len)`   | `str.substr` (extract)| String |
//! | `replace(text, src, dst)`  | `str.replace`         | String |
//! | `char_at(text, i)`         | `str.at`              | String |
//! | `str_contains(t, sub)` / `sub ∈ t` | `str.contains` | Bool  |
//! | `starts_with(t, pre)`      | `str.prefixof`        | Bool   |
//! | `ends_with(t, suf)`        | `str.suffixof`        | Bool   |
//!
//! `contains` / `prefixof` / `suffixof` are wrapped by the safe z3 crate
//! (`Z3Str::contains` / `prefix` / `suffix`); the rest are not, so we reach
//! the raw C builders via `z3-sys`. These ops are intended for LOAD/setup-time
//! passes (generics monomorphizes at load); Z3's string solver is only invoked
//! when one of these appears in a queried constraint, so per-tick runtime is
//! unaffected.

use std::collections::HashMap;

use z3::ast::{Ast, Bool, Int, String as Z3Str};
use z3::Context;

use crate::core::ast::Expr;
use crate::core::Var;

use super::scalar::{translate_int, translate_str};

// ── Raw context access ───────────────────────────────────────────────
//
// The z3 0.12 crate keeps `Context.z3_ctx` private and exposes no
// accessor, yet only wraps four of the seq builders (concat / contains /
// prefix / suffix). `str.len` / `extract` / `indexof` / `replace` / `at`
// are unwrapped, so we call the raw `z3-sys` builders, which need the raw
// `Z3_context`. `z3::Context` is a single-field newtype `{ z3_ctx:
// Z3_context }`, so the struct's address coincides with the field's. The
// const assertion below fails the build if the crate ever changes that
// layout (e.g. adds a field), so this never silently reads garbage.
const _: () = {
    assert!(
        std::mem::size_of::<Context>() == std::mem::size_of::<z3_sys::Z3_context>(),
        "z3::Context is no longer a single-pointer newtype; raw_ctx is unsound"
    );
};

#[inline]
fn raw_ctx(ctx: &Context) -> z3_sys::Z3_context {
    // SAFETY: see the module note above — `Context` is a single-field
    // newtype around `Z3_context` (a raw pointer), verified by the
    // `size_of` assertion, so reading offset 0 yields the field.
    unsafe { *(ctx as *const Context as *const z3_sys::Z3_context) }
}

// ── Raw seq-theory builders (str.len / extract / indexof / replace / at) ──

/// `str.len text` — length of `text` as a non-negative Int.
pub(super) fn str_length<'ctx>(ctx: &'ctx Context, s: &Z3Str<'ctx>) -> Int<'ctx> {
    unsafe { Int::wrap(ctx, z3_sys::Z3_mk_seq_length(raw_ctx(ctx), s.get_z3_ast())) }
}

/// `str.substr text off len` — the substring of `text` starting at byte
/// `off`, up to `len` characters. Z3 semantics: out-of-range offset or
/// negative length yields the empty string; a `len` past the end is
/// clamped. Mirrors SMT-LIB `str.substr`.
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

/// `str.replace text src dst` — replace the FIRST occurrence of `src` in
/// `text` with `dst` (SMT-LIB `str.replace` semantics). If `src` does not
/// occur, `text` is returned unchanged.
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

/// `str.indexof text sub off` — the index of the first occurrence of
/// `sub` in `text` at or after `off`, or `-1` if absent.
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

/// `str.at text i` — the length-1 string at index `i` (empty if out of
/// range). SMT-LIB `str.at`.
pub(super) fn str_char_at<'ctx>(ctx: &'ctx Context, s: &Z3Str<'ctx>, i: &Int<'ctx>) -> Z3Str<'ctx> {
    unsafe { Z3Str::wrap(ctx, z3_sys::Z3_mk_seq_at(raw_ctx(ctx), s.get_z3_ast(), i.get_z3_ast())) }
}

// ── Builtin-call dispatchers (called from scalar.rs / bool.rs) ───────────

/// String-producing builtins: `substr`, `replace`, `char_at`. Returns
/// `None` if `name`/arity doesn't match a string builtin (so the caller
/// falls through to its other translation paths).
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

/// Int-producing string builtins: `str_len`, `index_of` (2- or 3-arg).
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

/// Bool-producing string builtins: `str_contains`, `starts_with`,
/// `ends_with`. (The infix `sub ∈ text` form is handled in bool.rs's
/// `InExpr` arm; `str_contains` is the explicit call form, distinct from
/// the seq-element `contains` builtin.)
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
        // `starts_with(text, pre)` = "pre is a prefix of text". The z3
        // `prefix` method reads `pre.prefix(&text)`.
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
