//! Encode AST fragments (BodyItems, Effects, Results) as Z3
//! `Datatype` values, and re-encode `Value::Enum` trees as Z3
//! datatypes.
//!
//! Used by the executor: `encode_body_items_into_seq` pins an
//! enum-typed `Seq` variable's elements; `encode_effect_result` /
//! `effect_results_to_value` build `Result` values for the multi-FSM
//! scheduler's `given` map; `value_enum_to_datatype` re-encodes a
//! `Value::Enum` field for the `given` loop.
//!
//! Per-type encoders are mostly mechanical: look up the constructor
//! by name in the `EnumRegistry`, translate each field, apply. The
//! recursion follows the AST structure; lists become a Cons-chain
//! through the relevant `*List` enum.

use z3::ast::{Ast, Bool, Datatype, Int, Real, String as Z3Str};
use z3::Context;

use crate::core::ast::*;
use crate::core::EnumRegistry;

#[derive(Debug)]
pub enum EncodeError {
    /// `stdlib/ast.ev` isn't loaded — the named enum is missing
    /// from the registry. Tell the user to import the stdlib.
    EnumNotRegistered(String),
    /// The named variant doesn't exist on its enum. Means
    /// `stdlib/ast.ev` drifted from the Rust AST shape — fix the
    /// stdlib file to add the variant.
    VariantNotFound { enum_name: String, variant: String },
}

impl std::fmt::Display for EncodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            EncodeError::EnumNotRegistered(name) =>
                write!(f, "stdlib/ast.ev not loaded — enum `{}` is unknown", name),
            EncodeError::VariantNotFound { enum_name, variant } =>
                write!(f, "stdlib/ast.ev is missing variant `{}` of `{}`",
                       variant, enum_name),
        }
    }
}

impl std::error::Error for EncodeError {}

pub type Result<T> = std::result::Result<T, EncodeError>;

/// Helper: find a variant by name on the given enum and apply its
/// constructor with `args`. The args' Z3 types must already match
/// the variant's declared payload field types — caller's job.
fn apply<'ctx>(
    enums: &EnumRegistry,
    enum_name: &str,
    variant: &str,
    args: &[&dyn Ast<'ctx>],
) -> Result<Datatype<'ctx>> {
    let dts = enums.by_name.borrow();
    let (sort, _decl_variants) = dts.get(enum_name)
        .ok_or_else(|| EncodeError::EnumNotRegistered(enum_name.to_string()))?;
    for v in &sort.variants {
        if v.constructor.name() == variant {
            return v.constructor.apply(args).as_datatype()
                .ok_or_else(|| EncodeError::VariantNotFound {
                    enum_name: enum_name.to_string(),
                    variant: variant.to_string(),
                });
        }
    }
    Err(EncodeError::VariantNotFound {
        enum_name: enum_name.to_string(),
        variant: variant.to_string(),
    })
}

// ── Primitives ──────────────────────────────────────────────────

fn z3_int<'ctx>(ctx: &'ctx Context, n: i64) -> Int<'ctx> {
    Int::from_i64(ctx, n)
}

fn z3_str<'ctx>(ctx: &'ctx Context, s: &str) -> Z3Str<'ctx> {
    Z3Str::from_str(ctx, s).expect("nul byte in source string")
}

fn z3_bool<'ctx>(ctx: &'ctx Context, b: bool) -> Bool<'ctx> {
    Bool::from_bool(ctx, b)
}

/// Encode an f64 as a Z3 Real literal. Mirrors the runtime's existing
/// real-from-f64 logic; copy lives here to avoid a cross-module dep.
fn z3_real<'ctx>(ctx: &'ctx Context, f: f64) -> Real<'ctx> {
    if f.is_nan() || f.is_infinite() {
        return Real::from_real(ctx, 0, 1);
    }
    let s = f.to_string();
    if let Some(dot) = s.find('.') {
        let (int_part, frac_with_dot) = s.split_at(dot);
        let frac = &frac_with_dot[1..];
        let num = format!("{}{}", int_part, frac);
        let den = format!("1{}", "0".repeat(frac.len()));
        Real::from_real_str(ctx, &num, &den)
            .unwrap_or_else(|| Real::from_real(ctx, 0, 1))
    } else {
        Real::from_real_str(ctx, &s, "1")
            .unwrap_or_else(|| Real::from_real(ctx, 0, 1))
    }
}

// ── Operators / Keyword / Pins ─────────────────────────────────

pub fn encode_binop(
    op: &BinOp,
    enums: &EnumRegistry,
) -> Result<Datatype<'static>> {
    let v = match op {
        BinOp::Eq      => "OpEq",
        BinOp::Neq     => "OpNeq",
        BinOp::Lt      => "OpLt",
        BinOp::Le      => "OpLe",
        BinOp::Gt      => "OpGt",
        BinOp::Ge      => "OpGe",
        BinOp::And     => "OpAnd",
        BinOp::Or      => "OpOr",
        BinOp::Implies => "OpImplies",
        BinOp::Add     => "OpAdd",
        BinOp::Sub     => "OpSub",
        BinOp::Mul     => "OpMul",
        BinOp::Div     => "OpDiv",
        BinOp::Concat  => "OpConcat",
    };
    apply(enums, "BinOp", v, &[])
}

pub fn encode_keyword(
    kw: &Keyword,
    enums: &EnumRegistry,
) -> Result<Datatype<'static>> {
    let v = match kw {
        Keyword::Schema   => "KSchema",
        Keyword::Claim    => "KClaim",
        Keyword::Type     => "KType",
        Keyword::Subclaim => "KSubclaim",
        Keyword::Fsm      => "KFsm",
    };
    apply(enums, "Keyword", v, &[])
}

pub fn encode_mapping<'ctx>(
    m: &Mapping,
    ctx: &'ctx Context,
    enums: &EnumRegistry,
) -> Result<Datatype<'ctx>> where 'ctx: 'static {
    let slot = z3_str(ctx, &m.slot);
    let value = encode_expr(&m.value, ctx, enums)?;
    apply(enums, "Mapping", "MakeMapping", &[&slot, &value])
}

pub fn encode_pins<'ctx>(
    p: &Pins,
    ctx: &'ctx Context,
    enums: &EnumRegistry,
) -> Result<Datatype<'ctx>> where 'ctx: 'static {
    match p {
        Pins::None => apply(enums, "Pins", "PNone", &[]),
        Pins::Named(maps) => {
            let list = encode_mapping_list(maps, ctx, enums)?;
            apply(enums, "Pins", "PNamed", &[&list])
        }
        Pins::Positional(args) => {
            let list = encode_expr_list(args, ctx, enums)?;
            apply(enums, "Pins", "PPositional", &[&list])
        }
    }
}

// ── Lists (Vec<T> → internal-Cons helper datatype) ─────────────
//
// Each `Seq(T)` field in stdlib/ast.ev is backed by a runtime-
// generated `__SeqOf_T` helper enum (see runtime::generate_internal_
// cons_helpers). Building one of these list values means walking
// the items, applying `__Cell_T(head, tail)` from right to left
// over a terminating `__Empty_T`. From the .ev user's perspective
// they wrote `Seq(T)` and `⟨a, b, c⟩` — these encoders are the
// Rust-side bridge that puts the same shape into Z3.

fn encode_cons_list<'ctx, T>(
    items: &[T],
    elem_type_name: &str,
    ctx: &'ctx Context,
    enums: &EnumRegistry,
    encode_head: impl Fn(&T, &'ctx Context, &EnumRegistry) -> Result<Datatype<'ctx>>,
) -> Result<Datatype<'ctx>> where 'ctx: 'static {
    let helper = crate::core::internal_cons_helper_name(elem_type_name);
    let empty  = format!("__Empty_{}", elem_type_name);
    let cell   = format!("__Cell_{}", elem_type_name);
    let mut acc = apply(enums, &helper, &empty, &[])?;
    for it in items.iter().rev() {
        let head = encode_head(it, ctx, enums)?;
        acc = apply(enums, &helper, &cell, &[&head, &acc])?;
    }
    Ok(acc)
}

pub fn encode_expr_list<'ctx>(
    items: &[Expr],
    ctx: &'ctx Context,
    enums: &EnumRegistry,
) -> Result<Datatype<'ctx>> where 'ctx: 'static {
    encode_cons_list(items, "Expr", ctx, enums, encode_expr)
}

pub fn encode_mapping_list<'ctx>(
    items: &[Mapping],
    ctx: &'ctx Context,
    enums: &EnumRegistry,
) -> Result<Datatype<'ctx>> where 'ctx: 'static {
    encode_cons_list(items, "Mapping", ctx, enums, encode_mapping)
}

pub fn encode_body_item_list<'ctx>(
    items: &[BodyItem],
    ctx: &'ctx Context,
    enums: &EnumRegistry,
) -> Result<Datatype<'ctx>> where 'ctx: 'static {
    encode_cons_list(items, "BodyItem", ctx, enums, encode_body_item)
}

/// Encode `Vec<String>` for an EForall/EExists vars slot. After
/// Phase 6.5 the Seq(String) field is two-accessor Array+Int
/// (String is primitive), but the constructor expects a single
/// arg-list — we caller-pin both via translate_seq_arg_for_ctor's
/// equivalent here. Helper for encode_expr's Forall/Exists arms.
pub fn encode_string_seq_pair<'ctx>(
    items: &[String],
    ctx: &'ctx Context,
) -> (z3::ast::Array<'ctx>, Int<'ctx>) where 'ctx: 'static {
    use z3::ast::Array;
    use z3::Sort;
    let default = z3_str(ctx, "");
    let mut arr = Array::const_array(ctx, &Sort::int(ctx), &default);
    for (i, s) in items.iter().enumerate() {
        arr = arr.store(&Int::from_i64(ctx, i as i64), &z3_str(ctx, s));
    }
    (arr, Int::from_i64(ctx, items.len() as i64))
}

/// Encode a SchemaDecl into the `MakeSchemaDecl(Keyword, String,
/// BodyItemList)` shape declared in stdlib/ast.ev.
///
/// **Intentional drop**: the Rust `SchemaDecl::param_count` field
/// (which tracks how many of the body's leading Memberships are
/// first-line interface params, vs. helper-locals) has no slot in
/// `MakeSchemaDecl`. No current self-hosted pass uses interface-vs-
/// helper distinction — every pass walks the body items uniformly —
/// so encoding `param_count` would add a constructor slot every
/// `decode_schema_decl` consumer must round-trip without observable
/// benefit. The decoder reconstructs `param_count: 0`. If a future
/// pass needs the distinction: add a fourth `Nat` slot to
/// `MakeSchemaDecl` here and in stdlib/ast.ev; update
/// `decode_schema_decl`; the cross-language contract is then carried
/// explicitly.
pub fn encode_schema_decl<'ctx>(
    s: &SchemaDecl,
    ctx: &'ctx Context,
    enums: &EnumRegistry,
) -> Result<Datatype<'ctx>> where 'ctx: 'static {
    let kw = encode_keyword(&s.keyword, enums)?;
    let name = z3_str(ctx, &s.name);
    let body = encode_body_item_list(&s.body, ctx, enums)?;
    apply(enums, "SchemaDecl", "MakeSchemaDecl", &[&kw, &name, &body])
}

// ── BodyItem ────────────────────────────────────────────────────

pub fn encode_body_item<'ctx>(
    bi: &BodyItem,
    ctx: &'ctx Context,
    enums: &EnumRegistry,
) -> Result<Datatype<'ctx>> where 'ctx: 'static {
    match bi {
        BodyItem::Membership { name, type_name, pins } => {
            let n = z3_str(ctx, name);
            let t = z3_str(ctx, type_name);
            let p = encode_pins(pins, ctx, enums)?;
            apply(enums, "BodyItem", "BIMembership", &[&n, &t, &p])
        }
        BodyItem::Passthrough(name) => {
            let n = z3_str(ctx, name);
            apply(enums, "BodyItem", "BIPassthrough", &[&n])
        }
        BodyItem::ClaimCall { name, mappings } => {
            let n = z3_str(ctx, name);
            let m = encode_mapping_list(mappings, ctx, enums)?;
            apply(enums, "BodyItem", "BIClaimCall", &[&n, &m])
        }
        BodyItem::Constraint(e) => {
            let ee = encode_expr(e, ctx, enums)?;
            apply(enums, "BodyItem", "BIConstraint", &[&ee])
        }
        BodyItem::SubclaimDecl(s) => {
            let sd = encode_schema_decl(s, ctx, enums)?;
            apply(enums, "BodyItem", "BISubclaim", &[&sd])
        }
    }
}

// ── Expr (recursive) ────────────────────────────────────────────

pub fn encode_expr<'ctx>(
    e: &Expr,
    ctx: &'ctx Context,
    enums: &EnumRegistry,
) -> Result<Datatype<'ctx>> where 'ctx: 'static {
    match e {
        Expr::Identifier(s) => {
            let v = z3_str(ctx, s);
            apply(enums, "Expr", "EIdentifier", &[&v])
        }
        Expr::Int(n) => {
            let v = z3_int(ctx, *n);
            apply(enums, "Expr", "EInt", &[&v])
        }
        Expr::Real(f) => {
            let v = z3_real(ctx, *f);
            apply(enums, "Expr", "EReal", &[&v])
        }
        Expr::Bool(b) => {
            let v = z3_bool(ctx, *b);
            apply(enums, "Expr", "EBool", &[&v])
        }
        Expr::Str(s) => {
            let v = z3_str(ctx, s);
            apply(enums, "Expr", "EStr", &[&v])
        }
        Expr::SetLit(items) => {
            let list = encode_expr_list(items, ctx, enums)?;
            apply(enums, "Expr", "ESetLit", &[&list])
        }
        Expr::SeqLit(items) => {
            let list = encode_expr_list(items, ctx, enums)?;
            apply(enums, "Expr", "ESeqLit", &[&list])
        }
        Expr::Tuple(items) => {
            let list = encode_expr_list(items, ctx, enums)?;
            apply(enums, "Expr", "ETuple", &[&list])
        }
        Expr::Range(lo, hi) => {
            let l = encode_expr(lo, ctx, enums)?;
            let h = encode_expr(hi, ctx, enums)?;
            apply(enums, "Expr", "ERange", &[&l, &h])
        }
        Expr::InExpr(lhs, rhs) => {
            let l = encode_expr(lhs, ctx, enums)?;
            let r = encode_expr(rhs, ctx, enums)?;
            apply(enums, "Expr", "EInExpr", &[&l, &r])
        }
        Expr::Forall(vars, range, body) => {
            let (vs_arr, vs_len) = encode_string_seq_pair(vars, ctx);
            let r  = encode_expr(range, ctx, enums)?;
            let b  = encode_expr(body, ctx, enums)?;
            apply(enums, "Expr", "EForall", &[&vs_arr, &vs_len, &r, &b])
        }
        Expr::Exists(vars, range, body) => {
            let (vs_arr, vs_len) = encode_string_seq_pair(vars, ctx);
            let r  = encode_expr(range, ctx, enums)?;
            let b  = encode_expr(body, ctx, enums)?;
            apply(enums, "Expr", "EExists", &[&vs_arr, &vs_len, &r, &b])
        }
        Expr::Call(name, args) => {
            let n = z3_str(ctx, name);
            let a = encode_expr_list(args, ctx, enums)?;
            apply(enums, "Expr", "ECall", &[&n, &a])
        }
        Expr::Cardinality(inner) => {
            let i = encode_expr(inner, ctx, enums)?;
            apply(enums, "Expr", "ECardinality", &[&i])
        }
        Expr::Index(seq, idx) => {
            let s = encode_expr(seq, ctx, enums)?;
            let i = encode_expr(idx, ctx, enums)?;
            apply(enums, "Expr", "EIndex", &[&s, &i])
        }
        Expr::Field(base, name) => {
            let b = encode_expr(base, ctx, enums)?;
            let n = z3_str(ctx, name);
            apply(enums, "Expr", "EField", &[&b, &n])
        }
        Expr::Binary(op, lhs, rhs) => {
            let o = encode_binop(op, enums)?;
            let l = encode_expr(lhs, ctx, enums)?;
            let r = encode_expr(rhs, ctx, enums)?;
            apply(enums, "Expr", "EBinary", &[&o, &l, &r])
        }
        Expr::Not(inner) => {
            let i = encode_expr(inner, ctx, enums)?;
            apply(enums, "Expr", "ENot", &[&i])
        }
        Expr::Ternary(c, a, b) => {
            let c = encode_expr(c, ctx, enums)?;
            let a = encode_expr(a, ctx, enums)?;
            let b = encode_expr(b, ctx, enums)?;
            apply(enums, "Expr", "ETernary", &[&c, &a, &b])
        }
        Expr::Match(scr, arms) => {
            let scr = encode_expr(scr, ctx, enums)?;
            let arm_list = encode_match_arm_list(arms, ctx, enums)?;
            apply(enums, "Expr", "EMatch", &[&scr, &arm_list])
        }
        Expr::Matches(e, pat) => {
            let e = encode_expr(e, ctx, enums)?;
            let p = encode_match_pattern(pat, ctx, enums)?;
            apply(enums, "Expr", "EMatches", &[&e, &p])
        }
    }
}

fn encode_match_arm_list<'ctx>(
    arms: &[crate::core::ast::MatchArm],
    ctx: &'ctx Context,
    enums: &EnumRegistry,
) -> Result<Datatype<'ctx>> where 'ctx: 'static {
    encode_cons_list(arms, "MatchArm", ctx, enums, encode_match_arm)
}

fn encode_match_arm<'ctx>(
    arm: &crate::core::ast::MatchArm,
    ctx: &'ctx Context,
    enums: &EnumRegistry,
) -> Result<Datatype<'ctx>> where 'ctx: 'static {
    let pat = encode_match_pattern(&arm.pattern, ctx, enums)?;
    let body = encode_expr(&arm.body, ctx, enums)?;
    apply(enums, "MatchArm", "MakeMatchArm", &[&pat, &body])
}

fn encode_match_pattern<'ctx>(
    pat: &crate::core::ast::MatchPattern,
    ctx: &'ctx Context,
    enums: &EnumRegistry,
) -> Result<Datatype<'ctx>> where 'ctx: 'static {
    use crate::core::ast::MatchPattern;
    match pat {
        MatchPattern::Wildcard => apply(enums, "MatchPattern", "PatWildcard", &[]),
        MatchPattern::Ctor { name, binds } => {
            let n = z3_str(ctx, name);
            let binds_list = encode_bind_list(binds, ctx, enums)?;
            apply(enums, "MatchPattern", "PatCtor", &[&n, &binds_list])
        }
    }
}

fn encode_bind_list<'ctx>(
    binds: &[Option<String>],
    ctx: &'ctx Context,
    enums: &EnumRegistry,
) -> Result<Datatype<'ctx>> where 'ctx: 'static {
    let helper = crate::core::internal_cons_helper_name("MatchBind");
    let empty  = "__Empty_MatchBind";
    let cell   = "__Cell_MatchBind";
    let mut acc = apply(enums, &helper, empty, &[])?;
    for b in binds.iter().rev() {
        let head = match b {
            None => apply(enums, "MatchBind", "BindWildcard", &[])?,
            Some(name) => {
                let n = z3_str(ctx, name);
                apply(enums, "MatchBind", "BindName", &[&n])?
            }
        };
        acc = apply(enums, &helper, cell, &[&head, &acc])?;
    }
    Ok(acc)
}

// `use _ as _` to keep imports tidy at the module top while still
// avoiding unused-import warnings if someone strips a helper.
#[allow(unused_imports)]
use std::collections::HashMap as _Sentinel;

/// Stage 5.5: encode a `Vec<BodyItem>` as a list of per-index Z3
/// Bool assertions for an enum-typed Seq variable. The caller has
/// declared something like `body ∈ Seq(BodyItem)`; this function
/// returns:
///   * `len_assertion`: the seq's length must equal `items.len()`
///   * `elem_assertions`: one `seq[i] = <encoded item>` per item
/// Caller asserts each into the solver before the satisfiability
/// check. The seq variable must be in env as `Var::DatatypeSeqVar`
/// with empty `fields` (the enum-seq marker from declare.rs).
pub fn encode_body_items_into_seq<'ctx>(
    items: &[BodyItem],
    seq_arr: &z3::ast::Array<'ctx>,
    seq_len: &z3::ast::Int<'ctx>,
    ctx: &'ctx Context,
    enums: &EnumRegistry,
) -> Result<Vec<z3::ast::Bool<'ctx>>> where 'ctx: 'static {
    let mut asserts: Vec<z3::ast::Bool<'ctx>> = Vec::with_capacity(items.len() + 1);
    asserts.push(seq_len._eq(&z3::ast::Int::from_i64(ctx, items.len() as i64)));
    for (i, item) in items.iter().enumerate() {
        let encoded = encode_body_item(item, ctx, enums)?;
        let idx = z3::ast::Int::from_i64(ctx, i as i64);
        let elem = seq_arr.select(&idx).as_datatype()
            .expect("Seq(enum)'s element select must yield a Datatype value");
        asserts.push(elem._eq(&encoded));
    }
    Ok(asserts)
}

// ── stdlib/runtime.ev: Effect / Result encoders ────────────────

pub fn encode_effect_result<'ctx>(
    r: &crate::core::ast::EffectResult,
    ctx: &'ctx Context,
    enums: &EnumRegistry,
) -> Result<Datatype<'ctx>> where 'ctx: 'static {
    use crate::core::ast::EffectResult;
    match r {
        EffectResult::NoResult    => apply(enums, "Result", "NoResult", &[]),
        EffectResult::Int(n)      => {
            let v = z3_int(ctx, *n);
            apply(enums, "Result", "IntResult", &[&v])
        }
        EffectResult::Str(s)      => {
            let v = z3_str(ctx, s);
            apply(enums, "Result", "StringResult", &[&v])
        }
        EffectResult::Bool(b)     => {
            let v = z3_bool(ctx, *b);
            apply(enums, "Result", "BoolResult", &[&v])
        }
        EffectResult::Real(f)     => {
            let v = z3_real(ctx, *f);
            apply(enums, "Result", "RealResult", &[&v])
        }
        EffectResult::Handle(h)   => {
            let v = z3_int(ctx, *h as i64);
            apply(enums, "Result", "HandleResult", &[&v])
        }
        EffectResult::Error(s)    => {
            let v = z3_str(ctx, s);
            apply(enums, "Result", "ErrorResult", &[&v])
        }
    }
}

/// Build a `Value::SeqEnum` of `Result` enums from a slice of
/// `EffectResult`s. Used by the multi-FSM scheduler to pin
/// `last_results ∈ Seq(Result)` via the `given` map; the
/// `(DatatypeSeqVar, SeqEnum)` case in `assert_seq_given` does
/// the per-index Z3 assertions.
pub fn effect_results_to_value(items: &[crate::core::ast::EffectResult]) -> Value {
    let mk = |n: &str, fields: Vec<Value>| Value::Enum {
        enum_name: "Result".into(),
        variant: n.into(),
        fields,
    };
    let elems: Vec<Value> = items.iter().map(|r| match r {
        crate::core::ast::EffectResult::NoResult     => mk("NoResult", vec![]),
        crate::core::ast::EffectResult::Int(n)       => mk("IntResult", vec![Value::Int(*n)]),
        crate::core::ast::EffectResult::Str(s)       => mk("StringResult", vec![Value::Str(s.clone())]),
        crate::core::ast::EffectResult::Bool(b)      => mk("BoolResult", vec![Value::Bool(*b)]),
        crate::core::ast::EffectResult::Real(f)      => mk("RealResult", vec![Value::Real(*f)]),
        crate::core::ast::EffectResult::Handle(h)    => mk("HandleResult", vec![Value::Int(*h as i64)]),
        crate::core::ast::EffectResult::Error(s)     => mk("ErrorResult", vec![Value::Str(s.clone())]),
    }).collect();
    Value::SeqEnum(elems)
}

// ── Value::Enum → Z3 Datatype ──────────────────────────────────

use crate::core::Value;

/// Re-encode a `Value::Enum` tree as a Z3 `Datatype` value, looking
/// up constructors against the supplied `EnumRegistry`. Returns
/// `None` if the value isn't an Enum, the enum/variant isn't
/// registered, or any payload field has a type that doesn't match
/// what the constructor expects.
///
/// Used by:
///   * The `given` loop in `evaluate_with_extra_assertions` to pin
///     enum-typed world fields produced by plugin writes (notably
///     the reflection plugin's `world.program` value).
///   * Any future caller that needs a `Datatype` from a `Value::Enum`
///     once the registry is loaded — same logic
///     `effect_loop::encode_state_value` performs against
///     `&EvidentRuntime`, but available without crossing back to
///     the public facade.
pub fn value_enum_to_datatype<'ctx>(
    v:     &Value,
    ctx:   &'ctx Context,
    enums: &EnumRegistry,
) -> Option<Datatype<'ctx>>
where 'ctx: 'static
{
    use z3::ast::{Bool as Z3Bool, Dynamic, Int as Z3Int, String as Z3Str};
    let Value::Enum { enum_name, variant, fields } = v else { return None };
    let by_name = enums.by_name.borrow();
    let (sort, _decl) = by_name.get(enum_name)?;
    let var_idx = sort.variants.iter()
        .position(|v| v.constructor.name() == *variant)?;
    let ctor = &sort.variants[var_idx].constructor;
    if fields.is_empty() {
        return ctor.apply(&[]).as_datatype();
    }
    drop(by_name);
    let owned: Vec<Dynamic<'static>> = fields.iter().filter_map(|f| {
        let dyn_v: Dynamic<'static> = match f {
            Value::Int(n)  => Dynamic::from_ast(&Z3Int::from_i64(ctx, *n)),
            Value::Bool(b) => Dynamic::from_ast(&Z3Bool::from_bool(ctx, *b)),
            Value::Str(s)  => Dynamic::from_ast(&Z3Str::from_str(ctx, s).ok()?),
            Value::Real(r) => {
                let i = (*r * 1_000_000.0) as i64;
                Dynamic::from_ast(&Real::from_real(ctx, i as i32, 1_000_000))
            }
            Value::Enum { .. } => {
                let dt = value_enum_to_datatype(f, ctx, enums)?;
                Dynamic::from_ast(&dt)
            }
            _ => return None,
        };
        Some(dyn_v)
    }).collect();
    if owned.len() != fields.len() { return None; }
    let refs: Vec<&dyn Ast> = owned.iter().map(|v| v as &dyn Ast).collect();
    ctor.apply(&refs).as_datatype()
}

