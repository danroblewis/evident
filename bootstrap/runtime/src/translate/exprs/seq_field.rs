//! Seq field resolution: uniform handle + resolver for Seq values reachable
//! from Identifiers, Index, and Field chains.

use std::collections::HashMap;
use z3::ast::Int;
use z3::{Context, DatatypeSort};

use crate::core::ast::*;
use crate::core::{FieldKind, SeqElem, Var};

use super::scalar::translate_int;

/// Uniform Seq handle used by Index / Cardinality / ∀ paths; covers both
/// top-level bindings and SeqField-on-composite-element shapes.
pub(super) enum SeqHandleRef<'ctx> {
    Primitive {
        arr: z3::ast::Array<'ctx>,
        len: Int<'ctx>,
        elem: SeqElem,
    },
    Composite {
        arr: z3::ast::Array<'ctx>,
        len: Int<'ctx>,
        #[allow(dead_code)]
        type_name: String,
        dt: &'static DatatypeSort<'static>,
        fields: Vec<FieldKind>,
    },
}

impl<'ctx> SeqHandleRef<'ctx> {
    pub(super) fn len(&self) -> &Int<'ctx> {
        match self {
            SeqHandleRef::Primitive { len, .. } => len,
            SeqHandleRef::Composite { len, .. } => len,
        }
    }
}

/// Resolve an Expr to a `SeqHandleRef` (bare Identifier or
/// `Field(Index(Identifier, idx), seq_field)`). Returns None otherwise.
pub(super) fn resolve_seq_handle<'ctx>(
    expr: &Expr,
    ctx: &'ctx Context,
    env: &HashMap<String, Var<'ctx>>,
) -> Option<SeqHandleRef<'ctx>> {
    use crate::core::SeqFieldElem;
    if let Expr::Identifier(name) = expr {
        if let Some(var) = env.get(name) {
            if let Some((arr, len, elem)) = var.as_seq() {
                return Some(SeqHandleRef::Primitive {
                    arr: arr.clone(), len: len.clone(), elem,
                });
            }
            if let Some((arr, len, type_name, dt, fields)) = var.as_datatype_seq() {
                return Some(SeqHandleRef::Composite {
                    arr: arr.clone(), len: len.clone(),
                    type_name: type_name.to_string(),
                    dt, fields: fields.to_vec(),
                });
            }
        }
        return None;
    }
    let Expr::Field(receiver, field_name) = expr else { return None };
    let Expr::Index(seq_expr, idx_expr) = receiver.as_ref() else { return None };
    let Expr::Identifier(outer_name) = seq_expr.as_ref() else { return None };
    let var = env.get(outer_name)?;
    let (arr, _, _, dt, fields) = var.as_datatype_seq()?;
    let i = translate_int(idx_expr, ctx, env)?;
    let elem_dyn = arr.select(&i);
    let elem = elem_dyn.as_datatype()?;

    let fk = fields.iter().find(|f| f.name() == field_name)?;
    let FieldKind::SeqField { arr_idx, len_idx, elem: seq_elem, .. } = fk else {
        return None;
    };
    if *len_idx >= dt.variants[0].accessors.len() { return None; }
    let inner_arr_dyn = dt.variants[0].accessors[*arr_idx].apply(&[&elem]);
    let inner_len_dyn = dt.variants[0].accessors[*len_idx].apply(&[&elem]);
    let inner_arr = inner_arr_dyn.as_array()?;
    let inner_len = inner_len_dyn.as_int()?;
    match seq_elem {
        SeqFieldElem::Primitive(e) => Some(SeqHandleRef::Primitive {
            arr: inner_arr, len: inner_len, elem: *e,
        }),
        SeqFieldElem::Enum { dt, enum_name } => Some(SeqHandleRef::Composite {
            arr: inner_arr, len: inner_len,
            type_name: enum_name.clone(),
            dt: *dt, fields: Vec::new(),    // enum-element marker
        }),
        SeqFieldElem::Composite { dt, type_name, sub_fields } => Some(SeqHandleRef::Composite {
            arr: inner_arr, len: inner_len,
            type_name: type_name.clone(),
            dt: *dt, fields: sub_fields.clone(),
        }),
    }
}


/// Resolve `seq[idx].field…` to a primitive Z3 Dynamic + type name.
/// Returns None when the chain doesn't bottom out at a primitive leaf.
pub(super) fn resolve_seq_field<'ctx>(
    field_expr: &Expr,
    ctx: &'ctx Context,
    env: &HashMap<String, Var<'ctx>>,
) -> Option<(z3::ast::Dynamic<'ctx>, String)> {
    let mut path: Vec<&str> = Vec::new();
    let mut cur = field_expr;
    let (seq_name, idx_expr) = loop {
        match cur {
            Expr::Field(receiver, field_name) => {
                path.push(field_name.as_str());
                cur = receiver.as_ref();
            }
            Expr::Index(seq_expr, idx_expr) => {
                let Expr::Identifier(seq_name) = seq_expr.as_ref() else { return None };
                break (seq_name.as_str(), idx_expr.as_ref());
            }
            _ => return None,
        }
    };
    // path is leaf-first; reverse so accessors apply root-to-leaf.
    path.reverse();
    if path.is_empty() { return None; }

    let var = env.get(seq_name)?;
    let (arr, _, _, root_dt, root_fields) = var.as_datatype_seq()?;
    let i = translate_int(idx_expr, ctx, env)?;
    let elem_dyn = arr.select(&i);
    let mut cur_dyn = elem_dyn;

    let mut cur_dt: &DatatypeSort = root_dt;
    let mut cur_fields: &[FieldKind] = root_fields;
    for (depth, fname) in path.iter().enumerate() {
        let field_idx = cur_fields.iter().position(|fk| fk.name() == *fname)?;
        if field_idx >= cur_dt.variants[0].accessors.len() { return None; }
        let elem = cur_dyn.as_datatype()?;
        let raw = cur_dt.variants[0].accessors[field_idx].apply(&[&elem]);
        let is_last = depth == path.len() - 1;
        match &cur_fields[field_idx] {
            FieldKind::Primitive { prim_type, .. } => {
                if !is_last { return None; } // can't index into a primitive
                return Some((raw, prim_type.clone()));
            }
            FieldKind::Nested { dt: nested_dt, sub_fields, .. } => {
                if is_last { return None; } // composite isn't a primitive leaf
                cur_dt = nested_dt;
                cur_fields = sub_fields.as_slice();
                cur_dyn = raw;
            }
            FieldKind::SeqField { .. } => {
                // Seq field needs further indexing ([i]) to reach a scalar.
                return None;
            }
        }
    }
    None
}
