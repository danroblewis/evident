//! Decoder: Z3 model `Value` (`Value::Enum` / `Value::SeqEnum`
//! trees) → Rust runtime AST fragments.
//!
//! Walks a value tree and reconstructs the corresponding Rust nodes
//! by pattern-matching on `(enum_name, variant)` and decoding each
//! field recursively. Used by the executor to read back `Effect`s,
//! `Result`s, FFI args, and the declarative `Seq(InstallStep)` path
//! from a solved model.

use crate::core::Value;

#[derive(Debug)]
pub enum DecodeError {
    /// Expected an `Enum` value but got something else.
    NotEnum { got: String },
    /// Expected an `Enum` of `expected_enum` but got a different one.
    WrongEnum { expected: &'static str, got: String },
    /// `(enum_name, variant)` doesn't match anything we know how to
    /// decode for the requested type — the model produced an
    /// unexpected value.
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
/// declarative install path in `effect_loop/install.rs`.
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
