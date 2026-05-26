//! ClaimCall mapping resolution: `resolve_mapping` → `(env-key, Var)` bindings;
//! `resolve_field_chain_to_bindings` drills into `Seq(Composite)` along a field path.

use std::collections::HashMap;
use z3::ast::{Bool, Int, String as Z3Str};
use z3::{Context, DatatypeSort};

use crate::core::ast::*;
use crate::core::{FieldKind, Var};

use super::bool::translate_bool;
use super::scalar::{real_from_f64, translate_int, translate_real, translate_str};
use super::seq_eq::bind_composite_fields;

/// Resolve a mapping-value expr to `(env-key, Var)` bindings for a ClaimCall inner env.
/// Tries sub-schema prefix, record literal, Seq-index, Field-chain, then scalar `expr_as_var`.
pub(crate) fn resolve_mapping<'ctx>(
    slot: &str,
    value: &Expr,
    ctx: &'ctx Context,
    env: &HashMap<String, Var<'ctx>>,
    schemas: &HashMap<String, SchemaDecl>,
) -> Vec<(String, Var<'ctx>)> {
    if let Expr::Identifier(name) = value {
        if env.contains_key(name) {
            return vec![(slot.to_string(), env[name].clone())];
        }
        // Sub-schema expansion: gather env keys with prefix `name.`, re-key under `slot.field`.
        let prefix = format!("{}.", name);
        let mut out = Vec::new();
        for (k, v) in env {
            if let Some(field) = k.strip_prefix(&prefix) {
                out.push((format!("{}.{}", slot, field), v.clone()));
            }
        }
        if !out.is_empty() {
            return out;
        }
    }
    // Record literal: expand each arg to `slot.field_name`; unspecified fields stay free.
    if let Expr::Call(type_name, args) = value {
        if let Some(schema) = schemas.get(type_name) {
            let fields: Vec<(String, String)> = schema.body.iter()
                .filter_map(|i| if let BodyItem::Membership { name, type_name, .. } = i {
                    Some((name.clone(), type_name.clone()))
                } else { None })
                .collect();
            if args.len() <= fields.len() {
                let mut out = Vec::new();
                let mut ok = true;
                for (arg, (field_name, field_type)) in args.iter().zip(fields.iter()) {
                    let key = format!("{}.{}", slot, field_name);
                    // Tuple → sub-record coercion: bare `(a,b,c)` for a known schema type.
                    let coerced_storage: Expr;
                    let arg_ref: &Expr = match arg {
                        Expr::Tuple(items) if schemas.contains_key(field_type) => {
                            coerced_storage = Expr::Call(field_type.clone(), items.clone());
                            &coerced_storage
                        }
                        other => other,
                    };
                    let v: Option<Var<'ctx>> = match field_type.as_str() {
                        "Int" | "Nat" | "Pos" =>
                            translate_int(arg_ref, ctx, env).map(Var::IntVar),
                        "Bool" =>
                            translate_bool(arg_ref, ctx, env, schemas).map(Var::BoolVar),
                        "String" =>
                            translate_str(arg_ref, ctx, env).map(Var::StrVar),
                        "Real" =>
                            translate_real(arg_ref, ctx, env).map(Var::RealVar),
                        _ => {
                            // Composite field: recurse for sub-record literals and identifier passthrough.
                            let nested = resolve_mapping(&key, arg_ref, ctx, env, schemas);
                            if !nested.is_empty() { out.extend(nested); continue; }
                            None
                        }
                    };
                    if let Some(var) = v {
                        out.push((key, var));
                    } else {
                        ok = false;
                        break;
                    }
                }
                if ok && !out.is_empty() {
                    return out;
                }
            }
        }
    }
    // seq[i] composite: bind element fields under `slot.field_name`.
    if let Expr::Index(seq_expr, idx_expr) = value {
        if let Expr::Identifier(seq_name) = seq_expr.as_ref() {
            if let Some(var) = env.get(seq_name) {
                if let Some((arr, _, _, dt, fields)) = var.as_datatype_seq() {
                    if let Some(i) = translate_int(idx_expr, ctx, env) {
                        let elem_dyn = arr.select(&i);
                        let mut tmp: HashMap<String, Var<'ctx>> = HashMap::new();
                        if bind_composite_fields(&mut tmp, &elem_dyn, fields, dt, slot) {
                            return tmp.into_iter().collect();
                        }
                    }
                }
            }
        }
    }

    // Field(Index(seq,i), field): walk outward to Index root, drill in via composite accessors.
    if matches!(value, Expr::Field(_, _)) {
        let mut path: Vec<String> = Vec::new();
        let mut cur = value;
        let (seq_name, idx_expr) = loop {
            match cur {
                Expr::Field(recv, fname) => {
                    path.push(fname.clone());
                    cur = recv.as_ref();
                }
                Expr::Index(seq, idx) => {
                    if let Expr::Identifier(name) = seq.as_ref() {
                        break (name.clone(), idx.clone());
                    }
                    return Vec::new();
                }
                _ => return Vec::new(),
            }
        };
        path.reverse();
        let Some(out) = resolve_field_chain_to_bindings(
            &seq_name, &idx_expr, &path, slot, ctx, env) else {
            return Vec::new();
        };
        return out;
    }
    if let Some(v) = expr_as_var(value, ctx, env) {
        return vec![(slot.to_string(), v)];
    }
    Vec::new()
}

/// Drill into a `Seq(Composite)` element along a field path, producing bindings under `slot`.
/// Terminates at primitive, nested composite (all leaves via `bind_composite_fields`), or SeqField.
fn resolve_field_chain_to_bindings<'ctx>(
    seq_name: &str,
    idx_expr: &Expr,
    path: &[String],
    slot: &str,
    ctx: &'ctx Context,
    env: &HashMap<String, Var<'ctx>>,
) -> Option<Vec<(String, Var<'ctx>)>> {
    use crate::core::SeqFieldElem;
    let var = env.get(seq_name)?;
    let (arr, _, _, root_dt, root_fields) = var.as_datatype_seq()?;
    let i = translate_int(idx_expr, ctx, env)?;
    let elem_dyn = arr.select(&i);

    let mut cur_dyn = elem_dyn;
    let mut cur_dt: &DatatypeSort = root_dt;
    let mut cur_fields: &[FieldKind] = root_fields;
    for (depth, fname) in path.iter().enumerate() {
        let pos = cur_fields.iter().position(|fk| fk.name() == fname)?;
        let fk = &cur_fields[pos];
        let is_last = depth == path.len() - 1;
        match fk {
            FieldKind::Primitive { prim_type, .. } => {
                if !is_last { return None; }
                if pos >= cur_dt.variants[0].accessors.len() { return None; }
                let raw = cur_dt.variants[0].accessors[pos].apply(
                    &[&cur_dyn.as_datatype()?]);
                let var: Option<Var<'ctx>> = match prim_type.as_str() {
                    "Int" | "Nat" | "Pos" => raw.as_int().map(Var::IntVar),
                    "Bool" => raw.as_bool().map(Var::BoolVar),
                    "String" => raw.as_string().map(Var::StrVar),
                    _ => None,
                };
                return Some(vec![(slot.to_string(), var?)]);
            }
            FieldKind::Nested { dt: nested_dt, sub_fields, .. } => {
                if pos >= cur_dt.variants[0].accessors.len() { return None; }
                let raw = cur_dt.variants[0].accessors[pos].apply(
                    &[&cur_dyn.as_datatype()?]);
                if is_last {
                    let mut tmp: HashMap<String, Var<'ctx>> = HashMap::new();
                    if !bind_composite_fields(&mut tmp, &raw, sub_fields, nested_dt, slot) {
                        return None;
                    }
                    return Some(tmp.into_iter().collect());
                }
                cur_dyn = raw;
                cur_dt = nested_dt;
                cur_fields = sub_fields;
            }
            FieldKind::SeqField { arr_idx, len_idx, elem: seq_elem, .. } => {
                if !is_last { return None; }
                if *len_idx >= cur_dt.variants[0].accessors.len() { return None; }
                let elem_d = cur_dyn.as_datatype()?;
                let arr_d = cur_dt.variants[0].accessors[*arr_idx].apply(&[&elem_d]);
                let len_d = cur_dt.variants[0].accessors[*len_idx].apply(&[&elem_d]);
                let inner_arr = arr_d.as_array()?;
                let inner_len = len_d.as_int()?;
                let var = match seq_elem {
                    SeqFieldElem::Primitive(e) => Var::SeqVar {
                        arr: inner_arr, len: inner_len, elem: *e,
                    },
                    SeqFieldElem::Enum { dt, enum_name } => Var::DatatypeSeqVar {
                        arr: inner_arr, len: inner_len,
                        type_name: enum_name.clone(),
                        dt: *dt, fields: Vec::new(),
                    },
                    SeqFieldElem::Composite { dt, type_name, sub_fields } => Var::DatatypeSeqVar {
                        arr: inner_arr, len: inner_len,
                        type_name: type_name.clone(),
                        dt: *dt, fields: sub_fields.clone(),
                    },
                };
                return Some(vec![(slot.to_string(), var)]);
            }
        }
    }
    None
}

/// Resolve a scalar expression to a single `Var` (leaf case of `resolve_mapping`).
fn expr_as_var<'ctx>(
    e: &Expr,
    ctx: &'ctx Context,
    env: &HashMap<String, Var<'ctx>>,
) -> Option<Var<'ctx>> {
    match e {
        Expr::Identifier(name) => env.get(name).cloned(),
        Expr::Int(n)  => Some(Var::IntVar(Int::from_i64(ctx, *n))),
        Expr::Bool(b) => Some(Var::BoolVar(Bool::from_bool(ctx, *b))),
        Expr::Real(f) => Some(Var::RealVar(real_from_f64(ctx, *f))),
        Expr::Str(s)  => Z3Str::from_str(ctx, s).ok().map(Var::StrVar),
        _ => None,
    }
}
