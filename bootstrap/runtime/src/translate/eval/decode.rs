//! Z3 model → Rust `Value` decoders shared by all `evaluate*` entry points.
//! `extract_enum_value` finds the active variant and recurses into payload fields.

use z3::ast::Int;
use z3::Context;

use crate::core::{EnumRegistry, Value};
use super::super::extract::unescape_z3_string;
use super::solver::real_value_to_f64;

/// Extract an enum-typed Z3 const from the model by finding the active variant via its
/// tester, then recursively decoding each payload field (including nested enums).
pub(super) fn extract_enum_value<'ctx>(
    ast: &z3::ast::Datatype<'ctx>,
    enum_name: &str,
    dt: &'static z3::DatatypeSort<'static>,
    model: &z3::Model<'ctx>,
    ctx: &'ctx Context,
    enums: Option<&EnumRegistry>,
) -> Option<Value> {
    let evaluated = model.eval(ast, true)?;
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

    // Seq(T) payload fields are two-accessor-expanded (arr + len), so physical accessor
    // offset advances by 1 for primitives/enums and by 2 for Seq fields.
    let mut field_values: Vec<Value> = Vec::new();
    if let Some(reg) = enums {
        if let Some((_, decl_variants)) = reg.by_name.borrow().get(enum_name) {
            if let Some(decl_variant) = decl_variants.get(idx) {
                let mut acc_idx: usize = 0;
                for decl_field in decl_variant.fields.iter() {
                    if let Some(inner) = crate::core::parse_seq_type(&decl_field.type_name) {
                        // Internal-Cons backing: one Datatype accessor; walk __SeqOf_T chain.
                        let helper_name = crate::core::internal_cons_helper_name(inner);
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

/// Extract a Seq-typed payload field from the (arr, len) two-accessor pair.
fn extract_seq_payload<'ctx>(
    inner_type: &str,
    arr_dyn: &z3::ast::Dynamic<'ctx>,
    len_dyn: &z3::ast::Dynamic<'ctx>,
    model: &z3::Model<'ctx>,
    ctx: &'ctx Context,
    enums: Option<&EnumRegistry>,
) -> Option<Value> {
    use crate::core::SeqElem;
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
            let reg = enums?;
            let dt = reg.by_name.borrow().get(enum_type).map(|(d, _)| *d)?;
            let arr = arr_dyn.as_array()?;
            extract_seq_enum(&arr, &len, enum_type, dt, model, ctx, enums)
        }
    }
}

/// Walk a `__SeqOf_T` Cons chain (`__Empty_T` / `__Cell_T(head, tail)`) in the model.
/// Used by `extract_enum_value` when a Seq(T) field has internal-Cons backing.
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
    for _ in 0..10_000 { // cap so a model bug can't loop forever
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

/// Walk `arr[0..len]`, decoding each element via `extract_enum_value`.
/// Returns `Value::SeqEnum`; mirror of `extract_seq_composite` for enum-typed elements.
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
