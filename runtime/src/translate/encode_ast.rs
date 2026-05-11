//! Stage 2 of self-hosting: encode a parsed `Program` (Rust AST) as
//! a Z3 `Datatype` value matching the shape of `stdlib/ast.ev`.
//!
//! This is the bridge that lets self-hosted compiler passes consume
//! real source. Pass writers will write Evident programs that take a
//! `Program` value as a `given` and produce constraints over it; the
//! Rust runtime parses the user's source, calls into here to encode
//! it, then injects the encoded value as a `given` to the pass.
//!
//! Per-type encoders are mostly mechanical: look up the constructor
//! by name in the `EnumRegistry`, translate each field, apply. The
//! recursion follows the AST structure; lists become a Cons-chain
//! through the relevant `*List` enum.
//!
//! Limitations:
//!   - `TraceDecl` and `ShaderDecl` are not in `stdlib/ast.ev` v0.1
//!     and are silently skipped during program encoding.
//!   - Self-hosted passes that don't load `stdlib/ast.ev` will see
//!     `EncodeError::EnumNotRegistered` for every constructor — load
//!     the file first.

use std::collections::HashMap;
use z3::ast::{Ast, Bool, Datatype, Int, Real, String as Z3Str};
use z3::{Context, DatatypeSort};

use crate::ast::*;
use super::types::EnumRegistry;

#[derive(Debug)]
pub enum EncodeError {
    /// `stdlib/ast.ev` isn't loaded — the named enum is missing
    /// from the registry. Tell the user to import the stdlib.
    EnumNotRegistered(&'static str),
    /// The named variant doesn't exist on its enum. Means
    /// `stdlib/ast.ev` drifted from the Rust AST shape — fix the
    /// stdlib file to add the variant.
    VariantNotFound { enum_name: &'static str, variant: String },
    /// Something we can't encode in v0.1 (TraceDecl, ShaderDecl,
    /// etc.). Skipped silently for whole-program encoding; caller
    /// can still hit this for individual encoder calls on those
    /// types.
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

/// Helper: find a variant by name on the given enum and apply its
/// constructor with `args`. The args' Z3 types must already match
/// the variant's declared payload field types — caller's job.
fn apply<'ctx>(
    enums: &EnumRegistry,
    enum_name: &'static str,
    variant: &str,
    args: &[&dyn Ast<'ctx>],
) -> Result<Datatype<'ctx>> {
    let dts = enums.by_name.borrow();
    let (sort, _decl_variants) = dts.get(enum_name)
        .ok_or(EncodeError::EnumNotRegistered(enum_name))?;
    for v in &sort.variants {
        if v.constructor.name() == variant {
            return v.constructor.apply(args).as_datatype()
                .ok_or(EncodeError::VariantNotFound {
                    enum_name,
                    variant: variant.to_string(),
                });
        }
    }
    Err(EncodeError::VariantNotFound {
        enum_name,
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

// ── Lists (Vec<T> → recursive Cons enum) ───────────────────────

pub fn encode_string_list<'ctx>(
    items: &[String],
    ctx: &'ctx Context,
    enums: &EnumRegistry,
) -> Result<Datatype<'ctx>> where 'ctx: 'static {
    let mut acc = apply(enums, "StringList", "SLNil", &[])?;
    for s in items.iter().rev() {
        let head = z3_str(ctx, s);
        acc = apply(enums, "StringList", "SLCons", &[&head, &acc])?;
    }
    Ok(acc)
}

pub fn encode_expr_list<'ctx>(
    items: &[Expr],
    ctx: &'ctx Context,
    enums: &EnumRegistry,
) -> Result<Datatype<'ctx>> where 'ctx: 'static {
    let mut acc = apply(enums, "ExprList", "ELNil", &[])?;
    for e in items.iter().rev() {
        let head = encode_expr(e, ctx, enums)?;
        acc = apply(enums, "ExprList", "ELCons", &[&head, &acc])?;
    }
    Ok(acc)
}

pub fn encode_mapping_list<'ctx>(
    items: &[Mapping],
    ctx: &'ctx Context,
    enums: &EnumRegistry,
) -> Result<Datatype<'ctx>> where 'ctx: 'static {
    let mut acc = apply(enums, "MappingList", "MLNil", &[])?;
    for m in items.iter().rev() {
        let head = encode_mapping(m, ctx, enums)?;
        acc = apply(enums, "MappingList", "MLCons", &[&head, &acc])?;
    }
    Ok(acc)
}

pub fn encode_body_item_list<'ctx>(
    items: &[BodyItem],
    ctx: &'ctx Context,
    enums: &EnumRegistry,
) -> Result<Datatype<'ctx>> where 'ctx: 'static {
    let mut acc = apply(enums, "BodyItemList", "BILNil", &[])?;
    for it in items.iter().rev() {
        let head = encode_body_item(it, ctx, enums)?;
        acc = apply(enums, "BodyItemList", "BILCons", &[&head, &acc])?;
    }
    Ok(acc)
}

pub fn encode_schema_list<'ctx>(
    items: &[SchemaDecl],
    ctx: &'ctx Context,
    enums: &EnumRegistry,
) -> Result<Datatype<'ctx>> where 'ctx: 'static {
    let mut acc = apply(enums, "SchemaList", "SchLNil", &[])?;
    for s in items.iter().rev() {
        let head = encode_schema_decl(s, ctx, enums)?;
        acc = apply(enums, "SchemaList", "SchLCons", &[&head, &acc])?;
    }
    Ok(acc)
}

pub fn encode_enum_decl_list<'ctx>(
    items: &[EnumDecl],
    ctx: &'ctx Context,
    enums: &EnumRegistry,
) -> Result<Datatype<'ctx>> where 'ctx: 'static {
    let mut acc = apply(enums, "EnumDeclList", "EDLNil", &[])?;
    for e in items.iter().rev() {
        let head = encode_enum_decl(e, ctx, enums)?;
        acc = apply(enums, "EnumDeclList", "EDLCons", &[&head, &acc])?;
    }
    Ok(acc)
}

pub fn encode_enum_variant_list<'ctx>(
    items: &[EnumVariant],
    ctx: &'ctx Context,
    enums: &EnumRegistry,
) -> Result<Datatype<'ctx>> where 'ctx: 'static {
    let mut acc = apply(enums, "EnumVariantList", "EVLNil", &[])?;
    for v in items.iter().rev() {
        let head = encode_enum_variant(v, ctx, enums)?;
        acc = apply(enums, "EnumVariantList", "EVLCons", &[&head, &acc])?;
    }
    Ok(acc)
}

pub fn encode_enum_field_list<'ctx>(
    items: &[EnumField],
    ctx: &'ctx Context,
    enums: &EnumRegistry,
) -> Result<Datatype<'ctx>> where 'ctx: 'static {
    let mut acc = apply(enums, "EnumFieldList", "EFLNil", &[])?;
    for f in items.iter().rev() {
        let head = encode_enum_field(f, ctx, enums)?;
        acc = apply(enums, "EnumFieldList", "EFLCons", &[&head, &acc])?;
    }
    Ok(acc)
}

// ── Schema-shape singletons (single-variant enums in stdlib/ast.ev) ──

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
            let vs = encode_string_list(vars, ctx, enums)?;
            let r  = encode_expr(range, ctx, enums)?;
            let b  = encode_expr(body, ctx, enums)?;
            apply(enums, "Expr", "EForall", &[&vs, &r, &b])
        }
        Expr::Exists(vars, range, body) => {
            let vs = encode_string_list(vars, ctx, enums)?;
            let r  = encode_expr(range, ctx, enums)?;
            let b  = encode_expr(body, ctx, enums)?;
            apply(enums, "Expr", "EExists", &[&vs, &r, &b])
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
    arms: &[crate::ast::MatchArm],
    ctx: &'ctx Context,
    enums: &EnumRegistry,
) -> Result<Datatype<'ctx>> where 'ctx: 'static {
    if arms.is_empty() { return apply(enums, "MatchArmList", "MALNil", &[]); }
    let head = encode_match_arm(&arms[0], ctx, enums)?;
    let tail = encode_match_arm_list(&arms[1..], ctx, enums)?;
    apply(enums, "MatchArmList", "MALCons", &[&head, &tail])
}

fn encode_match_arm<'ctx>(
    arm: &crate::ast::MatchArm,
    ctx: &'ctx Context,
    enums: &EnumRegistry,
) -> Result<Datatype<'ctx>> where 'ctx: 'static {
    let pat = encode_match_pattern(&arm.pattern, ctx, enums)?;
    let body = encode_expr(&arm.body, ctx, enums)?;
    apply(enums, "MatchArm", "MakeMatchArm", &[&pat, &body])
}

fn encode_match_pattern<'ctx>(
    pat: &crate::ast::MatchPattern,
    ctx: &'ctx Context,
    enums: &EnumRegistry,
) -> Result<Datatype<'ctx>> {
    use crate::ast::MatchPattern;
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
) -> Result<Datatype<'ctx>> {
    if binds.is_empty() { return apply(enums, "BindList", "BLNil", &[]); }
    let head = match &binds[0] {
        None => apply(enums, "MatchBind", "BindWildcard", &[])?,
        Some(name) => {
            let n = z3_str(ctx, name);
            apply(enums, "MatchBind", "BindName", &[&n])?
        }
    };
    let tail = encode_bind_list(&binds[1..], ctx, enums)?;
    apply(enums, "BindList", "BLCons", &[&head, &tail])
}

// ── Top-level Program ──────────────────────────────────────────

pub fn encode_program<'ctx>(
    prog: &Program,
    ctx: &'ctx Context,
    enums: &EnumRegistry,
) -> Result<Datatype<'ctx>> where 'ctx: 'static {
    // TraceDecl/ShaderDecl are intentionally omitted from
    // stdlib/ast.ev's Program — they're runtime-loaded scaffolding,
    // not part of what passes need to consume. Skip silently.
    let schemas = encode_schema_list(&prog.schemas, ctx, enums)?;
    let enums_v = encode_enum_decl_list(&prog.enums, ctx, enums)?;
    apply(enums, "Program", "MakeProgram", &[&schemas, &enums_v])
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
    r: &crate::ast::EffectResult,
    ctx: &'ctx Context,
    enums: &EnumRegistry,
) -> Result<Datatype<'ctx>> where 'ctx: 'static {
    use crate::ast::EffectResult;
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

pub fn encode_effect_result_list<'ctx>(
    items: &[crate::ast::EffectResult],
    ctx: &'ctx Context,
    enums: &EnumRegistry,
) -> Result<Datatype<'ctx>> where 'ctx: 'static {
    if items.is_empty() {
        return apply(enums, "ResultList", "ResNil", &[]);
    }
    let head = encode_effect_result(&items[0], ctx, enums)?;
    let tail = encode_effect_result_list(&items[1..], ctx, enums)?;
    apply(enums, "ResultList", "ResCons", &[&head, &tail])
}

// ── Pure-Rust mirror: Program → Value::Enum tree ───────────────
//
// The encoders above produce Z3 `Datatype<'static>` values for use
// as solver assertions. The reflection world-plugin (and other
// future consumers) need the SAME information shaped as a
// `Value::Enum` tree — the runtime's neutral value currency that
// flows through `world_snapshot` and the `given` map.
//
// These helpers mirror `encode_program` / `encode_schema_decl` /
// etc. but produce `Value` directly, never touching Z3. The shape
// is identical to what `encode_program` would emit and what
// `decode_ast`'s round-trip expects — same constructor names, same
// argument order. Adding a variant here means the Z3 path AND
// stdlib/ast.ev's enum decl must be kept in sync (same as the
// existing encoders).

use super::types::Value;

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

fn schema_list_to_value(items: &[SchemaDecl]) -> Value {
    let mut acc = ev("SchemaList", "SchLNil", vec![]);
    for s in items.iter().rev() {
        acc = ev("SchemaList", "SchLCons",
                 vec![schema_decl_to_value(s), acc]);
    }
    acc
}

fn enum_decl_list_to_value(items: &[EnumDecl]) -> Value {
    let mut acc = ev("EnumDeclList", "EDLNil", vec![]);
    for e in items.iter().rev() {
        acc = ev("EnumDeclList", "EDLCons",
                 vec![enum_decl_to_value(e), acc]);
    }
    acc
}

fn schema_decl_to_value(s: &SchemaDecl) -> Value {
    let kw = keyword_to_value(&s.keyword);
    let body = body_item_list_to_value(&s.body);
    ev("SchemaDecl", "MakeSchemaDecl",
       vec![kw, Value::Str(s.name.clone()), body])
}

fn keyword_to_value(kw: &Keyword) -> Value {
    let v = match kw {
        Keyword::Schema   => "KSchema",
        Keyword::Claim    => "KClaim",
        Keyword::Type     => "KType",
        Keyword::Subclaim => "KSubclaim",
    };
    ev("Keyword", v, vec![])
}

fn body_item_list_to_value(items: &[BodyItem]) -> Value {
    let mut acc = ev("BodyItemList", "BILNil", vec![]);
    for it in items.iter().rev() {
        acc = ev("BodyItemList", "BILCons",
                 vec![body_item_to_value(it), acc]);
    }
    acc
}

fn body_item_to_value(bi: &BodyItem) -> Value {
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
    }
}

fn pins_to_value(p: &Pins) -> Value {
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

fn mapping_to_value(m: &Mapping) -> Value {
    ev("Mapping", "MakeMapping",
       vec![Value::Str(m.slot.clone()), expr_to_value(&m.value)])
}

fn mapping_list_to_value(items: &[Mapping]) -> Value {
    let mut acc = ev("MappingList", "MLNil", vec![]);
    for m in items.iter().rev() {
        acc = ev("MappingList", "MLCons",
                 vec![mapping_to_value(m), acc]);
    }
    acc
}

fn string_list_to_value(items: &[String]) -> Value {
    let mut acc = ev("StringList", "SLNil", vec![]);
    for s in items.iter().rev() {
        acc = ev("StringList", "SLCons",
                 vec![Value::Str(s.clone()), acc]);
    }
    acc
}

fn expr_list_to_value(items: &[Expr]) -> Value {
    let mut acc = ev("ExprList", "ELNil", vec![]);
    for e in items.iter().rev() {
        acc = ev("ExprList", "ELCons",
                 vec![expr_to_value(e), acc]);
    }
    acc
}

fn binop_to_value(op: &BinOp) -> Value {
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

fn expr_to_value(e: &Expr) -> Value {
    match e {
        Expr::Identifier(s) => ev("Expr", "EIdentifier", vec![Value::Str(s.clone())]),
        Expr::Int(n)        => ev("Expr", "EInt",        vec![Value::Int(*n)]),
        Expr::Real(f)       => ev("Expr", "EReal",       vec![Value::Real(*f)]),
        Expr::Bool(b)       => ev("Expr", "EBool",       vec![Value::Bool(*b)]),
        Expr::Str(s)        => ev("Expr", "EStr",        vec![Value::Str(s.clone())]),
        Expr::SetLit(items) => ev("Expr", "ESetLit",     vec![expr_list_to_value(items)]),
        Expr::SeqLit(items) => ev("Expr", "ESeqLit",     vec![expr_list_to_value(items)]),
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
    }
}

fn match_arm_list_to_value(arms: &[crate::ast::MatchArm]) -> Value {
    let mut acc = ev("MatchArmList", "MALNil", vec![]);
    for a in arms.iter().rev() {
        acc = ev("MatchArmList", "MALCons",
                 vec![match_arm_to_value(a), acc]);
    }
    acc
}

fn match_arm_to_value(a: &crate::ast::MatchArm) -> Value {
    ev("MatchArm", "MakeMatchArm",
       vec![match_pattern_to_value(&a.pattern), expr_to_value(&a.body)])
}

fn match_pattern_to_value(p: &crate::ast::MatchPattern) -> Value {
    use crate::ast::MatchPattern;
    match p {
        MatchPattern::Wildcard => ev("MatchPattern", "PatWildcard", vec![]),
        MatchPattern::Ctor { name, binds } => {
            ev("MatchPattern", "PatCtor",
               vec![Value::Str(name.clone()), bind_list_to_value(binds)])
        }
    }
}

fn bind_list_to_value(binds: &[Option<String>]) -> Value {
    let mut acc = ev("BindList", "BLNil", vec![]);
    for b in binds.iter().rev() {
        let head = match b {
            None => ev("MatchBind", "BindWildcard", vec![]),
            Some(n) => ev("MatchBind", "BindName", vec![Value::Str(n.clone())]),
        };
        acc = ev("BindList", "BLCons", vec![head, acc]);
    }
    acc
}

fn enum_decl_to_value(e: &EnumDecl) -> Value {
    ev("EnumDecl", "MakeEnumDecl",
       vec![Value::Str(e.name.clone()),
            enum_variant_list_to_value(&e.variants)])
}

fn enum_variant_list_to_value(items: &[EnumVariant]) -> Value {
    let mut acc = ev("EnumVariantList", "EVLNil", vec![]);
    for v in items.iter().rev() {
        acc = ev("EnumVariantList", "EVLCons",
                 vec![enum_variant_to_value(v), acc]);
    }
    acc
}

fn enum_variant_to_value(v: &EnumVariant) -> Value {
    ev("EnumVariant", "MakeEnumVariant",
       vec![Value::Str(v.name.clone()),
            enum_field_list_to_value(&v.fields)])
}

fn enum_field_list_to_value(items: &[EnumField]) -> Value {
    let mut acc = ev("EnumFieldList", "EFLNil", vec![]);
    for f in items.iter().rev() {
        acc = ev("EnumFieldList", "EFLCons",
                 vec![enum_field_to_value(f), acc]);
    }
    acc
}

fn enum_field_to_value(f: &EnumField) -> Value {
    ev("EnumField", "MakeEnumField",
       vec![Value::Str(f.name.clone()), Value::Str(f.type_name.clone())])
}
