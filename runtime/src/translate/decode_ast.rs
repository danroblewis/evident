//! Decoder: Z3 model `Value` (specifically `Value::Enum` trees
//! matching `stdlib/ast.ev`'s `Program` shape) → Rust `ast::Program`.
//!
//! Mirrors `encode_ast.rs` in reverse. Walks a `Value::Enum` tree
//! and reconstructs the corresponding Rust AST nodes by
//! pattern-matching on `(enum_name, variant)` and decoding each
//! field recursively.
//!
//! Used by self-hosted desugar passes: a pass produces a transformed
//! Program in the model; the runtime decodes it back to Rust AST and
//! replaces the loaded Program with the transformed one.

use crate::core::ast::*;
use crate::core::Value;

#[derive(Debug)]
pub enum DecodeError {
    /// Expected an `Enum` value but got something else.
    NotEnum { got: String },
    /// Expected an `Enum` of `expected_enum` but got a different one.
    WrongEnum { expected: &'static str, got: String },
    /// `(enum_name, variant)` doesn't match anything we know how to
    /// decode for the requested AST type. Means stdlib/ast.ev has
    /// drifted from the Rust AST shape, OR the pass produced an
    /// invalid value.
    UnknownVariant { enum_name: String, variant: String },
    /// Field count doesn't match the expected variant arity.
    WrongArity { variant: String, expected: usize, got: usize },
    /// Expected a primitive (`Int` / `Bool` / `Str` / `Real`) but
    /// got something else.
    NotPrimitive { expected: &'static str, got: String },
    /// A field had the wrong runtime kind (e.g. expected
    /// `Value::SeqStr` but got something else). Distinct from
    /// `NotPrimitive` because the expected kind isn't a primitive.
    FieldKind { what: String, want: String, got: String },
}

impl std::fmt::Display for DecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            DecodeError::NotEnum { got } =>
                write!(f, "expected an Enum value; got {got}"),
            DecodeError::WrongEnum { expected, got } =>
                write!(f, "expected enum `{expected}`; got `{got}`"),
            DecodeError::UnknownVariant { enum_name, variant } =>
                write!(f, "no decoder for variant `{variant}` of enum `{enum_name}`"),
            DecodeError::WrongArity { variant, expected, got } =>
                write!(f, "variant `{variant}`: expected {expected} fields, got {got}"),
            DecodeError::NotPrimitive { expected, got } =>
                write!(f, "expected primitive `{expected}`; got {got}"),
            DecodeError::FieldKind { what, want, got } =>
                write!(f, "{what}: expected {want}; got {got}"),
        }
    }
}

impl std::error::Error for DecodeError {}

pub type Result<T> = std::result::Result<T, DecodeError>;

// ── Helpers ────────────────────────────────────────────────────

fn variant_name(v: &Value) -> &str {
    if let Value::Enum { variant, .. } = v { variant } else { "<not enum>" }
}

fn check_enum<'a>(v: &'a Value, expected: &'static str)
    -> Result<(&'a str, &'a Vec<Value>)>
{
    match v {
        Value::Enum { enum_name, variant, fields } => {
            if enum_name != expected {
                return Err(DecodeError::WrongEnum {
                    expected, got: enum_name.clone(),
                });
            }
            Ok((variant.as_str(), fields))
        }
        other => Err(DecodeError::NotEnum {
            got: format!("{other:?}"),
        }),
    }
}

fn need_arity(variant: &str, fields: &[Value], expected: usize) -> Result<()> {
    if fields.len() != expected {
        return Err(DecodeError::WrongArity {
            variant: variant.to_string(),
            expected,
            got: fields.len(),
        });
    }
    Ok(())
}

fn decode_str(v: &Value) -> Result<String> {
    match v {
        Value::Str(s) => Ok(s.clone()),
        other => Err(DecodeError::NotPrimitive {
            expected: "Str", got: format!("{other:?}"),
        }),
    }
}

fn decode_int(v: &Value) -> Result<i64> {
    match v {
        Value::Int(n) => Ok(*n),
        other => Err(DecodeError::NotPrimitive {
            expected: "Int", got: format!("{other:?}"),
        }),
    }
}

fn decode_bool(v: &Value) -> Result<bool> {
    match v {
        Value::Bool(b) => Ok(*b),
        other => Err(DecodeError::NotPrimitive {
            expected: "Bool", got: format!("{other:?}"),
        }),
    }
}

fn decode_real(v: &Value) -> Result<f64> {
    match v {
        Value::Real(f) => Ok(*f),
        Value::Int(n)  => Ok(*n as f64),    // Z3 may return Int for whole Real values
        other => Err(DecodeError::NotPrimitive {
            expected: "Real", got: format!("{other:?}"),
        }),
    }
}

// ── Seq decoders ───────────────────────────────────────────────
//
// Post-Phase-6.5 the AST's list-typed fields are `Seq(T)`, backed
// by an internal Cons helper for mutually-recursive cases. The
// extract path walks the helper and produces a `Value::SeqEnum`
// (or `Value::SeqStr` for Seq(String)). These decoders take that
// SeqEnum/SeqStr and map per-element decoders over its contents.

fn decode_seq_enum<T>(
    v: &Value,
    what: &'static str,
    decode_elem: impl Fn(&Value) -> Result<T>,
) -> Result<Vec<T>> {
    if let Value::SeqEnum(items) = v {
        return items.iter().map(decode_elem).collect();
    }
    Err(DecodeError::FieldKind {
        what: what.into(),
        want: "Value::SeqEnum".into(),
        got: format!("{:?}", v),
    })
}

/// `PackedFieldList` is still a Cons-shaped user-declared enum
/// (gates on Seq concatenation; see plans/06-cons-removal). Keep
/// the Cons-walker for it.
fn decode_list<T>(
    v: &Value,
    list_enum: &'static str,
    nil_variant: &str,
    cons_variant: &str,
    decode_head: impl Fn(&Value) -> Result<T>,
) -> Result<Vec<T>> {
    let mut out = Vec::new();
    let mut cur = v;
    loop {
        let (variant, fields) = check_enum(cur, list_enum)?;
        if variant == nil_variant { return Ok(out); }
        if variant == cons_variant {
            need_arity(variant, fields, 2)?;
            out.push(decode_head(&fields[0])?);
            cur = &fields[1];
            continue;
        }
        return Err(DecodeError::UnknownVariant {
            enum_name: list_enum.to_string(),
            variant: variant.to_string(),
        });
    }
}

pub fn decode_string_list(v: &Value) -> Result<Vec<String>> {
    if let Value::SeqStr(items) = v { return Ok(items.clone()); }
    Err(DecodeError::FieldKind {
        what: "string list".into(),
        want: "Value::SeqStr".into(),
        got: format!("{:?}", v),
    })
}

pub fn decode_expr_list(v: &Value) -> Result<Vec<Expr>> {
    decode_seq_enum(v, "expr list", decode_expr)
}

pub fn decode_mapping_list(v: &Value) -> Result<Vec<Mapping>> {
    decode_seq_enum(v, "mapping list", decode_mapping)
}

pub fn decode_body_item_list(v: &Value) -> Result<Vec<BodyItem>> {
    decode_seq_enum(v, "body item list", decode_body_item)
}

pub fn decode_match_arm_list(v: &Value) -> Result<Vec<crate::core::ast::MatchArm>> {
    decode_seq_enum(v, "match arm list", decode_match_arm)
}

pub fn decode_match_arm(v: &Value) -> Result<crate::core::ast::MatchArm> {
    let (variant, fields) = check_enum(v, "MatchArm")?;
    if variant != "MakeMatchArm" {
        return Err(DecodeError::UnknownVariant {
            enum_name: "MatchArm".into(), variant: variant.into(),
        });
    }
    need_arity(variant, fields, 2)?;
    let pattern = decode_match_pattern(&fields[0])?;
    let body    = decode_expr(&fields[1])?;
    Ok(crate::core::ast::MatchArm { pattern, body: Box::new(body) })
}

pub fn decode_match_pattern(v: &Value) -> Result<crate::core::ast::MatchPattern> {
    use crate::core::ast::MatchPattern;
    let (variant, fields) = check_enum(v, "MatchPattern")?;
    Ok(match variant {
        "PatWildcard" => {
            need_arity(variant, fields, 0)?;
            MatchPattern::Wildcard
        }
        "PatCtor" => {
            need_arity(variant, fields, 2)?;
            let name = decode_str(&fields[0])?;
            let binds = decode_bind_list(&fields[1])?;
            MatchPattern::Ctor { name, binds }
        }
        other => return Err(DecodeError::UnknownVariant {
            enum_name: "MatchPattern".into(), variant: other.into(),
        }),
    })
}

pub fn decode_bind_list(v: &Value) -> Result<Vec<MatchPattern>> {
    decode_seq_enum(v, "bind list", decode_match_bind)
}

/// A `MatchBind` (BindWildcard | BindName) decodes to a constructor
/// sub-pattern: `BindWildcard → Wildcard`, `BindName(n) → Bind(n)`.
/// (stdlib/ast.ev's flat `MatchBind` can't express a nested constructor
/// sub-pattern, so the decoder never produces a `Ctor` here.)
pub fn decode_match_bind(v: &Value) -> Result<MatchPattern> {
    let (variant, fields) = check_enum(v, "MatchBind")?;
    Ok(match variant {
        "BindWildcard" => { need_arity(variant, fields, 0)?; MatchPattern::Wildcard }
        "BindName"     => { need_arity(variant, fields, 1)?; MatchPattern::Bind(decode_str(&fields[0])?) }
        other => return Err(DecodeError::UnknownVariant {
            enum_name: "MatchBind".into(), variant: other.into(),
        }),
    })
}

pub fn decode_schema_list(v: &Value) -> Result<Vec<SchemaDecl>> {
    decode_seq_enum(v, "schema list", decode_schema_decl)
}

pub fn decode_enum_decl_list(v: &Value) -> Result<Vec<EnumDecl>> {
    decode_seq_enum(v, "enum decl list", decode_enum_decl)
}

pub fn decode_enum_variant_list(v: &Value) -> Result<Vec<EnumVariant>> {
    decode_seq_enum(v, "enum variant list", decode_enum_variant)
}

pub fn decode_enum_field_list(v: &Value) -> Result<Vec<EnumField>> {
    decode_seq_enum(v, "enum field list", decode_enum_field)
}

// ── Per-AST-type decoders ──────────────────────────────────────

pub fn decode_binop(v: &Value) -> Result<BinOp> {
    let (variant, _) = check_enum(v, "BinOp")?;
    Ok(match variant {
        "OpEq"      => BinOp::Eq,
        "OpNeq"     => BinOp::Neq,
        "OpLt"      => BinOp::Lt,
        "OpLe"      => BinOp::Le,
        "OpGt"      => BinOp::Gt,
        "OpGe"      => BinOp::Ge,
        "OpAnd"     => BinOp::And,
        "OpOr"      => BinOp::Or,
        "OpImplies" => BinOp::Implies,
        "OpAdd"     => BinOp::Add,
        "OpSub"     => BinOp::Sub,
        "OpMul"     => BinOp::Mul,
        "OpDiv"     => BinOp::Div,
        "OpConcat"  => BinOp::Concat,
        other => return Err(DecodeError::UnknownVariant {
            enum_name: "BinOp".into(), variant: other.into(),
        }),
    })
}

pub fn decode_keyword(v: &Value) -> Result<Keyword> {
    let (variant, _) = check_enum(v, "Keyword")?;
    Ok(match variant {
        "KSchema"   => Keyword::Schema,
        "KClaim"    => Keyword::Claim,
        "KType"     => Keyword::Type,
        "KSubclaim" => Keyword::Subclaim,
        "KFsm"      => Keyword::Fsm,
        other => return Err(DecodeError::UnknownVariant {
            enum_name: "Keyword".into(), variant: other.into(),
        }),
    })
}

pub fn decode_mapping(v: &Value) -> Result<Mapping> {
    let (variant, fields) = check_enum(v, "Mapping")?;
    if variant != "MakeMapping" {
        return Err(DecodeError::UnknownVariant {
            enum_name: "Mapping".into(), variant: variant.into(),
        });
    }
    need_arity(variant, fields, 2)?;
    let slot = decode_str(&fields[0])?;
    let value = decode_expr(&fields[1])?;
    Ok(Mapping { slot, value })
}

pub fn decode_pins(v: &Value) -> Result<Pins> {
    let (variant, fields) = check_enum(v, "Pins")?;
    Ok(match variant {
        "PNone" => Pins::None,
        "PNamed" => {
            need_arity(variant, fields, 1)?;
            Pins::Named(decode_mapping_list(&fields[0])?)
        }
        "PPositional" => {
            need_arity(variant, fields, 1)?;
            Pins::Positional(decode_expr_list(&fields[0])?)
        }
        other => return Err(DecodeError::UnknownVariant {
            enum_name: "Pins".into(), variant: other.into(),
        }),
    })
}

pub fn decode_enum_field(v: &Value) -> Result<EnumField> {
    let (variant, fields) = check_enum(v, "EnumField")?;
    if variant != "MakeEnumField" {
        return Err(DecodeError::UnknownVariant {
            enum_name: "EnumField".into(), variant: variant.into(),
        });
    }
    need_arity(variant, fields, 2)?;
    Ok(EnumField {
        name:      decode_str(&fields[0])?,
        type_name: decode_str(&fields[1])?,
    })
}

pub fn decode_enum_variant(v: &Value) -> Result<EnumVariant> {
    let (variant, fields) = check_enum(v, "EnumVariant")?;
    if variant != "MakeEnumVariant" {
        return Err(DecodeError::UnknownVariant {
            enum_name: "EnumVariant".into(), variant: variant.into(),
        });
    }
    need_arity(variant, fields, 2)?;
    Ok(EnumVariant {
        name:   decode_str(&fields[0])?,
        fields: decode_enum_field_list(&fields[1])?,
    })
}

pub fn decode_enum_decl(v: &Value) -> Result<EnumDecl> {
    let (variant, fields) = check_enum(v, "EnumDecl")?;
    if variant != "MakeEnumDecl" {
        return Err(DecodeError::UnknownVariant {
            enum_name: "EnumDecl".into(), variant: variant.into(),
        });
    }
    need_arity(variant, fields, 2)?;
    Ok(EnumDecl {
        name:     decode_str(&fields[0])?,
        variants: decode_enum_variant_list(&fields[1])?,
    })
}

pub fn decode_schema_decl(v: &Value) -> Result<SchemaDecl> {
    let (variant, fields) = check_enum(v, "SchemaDecl")?;
    if variant != "MakeSchemaDecl" {
        return Err(DecodeError::UnknownVariant {
            enum_name: "SchemaDecl".into(), variant: variant.into(),
        });
    }
    need_arity(variant, fields, 3)?;
    Ok(SchemaDecl {
        keyword: decode_keyword(&fields[0])?,
        name:    decode_str(&fields[1])?,
        body:    decode_body_item_list(&fields[2])?,
        // The encoded AST shape (stdlib/ast.ev) doesn't yet carry
        // first-line params or the external flag separately;
        // conservatively treat 0/false. Self-hosted passes can still
        // observe the body items.
        type_params: vec![],
        param_count: 0,
        external: false,
    })
}

pub fn decode_body_item(v: &Value) -> Result<BodyItem> {
    let (variant, fields) = check_enum(v, "BodyItem")?;
    Ok(match variant {
        "BIMembership" => {
            need_arity(variant, fields, 3)?;
            BodyItem::Membership {
                name:      decode_str(&fields[0])?,
                type_name: decode_str(&fields[1])?,
                pins:      decode_pins(&fields[2])?,
            }
        }
        "BIPassthrough" => {
            need_arity(variant, fields, 1)?;
            BodyItem::Passthrough(decode_str(&fields[0])?)
        }
        "BIClaimCall" => {
            need_arity(variant, fields, 2)?;
            BodyItem::ClaimCall {
                name:     decode_str(&fields[0])?,
                mappings: decode_mapping_list(&fields[1])?,
            }
        }
        "BIConstraint" => {
            need_arity(variant, fields, 1)?;
            BodyItem::Constraint(decode_expr(&fields[0])?)
        }
        "BISubclaim" => {
            need_arity(variant, fields, 1)?;
            BodyItem::SubclaimDecl(decode_schema_decl(&fields[0])?)
        }
        "BIHaltsWithin" => {
            need_arity(variant, fields, 2)?;
            BodyItem::HaltsWithin {
                fsm_name: decode_str(&fields[0])?,
                n:        decode_int(&fields[1])?,
            }
        }
        other => return Err(DecodeError::UnknownVariant {
            enum_name: "BodyItem".into(), variant: other.into(),
        }),
    })
}

pub fn decode_expr(v: &Value) -> Result<Expr> {
    let (variant, fields) = check_enum(v, "Expr")?;
    Ok(match variant {
        "EIdentifier" => {
            need_arity(variant, fields, 1)?;
            Expr::Identifier(decode_str(&fields[0])?)
        }
        "EInt" => {
            need_arity(variant, fields, 1)?;
            Expr::Int(decode_int(&fields[0])?)
        }
        "EReal" => {
            need_arity(variant, fields, 1)?;
            Expr::Real(decode_real(&fields[0])?)
        }
        "EBool" => {
            need_arity(variant, fields, 1)?;
            Expr::Bool(decode_bool(&fields[0])?)
        }
        "EStr" => {
            need_arity(variant, fields, 1)?;
            Expr::Str(decode_str(&fields[0])?)
        }
        "ESetLit" => {
            need_arity(variant, fields, 1)?;
            Expr::SetLit(decode_expr_list(&fields[0])?)
        }
        "ESeqLit" => {
            need_arity(variant, fields, 1)?;
            Expr::SeqLit(decode_expr_list(&fields[0])?)
        }
        "ETuple" => {
            need_arity(variant, fields, 1)?;
            Expr::Tuple(decode_expr_list(&fields[0])?)
        }
        "ERange" => {
            need_arity(variant, fields, 2)?;
            Expr::Range(Box::new(decode_expr(&fields[0])?),
                        Box::new(decode_expr(&fields[1])?))
        }
        "EInExpr" => {
            need_arity(variant, fields, 2)?;
            Expr::InExpr(Box::new(decode_expr(&fields[0])?),
                         Box::new(decode_expr(&fields[1])?))
        }
        "EForall" => {
            need_arity(variant, fields, 3)?;
            Expr::Forall(decode_string_list(&fields[0])?,
                         Box::new(decode_expr(&fields[1])?),
                         Box::new(decode_expr(&fields[2])?))
        }
        "EExists" => {
            need_arity(variant, fields, 3)?;
            Expr::Exists(decode_string_list(&fields[0])?,
                         Box::new(decode_expr(&fields[1])?),
                         Box::new(decode_expr(&fields[2])?))
        }
        "ECall" => {
            need_arity(variant, fields, 2)?;
            Expr::Call(decode_str(&fields[0])?,
                       decode_expr_list(&fields[1])?)
        }
        "ECardinality" => {
            need_arity(variant, fields, 1)?;
            Expr::Cardinality(Box::new(decode_expr(&fields[0])?))
        }
        "EIndex" => {
            need_arity(variant, fields, 2)?;
            Expr::Index(Box::new(decode_expr(&fields[0])?),
                        Box::new(decode_expr(&fields[1])?))
        }
        "EField" => {
            need_arity(variant, fields, 2)?;
            Expr::Field(Box::new(decode_expr(&fields[0])?),
                        decode_str(&fields[1])?)
        }
        "EBinary" => {
            need_arity(variant, fields, 3)?;
            Expr::Binary(decode_binop(&fields[0])?,
                         Box::new(decode_expr(&fields[1])?),
                         Box::new(decode_expr(&fields[2])?))
        }
        "ENot" => {
            need_arity(variant, fields, 1)?;
            Expr::Not(Box::new(decode_expr(&fields[0])?))
        }
        "ETernary" => {
            need_arity(variant, fields, 3)?;
            Expr::Ternary(Box::new(decode_expr(&fields[0])?),
                          Box::new(decode_expr(&fields[1])?),
                          Box::new(decode_expr(&fields[2])?))
        }
        "EMatch" => {
            need_arity(variant, fields, 2)?;
            let scr = decode_expr(&fields[0])?;
            let arms = decode_match_arm_list(&fields[1])?;
            Expr::Match(Box::new(scr), arms)
        }
        "EMatches" => {
            need_arity(variant, fields, 2)?;
            let e = decode_expr(&fields[0])?;
            let p = decode_match_pattern(&fields[1])?;
            Expr::Matches(Box::new(e), p)
        }
        "ERunFsm" => {
            need_arity(variant, fields, 2)?;
            Expr::RunFsm {
                fsm:  decode_str(&fields[0])?,
                init: Box::new(decode_expr(&fields[1])?),
            }
        }
        other => return Err(DecodeError::UnknownVariant {
            enum_name: "Expr".into(), variant: other.into(),
        }),
    })
}

pub fn decode_program(v: &Value) -> Result<Program> {
    let (variant, fields) = check_enum(v, "Program")?;
    if variant != "MakeProgram" {
        return Err(DecodeError::UnknownVariant {
            enum_name: "Program".into(), variant: variant.into(),
        });
    }
    need_arity(variant, fields, 2)?;
    Ok(Program {
        schemas: decode_schema_list(&fields[0])?,
        enums:   decode_enum_decl_list(&fields[1])?,
        // TraceDecl / ShaderDecl aren't in stdlib/ast.ev's Program;
        // decoded form has them empty (consistent with what the
        // encoder skips).
        imports: Vec::new(),
    })
}

// `variant_name` is exposed for diagnostic use (e.g. error
// messages on round-trip mismatches); silence unused-import
// warning when the only callers are inside this file.
#[allow(dead_code)]
fn _use_variant_name(v: &Value) -> &str { variant_name(v) }

// ── stdlib/runtime.ev: Effect / Result / FFIArg ────────────────

pub fn decode_effect(v: &Value) -> Result<crate::core::ast::Effect> {
    use crate::core::ast::Effect;
    let (variant, fields) = check_enum(v, "Effect")?;
    Ok(match variant {
        "NoEffect"     => { need_arity(variant, fields, 0)?; Effect::NoEffect }
        "Print"        => { need_arity(variant, fields, 1)?; Effect::Print(decode_str(&fields[0])?) }
        "Println"      => { need_arity(variant, fields, 1)?; Effect::Println(decode_str(&fields[0])?) }
        "ReadLine"     => { need_arity(variant, fields, 0)?; Effect::ReadLine }
        "Time"         => { need_arity(variant, fields, 0)?; Effect::Time }
        "Exit"         => { need_arity(variant, fields, 1)?; Effect::Exit(decode_int(&fields[0])?) }
        "ParseInt"     => { need_arity(variant, fields, 1)?; Effect::ParseInt(decode_str(&fields[0])?) }
        "ParseReal"    => { need_arity(variant, fields, 1)?; Effect::ParseReal(decode_str(&fields[0])?) }
        "IntToStr"     => { need_arity(variant, fields, 1)?; Effect::IntToStr(decode_int(&fields[0])?) }
        "RealToStr"    => { need_arity(variant, fields, 1)?; Effect::RealToStr(decode_real(&fields[0])?) }
        "ShellRun"     => { need_arity(variant, fields, 1)?; Effect::ShellRun(decode_str(&fields[0])?) }
        "SpawnFsm"     => {
            need_arity(variant, fields, 2)?;
            Effect::SpawnFsm(decode_str(&fields[0])?, decode_int(&fields[1])?)
        }
        "FFIOpen"      => { need_arity(variant, fields, 1)?; Effect::FFIOpen(decode_str(&fields[0])?) }
        "FFILookup"    => {
            need_arity(variant, fields, 2)?;
            Effect::FFILookup(decode_int(&fields[0])? as u64, decode_str(&fields[1])?)
        }
        "FFICall"      => {
            need_arity(variant, fields, 3)?;
            Effect::FFICall(
                decode_int(&fields[0])? as u64,
                decode_str(&fields[1])?,
                decode_arg_list(&fields[2])?,
            )
        }
        "CloseHandle"  => { need_arity(variant, fields, 1)?; Effect::CloseHandle(decode_int(&fields[0])? as u64) }
        "LibCall"      => {
            need_arity(variant, fields, 4)?;
            Effect::LibCall(
                decode_str(&fields[0])?,
                decode_str(&fields[1])?,
                decode_str(&fields[2])?,
                decode_arg_list(&fields[3])?,
            )
        }
        "ReadByte"     => {
            need_arity(variant, fields, 2)?;
            Effect::ReadByte(decode_int(&fields[0])? as u64, decode_int(&fields[1])?)
        }
        "ReadI16"      => {
            need_arity(variant, fields, 2)?;
            Effect::ReadI16(decode_int(&fields[0])? as u64, decode_int(&fields[1])?)
        }
        "ReadI32"      => {
            need_arity(variant, fields, 2)?;
            Effect::ReadI32(decode_int(&fields[0])? as u64, decode_int(&fields[1])?)
        }
        "ReadI64"      => {
            need_arity(variant, fields, 2)?;
            Effect::ReadI64(decode_int(&fields[0])? as u64, decode_int(&fields[1])?)
        }
        "ReadF32"      => {
            need_arity(variant, fields, 2)?;
            Effect::ReadF32(decode_int(&fields[0])? as u64, decode_int(&fields[1])?)
        }
        "ReadF64"      => {
            need_arity(variant, fields, 2)?;
            Effect::ReadF64(decode_int(&fields[0])? as u64, decode_int(&fields[1])?)
        }
        "ReadStr"      => {
            need_arity(variant, fields, 2)?;
            Effect::ReadStr(decode_int(&fields[0])? as u64, decode_int(&fields[1])?)
        }
        "WriteByte"    => {
            need_arity(variant, fields, 3)?;
            Effect::WriteByte(decode_int(&fields[0])? as u64,
                              decode_int(&fields[1])?,
                              decode_int(&fields[2])?)
        }
        "WriteI16"     => {
            need_arity(variant, fields, 3)?;
            Effect::WriteI16(decode_int(&fields[0])? as u64,
                             decode_int(&fields[1])?,
                             decode_int(&fields[2])?)
        }
        "WriteI32"     => {
            need_arity(variant, fields, 3)?;
            Effect::WriteI32(decode_int(&fields[0])? as u64,
                             decode_int(&fields[1])?,
                             decode_int(&fields[2])?)
        }
        "WriteI64"     => {
            need_arity(variant, fields, 3)?;
            Effect::WriteI64(decode_int(&fields[0])? as u64,
                             decode_int(&fields[1])?,
                             decode_int(&fields[2])?)
        }
        "WriteF32"     => {
            need_arity(variant, fields, 3)?;
            Effect::WriteF32(decode_int(&fields[0])? as u64,
                             decode_int(&fields[1])?,
                             decode_real(&fields[2])?)
        }
        "WriteF64"     => {
            need_arity(variant, fields, 3)?;
            Effect::WriteF64(decode_int(&fields[0])? as u64,
                             decode_int(&fields[1])?,
                             decode_real(&fields[2])?)
        }
        "WriteStr"     => {
            need_arity(variant, fields, 3)?;
            Effect::WriteStr(decode_int(&fields[0])? as u64,
                             decode_int(&fields[1])?,
                             decode_str(&fields[2])?)
        }
        "Malloc"       => {
            need_arity(variant, fields, 1)?;
            Effect::Malloc(decode_int(&fields[0])?)
        }
        "MonotonicTime" => { need_arity(variant, fields, 0)?; Effect::MonotonicTime }
        "RegisterCallback" => {
            need_arity(variant, fields, 2)?;
            Effect::RegisterCallback(decode_str(&fields[0])?, decode_str(&fields[1])?)
        }
        other => return Err(DecodeError::UnknownVariant {
            enum_name: "Effect".into(), variant: other.into(),
        }),
    })
}

/// Decode `effects ∈ Seq(Effect)` from the model — a `Value::SeqEnum`
/// of Effect enums (since Phase 6.4 retired the `EffectList` Cons
/// shape). Maps each element through `decode_effect`.
pub fn decode_effect_list(v: &Value) -> Result<Vec<crate::core::ast::Effect>> {
    if let Value::SeqEnum(items) = v {
        return items.iter().map(decode_effect).collect();
    }
    Err(DecodeError::FieldKind {
        what: "effects".into(),
        want: "Seq(Effect)".into(),
        got: format!("{:?}", v),
    })
}

/// Decoded `InstallStep`: an Effect to dispatch + an optional field
/// name to capture the result into. `None` = `Run(Effect)` (discard
/// result), `Some(field)` = `Bind(field, Effect)`. Used by the
/// declarative install path in `event_sources/declarative_install.rs`.
#[derive(Debug, Clone)]
pub struct InstallStep {
    pub field:  Option<String>,
    pub effect: crate::core::ast::Effect,
}

pub fn decode_install_step(v: &Value) -> Result<InstallStep> {
    let (variant, fields) = check_enum(v, "InstallStep")?;
    Ok(match variant {
        "Run" => {
            need_arity(variant, fields, 1)?;
            InstallStep { field: None, effect: decode_effect(&fields[0])? }
        }
        "Bind" => {
            need_arity(variant, fields, 2)?;
            InstallStep {
                field:  Some(decode_str(&fields[0])?),
                effect: decode_effect(&fields[1])?,
            }
        }
        other => return Err(DecodeError::UnknownVariant {
            enum_name: "InstallStep".into(), variant: other.into(),
        }),
    })
}

pub fn decode_install_step_list(v: &Value) -> Result<Vec<InstallStep>> {
    if let Value::SeqEnum(items) = v {
        return items.iter().map(decode_install_step).collect();
    }
    Err(DecodeError::FieldKind {
        what: "install".into(),
        want: "Seq(InstallStep)".into(),
        got: format!("{:?}", v),
    })
}

pub fn decode_ffi_arg(v: &Value) -> Result<crate::core::ast::EffectFfiArg> {
    use crate::core::ast::EffectFfiArg;
    let (variant, fields) = check_enum(v, "FFIArg")?;
    Ok(match variant {
        "ArgInt"    => { need_arity(variant, fields, 1)?; EffectFfiArg::Int(decode_int(&fields[0])?) }
        "ArgBool"   => { need_arity(variant, fields, 1)?; EffectFfiArg::Bool(decode_bool(&fields[0])?) }
        "ArgStr"    => { need_arity(variant, fields, 1)?; EffectFfiArg::Str(decode_str(&fields[0])?) }
        "ArgReal"   => { need_arity(variant, fields, 1)?; EffectFfiArg::Real(decode_real(&fields[0])?) }
        "ArgHandle" => { need_arity(variant, fields, 1)?; EffectFfiArg::Handle(decode_int(&fields[0])? as u64) }
        "ArgStrArr" => {
            need_arity(variant, fields, 1)?;
            EffectFfiArg::StrArr(decode_str_list(&fields[0])?)
        }
        "ArgIntOut" => {
            need_arity(variant, fields, 0)?;
            EffectFfiArg::IntOut
        }
        "ArgPriorResult" => {
            need_arity(variant, fields, 1)?;
            EffectFfiArg::PriorResult(decode_int(&fields[0])? as usize)
        }
        "ArgI32Buf" => {
            need_arity(variant, fields, 1)?;
            let ints = decode_int_list(&fields[0])?;
            EffectFfiArg::I32Buf(ints.into_iter().map(|n| n as i32).collect())
        }
        "ArgPackedBuf" => {
            need_arity(variant, fields, 1)?;
            EffectFfiArg::PackedBuf(decode_packed_field_list(&fields[0])?)
        }
        other => return Err(DecodeError::UnknownVariant {
            enum_name: "FFIArg".into(), variant: other.into(),
        }),
    })
}

/// `ArgStrArr`'s payload — `Seq(String)` since Phase 6.2.b.
/// Extract path produces Value::SeqStr; we just clone the Vec.
pub fn decode_str_list(v: &Value) -> Result<Vec<String>> {
    if let Value::SeqStr(items) = v {
        return Ok(items.clone());
    }
    Err(DecodeError::FieldKind {
        what: "ArgStrArr payload".into(),
        want: "Seq(String)".into(),
        got: format!("{:?}", v),
    })
}

/// `ArgI32Buf`'s payload — `Seq(Int)` since Phase 6.2.b.
pub fn decode_int_list(v: &Value) -> Result<Vec<i64>> {
    if let Value::SeqInt(items) = v {
        return Ok(items.clone());
    }
    Err(DecodeError::FieldKind {
        what: "ArgI32Buf payload".into(),
        want: "Seq(Int)".into(),
        got: format!("{:?}", v),
    })
}

/// Decode a single `PackedField` (PfU8 / PfI32 / PfF32) into the
/// matching Rust enum variant. The field's natural-width Evident type
/// (Int / Real) is narrowed to the storage width here; callers
/// should ensure values fit before this runs.
pub fn decode_packed_field(v: &Value) -> Result<crate::core::ast::PackedField> {
    let (variant, fields) = check_enum(v, "PackedField")?;
    Ok(match variant {
        "PfU8"  => { need_arity(variant, fields, 1)?;
                     crate::core::ast::PackedField::U8(decode_int(&fields[0])? as u8) }
        "PfI32" => { need_arity(variant, fields, 1)?;
                     crate::core::ast::PackedField::I32(decode_int(&fields[0])? as i32) }
        "PfF32" => { need_arity(variant, fields, 1)?;
                     crate::core::ast::PackedField::F32(decode_real(&fields[0])? as f32) }
        other => return Err(DecodeError::UnknownVariant {
            enum_name: "PackedField".into(), variant: other.into(),
        }),
    })
}

/// Cons/Nil-shaped `PackedFieldList` decoder.
pub fn decode_packed_field_list(v: &Value) -> Result<Vec<crate::core::ast::PackedField>> {
    decode_list(v, "PackedFieldList", "PfNil", "PfCons", decode_packed_field)
}

/// `Effect::FFICall` / `Effect::LibCall`'s args payload —
/// `Seq(FFIArg)` since Phase 6.2.c. Extract path produces
/// `Value::SeqEnum`; we map each enum element through
/// `decode_ffi_arg`.
pub fn decode_arg_list(v: &Value) -> Result<Vec<crate::core::ast::EffectFfiArg>> {
    if let Value::SeqEnum(items) = v {
        return items.iter().map(decode_ffi_arg).collect();
    }
    Err(DecodeError::FieldKind {
        what: "FFICall/LibCall args".into(),
        want: "Seq(FFIArg)".into(),
        got: format!("{:?}", v),
    })
}

pub fn decode_result(v: &Value) -> Result<crate::core::ast::EffectResult> {
    use crate::core::ast::EffectResult;
    let (variant, fields) = check_enum(v, "Result")?;
    Ok(match variant {
        "NoResult"     => { need_arity(variant, fields, 0)?; EffectResult::NoResult }
        "IntResult"    => { need_arity(variant, fields, 1)?; EffectResult::Int(decode_int(&fields[0])?) }
        "StringResult" => { need_arity(variant, fields, 1)?; EffectResult::Str(decode_str(&fields[0])?) }
        "BoolResult"   => { need_arity(variant, fields, 1)?; EffectResult::Bool(decode_bool(&fields[0])?) }
        "RealResult"   => { need_arity(variant, fields, 1)?; EffectResult::Real(decode_real(&fields[0])?) }
        "HandleResult" => { need_arity(variant, fields, 1)?; EffectResult::Handle(decode_int(&fields[0])? as u64) }
        "ErrorResult"  => { need_arity(variant, fields, 1)?; EffectResult::Error(decode_str(&fields[0])?) }
        other => return Err(DecodeError::UnknownVariant {
            enum_name: "Result".into(), variant: other.into(),
        }),
    })
}

#[cfg(test)]
mod effect_decoder_tests {
    use super::*;
    use crate::core::ast::{Effect, EffectFfiArg, EffectResult};

    /// Helper: construct a `Value::Enum`.
    fn e(enum_name: &str, variant: &str, fields: Vec<Value>) -> Value {
        Value::Enum {
            enum_name: enum_name.into(),
            variant: variant.into(),
            fields,
        }
    }

    #[test]
    fn decode_println_effect() {
        let v = e("Effect", "Println", vec![Value::Str("hello".into())]);
        match decode_effect(&v).unwrap() {
            Effect::Println(s) => assert_eq!(s, "hello"),
            other => panic!("expected Println, got {other:?}"),
        }
    }

    #[test]
    fn decode_no_effect_zero_arity() {
        let v = e("Effect", "NoEffect", vec![]);
        assert!(matches!(decode_effect(&v).unwrap(), Effect::NoEffect));
    }

    #[test]
    fn decode_ffi_call_with_args() {
        // Phase 6.2.c: args are now Seq(FFIArg), extracted as
        // Value::SeqEnum of FFIArg Value::Enum elements.
        let arglist = Value::SeqEnum(vec![
            e("FFIArg", "ArgStr", vec![Value::Str("hi".into())]),
            e("FFIArg", "ArgInt", vec![Value::Int(42)]),
        ]);
        let v = e("Effect", "FFICall", vec![
            Value::Int(7),
            Value::Str("i(si)".into()),
            arglist,
        ]);
        match decode_effect(&v).unwrap() {
            Effect::FFICall(h, sig, args) => {
                assert_eq!(h, 7);
                assert_eq!(sig, "i(si)");
                assert_eq!(args.len(), 2);
                assert!(matches!(&args[0], EffectFfiArg::Str(s) if s == "hi"));
                assert!(matches!(&args[1], EffectFfiArg::Int(42)));
            }
            other => panic!("expected FFICall, got {other:?}"),
        }
    }

    #[test]
    fn decode_effect_list_three_items() {
        // Post-6.4: effects come back as Value::SeqEnum of Effect.
        let list = Value::SeqEnum(vec![
            e("Effect", "Println", vec![Value::Str("a".into())]),
            e("Effect", "Time", vec![]),
            e("Effect", "Exit", vec![Value::Int(0)]),
        ]);
        let decoded = decode_effect_list(&list).unwrap();
        assert_eq!(decoded.len(), 3);
        assert!(matches!(&decoded[0], Effect::Println(s) if s == "a"));
        assert!(matches!(&decoded[1], Effect::Time));
        assert!(matches!(&decoded[2], Effect::Exit(0)));
    }

    #[test]
    fn decode_int_result() {
        let v = e("Result", "IntResult", vec![Value::Int(42)]);
        assert!(matches!(decode_result(&v).unwrap(), EffectResult::Int(42)));
    }

    #[test]
    fn decode_unknown_variant_errors() {
        let v = e("Effect", "BogusVariant", vec![]);
        let err = decode_effect(&v).unwrap_err();
        assert!(matches!(err, DecodeError::UnknownVariant { .. }));
    }
}
