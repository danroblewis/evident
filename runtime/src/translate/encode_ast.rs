//! Encode a Rust `Program` AST as a Z3 `Datatype` matching `stdlib/ast.ev`.
//! Lists → Cons-chains; TraceDecl/ShaderDecl silently skipped.

use std::collections::HashMap;
use z3::ast::{Ast, Bool, Datatype, Int, Real, String as Z3Str};
use z3::{Context, DatatypeSort};

use crate::core::ast::*;
use crate::core::EnumRegistry;

#[derive(Debug)]
pub enum EncodeError {
    /// `stdlib/ast.ev` not loaded — the named enum is missing from the registry.
    EnumNotRegistered(String),
    /// The variant doesn't exist on its enum — `stdlib/ast.ev` drifted from the Rust AST.
    VariantNotFound { enum_name: String, variant: String },
    /// Unsupported construct (TraceDecl, ShaderDecl, etc.); skipped silently for whole-program encoding.
    Unsupported(&'static str),
}

impl std::fmt::Display for EncodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            EncodeError::EnumNotRegistered(name) =>
                write!(f, "stdlib/ast.ev not loaded — enum `{}` is unknown", name),
            EncodeError::VariantNotFound { enum_name, variant } =>
                write!(f, "stdlib/ast.ev is missing variant `{}` of `{}`",
                       variant, enum_name),
            EncodeError::Unsupported(what) =>
                write!(f, "encoding `{}` is not yet supported", what),
        }
    }
}

impl std::error::Error for EncodeError {}

pub type Result<T> = std::result::Result<T, EncodeError>;

/// Find a variant by name on the enum and apply its constructor with `args`.
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

fn z3_int<'ctx>(ctx: &'ctx Context, n: i64) -> Int<'ctx> {
    Int::from_i64(ctx, n)
}

fn z3_str<'ctx>(ctx: &'ctx Context, s: &str) -> Z3Str<'ctx> {
    crate::translate::z3_string(ctx, s).expect("nul byte in source string")
}

fn z3_bool<'ctx>(ctx: &'ctx Context, b: bool) -> Bool<'ctx> {
    Bool::from_bool(ctx, b)
}

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

// Each Seq(T) field in stdlib/ast.ev is backed by a `__SeqOf_T` helper enum;
// build via __Cell_T(head, tail) from right to left over a __Empty_T terminator.
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

pub fn encode_string_list<'ctx>(
    items: &[String],
    ctx: &'ctx Context,
    enums: &EnumRegistry,
) -> Result<Datatype<'ctx>> where 'ctx: 'static {
    // Top-level Seq(String) uses Array+Int (String is not in a recursive enum batch).
    // No current caller uses this for AST encoding; EForall etc. go via the field-aware path.
    use z3::ast::Array;
    use z3::Sort;
    let default = z3_str(ctx, "");
    let mut arr = Array::const_array(ctx, &Sort::int(ctx), &default);
    for (i, s) in items.iter().enumerate() {
        arr = arr.store(&Int::from_i64(ctx, i as i64), &z3_str(ctx, s));
    }
    let _ = enums;
    Err(EncodeError::Unsupported("encode_string_list (top-level Array+Int) — use the field-aware encoder path"))
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

pub fn encode_schema_list<'ctx>(
    items: &[SchemaDecl],
    ctx: &'ctx Context,
    enums: &EnumRegistry,
) -> Result<Datatype<'ctx>> where 'ctx: 'static {
    encode_cons_list(items, "SchemaDecl", ctx, enums, encode_schema_decl)
}

pub fn encode_enum_decl_list<'ctx>(
    items: &[EnumDecl],
    ctx: &'ctx Context,
    enums: &EnumRegistry,
) -> Result<Datatype<'ctx>> where 'ctx: 'static {
    encode_cons_list(items, "EnumDecl", ctx, enums, encode_enum_decl)
}

pub fn encode_enum_variant_list<'ctx>(
    items: &[EnumVariant],
    ctx: &'ctx Context,
    enums: &EnumRegistry,
) -> Result<Datatype<'ctx>> where 'ctx: 'static {
    encode_cons_list(items, "EnumVariant", ctx, enums, encode_enum_variant)
}

pub fn encode_enum_field_list<'ctx>(
    items: &[EnumField],
    ctx: &'ctx Context,
    enums: &EnumRegistry,
) -> Result<Datatype<'ctx>> where 'ctx: 'static {
    encode_cons_list(items, "EnumField", ctx, enums, encode_enum_field)
}

/// Encode `Vec<String>` as (Array+Int) for EForall/EExists vars — the two-accessor
/// representation for Seq(String) fields (String is a primitive, not a Cons-list element).
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

pub fn encode_enum_field<'ctx>(
    f: &EnumField,
    ctx: &'ctx Context,
    enums: &EnumRegistry,
) -> Result<Datatype<'ctx>> where 'ctx: 'static {
    let name = z3_str(ctx, &f.name);
    let type_name = z3_str(ctx, &f.type_name);
    apply(enums, "EnumField", "MakeEnumField", &[&name, &type_name])
}

pub fn encode_enum_variant<'ctx>(
    v: &EnumVariant,
    ctx: &'ctx Context,
    enums: &EnumRegistry,
) -> Result<Datatype<'ctx>> where 'ctx: 'static {
    let name = z3_str(ctx, &v.name);
    let fields = encode_enum_field_list(&v.fields, ctx, enums)?;
    apply(enums, "EnumVariant", "MakeEnumVariant", &[&name, &fields])
}

pub fn encode_enum_decl<'ctx>(
    e: &EnumDecl,
    ctx: &'ctx Context,
    enums: &EnumRegistry,
) -> Result<Datatype<'ctx>> where 'ctx: 'static {
    let name = z3_str(ctx, &e.name);
    let variants = encode_enum_variant_list(&e.variants, ctx, enums)?;
    apply(enums, "EnumDecl", "MakeEnumDecl", &[&name, &variants])
}

/// Encode a SchemaDecl; third slot carries `param_count` (first-line-param insertion index).
/// Keep in sync with `schema_decl_to_value` and stdlib/ast.ev's `MakeSchemaDecl`.
pub fn encode_schema_decl<'ctx>(
    s: &SchemaDecl,
    ctx: &'ctx Context,
    enums: &EnumRegistry,
) -> Result<Datatype<'ctx>> where 'ctx: 'static {
    let kw = encode_keyword(&s.keyword, enums)?;
    let name = z3_str(ctx, &s.name);
    let param_count = z3_int(ctx, s.param_count as i64);
    let body = encode_body_item_list(&s.body, ctx, enums)?;
    apply(enums, "SchemaDecl", "MakeSchemaDecl", &[&kw, &name, &param_count, &body])
}

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
        BodyItem::HaltsWithin { fsm_name, n } => {
            let nm = z3_str(ctx, fsm_name);
            let nn = z3_int(ctx, *n);
            apply(enums, "BodyItem", "BIHaltsWithin", &[&nm, &nn])
        }
    }
}

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
        Expr::RunFsm { fsm, init } => {
            let f = z3_str(ctx, fsm);
            let i = encode_expr(init, ctx, enums)?;
            apply(enums, "Expr", "ERunFsm", &[&f, &i])
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
        MatchPattern::Wildcard =>
            apply(enums, "MatchPattern", "PatWildcard", &[]),
        MatchPattern::Bind(name) => {
            let n = z3_str(ctx, name);
            apply(enums, "MatchPattern", "PatBind", &[&n])
        }
        MatchPattern::Ctor { name, binds } => {
            let n = z3_str(ctx, name);
            let binds_list = encode_bind_list(binds, ctx, enums)?;
            apply(enums, "MatchPattern", "PatCtor", &[&n, &binds_list])
        }
    }
}

fn encode_bind_list<'ctx>(
    binds: &[crate::core::ast::MatchPattern],
    ctx: &'ctx Context,
    enums: &EnumRegistry,
) -> Result<Datatype<'ctx>> where 'ctx: 'static {
    use crate::core::ast::MatchPattern;
    let helper = crate::core::internal_cons_helper_name("MatchBind");
    let empty  = "__Empty_MatchBind";
    let cell   = "__Cell_MatchBind";
    let mut acc = apply(enums, &helper, empty, &[])?;
    for b in binds.iter().rev() {
        let head = encode_match_bind(b, ctx, enums)?;
        acc = apply(enums, &helper, cell, &[&head, &acc])?;
    }
    Ok(acc)
}

/// Encode one sub-pattern as a `MatchBind`. `BindCtor` recurses through
/// `encode_bind_list` so nested constructor sub-patterns round-trip to any depth.
fn encode_match_bind<'ctx>(
    b: &crate::core::ast::MatchPattern,
    ctx: &'ctx Context,
    enums: &EnumRegistry,
) -> Result<Datatype<'ctx>> where 'ctx: 'static {
    use crate::core::ast::MatchPattern;
    match b {
        MatchPattern::Bind(name) => {
            let n = z3_str(ctx, name);
            apply(enums, "MatchBind", "BindName", &[&n])
        }
        MatchPattern::Wildcard =>
            apply(enums, "MatchBind", "BindWildcard", &[]),
        MatchPattern::Ctor { name, binds } => {
            let n = z3_str(ctx, name);
            let inner = encode_bind_list(binds, ctx, enums)?;
            apply(enums, "MatchBind", "BindCtor", &[&n, &inner])
        }
    }
}

pub fn encode_program<'ctx>(
    prog: &Program,
    ctx: &'ctx Context,
    enums: &EnumRegistry,
) -> Result<Datatype<'ctx>> where 'ctx: 'static {
    // TraceDecl/ShaderDecl omitted from stdlib/ast.ev's Program; skip silently.
    let schemas = encode_schema_list(&prog.schemas, ctx, enums)?;
    let enums_v = encode_enum_decl_list(&prog.enums, ctx, enums)?;
    apply(enums, "Program", "MakeProgram", &[&schemas, &enums_v])
}

#[allow(unused_imports)]
use std::collections::HashMap as _Sentinel;

/// Encode `Vec<BodyItem>` as per-index Z3 Bool assertions for an enum-typed `Seq(BodyItem)`.
/// Returns a len assertion + one `seq[i] = encoded` per item; caller asserts all before check.
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

/// Build a `Value::SeqEnum` of `Result` enums for pinning `last_results ∈ Seq(Result)`.
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

// THE shared marshaler: produces `Value::Enum` trees (no Z3). Lists are named Cons enums,
// NOT Seq(T) — so stack-FSM passes can pop in-step.
use crate::core::Value;

/// Re-encode a `Value::Enum` tree as a Z3 `Datatype` by looking up constructors in
/// the `EnumRegistry`. Used by `evaluate_with_extra_assertions` to pin enum-typed world fields.
pub fn value_enum_to_datatype<'ctx>(
    v:     &Value,
    ctx:   &'ctx Context,
    enums: &EnumRegistry,
) -> Option<Datatype<'ctx>>
where 'ctx: 'static
{
    use z3::ast::{Bool as Z3Bool, Dynamic, Int as Z3Int};
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
            Value::Str(s)  => Dynamic::from_ast(&crate::translate::z3_string(ctx, s).ok()?),
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

fn ev(enum_name: &str, variant: &str, fields: Vec<Value>) -> Value {
    Value::Enum {
        enum_name: enum_name.to_string(),
        variant:   variant.to_string(),
        fields,
    }
}

pub fn program_to_value(prog: &Program) -> Value {
    let schemas = schema_list_to_value(&prog.schemas);
    let enums   = enum_decl_list_to_value(&prog.enums);
    ev("Program", "MakeProgram", vec![schemas, enums])
}

pub fn schema_list_to_value(items: &[SchemaDecl]) -> Value {
    let mut acc = ev("SchemaList", "SchLNil", vec![]);
    for s in items.iter().rev() {
        acc = ev("SchemaList", "SchLCons",
                 vec![schema_decl_to_value(s), acc]);
    }
    acc
}

pub fn enum_decl_list_to_value(items: &[EnumDecl]) -> Value {
    let mut acc = ev("EnumDeclList", "EDLNil", vec![]);
    for e in items.iter().rev() {
        acc = ev("EnumDeclList", "EDLCons",
                 vec![enum_decl_to_value(e), acc]);
    }
    acc
}

pub fn schema_decl_to_value(s: &SchemaDecl) -> Value {
    let kw = keyword_to_value(&s.keyword);
    let body = body_item_list_to_value(&s.body);
    // param_count round-trips losslessly; keep in sync with encode_schema_decl and stdlib/ast.ev.
    ev("SchemaDecl", "MakeSchemaDecl",
       vec![kw, Value::Str(s.name.clone()),
            Value::Int(s.param_count as i64), body])
}

pub fn keyword_to_value(kw: &Keyword) -> Value {
    let v = match kw {
        Keyword::Schema   => "KSchema",
        Keyword::Claim    => "KClaim",
        Keyword::Type     => "KType",
        Keyword::Subclaim => "KSubclaim",
        Keyword::Fsm      => "KFsm",
    };
    ev("Keyword", v, vec![])
}

pub fn body_item_list_to_value(items: &[BodyItem]) -> Value {
    let mut acc = ev("BodyItemList", "BILNil", vec![]);
    for it in items.iter().rev() {
        acc = ev("BodyItemList", "BILCons",
                 vec![body_item_to_value(it), acc]);
    }
    acc
}

pub fn body_item_to_value(bi: &BodyItem) -> Value {
    match bi {
        BodyItem::Membership { name, type_name, pins } => {
            ev("BodyItem", "BIMembership",
               vec![Value::Str(name.clone()),
                    Value::Str(type_name.clone()),
                    pins_to_value(pins)])
        }
        BodyItem::Passthrough(name) => {
            ev("BodyItem", "BIPassthrough", vec![Value::Str(name.clone())])
        }
        BodyItem::ClaimCall { name, mappings } => {
            ev("BodyItem", "BIClaimCall",
               vec![Value::Str(name.clone()),
                    mapping_list_to_value(mappings)])
        }
        BodyItem::Constraint(e) => {
            ev("BodyItem", "BIConstraint", vec![expr_to_value(e)])
        }
        BodyItem::SubclaimDecl(s) => {
            ev("BodyItem", "BISubclaim", vec![schema_decl_to_value(s)])
        }
        BodyItem::HaltsWithin { fsm_name, n } => {
            ev("BodyItem", "BIHaltsWithin",
               vec![Value::Str(fsm_name.clone()), Value::Int(*n)])
        }
    }
}

pub fn pins_to_value(p: &Pins) -> Value {
    match p {
        Pins::None => ev("Pins", "PNone", vec![]),
        Pins::Named(maps) => {
            ev("Pins", "PNamed", vec![mapping_list_to_value(maps)])
        }
        Pins::Positional(args) => {
            ev("Pins", "PPositional", vec![expr_list_to_value(args)])
        }
    }
}

pub fn mapping_to_value(m: &Mapping) -> Value {
    ev("Mapping", "MakeMapping",
       vec![Value::Str(m.slot.clone()), expr_to_value(&m.value)])
}

pub fn mapping_list_to_value(items: &[Mapping]) -> Value {
    let mut acc = ev("MappingList", "MLNil", vec![]);
    for m in items.iter().rev() {
        acc = ev("MappingList", "MLCons",
                 vec![mapping_to_value(m), acc]);
    }
    acc
}

pub fn string_list_to_value(items: &[String]) -> Value {
    let mut acc = ev("StringList", "SLNil", vec![]);
    for s in items.iter().rev() {
        acc = ev("StringList", "SLCons",
                 vec![Value::Str(s.clone()), acc]);
    }
    acc
}

pub fn expr_list_to_value(items: &[Expr]) -> Value {
    let mut acc = ev("ExprList", "ELNil", vec![]);
    for e in items.iter().rev() {
        acc = ev("ExprList", "ELCons",
                 vec![expr_to_value(e), acc]);
    }
    acc
}

pub fn binop_to_value(op: &BinOp) -> Value {
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
    ev("BinOp", v, vec![])
}

pub fn expr_to_value(e: &Expr) -> Value {
    match e {
        Expr::Identifier(s) => ev("Expr", "EIdentifier", vec![Value::Str(s.clone())]),
        Expr::Int(n)        => ev("Expr", "EInt",        vec![Value::Int(*n)]),
        Expr::Real(f)       => ev("Expr", "EReal",       vec![Value::Real(*f)]),
        Expr::Bool(b)       => ev("Expr", "EBool",       vec![Value::Bool(*b)]),
        Expr::Str(s)        => ev("Expr", "EStr",        vec![Value::Str(s.clone())]),
        Expr::SetLit(items) => ev("Expr", "ESetLit",     vec![expr_list_to_value(items)]),
        Expr::SeqLit(items) => ev("Expr", "ESeqLit",     vec![expr_list_to_value(items)]),
        Expr::Tuple(items)  => ev("Expr", "ETuple",      vec![expr_list_to_value(items)]),
        Expr::Range(lo, hi) => ev("Expr", "ERange",
                                   vec![expr_to_value(lo), expr_to_value(hi)]),
        Expr::InExpr(l, r)  => ev("Expr", "EInExpr",
                                   vec![expr_to_value(l), expr_to_value(r)]),
        Expr::Forall(vars, range, body) =>
            ev("Expr", "EForall",
               vec![string_list_to_value(vars),
                    expr_to_value(range),
                    expr_to_value(body)]),
        Expr::Exists(vars, range, body) =>
            ev("Expr", "EExists",
               vec![string_list_to_value(vars),
                    expr_to_value(range),
                    expr_to_value(body)]),
        Expr::Call(name, args) =>
            ev("Expr", "ECall",
               vec![Value::Str(name.clone()), expr_list_to_value(args)]),
        Expr::Cardinality(inner) =>
            ev("Expr", "ECardinality", vec![expr_to_value(inner)]),
        Expr::Index(seq, idx) =>
            ev("Expr", "EIndex", vec![expr_to_value(seq), expr_to_value(idx)]),
        Expr::Field(base, name) =>
            ev("Expr", "EField",
               vec![expr_to_value(base), Value::Str(name.clone())]),
        Expr::Binary(op, l, r) =>
            ev("Expr", "EBinary",
               vec![binop_to_value(op), expr_to_value(l), expr_to_value(r)]),
        Expr::Not(inner) =>
            ev("Expr", "ENot", vec![expr_to_value(inner)]),
        Expr::Ternary(c, a, b) =>
            ev("Expr", "ETernary",
               vec![expr_to_value(c), expr_to_value(a), expr_to_value(b)]),
        Expr::Match(scr, arms) =>
            ev("Expr", "EMatch",
               vec![expr_to_value(scr), match_arm_list_to_value(arms)]),
        Expr::Matches(e, pat) =>
            ev("Expr", "EMatches",
               vec![expr_to_value(e), match_pattern_to_value(pat)]),
        Expr::RunFsm { fsm, init } =>
            ev("Expr", "ERunFsm",
               vec![Value::Str(fsm.clone()), expr_to_value(init)]),
    }
}

pub fn match_arm_list_to_value(arms: &[crate::core::ast::MatchArm]) -> Value {
    let mut acc = ev("MatchArmList", "MALNil", vec![]);
    for a in arms.iter().rev() {
        acc = ev("MatchArmList", "MALCons",
                 vec![match_arm_to_value(a), acc]);
    }
    acc
}

pub fn match_arm_to_value(a: &crate::core::ast::MatchArm) -> Value {
    ev("MatchArm", "MakeMatchArm",
       vec![match_pattern_to_value(&a.pattern), expr_to_value(&a.body)])
}

pub fn match_pattern_to_value(p: &crate::core::ast::MatchPattern) -> Value {
    use crate::core::ast::MatchPattern;
    match p {
        MatchPattern::Wildcard =>
            ev("MatchPattern", "PatWildcard", vec![]),
        MatchPattern::Bind(name) =>
            ev("MatchPattern", "PatBind", vec![Value::Str(name.clone())]),
        MatchPattern::Ctor { name, binds } => {
            ev("MatchPattern", "PatCtor",
               vec![Value::Str(name.clone()), bind_list_to_value(binds)])
        }
    }
}

pub fn bind_list_to_value(binds: &[crate::core::ast::MatchPattern]) -> Value {
    use crate::core::ast::MatchPattern;
    let mut acc = ev("BindList", "BLNil", vec![]);
    for b in binds.iter().rev() {
        let head = match b {
            MatchPattern::Bind(n) => ev("MatchBind", "BindName", vec![Value::Str(n.clone())]),
            MatchPattern::Wildcard => ev("MatchBind", "BindWildcard", vec![]),
            // BindCtor recurses so nested sub-patterns round-trip. COUPLED to consuming
            // passes' enum decls — MatchBind/MatchPattern shapes must move together.
            MatchPattern::Ctor { name, binds } =>
                ev("MatchBind", "BindCtor",
                   vec![Value::Str(name.clone()), bind_list_to_value(binds)]),
        };
        acc = ev("BindList", "BLCons", vec![head, acc]);
    }
    acc
}

pub fn enum_decl_to_value(e: &EnumDecl) -> Value {
    ev("EnumDecl", "MakeEnumDecl",
       vec![Value::Str(e.name.clone()),
            enum_variant_list_to_value(&e.variants)])
}

pub fn enum_variant_list_to_value(items: &[EnumVariant]) -> Value {
    let mut acc = ev("EnumVariantList", "EVLNil", vec![]);
    for v in items.iter().rev() {
        acc = ev("EnumVariantList", "EVLCons",
                 vec![enum_variant_to_value(v), acc]);
    }
    acc
}

pub fn enum_variant_to_value(v: &EnumVariant) -> Value {
    ev("EnumVariant", "MakeEnumVariant",
       vec![Value::Str(v.name.clone()),
            enum_field_list_to_value(&v.fields)])
}

pub fn enum_field_list_to_value(items: &[EnumField]) -> Value {
    let mut acc = ev("EnumFieldList", "EFLNil", vec![]);
    for f in items.iter().rev() {
        acc = ev("EnumFieldList", "EFLCons",
                 vec![enum_field_to_value(f), acc]);
    }
    acc
}

pub fn enum_field_to_value(f: &EnumField) -> Value {
    ev("EnumField", "MakeEnumField",
       vec![Value::Str(f.name.clone()), Value::Str(f.type_name.clone())])
}
