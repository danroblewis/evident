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

use crate::ast::*;
use super::types::Value;

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

// ── List decoders (Nil/Cons recursive enums in stdlib/ast.ev) ──

/// Generic Nil/Cons walker: `nil_variant` ends the list,
/// `cons_variant` has fields `[head, tail]`.
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
    decode_list(v, "StringList", "SLNil", "SLCons", decode_str)
}

pub fn decode_expr_list(v: &Value) -> Result<Vec<Expr>> {
    decode_list(v, "ExprList", "ELNil", "ELCons", decode_expr)
}

pub fn decode_mapping_list(v: &Value) -> Result<Vec<Mapping>> {
    decode_list(v, "MappingList", "MLNil", "MLCons", decode_mapping)
}

pub fn decode_body_item_list(v: &Value) -> Result<Vec<BodyItem>> {
    decode_list(v, "BodyItemList", "BILNil", "BILCons", decode_body_item)
}

pub fn decode_match_arm_list(v: &Value) -> Result<Vec<crate::ast::MatchArm>> {
    decode_list(v, "MatchArmList", "MALNil", "MALCons", decode_match_arm)
}

pub fn decode_match_arm(v: &Value) -> Result<crate::ast::MatchArm> {
    let (variant, fields) = check_enum(v, "MatchArm")?;
    if variant != "MakeMatchArm" {
        return Err(DecodeError::UnknownVariant {
            enum_name: "MatchArm".into(), variant: variant.into(),
        });
    }
    need_arity(variant, fields, 2)?;
    let pattern = decode_match_pattern(&fields[0])?;
    let body    = decode_expr(&fields[1])?;
    Ok(crate::ast::MatchArm { pattern, body: Box::new(body) })
}

pub fn decode_match_pattern(v: &Value) -> Result<crate::ast::MatchPattern> {
    use crate::ast::MatchPattern;
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

pub fn decode_bind_list(v: &Value) -> Result<Vec<Option<String>>> {
    decode_list(v, "BindList", "BLNil", "BLCons", decode_match_bind)
}

pub fn decode_match_bind(v: &Value) -> Result<Option<String>> {
    let (variant, fields) = check_enum(v, "MatchBind")?;
    Ok(match variant {
        "BindWildcard" => { need_arity(variant, fields, 0)?; None }
        "BindName"     => { need_arity(variant, fields, 1)?; Some(decode_str(&fields[0])?) }
        other => return Err(DecodeError::UnknownVariant {
            enum_name: "MatchBind".into(), variant: other.into(),
        }),
    })
}

pub fn decode_schema_list(v: &Value) -> Result<Vec<SchemaDecl>> {
    decode_list(v, "SchemaList", "SchLNil", "SchLCons", decode_schema_decl)
}

pub fn decode_enum_decl_list(v: &Value) -> Result<Vec<EnumDecl>> {
    decode_list(v, "EnumDeclList", "EDLNil", "EDLCons", decode_enum_decl)
}

pub fn decode_enum_variant_list(v: &Value) -> Result<Vec<EnumVariant>> {
    decode_list(v, "EnumVariantList", "EVLNil", "EVLCons", decode_enum_variant)
}

pub fn decode_enum_field_list(v: &Value) -> Result<Vec<EnumField>> {
    decode_list(v, "EnumFieldList", "EFLNil", "EFLCons", decode_enum_field)
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
        traces:  Vec::new(),
        shaders: Vec::new(),
    })
}

// `variant_name` is exposed for diagnostic use (e.g. error
// messages on round-trip mismatches); silence unused-import
// warning when the only callers are inside this file.
#[allow(dead_code)]
fn _use_variant_name(v: &Value) -> &str { variant_name(v) }
