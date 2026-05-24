//! Z3 model → Rust `Value` decoders. Pulled out of `evaluate*` so
//! every entry point shares one extraction implementation.
//!
//!   * `extract_binding`        — top-level dispatcher: take one
//!                                 `(name, Var)` entry from the env
//!                                 and stick its model value into the
//!                                 bindings map.
//!   * `extract_enum_value`     — walk a Datatype's variants, find
//!                                 the active one via its tester, and
//!                                 recursively decode each payload
//!                                 field (handles Seq payloads via
//!                                 the two helpers below).
//!   * `extract_seq_enum`       — read a `Seq(EnumType)` value out of
//!                                 the model: walk arr[0..len], decode
//!                                 each element via `extract_enum_value`.
//!   * `extract_seq_payload`,
//!     `extract_internal_cons_seq`
//!                              — file-private helpers consumed only
//!                                 by `extract_enum_value`.

use std::collections::HashMap;
use z3::ast::Int;
use z3::Context;

use super::super::types::{EnumRegistry, Value, Var};
use super::super::extract::{extract_seq, extract_seq_composite, extract_set, unescape_z3_string};
use super::solver::real_value_to_f64;

/// Pull one variable's value out of the model into the bindings map.
/// Mirrors the inline match in `evaluate`'s SAT branch — extracted so
/// `evaluate_with_core` doesn't have to duplicate it.
pub(crate) fn extract_binding(
    name: &str, var: &Var<'static>, model: &z3::Model<'_>, ctx: &'static Context,
    bindings: &mut HashMap<String, Value>,
    enums: Option<&EnumRegistry>,
) {
    match var {
        Var::IntVar(i) => {
            if let Some(val) = model.eval(i, true) {
                if let Some(n) = val.as_i64() {
                    bindings.insert(name.to_string(), Value::Int(n));
                }
            }
        }
        Var::BoolVar(b) => {
            if let Some(val) = model.eval(b, true) {
                if let Some(bv) = val.as_bool() {
                    bindings.insert(name.to_string(), Value::Bool(bv));
                }
            }
        }
        Var::RealVar(r) => {
            if let Some((num, den)) = model.eval(r, true).and_then(|x| x.as_real()) {
                bindings.insert(name.to_string(), Value::Real(real_value_to_f64(num, den)));
            }
        }
        Var::StrVar(s) => {
            if let Some(val) = model.eval(s, true) {
                if let Some(sv) = val.as_string() {
                    bindings.insert(name.to_string(), Value::Str(unescape_z3_string(&sv)));
                }
            }
        }
        Var::SeqVar { arr, len, elem } => {
            if let Some(v) = extract_seq(arr, len, *elem, model, ctx) {
                bindings.insert(name.to_string(), v);
            }
        }
        Var::PinnedInt(v) => { bindings.insert(name.to_string(), Value::Int(*v)); }
        Var::SetVar { set, elem, candidates } => {
            if let Some(v) = extract_set(set, *elem, candidates, model, ctx) {
                bindings.insert(name.to_string(), v);
            }
        }
        Var::DatatypeSetVar { .. } => { /* unsupported in v1 */ }
        Var::DatatypeSeqVar { arr, len, dt, fields, type_name } => {
            let extracted = if fields.is_empty() {
                extract_seq_enum(arr, len, type_name, *dt, model, ctx, enums)
            } else {
                extract_seq_composite(arr, len, fields.as_slice(), *dt, model, ctx, enums)
            };
            if let Some(v) = extracted {
                bindings.insert(name.to_string(), v);
            }
        }
        Var::EnumVar { ast, enum_name, dt } => {
            if let Some(v) = extract_enum_value(ast, enum_name, dt, model, ctx, enums) {
                bindings.insert(name.to_string(), v);
            }
        }
        Var::EnumValue { .. } => { /* literal */ }
        Var::EnumCtor { .. }  => { /* constructor */ }
    }
}

/// Extract an enum-typed Z3 const from the model. Walks the
/// DatatypeSort's variants looking for the one whose `tester` returns
/// true on the model-evaluated value, then recursively extracts each
/// payload field. Recursion handles self-referential enums — the
/// EnumRegistry is consulted to find the field's enum (by type name)
/// when a payload field is itself an enum-typed value.
pub(super) fn extract_enum_value<'ctx>(
    ast: &z3::ast::Datatype<'ctx>,
    enum_name: &str,
    dt: &'static z3::DatatypeSort<'static>,
    model: &z3::Model<'ctx>,
    ctx: &'ctx Context,
    enums: Option<&EnumRegistry>,
) -> Option<Value> {
    let evaluated = model.eval(ast, true)?;
    // Find the active variant via its tester.
    let mut active_idx: Option<usize> = None;
    for (i, variant) in dt.variants.iter().enumerate() {
        let test = variant.tester.apply(&[&evaluated]).as_bool()?;
        if let Some(true) = model.eval(&test, true).and_then(|b| b.as_bool()) {
            active_idx = Some(i);
            break;
        }
    }
    let idx = active_idx?;
    let variant = &dt.variants[idx];
    let variant_name = variant.constructor.name();

    // Look up the variant's declared field types so we can route each
    // accessor's Dynamic through the right `as_int` / `as_bool` /
    // `as_string` extractor (or recurse for nested enums).
    //
    // Seq(T) payload fields are two-accessor-expanded in the Z3
    // datatype (one logical field → arr accessor + len accessor),
    // so we maintain a separate physical accessor offset that
    // advances by 1 for primitive/enum fields and by 2 for Seq.
    let mut field_values: Vec<Value> = Vec::new();
    if let Some(reg) = enums {
        if let Some((_, decl_variants)) = reg.by_name.borrow().get(enum_name) {
            if let Some(decl_variant) = decl_variants.get(idx) {
                let mut acc_idx: usize = 0;
                for decl_field in decl_variant.fields.iter() {
                    if let Some(inner) = crate::runtime::parse_seq_type(&decl_field.type_name) {
                        // Internal-Cons backing: single Datatype
                        // accessor; walk the __SeqOf_T chain to
                        // recover the elements.
                        let helper_name = crate::runtime::internal_cons_helper_name(inner);
                        let has_helper = reg.by_name.borrow().contains_key(&helper_name);
                        if has_helper {
                            let acc = &variant.accessors[acc_idx];
                            let cons_dyn = acc.apply(&[&evaluated]);
                            let extracted = extract_internal_cons_seq(
                                &helper_name, inner, &cons_dyn, model, ctx, enums);
                            if let Some(v) = extracted {
                                field_values.push(v);
                            }
                            acc_idx += 1;
                            continue;
                        }
                        // Two-accessor expansion: arr at acc_idx, len at acc_idx+1.
                        let arr_acc = &variant.accessors[acc_idx];
                        let len_acc = &variant.accessors[acc_idx + 1];
                        let arr_dyn = arr_acc.apply(&[&evaluated]);
                        let len_dyn = len_acc.apply(&[&evaluated]);
                        let extracted = extract_seq_payload(
                            inner, &arr_dyn, &len_dyn, model, ctx, enums);
                        if let Some(v) = extracted {
                            field_values.push(v);
                        }
                        acc_idx += 2;
                        continue;
                    }
                    let accessor = &variant.accessors[acc_idx];
                    let raw = accessor.apply(&[&evaluated]);
                    let extracted = match decl_field.type_name.as_str() {
                        "Int" | "Nat" | "Pos" => raw.as_int()
                            .and_then(|i| model.eval(&i, true))
                            .and_then(|x| x.as_i64())
                            .map(Value::Int),
                        "Bool" => raw.as_bool()
                            .and_then(|b| model.eval(&b, true))
                            .and_then(|x| x.as_bool())
                            .map(Value::Bool),
                        "String" => raw.as_string()
                            .and_then(|s| model.eval(&s, true))
                            .and_then(|x| x.as_string())
                            .map(|s| Value::Str(unescape_z3_string(&s))),
                        "Real" => raw.as_real()
                            .and_then(|r| model.eval(&r, true))
                            .and_then(|x| x.as_real())
                            .map(|(num, den)| Value::Real(real_value_to_f64(num, den))),
                        // Self-reference or another enum: recurse.
                        ref_type => {
                            let target: &str = if ref_type == enum_name { enum_name }
                                               else { ref_type };
                            let nested_dt = reg.by_name.borrow().get(target)
                                .map(|(d, _)| *d);
                            if let Some(nested_dt) = nested_dt {
                                raw.as_datatype().and_then(|child_ast| {
                                    extract_enum_value(&child_ast, target,
                                                       nested_dt, model, ctx, enums)
                                })
                            } else { None }
                        }
                    };
                    if let Some(v) = extracted {
                        field_values.push(v);
                    }
                    acc_idx += 1;
                }
            }
        }
    }
    Some(Value::Enum {
        enum_name: enum_name.to_string(),
        variant: variant_name,
        fields: field_values,
    })
}

/// Extract a Seq-typed enum-variant payload field given the (arr,
/// len) pair produced by the two-accessor expansion. Routes to
/// `extract_seq` for primitive element types or `extract_seq_enum`
/// for enum elements. Used by `extract_enum_value` when it
/// encounters a `Seq(T)` field in a variant's declared types.
fn extract_seq_payload<'ctx>(
    inner_type: &str,
    arr_dyn: &z3::ast::Dynamic<'ctx>,
    len_dyn: &z3::ast::Dynamic<'ctx>,
    model: &z3::Model<'ctx>,
    ctx: &'ctx Context,
    enums: Option<&EnumRegistry>,
) -> Option<Value> {
    use super::super::types::SeqElem;
    let len = len_dyn.as_int()?;
    match inner_type {
        "Int" | "Nat" | "Pos" => {
            let arr = arr_dyn.as_array()?;
            super::super::extract::extract_seq(&arr, &len, SeqElem::Int, model, ctx)
        }
        "Bool" => {
            let arr = arr_dyn.as_array()?;
            super::super::extract::extract_seq(&arr, &len, SeqElem::Bool, model, ctx)
        }
        "String" => {
            let arr = arr_dyn.as_array()?;
            super::super::extract::extract_seq(&arr, &len, SeqElem::Str, model, ctx)
        }
        enum_type => {
            // Enum element: look up the DatatypeSort and walk
            // arr[0..len], calling extract_enum_value per element.
            let reg = enums?;
            let dt = reg.by_name.borrow().get(enum_type).map(|(d, _)| *d)?;
            let arr = arr_dyn.as_array()?;
            extract_seq_enum(&arr, &len, enum_type, dt, model, ctx, enums)
        }
    }
}

/// Walk a `__SeqOf_T`-shaped Cons chain in the model and extract
/// the element list. Used by `extract_enum_value` when a variant
/// field is `Seq(T)` and T has internal-Cons backing (the field is
/// a single Datatype slot pointing to `__SeqOf_T`).
///
/// `__SeqOf_T` has variants `__Empty_T` (0-ary terminator) and
/// `__Cell_T(head: T, tail: __SeqOf_T)`. Walk via tester +
/// accessors until Empty.
fn extract_internal_cons_seq<'ctx>(
    helper_name: &str,
    elem_type: &str,
    cons_dyn: &z3::ast::Dynamic<'ctx>,
    model: &z3::Model<'ctx>,
    ctx: &'ctx Context,
    enums: Option<&EnumRegistry>,
) -> Option<Value> {
    let reg = enums?;
    let by_name = reg.by_name.borrow();
    let (helper_dt, helper_variants) = by_name.get(helper_name)?;
    let helper_dt: &'static z3::DatatypeSort<'static> = *helper_dt;
    let empty_idx = helper_variants.iter().position(|v| v.fields.is_empty())?;
    let cell_idx = helper_variants.iter().position(|v| v.fields.len() == 2)?;
    let (elem_dt, _) = by_name.get(elem_type)?;
    let elem_dt: &'static z3::DatatypeSort<'static> = *elem_dt;
    drop(by_name);

    let empty_tester = &helper_dt.variants[empty_idx].tester;
    let cell_v = &helper_dt.variants[cell_idx];
    let head_acc = &cell_v.accessors[0];
    let tail_acc = &cell_v.accessors[1];

    let mut out: Vec<Value> = Vec::new();
    let mut cur = cons_dyn.clone();
    // Cap iteration so a model bug can't make us walk forever.
    for _ in 0..10_000 {
        let is_empty_bool = empty_tester.apply(&[&cur]).as_bool()?;
        let is_empty = model.eval(&is_empty_bool, true)?.as_bool()?;
        if is_empty {
            return Some(Value::SeqEnum(out));
        }
        let head_dyn = head_acc.apply(&[&cur]);
        let head_dt = head_dyn.as_datatype()?;
        let head_val = extract_enum_value(&head_dt, elem_type, elem_dt, model, ctx, enums)?;
        out.push(head_val);
        cur = tail_acc.apply(&[&cur]);
    }
    None
}

/// Read a `Seq(EnumType)` value out of the model. Mirror of
/// `extract_seq_composite` but for enum-typed elements: each array
/// element is a Datatype value of the enum's sort, decoded via
/// `extract_enum_value` (which handles variant detection + payload
/// recursion). Returned as `Value::SeqEnum(Vec<Value::Enum>)`.
pub(in super::super) fn extract_seq_enum<'ctx>(
    arr: &z3::ast::Array<'ctx>,
    len: &Int<'ctx>,
    type_name: &str,
    dt: &'static z3::DatatypeSort<'static>,
    model: &z3::Model<'ctx>,
    ctx: &'ctx Context,
    enums: Option<&EnumRegistry>,
) -> Option<Value> {
    let n = model.eval(len, true)?.as_i64()?;
    if n < 0 { return None; }
    let mut out: Vec<Value> = Vec::with_capacity(n as usize);
    for i in 0..n {
        let idx = Int::from_i64(ctx, i);
        let elem_dyn = arr.select(&idx);
        let elem = elem_dyn.as_datatype()?;
        let v = extract_enum_value(&elem, type_name, dt, model, ctx, enums)?;
        out.push(v);
    }
    Some(Value::SeqEnum(out))
}
