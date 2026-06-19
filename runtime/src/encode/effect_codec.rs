use z3::ast::{Ast, Datatype, Real};
use z3::Context;

use crate::core::Value;
use crate::core::EnumRegistry;

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

#[derive(Debug)]
pub enum DecodeError {

    NotEnum { got: String },

    WrongEnum { expected: &'static str, got: String },

    UnknownVariant { enum_name: String, variant: String },

    WrongArity { variant: String, expected: usize, got: usize },

    NotPrimitive { expected: &'static str, got: String },

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
        Value::Int(n)  => Ok(*n as f64),
        other => Err(DecodeError::NotPrimitive {
            expected: "Real", got: format!("{other:?}"),
        }),
    }
}

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

pub fn decode_effect(v: &Value) -> Result<crate::core::ast::Effect> {
    use crate::core::ast::Effect;
    let (variant, fields) = check_enum(v, "Effect")?;
    Ok(match variant {
        "NoEffect"     => { need_arity(variant, fields, 0)?; Effect::NoEffect }
        "Print"        => { need_arity(variant, fields, 1)?; Effect::Print(decode_str(&fields[0])?) }
        "Println"      => { need_arity(variant, fields, 1)?; Effect::Println(decode_str(&fields[0])?) }
        "Exit"         => { need_arity(variant, fields, 1)?; Effect::Exit(decode_int(&fields[0])?) }
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
        "RegisterCallback" => {
            need_arity(variant, fields, 2)?;
            Effect::RegisterCallback(decode_str(&fields[0])?, decode_str(&fields[1])?)
        }
        other => return Err(DecodeError::UnknownVariant {
            enum_name: "Effect".into(), variant: other.into(),
        }),
    })
}

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

pub fn decode_packed_field_list(v: &Value) -> Result<Vec<crate::core::ast::PackedField>> {
    decode_list(v, "PackedFieldList", "PfNil", "PfCons", decode_packed_field)
}

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
