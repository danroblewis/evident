//! Seq field resolution. `SeqHandleRef` is the uniform handle for a Seq
//! value reachable from various expression shapes; `resolve_seq_handle`
//! produces one from an Expr; `resolve_seq_field` drills a dotted field
//! chain into a `Seq(Composite)` element to reach a primitive leaf.

use std::collections::HashMap;
use z3::ast::Int;
use z3::{Context, DatatypeSort};

use crate::core::ast::*;
use crate::core::{FieldKind, SeqElem, Var};

use super::scalar::translate_int;

/// Internal handle for a Seq value reachable from various expression
/// shapes — used by the Index / Cardinality / ∀ paths to consume seqs
/// uniformly whether the source is a top-level binding or a SeqField on
/// a composite-Seq element.
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
    pub(super) fn arr(&self) -> &z3::ast::Array<'ctx> {
        match self {
            SeqHandleRef::Primitive { arr, .. } => arr,
            SeqHandleRef::Composite { arr, .. } => arr,
        }
    }
    pub(super) fn len(&self) -> &Int<'ctx> {
        match self {
            SeqHandleRef::Primitive { len, .. } => len,
            SeqHandleRef::Composite { len, .. } => len,
        }
    }
}

/// Resolve an Expr to a `SeqHandleRef` — the (arr, len, elem info) for
/// the Seq it names. Handles two shapes:
///
///   * `Identifier(name)` resolving to `Var::SeqVar` / `DatatypeSeqVar`
///     (the top-level Seq binding case; covers `s.rects` since the
///     parser folds dotted names into a single Identifier).
///   * `Field(Index(Identifier(outer), idx), seq_field_name)` where
///     `outer` is a `DatatypeSeqVar` whose element type has
///     `seq_field_name` as a `FieldKind::SeqField` — i.e., reaching
///     into a Seq-typed field of a Seq element (the Seq-of-Seq
///     unlocking case).
///
/// Returns None when neither shape applies. Recursion into deeper
/// `Field(Field(...), ...)` chains over composite-with-Seq-field
/// elements is supported but rare; the immediate Mario use case stays
/// one level deep.
pub(super) fn resolve_seq_handle<'ctx>(
    expr: &Expr,
    ctx: &'ctx Context,
    env: &HashMap<String, Var<'ctx>>,
) -> Option<SeqHandleRef<'ctx>> {
    use crate::core::SeqFieldElem;
    // Shape 1: bare Identifier — env lookup.
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
    // Shape 2: Field(Index(Identifier(outer), idx), seq_field_name)
    let Expr::Field(receiver, field_name) = expr else { return None };
    let Expr::Index(seq_expr, idx_expr) = receiver.as_ref() else { return None };
    let Expr::Identifier(outer_name) = seq_expr.as_ref() else { return None };
    let var = env.get(outer_name)?;
    let (arr, _, _, dt, fields) = var.as_datatype_seq()?;
    let i = translate_int(idx_expr, ctx, env)?;
    let elem_dyn = arr.select(&i);
    let elem = elem_dyn.as_datatype()?;

    // Find the named field; must be a SeqField.
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


/// Resolve a (possibly-nested) field access chain against a
/// `DatatypeSeqVar` in the env. Two shapes:
///
///   `Field(Index(Identifier(seq_name), idx_expr), field_name)` —
///       direct primitive field of a `Seq(UserType)` element.
///       Returns the field's primitive `Dynamic` and its type name.
///
///   `Field(Field(... , inner_field), leaf_field)` (recursively) —
///       nested field of a composite element field. Walks the chain
///       outward-in: bottom of the chain is the same Index pattern,
///       each enclosing `Field` peels another level by applying the
///       nested type's accessor and threading the new (dt, fields)
///       pair down the recursion.
///
/// Returns the raw `Dynamic` for the final leaf field plus the
/// primitive type name ("Int" / "Nat" / "Pos" / "Bool" / "String") so
/// the caller can route through `as_int` / `as_bool` / `as_string`.
pub(super) fn resolve_seq_field<'ctx>(
    field_expr: &Expr,
    ctx: &'ctx Context,
    env: &HashMap<String, Var<'ctx>>,
) -> Option<(z3::ast::Dynamic<'ctx>, String)> {
    // Decompose the chain. `outer_path` is the leaf-to-root list of
    // field names; the receiver at the bottom of the chain must be
    // the `Index(Identifier(seq_name), idx_expr)` pattern.
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
    // path is leaf-first; reverse to get root-to-leaf so we can apply
    // accessors in forward order (outer composite → inner field → ...).
    path.reverse();
    if path.is_empty() { return None; }

    let var = env.get(seq_name)?;
    let (arr, _, _, root_dt, root_fields) = var.as_datatype_seq()?;
    let i = translate_int(idx_expr, ctx, env)?;
    let elem_dyn = arr.select(&i);
    let mut cur_dyn = elem_dyn;

    // Walk the field chain. At each step we're at a Datatype value
    // (`cur_dyn`); look up the field in the current `(dt, fields)`
    // pair, apply its accessor, and either return (if it's a
    // primitive leaf) or recurse with the nested `(dt, sub_fields)`.
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
                if !is_last {
                    // Trying to index into a primitive — programmer error.
                    return None;
                }
                return Some((raw, prim_type.clone()));
            }
            FieldKind::Nested { dt: nested_dt, sub_fields, .. } => {
                if is_last {
                    // The chain ends on a composite — translators only
                    // know how to consume primitive leaves, so signal
                    // "no leaf primitive" by returning None. Composite
                    // values aren't first-class in Evident expressions
                    // (you always reach into one of their fields).
                    return None;
                }
                cur_dt = nested_dt;
                cur_fields = sub_fields.as_slice();
                cur_dyn = raw;
            }
            FieldKind::SeqField { .. } => {
                // Reaching a Seq-typed field by dotted access doesn't
                // produce a primitive leaf — the caller would need to
                // index into it (`field[i]`) to reach a scalar. Signal
                // "no leaf primitive" the same way Nested does.
                return None;
            }
        }
    }
    None
}
