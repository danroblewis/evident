//! Record / vector lifting: broadcasts comparisons/equality over leaf fields of a record expression.

use std::collections::HashMap;
use z3::ast::Bool;
use z3::Context;

use crate::core::ast::*;
use crate::core::{FieldKind, Var};

use super::bool::translate_bool;

/// Broadcast `lhs OP rhs` field-wise over record leaves (=,≠,<,≤,>,≥); both sides must contain
/// at least one record ref with the same leaf shape, else returns None. `≠` folds with Or.
pub(super) fn lift_record_op<'ctx>(
    op: &BinOp,
    lhs: &Expr,
    rhs: &Expr,
    ctx: &'ctx Context,
    env: &HashMap<String, Var<'ctx>>,
    schemas: &HashMap<String, SchemaDecl>,
) -> Option<Bool<'ctx>> {
    if !matches!(op,
        BinOp::Eq | BinOp::Neq | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge
    ) {
        return None;
    }
    let mut lhs_records = Vec::new();
    let mut rhs_records = Vec::new();
    collect_record_refs(lhs, env, schemas, &mut lhs_records);
    collect_record_refs(rhs, env, schemas, &mut rhs_records);
    if lhs_records.is_empty() || rhs_records.is_empty() { return None; }
    let mut all_records = lhs_records;
    all_records.extend(rhs_records);

    // All record references must share the same leaf shape.
    let leaves = lhs_record_leaves(&all_records[0], env, schemas)?;
    for rec in all_records.iter().skip(1) {
        let rec_leaves = lhs_record_leaves(rec, env, schemas)?;
        if rec_leaves != leaves { return None; }
    }

    let mut clauses = Vec::with_capacity(leaves.len());
    for leaf in &leaves {
        let lhs_leaf = substitute_record_refs(lhs, leaf, env, schemas)?;
        let rhs_leaf = substitute_record_refs(rhs, leaf, env, schemas)?;
        let leaf_op = Expr::Binary(
            op.clone(),
            Box::new(lhs_leaf),
            Box::new(rhs_leaf),
        );
        clauses.push(translate_bool(&leaf_op, ctx, env, schemas)?);
    }
    let refs: Vec<&Bool> = clauses.iter().collect();
    Some(match op {
        BinOp::Neq => Bool::or(ctx, &refs),
        _ => Bool::and(ctx, &refs),
    })
}

/// Enumerate leaf field paths for a record expression (single-level or dotted for nested records).
fn lhs_record_leaves<'ctx>(
    lhs: &Expr,
    env: &HashMap<String, Var<'ctx>>,
    schemas: &HashMap<String, SchemaDecl>,
) -> Option<Vec<String>> {
    match lhs {
        // Record literal `IVec2(380, 280)`: leaves from SchemaDecl, recursing into nested fields.
        Expr::Call(type_name, _args) => {
            let schema = schemas.get(type_name)?;
            let mut leaves = schema_leaf_paths(schema, schemas);
            if leaves.is_empty() { return None; }
            leaves.sort();
            Some(leaves)
        }
        Expr::Identifier(name) => {
            if env.contains_key(name) { return None; }   // primitive, not a record
            let prefix = format!("{}.", name);
            let mut leaves: Vec<String> = env.keys()
                .filter_map(|k| k.strip_prefix(&prefix).map(String::from))
                .collect();
            if leaves.is_empty() { return None; }
            leaves.sort();
            Some(leaves)
        }
        Expr::Field(receiver, field) => {
            // `seq[i].pos` where pos is a Nested record sub-field.
            let Expr::Index(seq_expr, _) = receiver.as_ref() else { return None };
            let Expr::Identifier(seq_name) = seq_expr.as_ref() else { return None };
            let Some(Var::DatatypeSeqVar { fields, .. }) = env.get(seq_name) else { return None };
            let nested_sub = fields.iter().find_map(|f| match f {
                FieldKind::Nested { name, sub_fields, .. } if name == field => Some(sub_fields),
                _ => None,
            })?;
            let mut leaves = enumerate_nested_leaves(nested_sub);
            if leaves.is_empty() { return None; }
            leaves.sort();
            Some(leaves)
        }
        Expr::Index(receiver, _) => {
            // Direct Seq-element record (`output.rects[4]`): all leaves of the element type.
            let Expr::Identifier(seq_name) = receiver.as_ref() else { return None };
            let Some(Var::DatatypeSeqVar { fields, .. }) = env.get(seq_name) else { return None };
            let mut leaves = enumerate_nested_leaves(fields);
            if leaves.is_empty() { return None; }
            leaves.sort();
            Some(leaves)
        }
        _ => None,
    }
}

/// Walk a SchemaDecl and produce flat leaf paths; recurses into fields whose type is in `schemas`.
fn schema_leaf_paths(
    schema: &SchemaDecl,
    schemas: &HashMap<String, SchemaDecl>,
) -> Vec<String> {
    let mut out = Vec::new();
    for item in &schema.body {
        if let BodyItem::Membership { name, type_name, .. } = item {
            if let Some(sub) = schemas.get(type_name) {
                for leaf in schema_leaf_paths(sub, schemas) {
                    out.push(format!("{}.{}", name, leaf));
                }
            } else {
                out.push(name.clone());
            }
        }
    }
    out
}

fn enumerate_nested_leaves(fields: &[FieldKind]) -> Vec<String> {
    let mut out = Vec::new();
    for f in fields {
        match f {
            FieldKind::Primitive { name, .. } => out.push(name.clone()),
            FieldKind::Nested { name, sub_fields, .. } => {
                for sub in enumerate_nested_leaves(sub_fields) {
                    out.push(format!("{}.{}", name, sub));
                }
            }
            FieldKind::SeqField { name, .. } => {
                // Seq fields have no primitive leaves; surface bare name for consumers that skip it.
                out.push(name.clone());
            }
        }
    }
    out
}

/// Substitute each record reference in `expr` with its `.leaf` extension; scalars pass through.
fn substitute_record_refs<'ctx>(
    expr: &Expr,
    leaf: &str,
    env: &HashMap<String, Var<'ctx>>,
    schemas: &HashMap<String, SchemaDecl>,
) -> Option<Expr> {
    match expr {
        // Record literal `IVec2(380, 280)`: find the leaf's position in declaration order,
        // return the arg directly (single-level) or recurse (multi-level / nested).
        Expr::Call(type_name, args) => {
            let schema = schemas.get(type_name)?;
            let fields: Vec<(&str, &str)> = schema.body.iter()
                .filter_map(|item| match item {
                    BodyItem::Membership { name, type_name, .. } =>
                        Some((name.as_str(), type_name.as_str())),
                    _ => None,
                })
                .collect();
            // Split the leaf into its first segment (which is one of
            // `fields`) and the remainder.
            let (first, rest) = match leaf.split_once('.') {
                Some((a, b)) => (a, Some(b)),
                None => (leaf, None),
            };
            let pos = fields.iter().position(|(n, _)| *n == first)?;
            if pos >= args.len() { return None; }
            // Coerce bare tuple `(a, b, c)` to `TypeName(a, b, c)` when the field type is a known record.
            let coerced: Expr;
            let arg_ref: &Expr = match &args[pos] {
                Expr::Tuple(items) if schemas.contains_key(fields[pos].1) => {
                    coerced = Expr::Call(fields[pos].1.to_string(), items.clone());
                    &coerced
                }
                other => other,
            };
            match rest {
                None => Some(arg_ref.clone()),
                Some(rest_path) => substitute_record_refs(arg_ref, rest_path, env, schemas),
            }
        }
        Expr::Identifier(name) => {
            if env.contains_key(name) { return Some(expr.clone()); }   // scalar
            let prefix = format!("{}.", name);
            if env.keys().any(|k| k.starts_with(&prefix)) {
                let mut extended = name.clone();
                for p in leaf.split('.') { extended.push('.'); extended.push_str(p); }
                if env.contains_key(&extended) { Some(Expr::Identifier(extended)) } else { None }
            } else {
                Some(expr.clone())   // unknown — leave for later translation
            }
        }
        Expr::Field(receiver, field) => {
            if is_field_of_index_record(receiver, field, env) {
                let mut result = expr.clone();
                for p in leaf.split('.') {
                    result = Expr::Field(Box::new(result), p.to_string());
                }
                return Some(result);
            }
            Some(expr.clone())   // primitive field access
        }
        Expr::Index(receiver, _) => {
            // Seq-element record (`output.rects[4]`): wrap with Field accesses per leaf component.
            if is_seq_element_record(receiver, env) {
                let mut result = expr.clone();
                for p in leaf.split('.') {
                    result = Expr::Field(Box::new(result), p.to_string());
                }
                return Some(result);
            }
            Some(expr.clone())   // primitive Seq indexing
        }
        Expr::Binary(op, a, b) => {
            let a2 = substitute_record_refs(a, leaf, env, schemas)?;
            let b2 = substitute_record_refs(b, leaf, env, schemas)?;
            Some(Expr::Binary(op.clone(), Box::new(a2), Box::new(b2)))
        }
        Expr::Not(x) => substitute_record_refs(x, leaf, env, schemas).map(|y| Expr::Not(Box::new(y))),
        _ => Some(expr.clone()),
    }
}

/// True if `Field(receiver, field)` is a record-typed sub-field of a Seq element (e.g. `dots[i].pos`).
fn is_field_of_index_record<'ctx>(
    receiver: &Expr,
    field: &str,
    env: &HashMap<String, Var<'ctx>>,
) -> bool {
    let Expr::Index(seq_expr, _) = receiver else { return false };
    let Expr::Identifier(seq_name) = seq_expr.as_ref() else { return false };
    let Some(Var::DatatypeSeqVar { fields, .. }) = env.get(seq_name) else { return false };
    fields.iter().any(|f| matches!(f, FieldKind::Nested { name, .. } if name == field))
}

/// True if `Index(receiver, _)` indexes into a `Seq(UserType)` (Datatype element).
fn is_seq_element_record<'ctx>(
    receiver: &Expr,
    env: &HashMap<String, Var<'ctx>>,
) -> bool {
    let Expr::Identifier(seq_name) = receiver else { return false };
    matches!(env.get(seq_name), Some(Var::DatatypeSeqVar { .. }))
}

/// Collect all record-reference sub-expressions in `expr` (bare-identifier records, Field-of-Index).
fn collect_record_refs<'ctx>(
    expr: &Expr,
    env: &HashMap<String, Var<'ctx>>,
    schemas: &HashMap<String, SchemaDecl>,
    out: &mut Vec<Expr>,
) {
    match expr {
        Expr::Call(type_name, _) if schemas.contains_key(type_name) => {
            out.push(expr.clone());
        }
        Expr::Identifier(name) => {
            if !env.contains_key(name)
                && env.keys().any(|k| k.starts_with(&format!("{}.", name)))
            {
                out.push(expr.clone());
            }
        }
        Expr::Field(receiver, field) => {
            if is_field_of_index_record(receiver, field, env) {
                out.push(expr.clone());
            }
        }
        Expr::Index(receiver, _) => {
            if is_seq_element_record(receiver, env) {
                out.push(expr.clone());
            }
        }
        Expr::Binary(_, a, b) => {
            collect_record_refs(a, env, schemas, out);
            collect_record_refs(b, env, schemas, out);
        }
        Expr::Not(x) => collect_record_refs(x, env, schemas, out),
        _ => {}
    }
}
