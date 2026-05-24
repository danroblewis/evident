//! Record / vector lifting. `lift_record_op` broadcasts a comparison or
//! equality over the leaf fields of a record-typed expression; the
//! helpers enumerate a record's leaf paths (`lhs_record_leaves`,
//! `schema_leaf_paths`, `enumerate_nested_leaves`), substitute each leaf
//! into a sub-expression (`substitute_record_refs`), and classify
//! record-reference shapes (`is_field_of_index_record`,
//! `is_seq_element_record`, `collect_record_refs`).

use std::collections::HashMap;
use z3::ast::Bool;
use z3::Context;

use crate::core::ast::*;
use crate::core::{FieldKind, Var};

use super::bool::translate_bool;

/// Field-wise broadcast for `lhs OP rhs` where at least one side is a
/// record reference (Identifier or Field-of-Index) and the operator is
/// any of `=`, `≠`, `<`, `≤`, `>`, `≥`. Either side may be an
/// arithmetic expression involving record references and scalars.
///
/// For each leaf field path of the record's type, we substitute *both*
/// sides by extending every record sub-expression with that leaf path,
/// then translate the per-leaf op. Results fold with `Or` for `≠`
/// (some-field-differs) and `And` for the others (componentwise).
///
/// Supported record reference shapes (anywhere in the expression):
///   - `Identifier(name)` where `name.*` keys exist in env
///   - `Field(Index(Identifier(seq), idx), name)` where `seq` is a
///     `DatatypeSeqVar` whose element type has `name` as Nested
///
/// Other sub-expressions (literals, scalar identifiers like `input.dt`,
/// scalar arithmetic, primitive Seq indexing) pass through unchanged.
///
/// Guards:
///   - At least one side must contain a record reference. `vec = 5`
///     (scalar-only RHS) would otherwise silently broadcast the
///     scalar to every axis, which is almost always a bug.
///   - All record references must have the *same* leaf set
///     (bidirectional shape check). `vec2 = vec3` returns None
///     so the constraint drops with a translator error rather than
///     producing a partial-overlap conjunction.
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
    // Each side must contribute at least one record reference. Without
    // this, `vec = 5` (or `vec ≤ 100`) would broadcast the scalar to
    // every leaf — almost always a bug. Per-side counts also make it
    // clear we're operating on records "all the way through" rather
    // than mixing a record with a scalar at the top level.
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
        // Two records "differ" iff at least one field differs.
        BinOp::Neq => Bool::or(ctx, &refs),
        // =, <, ≤, >, ≥ are all componentwise (all axes must satisfy).
        _ => Bool::and(ctx, &refs),
    })
}

/// Enumerate the leaf field paths of an expression representing a
/// record. Single-level paths (`["x", "y"]` for an IVec2) for
/// flat records; dotted paths (`["pos.x", "pos.y", "color.r", …]`)
/// for records containing sub-records.
fn lhs_record_leaves<'ctx>(
    lhs: &Expr,
    env: &HashMap<String, Var<'ctx>>,
    schemas: &HashMap<String, SchemaDecl>,
) -> Option<Vec<String>> {
    match lhs {
        // Record-literal expression `IVec2(380, 280)` — the leaves come
        // from the type's SchemaDecl, walked recursively for any nested
        // fields the type might have.
        Expr::Call(type_name, _args) => {
            let schema = schemas.get(type_name)?;
            let mut leaves = schema_leaf_paths(schema, schemas);
            if leaves.is_empty() { return None; }
            leaves.sort();
            Some(leaves)
        }
        Expr::Identifier(name) => {
            if env.contains_key(name) { return None; }   // not a record (already a primitive)
            let prefix = format!("{}.", name);
            let mut leaves: Vec<String> = env.keys()
                .filter_map(|k| k.strip_prefix(&prefix).map(String::from))
                .collect();
            if leaves.is_empty() { return None; }
            leaves.sort();
            Some(leaves)
        }
        Expr::Field(receiver, field) => {
            // Field-of-Index path: `seq[i].pos` where pos is a Nested
            // record sub-field. Enumerate the Nested's sub-leaves from
            // the DatatypeSeqVar's field metadata.
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
            // Direct Seq-element record: `output.rects[4] = player_rect`.
            // The element type is the entire DatatypeSeqVar's field
            // shape — every leaf, including those reached through
            // Nested sub-records.
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

/// Recursively walk a `FieldKind` list and produce flat leaf paths.
/// Primitive fields yield their name; Nested fields yield
/// `name.<sub-leaf>` for each sub-leaf in their `sub_fields`.
/// Walk a SchemaDecl and produce flat leaf paths the same way
/// `enumerate_nested_leaves` does for `FieldKind`. Used for
/// `lhs_record_leaves` on `Expr::Call(type, args)` (record literals
/// in expression position) where we don't have the Z3 Datatype yet —
/// just need leaf NAMES for the lift's positional substitution.
///
/// A field whose type appears in `schemas` is treated as nested
/// (recurse into its body). Anything else (primitives, compound
/// types like `Seq(T)`) is treated as a primitive leaf.
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
                // A Seq field doesn't have addressable primitive leaves
                // — accesses are via `field[i]`, not `field.x`. Surface
                // the bare field name so the record-leaf consumers see
                // it (most translate paths skip non-primitive names).
                out.push(name.clone());
            }
        }
    }
    out
}

/// Walk an expression and substitute each record reference with its
/// `.leaf` extension. Scalars and non-record expressions pass through.
/// Returns None on shape mismatch (record reference whose `.leaf`
/// component doesn't exist in env).
fn substitute_record_refs<'ctx>(
    expr: &Expr,
    leaf: &str,
    env: &HashMap<String, Var<'ctx>>,
    schemas: &HashMap<String, SchemaDecl>,
) -> Option<Expr> {
    match expr {
        // Record-literal expression: `IVec2(380, 280)` → 380 for leaf x,
        // 280 for leaf y. Looks up the type's field declaration order
        // in schemas, finds the leaf's first segment in that order,
        // then either returns the matching arg directly (single-level
        // leaf) or recurses into it (multi-level leaf for a nested
        // record).
        Expr::Call(type_name, args) => {
            let schema = schemas.get(type_name)?;
            // Field name + type in declaration order — for nested
            // sub-records this is the field ("pos"), not the sub-leaves.
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
            // Tuple → sub-record coercion: if the arg is a bare
            // `(a, b, c)` AND the field's declared type is a known
            // record schema, treat the tuple as positional args for
            // that schema. Lets the caller write
            //     Rect((220, 40, 40, 255), (0, 432), (640, 48))
            // instead of fully spelling out each ctor.
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
                // Nested leaf: recurse into the arg with the rest of
                // the path. Works for `SDLRect(IVec2(...), IVec2(...),
                // Color(...))` accessed at leaf "pos.x".
                Some(rest_path) => substitute_record_refs(arg_ref, rest_path, env, schemas),
            }
        }
        Expr::Identifier(name) => {
            if env.contains_key(name) {
                // Scalar identifier — leave as-is.
                return Some(expr.clone());
            }
            let prefix = format!("{}.", name);
            if env.keys().any(|k| k.starts_with(&prefix)) {
                // Record identifier — extend with leaf path. Verify
                // the resulting key actually exists; else shape
                // mismatch (e.g. `vec2.r` for a Color leaf).
                let mut extended = name.clone();
                for p in leaf.split('.') {
                    extended.push('.');
                    extended.push_str(p);
                }
                if env.contains_key(&extended) { Some(Expr::Identifier(extended)) }
                else { None }
            } else {
                // Unknown identifier — leave; later translation
                // either resolves it or fails on its own.
                Some(expr.clone())
            }
        }
        Expr::Field(receiver, field) => {
            // Field-of-Index record sub-field? If so, wrap in Fields.
            if is_field_of_index_record(receiver, field, env) {
                let mut result = expr.clone();
                for p in leaf.split('.') {
                    result = Expr::Field(Box::new(result), p.to_string());
                }
                return Some(result);
            }
            // Primitive Field access — leave as-is.
            Some(expr.clone())
        }
        Expr::Index(receiver, _) => {
            // Direct Seq-element record (`output.rects[4] = player_rect`):
            // the indexed element IS a Datatype value. Wrap with Field
            // accesses for each leaf path component so the existing
            // `resolve_seq_field` chain reaches the leaf.
            if is_seq_element_record(receiver, env) {
                let mut result = expr.clone();
                for p in leaf.split('.') {
                    result = Expr::Field(Box::new(result), p.to_string());
                }
                return Some(result);
            }
            // Primitive Seq indexing (e.g. effective_vy[i]) — leave as-is.
            Some(expr.clone())
        }
        Expr::Binary(op, a, b) => {
            let a2 = substitute_record_refs(a, leaf, env, schemas)?;
            let b2 = substitute_record_refs(b, leaf, env, schemas)?;
            Some(Expr::Binary(op.clone(), Box::new(a2), Box::new(b2)))
        }
        Expr::Not(x) => substitute_record_refs(x, leaf, env, schemas).map(|y| Expr::Not(Box::new(y))),
        // Literals, etc.: scalar values, leave as-is.
        _ => Some(expr.clone()),
    }
}

/// True if `Field(receiver, field)` resolves to a record-typed
/// sub-field of a Seq element (e.g. `state.dots[i].pos`). Drives both
/// LHS leaf enumeration and RHS substitution.
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

/// True if `Index(receiver, _)` indexes into a `Seq(UserType)` whose
/// element is a Datatype record (e.g. `output.rects[4]` returns an
/// SDLRect value). Drives `output.rects[4] = player_rect` lifting.
fn is_seq_element_record<'ctx>(
    receiver: &Expr,
    env: &HashMap<String, Var<'ctx>>,
) -> bool {
    let Expr::Identifier(seq_name) = receiver else { return false };
    matches!(env.get(seq_name), Some(Var::DatatypeSeqVar { .. }))
}

/// Walk `expr` and collect every record-reference sub-expression
/// (bare-identifier records and Field-of-Index records). Used by
/// `lift_record_assignment` to verify each RHS record has the same
/// leaf shape as the LHS — without this check, `vec2 = vec3` would
/// produce a partial-overlap broadcast over the LHS's leaves only.
fn collect_record_refs<'ctx>(
    expr: &Expr,
    env: &HashMap<String, Var<'ctx>>,
    schemas: &HashMap<String, SchemaDecl>,
    out: &mut Vec<Expr>,
) {
    match expr {
        // Record literal `IVec2(380, 280)` IS a record reference.
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
