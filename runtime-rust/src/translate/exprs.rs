//! AST `Expr` → Z3 expression translators (Int / Bool / String) and
//! the helpers they share. Also `resolve_mapping` / `expr_as_var` for
//! `ClaimCall` mapping resolution; `translate_seq_lit_eq` and
//! `translate_seq_index_assign` for the two seq-equality shapes that
//! aren't pure scalar `_eq`.

use std::collections::HashMap;
use z3::ast::{Ast, Bool, Int, Real, String as Z3Str};
use z3::{Context, DatatypeSort};

use crate::ast::*;
use super::types::{FieldKind, SeqElem, Var};
use super::preprocess::{env_clone, literal_range};

/// Resolve a mapping-value expression to one-or-more `(env-key, Var)`
/// bindings to install in the inner env when entering a ClaimCall.
///
/// Three resolution paths, tried in order:
///   1. Sub-schema mapping: the value is a dotted identifier (e.g.
///      `state.player`) AND no env binding exists for that exact name,
///      but multiple env keys share it as a prefix (`state.player.x`,
///      `state.player.y`, …). Each matched leaf is bound under
///      `slot.field`. This matches the Python translator's behavior
///      for `state mapsto state.player`.
///   2. Leaf identifier or literal: `expr_as_var` produces a single
///      `Var`, bound to `slot` directly.
///   3. Otherwise → empty (caller logs a warning).
pub(super) fn resolve_mapping<'ctx>(
    slot: &str,
    value: &Expr,
    ctx: &'ctx Context,
    env: &HashMap<String, Var<'ctx>>,
) -> Vec<(String, Var<'ctx>)> {
    if let Expr::Identifier(name) = value {
        // If the exact name is in env, prefer leaf binding.
        if env.contains_key(name) {
            return vec![(slot.to_string(), env[name].clone())];
        }
        // Otherwise try sub-schema expansion: gather every env key
        // beginning with `name.` and re-key under `slot.field`.
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
    if let Some(v) = expr_as_var(value, ctx, env) {
        return vec![(slot.to_string(), v)];
    }
    Vec::new()
}

/// Resolve a leaf expression to a single `Var`. Used both for ClaimCall
/// scalar mappings and as the tail-case of `resolve_mapping`.
fn expr_as_var<'ctx>(
    e: &Expr,
    ctx: &'ctx Context,
    env: &HashMap<String, Var<'ctx>>,
) -> Option<Var<'ctx>> {
    match e {
        Expr::Identifier(name) => env.get(name).cloned(),
        Expr::Int(n)  => Some(Var::IntVar(Int::from_i64(ctx, *n))),
        Expr::Bool(b) => Some(Var::BoolVar(Bool::from_bool(ctx, *b))),
        Expr::Str(s)  => Z3Str::from_str(ctx, s).ok().map(Var::StrVar),
        _ => None,
    }
}

/// Resolve an expression to an enum-typed Z3 Datatype AST. Three shapes:
///
///   * `Identifier(name)` where env has `EnumVar` — the user's `today`
///   * `Identifier(name)` where env has `EnumValue` — bare nullary
///     variant identifier like `Mon`
///   * `Call(name, args)` where env has `EnumCtor` — payload variant
///     constructor application like `Ok(5)` or `Cons(7, Nil)`
///
/// For Call: each arg is translated against the constructor's declared
/// field type. Recursive payloads (a field whose type is the enum
/// itself, e.g. `LinkedList`) recurse through `resolve_enum_ast` again.
/// Arity mismatches and per-field type translation failures return None
/// (the calling expression then drops as untranslatable).
pub(super) fn resolve_enum_ast<'ctx>(
    e: &Expr,
    ctx: &'ctx Context,
    env: &HashMap<String, Var<'ctx>>,
    schemas: &HashMap<String, SchemaDecl>,
) -> Option<z3::ast::Datatype<'ctx>> {
    match e {
        Expr::Identifier(name) => match env.get(name)? {
            Var::EnumVar { ast, .. }   => Some(ast.clone()),
            Var::EnumValue { ast, .. } => Some(ast.clone()),
            _ => None,
        },
        Expr::Call(name, args) => {
            let ctor_info = env.get(name)?;
            let (dt, variant_idx, field_types) = match ctor_info {
                Var::EnumCtor { dt, variant_idx, field_types, .. } =>
                    (*dt, *variant_idx, field_types.clone()),
                _ => return None,
            };
            if args.len() != field_types.len() { return None; }
            let ctor = &dt.variants[variant_idx].constructor;
            // Translate each arg against its declared field type. We
            // need a Vec<Box<dyn Ast>> kind of structure to call
            // ctor.apply, but z3-rs uses `&[&dyn Ast]`. Build the
            // typed Vec then borrow.
            let mut owned_args: Vec<Box<dyn z3::ast::Ast<'ctx>>> = Vec::new();
            for (arg_expr, field_type) in args.iter().zip(field_types.iter()) {
                let v: Box<dyn z3::ast::Ast<'ctx>> = match field_type.as_str() {
                    "Int" | "Nat" | "Pos" =>
                        Box::new(translate_int(arg_expr, ctx, env)?),
                    "Bool" =>
                        Box::new(translate_bool(arg_expr, ctx, env, schemas)?),
                    "String" =>
                        Box::new(translate_str(arg_expr, ctx, env)?),
                    "Real" =>
                        Box::new(translate_real(arg_expr, ctx, env)?),
                    _ => {
                        // Either a self-reference or another enum.
                        // Recurse via resolve_enum_ast.
                        Box::new(resolve_enum_ast(arg_expr, ctx, env, schemas)?)
                    }
                };
                owned_args.push(v);
            }
            let arg_refs: Vec<&dyn z3::ast::Ast<'ctx>> =
                owned_args.iter().map(|b| b.as_ref()).collect();
            ctor.apply(&arg_refs).as_datatype()
        }
        _ => None,
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
fn resolve_seq_field<'ctx>(
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
        }
    }
    None
}

pub(super) fn translate_str<'ctx>(e: &Expr, ctx: &'ctx Context, env: &HashMap<String, Var<'ctx>>) -> Option<Z3Str<'ctx>> {
    match e {
        Expr::Str(s) => Z3Str::from_str(ctx, s).ok(),
        Expr::Identifier(name) => env.get(name).and_then(|v| v.as_str().cloned()),
        // `lhs ++ rhs` — string concatenation. Both operands must translate
        // as strings; the result is a Z3 string concat.
        Expr::Binary(BinOp::Concat, lhs, rhs) => {
            let l = translate_str(lhs, ctx, env)?;
            let r = translate_str(rhs, ctx, env)?;
            Some(Z3Str::concat(ctx, &[&l, &r]))
        }
        // `seq[i]` where seq holds String elements.
        Expr::Index(seq_expr, idx_expr) => {
            let name = match seq_expr.as_ref() {
                Expr::Identifier(n) => n,
                _ => return None,
            };
            let (arr, _, elem) = env.get(name)?.as_seq()?;
            if elem != SeqElem::Str { return None; }
            let i = translate_int(idx_expr, ctx, env)?;
            arr.select(&i).as_string()
        }
        // `pts[i].name` where pts is Seq(UserType) and `name` is a
        // String field of UserType.
        Expr::Field(_, _) => {
            let (raw, ftype) = resolve_seq_field(e, ctx, env)?;
            if ftype == "String" {
                raw.as_string()
            } else {
                None
            }
        }
        _ => None,
    }
}

pub(super) fn translate_int<'ctx>(e: &Expr, ctx: &'ctx Context, env: &HashMap<String, Var<'ctx>>) -> Option<Int<'ctx>> {
    match e {
        Expr::Int(n) => Some(Int::from_i64(ctx, *n)),
        Expr::Identifier(name) => match env.get(name) {
            Some(Var::IntVar(i)) => Some(i.clone()),
            Some(Var::PinnedInt(v)) => Some(Int::from_i64(ctx, *v)),
            _ => None,
        },
        Expr::Binary(op, lhs, rhs) => {
            let l = translate_int(lhs, ctx, env)?;
            let r = translate_int(rhs, ctx, env)?;
            Some(match op {
                BinOp::Add => Int::add(ctx, &[&l, &r]),
                BinOp::Sub => Int::sub(ctx, &[&l, &r]),
                BinOp::Mul => Int::mul(ctx, &[&l, &r]),
                BinOp::Div => l.div(&r),
                _ => return None,
            })
        }
        // `#seq` → the seq's length variable. Both primitive Seq and
        // composite-element Seq (DatatypeSeqVar) expose a length.
        Expr::Cardinality(inner) => {
            if let Expr::Identifier(name) = inner.as_ref() {
                if let Some(var) = env.get(name) {
                    if let Some((_, len, _)) = var.as_seq() {
                        return Some(len.clone());
                    }
                    if let Some((_, len, _, _, _)) = var.as_datatype_seq() {
                        return Some(len.clone());
                    }
                }
            }
            None
        }
        // `seq[i]` where seq holds Int elements → Array.select(i) → Int.
        Expr::Index(seq_expr, idx_expr) => {
            let name = match seq_expr.as_ref() {
                Expr::Identifier(n) => n,
                _ => return None,
            };
            let (arr, _, elem) = env.get(name)?.as_seq()?;
            if elem != SeqElem::Int { return None; }
            let i = translate_int(idx_expr, ctx, env)?;
            arr.select(&i).as_int()
        }
        // `pts[i].x` where pts is Seq(UserType) and `x` is an Int field.
        Expr::Field(_, _) => {
            let (raw, ftype) = resolve_seq_field(e, ctx, env)?;
            if matches!(ftype.as_str(), "Int" | "Nat" | "Pos") {
                raw.as_int()
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Translate an Expr that should evaluate to a Z3 Real. Mirrors
/// `translate_int` for the Real domain. Supports:
///   - Real literals (`3.14`)
///   - Identifier resolving to `Var::RealVar`
///   - Binary arithmetic (`+`, `-`, `*`, `/`) with operands that
///     translate as Real OR can be coerced from Int (Z3 supports
///     mixed Int/Real arithmetic by lifting Int to Real).
///   - Unary minus via `0 - e` desugaring (parser does this already).
/// Returns None if the expression doesn't fit any of these patterns —
/// caller (typically `translate_bool`'s Eq/comparison arms) tries
/// other type paths.
pub(super) fn translate_real<'ctx>(e: &Expr, ctx: &'ctx Context, env: &HashMap<String, Var<'ctx>>) -> Option<Real<'ctx>> {
    match e {
        Expr::Real(f) => Some(real_from_f64(ctx, *f)),
        Expr::Int(n)  => Some(Real::from_int(&Int::from_i64(ctx, *n))),  // numeric literal coercion
        Expr::Identifier(name) => match env.get(name) {
            Some(Var::RealVar(r)) => Some(r.clone()),
            Some(Var::IntVar(i))  => Some(Real::from_int(i)),     // promote int var
            Some(Var::PinnedInt(v)) => Some(Real::from_int(&Int::from_i64(ctx, *v))),
            _ => None,
        },
        Expr::Binary(op, lhs, rhs) => {
            let l = translate_real(lhs, ctx, env)?;
            let r = translate_real(rhs, ctx, env)?;
            Some(match op {
                BinOp::Add => Real::add(ctx, &[&l, &r]),
                BinOp::Sub => Real::sub(ctx, &[&l, &r]),
                BinOp::Mul => Real::mul(ctx, &[&l, &r]),
                BinOp::Div => l.div(&r),
                _ => return None,
            })
        }
        _ => None,
    }
}

/// Local copy of the Real-from-f64 helper. Same shape as the one in
/// `eval.rs` (private there); duplicated to avoid a cross-module
/// dependency for one tiny helper.
///
/// Splits f64's Display form (`"3.14"`) into pure-integer num/den
/// (`"314" / "100"`) so Z3's numeral parser only sees integers.
/// Z3's parser is finicky about decimals embedded in `"num/den"`.
fn real_from_f64<'ctx>(ctx: &'ctx Context, f: f64) -> Real<'ctx> {
    if f.is_nan() || f.is_infinite() {
        return Real::from_real(ctx, 0, 1);
    }
    let s = f.to_string();
    let (num, den) = if let Some(dot) = s.find('.') {
        let (int_part, frac_with_dot) = s.split_at(dot);
        let frac = &frac_with_dot[1..];
        (format!("{}{}", int_part, frac),
         format!("1{}", "0".repeat(frac.len())))
    } else {
        (s, "1".to_string())
    };
    Real::from_real_str(ctx, &num, &den)
        .unwrap_or_else(|| Real::from_real(ctx, 0, 1))
}

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
fn lift_record_op<'ctx>(
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
            // Direct field names in declaration order — for nested
            // sub-records this is the field ("pos"), not the sub-leaves.
            let field_names: Vec<&str> = schema.body.iter()
                .filter_map(|item| match item {
                    BodyItem::Membership { name, .. } => Some(name.as_str()),
                    _ => None,
                })
                .collect();
            // Split the leaf into its first segment (which is one of
            // `field_names`) and the remainder.
            let (first, rest) = match leaf.split_once('.') {
                Some((a, b)) => (a, Some(b)),
                None => (leaf, None),
            };
            let pos = field_names.iter().position(|&n| n == first)?;
            if pos >= args.len() { return None; }
            match rest {
                None => Some(args[pos].clone()),
                // Nested leaf: recurse into the arg with the rest of
                // the path. Works for `SDLRect(IVec2(...), IVec2(...),
                // Color(...))` accessed at leaf "pos.x".
                Some(rest_path) => substitute_record_refs(&args[pos], rest_path, env, schemas),
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

/// Handle `seq_var = ⟨e1, e2, …⟩` (sequence-literal assignment).
///
/// Returns the conjunction `len == items.len() ∧ ∀i: arr[i] == translated(e_i)`
/// when `lhs` is an `Identifier(name)` resolving to a `Var::SeqVar` (primitive
/// element) or `Var::DatatypeSeqVar` (composite element), and `rhs` is an
/// `Expr::SeqLit(items)`. Returns `None` otherwise — caller then falls back
/// through the Bool/Int/Str equality paths.
fn translate_seq_lit_eq<'ctx>(
    lhs: &Expr,
    rhs: &Expr,
    ctx: &'ctx Context,
    env: &HashMap<String, Var<'ctx>>,
    schemas: &HashMap<String, SchemaDecl>,
) -> Option<Bool<'ctx>> {
    let items = match rhs {
        Expr::SeqLit(items) => items,
        _ => return None,
    };
    let name = match lhs {
        Expr::Identifier(n) => n,
        _ => return None,
    };
    let var = env.get(name)?;

    // Primitive-element Seq: pin length, then per-element equality on the
    // underlying Z3 array.
    if let Some((arr, len, elem)) = var.as_seq() {
        let n = items.len() as i64;
        let mut clauses: Vec<Bool<'ctx>> = Vec::with_capacity(items.len() + 1);
        clauses.push(len._eq(&Int::from_i64(ctx, n)));
        for (i, item) in items.iter().enumerate() {
            let idx = Int::from_i64(ctx, i as i64);
            let cell = arr.select(&idx);
            let eq = match elem {
                SeqElem::Int => {
                    let z = cell.as_int()?;
                    let v = translate_int(item, ctx, env)?;
                    z._eq(&v)
                }
                SeqElem::Bool => {
                    let z = cell.as_bool()?;
                    let v = translate_bool(item, ctx, env, schemas)?;
                    z._eq(&v)
                }
                SeqElem::Str => {
                    let z = cell.as_string()?;
                    let v = translate_str(item, ctx, env)?;
                    z._eq(&v)
                }
            };
            clauses.push(eq);
        }
        let refs: Vec<&Bool<'ctx>> = clauses.iter().collect();
        return Some(Bool::and(ctx, &refs));
    }

    // Composite-element Seq: each item must be a bare Identifier referring to
    // flat sub-schema fields (e.g. `ball_rect`). Walk the Datatype's FieldKind
    // list and assemble a constructor application from `env["ident.field"]`
    // lookups, recursing for nested composites (e.g. `ball_rect.color.r`).
    if let Some((arr, len, _, dt, fields)) = var.as_datatype_seq() {
        let n = items.len() as i64;
        let mut clauses: Vec<Bool<'ctx>> = Vec::with_capacity(items.len() + 1);
        clauses.push(len._eq(&Int::from_i64(ctx, n)));
        for (i, item) in items.iter().enumerate() {
            // Each composite item must be an Identifier whose flat-expanded
            // sub-schema fields live in env under `ident.field` keys.
            let ident = match item {
                Expr::Identifier(s) => s,
                _ => return None,
            };
            let elem_dyn = build_composite_dynamic(ident, dt, fields, ctx, env)?;
            let idx = Int::from_i64(ctx, i as i64);
            let cell = arr.select(&idx);
            clauses.push(cell._eq(&elem_dyn));
        }
        let refs: Vec<&Bool<'ctx>> = clauses.iter().collect();
        return Some(Bool::and(ctx, &refs));
    }
    None
}

/// Build a single Datatype value (`Dynamic`) by applying `dt.variants[0]
/// .constructor` to one Dynamic per `FieldKind`. Each primitive field is
/// resolved via `env.get(&format!("{prefix}.{field_name}"))`; each nested
/// composite is resolved by recursing with prefix
/// `format!("{prefix}.{field_name}")`.
///
/// Used by `translate_seq_lit_eq` to translate `seq = ⟨ident1, ident2, …⟩`
/// when seq is a `Seq(UserType)` and each `identK` names a flat-expanded
/// sub-schema instance whose fields already exist in env as
/// `identK.field…` Z3 consts.
fn build_composite_dynamic<'ctx>(
    prefix: &str,
    dt: &'static DatatypeSort<'static>,
    fields: &[FieldKind],
    ctx: &'ctx Context,
    env: &HashMap<String, Var<'ctx>>,
) -> Option<z3::ast::Dynamic<'ctx>> {
    let mut field_dyns: Vec<z3::ast::Dynamic<'ctx>> = Vec::with_capacity(fields.len());
    for fk in fields.iter() {
        let dynamic = match fk {
            FieldKind::Primitive { name, prim_type } => {
                let key = format!("{}.{}", prefix, name);
                let var = env.get(&key)?;
                match (prim_type.as_str(), var) {
                    ("Int" | "Nat" | "Pos", Var::IntVar(i)) =>
                        z3::ast::Dynamic::from_ast(i),
                    ("Int" | "Nat" | "Pos", Var::PinnedInt(v)) =>
                        z3::ast::Dynamic::from_ast(&Int::from_i64(ctx, *v)),
                    ("Bool", Var::BoolVar(b)) =>
                        z3::ast::Dynamic::from_ast(b),
                    ("String", Var::StrVar(s)) =>
                        z3::ast::Dynamic::from_ast(s),
                    _ => return None,
                }
            }
            FieldKind::Nested { name, dt: nested_dt, sub_fields, .. } => {
                let sub_prefix = format!("{}.{}", prefix, name);
                build_composite_dynamic(&sub_prefix, nested_dt, sub_fields, ctx, env)?
            }
        };
        field_dyns.push(dynamic);
    }
    let dyn_refs: Vec<&dyn Ast> = field_dyns.iter().map(|d| d as &dyn Ast).collect();
    Some(dt.variants[0].constructor.apply(&dyn_refs))
}

/// Handle `seq[i] = composite_var` (single-element composite assignment
/// against a `Seq(UserType)`). Used by `output.rects[#state.dots] = player_rect`
/// in the dot-collect engine: assign one composite value into one slot of a
/// composite-element seq.
///
/// LHS must be `Index(Identifier(seq_name), idx_expr)` where `seq_name`
/// resolves to a `Var::DatatypeSeqVar`. RHS must be `Identifier(comp_name)`
/// where `comp_name.*` keys exist in env (flat-expanded composite from a
/// sub-schema membership). Builds the per-element Datatype value via
/// `build_composite_dynamic` and asserts `arr.select(idx) == composite`.
fn translate_seq_index_assign<'ctx>(
    lhs: &Expr,
    rhs: &Expr,
    ctx: &'ctx Context,
    env: &HashMap<String, Var<'ctx>>,
) -> Option<Bool<'ctx>> {
    let (seq_name, idx_expr) = match lhs {
        Expr::Index(seq_expr, idx_expr) => {
            let Expr::Identifier(name) = seq_expr.as_ref() else { return None };
            (name.as_str(), idx_expr.as_ref())
        }
        _ => return None,
    };
    let comp_name = match rhs {
        Expr::Identifier(n) => n.as_str(),
        _ => return None,
    };
    let var = env.get(seq_name)?;
    let (arr, _, _, dt, fields) = var.as_datatype_seq()?;
    // The composite must be flat-expanded — verify by checking at least one
    // expected leaf exists in env. Without this, `output.rects[i] = player_rect`
    // would silently match `player_rect ∈ Bool` and translate wrong.
    let first_field = fields.first().map(|f| f.name())?;
    if !env.contains_key(&format!("{}.{}", comp_name, first_field)) {
        return None;
    }
    let idx = translate_int(idx_expr, ctx, env)?;
    let composite = build_composite_dynamic(comp_name, dt, fields, ctx, env)?;
    let elem = arr.select(&idx);
    Some(elem._eq(&composite))
}

/// Walk a composite seq element and bind each declared field as
/// `<prefix>.<field_name>` in env, with the field's Z3 expression
/// extracted via the Datatype's accessor. Used by `∀ var ∈ <seq>`
/// composite iteration: for each iteration index i, the body
/// references `var.field1`, `var.field2`, etc. — those resolve via
/// env-key lookup, so we populate env with the right per-iteration
/// values before translating the body.
///
/// Recurses for `FieldKind::Nested` (e.g. `dot.color.r` where
/// `color ∈ Color`). Returns false on shape mismatch (caller
/// should fail the whole quantifier rather than silently produce
/// a wrong model).
fn bind_composite_fields<'ctx>(
    env: &mut HashMap<String, Var<'ctx>>,
    elem_dyn: &z3::ast::Dynamic<'ctx>,
    fields: &[FieldKind],
    dt: &DatatypeSort<'ctx>,
    prefix: &str,
) -> bool {
    let Some(elem) = elem_dyn.as_datatype() else { return false };
    for (fi, fk) in fields.iter().enumerate() {
        if fi >= dt.variants[0].accessors.len() { return false; }
        let raw = dt.variants[0].accessors[fi].apply(&[&elem]);
        match fk {
            FieldKind::Primitive { name, prim_type } => {
                let key = format!("{}.{}", prefix, name);
                let var = match prim_type.as_str() {
                    "Int" | "Nat" | "Pos" => raw.as_int().map(Var::IntVar),
                    "Bool"   => raw.as_bool().map(Var::BoolVar),
                    "String" => raw.as_string().map(Var::StrVar),
                    _ => None,
                };
                let Some(v) = var else { return false };
                env.insert(key, v);
            }
            FieldKind::Nested { name, dt: nested_dt, sub_fields, .. } => {
                let sub_prefix = format!("{}.{}", prefix, name);
                if !bind_composite_fields(env, &raw, sub_fields, nested_dt, &sub_prefix) {
                    return false;
                }
            }
        }
    }
    true
}

pub(super) fn translate_bool<'ctx>(
    e: &Expr,
    ctx: &'ctx Context,
    env: &HashMap<String, Var<'ctx>>,
    schemas: &HashMap<String, SchemaDecl>,
) -> Option<Bool<'ctx>> {
    match e {
        Expr::Bool(b) => Some(Bool::from_bool(ctx, *b)),
        Expr::Identifier(name) => env.get(name).and_then(|v| v.as_bool().cloned()),
        Expr::Not(inner) => Some(translate_bool(inner, ctx, env, schemas)?.not()),

        // `seq[i]` where seq holds Bool elements.
        Expr::Index(seq_expr, idx_expr) => {
            let name = match seq_expr.as_ref() {
                Expr::Identifier(n) => n,
                _ => return None,
            };
            let (arr, _, elem) = env.get(name)?.as_seq()?;
            if elem != SeqElem::Bool { return None; }
            let i = translate_int(idx_expr, ctx, env)?;
            arr.select(&i).as_bool()
        }
        // `pts[i].active` where pts is Seq(UserType) and `active` is a
        // Bool field.
        Expr::Field(_, _) => {
            let (raw, ftype) = resolve_seq_field(e, ctx, env)?;
            if ftype == "Bool" { raw.as_bool() } else { None }
        }

        // `x ∈ {a, b, c}` → x = a ∨ x = b ∨ x = c.
        // `x ∈ s` where s is a Set var → s.member(x).
        Expr::InExpr(lhs, rhs) => {
            // Set-var RHS (Identifier whose env entry is SetVar): use Z3's
            // native set membership.
            if let Expr::Identifier(name) = rhs.as_ref() {
                if let Some((set, elem)) = env.get(name).and_then(|v| v.as_set()) {
                    return match elem {
                        SeqElem::Int => {
                            let x = translate_int(lhs, ctx, env)?;
                            Some(set.member(&x))
                        }
                        SeqElem::Bool => {
                            let x = translate_bool(lhs, ctx, env, schemas)?;
                            Some(set.member(&x))
                        }
                        SeqElem::Str => {
                            let x = translate_str(lhs, ctx, env)?;
                            Some(set.member(&x))
                        }
                    };
                }
            }
            // Set-literal RHS: reduce to OR of equalities.
            let items = match rhs.as_ref() {
                Expr::SetLit(items) => items.clone(),
                _ => return None,
            };
            let mut clauses: Vec<Bool> = Vec::with_capacity(items.len());
            for it in &items {
                let eq = Expr::Binary(BinOp::Eq, lhs.clone(), Box::new(it.clone()));
                if let Some(b) = translate_bool(&eq, ctx, env, schemas) {
                    clauses.push(b);
                }
            }
            if clauses.is_empty() { return Some(Bool::from_bool(ctx, false)); }
            let refs: Vec<&Bool> = clauses.iter().collect();
            Some(Bool::or(ctx, &refs))
        }

        // `∀ vars ∈ <range> : body` / `∃ …`. Range shapes:
        //
        //   1. Integer range `{lo..hi}` — unrolls lo..=hi, binds the
        //      single var to each Int. Single-var binding only.
        //   2. Composite seq `state.dots` (Seq(UserType)) — unrolls
        //      0..len, binds `var.field` to each leaf of state.dots[i].
        //      Single-var only.
        //   3. Primitive seq `s` (Seq(Int|Bool|String)) — unrolls
        //      0..len, binds the single var to each element.
        //   4. `coindexed(A, B, C)` — N-arity zip. Tuple binding required;
        //      each iteration binds vars[k] to seqs[k][i] (positionally
        //      across all sequences).
        //   5. `edges(seq)` — consecutive-pair iteration. 2-tuple binding;
        //      each iteration binds vars[0] to seq[i], vars[1] to seq[i+1].
        Expr::Forall(vars, range, body) | Expr::Exists(vars, range, body) => {
            let mut clauses: Vec<Bool> = Vec::new();

            // Form 4: coindexed(A, B, …) — tuple-binding required.
            if let Expr::Call(name, args) = range.as_ref() {
                match (name.as_str(), args.len()) {
                    ("coindexed", n_seqs) if n_seqs >= 1 => {
                        if vars.len() != n_seqs {
                            return None; // arity mismatch — let the caller's
                                         // dropped-constraint path surface it
                        }
                        // All sequences must have the same pinned length.
                        // Build the (Var-handle, length) per sequence so we
                        // can iterate and bind each var per index.
                        let mut seq_lens: Vec<i64> = Vec::with_capacity(n_seqs);
                        for arg in args {
                            let Expr::Identifier(seq_name) = arg else { return None };
                            let seq_var = env.get(seq_name)?;
                            let len = if let Some((_, len, _, _, _)) = seq_var.as_datatype_seq() {
                                len.simplify().as_i64()?
                            } else if let Some((_, len, _)) = seq_var.as_seq() {
                                len.simplify().as_i64()?
                            } else {
                                return None;
                            };
                            seq_lens.push(len);
                        }
                        let n = *seq_lens.iter().min()?;
                        for i in 0..n {
                            let mut env2 = env_clone(env);
                            for (var, arg) in vars.iter().zip(args.iter()) {
                                let Expr::Identifier(seq_name) = arg else { return None };
                                let seq_var = env.get(seq_name)?;
                                let idx = Int::from_i64(ctx, i);
                                if let Some((arr, _, _, dt, fields)) = seq_var.as_datatype_seq() {
                                    let elem_dyn = arr.select(&idx);
                                    if !bind_composite_fields(&mut env2, &elem_dyn, fields, dt, var) {
                                        return None;
                                    }
                                } else if let Some((arr, _, elem)) = seq_var.as_seq() {
                                    let cell = arr.select(&idx);
                                    let v = match elem {
                                        SeqElem::Int  => cell.as_int().map(Var::IntVar),
                                        SeqElem::Bool => cell.as_bool().map(Var::BoolVar),
                                        SeqElem::Str  => cell.as_string().map(Var::StrVar),
                                    };
                                    env2.insert(var.clone(), v?);
                                } else {
                                    return None;
                                }
                            }
                            if let Some(b) = translate_bool(body, ctx, &env2, schemas) {
                                clauses.push(b);
                            }
                        }
                        let refs: Vec<&Bool> = clauses.iter().collect();
                        return Some(if matches!(e, Expr::Forall(..)) {
                            Bool::and(ctx, &refs)
                        } else if refs.is_empty() {
                            Bool::from_bool(ctx, false)
                        } else {
                            Bool::or(ctx, &refs)
                        });
                    }
                    ("edges", 1) => {
                        // edges(seq) — adjacent-pair iteration, requires
                        // a 2-tuple binding. Each step binds vars[0] to
                        // seq[i] and vars[1] to seq[i+1] for i in 0..n-1.
                        if vars.len() != 2 { return None; }
                        let arg = &args[0];
                        let Expr::Identifier(seq_name) = arg else { return None };
                        let seq_var = env.get(seq_name)?;
                        let (n, bind): (i64, Box<dyn Fn(&mut HashMap<String, Var<'ctx>>, i64, &str) -> bool>) =
                            if let Some((arr, len, _, dt, fields)) = seq_var.as_datatype_seq() {
                                let arr = arr.clone(); let fields = fields.to_vec();
                                let n = len.simplify().as_i64()?;
                                (n, Box::new(move |env2, i, var| {
                                    let idx = Int::from_i64(ctx, i);
                                    let elem_dyn = arr.select(&idx);
                                    bind_composite_fields(env2, &elem_dyn, &fields, dt, var)
                                }))
                            } else if let Some((arr, len, elem)) = seq_var.as_seq() {
                                let arr = arr.clone();
                                let n = len.simplify().as_i64()?;
                                (n, Box::new(move |env2, i, var| {
                                    let idx = Int::from_i64(ctx, i);
                                    let cell = arr.select(&idx);
                                    let v = match elem {
                                        SeqElem::Int  => cell.as_int().map(Var::IntVar),
                                        SeqElem::Bool => cell.as_bool().map(Var::BoolVar),
                                        SeqElem::Str  => cell.as_string().map(Var::StrVar),
                                    };
                                    match v {
                                        Some(v) => { env2.insert(var.to_string(), v); true }
                                        None => false,
                                    }
                                }))
                            } else {
                                return None;
                            };
                        for i in 0..(n - 1) {
                            let mut env2 = env_clone(env);
                            if !bind(&mut env2, i,     &vars[0]) { return None; }
                            if !bind(&mut env2, i + 1, &vars[1]) { return None; }
                            if let Some(b) = translate_bool(body, ctx, &env2, schemas) {
                                clauses.push(b);
                            }
                        }
                        let refs: Vec<&Bool> = clauses.iter().collect();
                        return Some(if matches!(e, Expr::Forall(..)) {
                            Bool::and(ctx, &refs)
                        } else if refs.is_empty() {
                            Bool::from_bool(ctx, false)
                        } else {
                            Bool::or(ctx, &refs)
                        });
                    }
                    _ => return None,    // unknown function in quantifier range
                }
            }

            // Forms 1–3 require a single-name binding.
            if vars.len() != 1 { return None; }
            let var = &vars[0];

            // Form 1: integer range.
            if let Some((lo, hi)) = literal_range(range, ctx, env) {
                for i in lo..=hi {
                    let mut env2 = env_clone(env);
                    env2.insert(var.clone(), Var::IntVar(Int::from_i64(ctx, i)));
                    if let Some(b) = translate_bool(body, ctx, &env2, schemas) {
                        clauses.push(b);
                    }
                }
            // Form 2 / 3: iterate over a Seq variable.
            } else if let Expr::Identifier(seq_name) = range.as_ref() {
                let seq_var = env.get(seq_name)?;
                if let Some((arr, len, _, dt, fields)) = seq_var.as_datatype_seq() {
                    // Composite seq: iterate elements, bind <var>.<field>
                    // for each declared field in env on each iteration.
                    let n = len.simplify().as_i64()?;
                    for i in 0..n {
                        let mut env2 = env_clone(env);
                        let idx = Int::from_i64(ctx, i);
                        let elem_dyn = arr.select(&idx);
                        if !bind_composite_fields(&mut env2, &elem_dyn, fields, dt, var) {
                            return None; // shape mismatch — fail loudly
                        }
                        if let Some(b) = translate_bool(body, ctx, &env2, schemas) {
                            clauses.push(b);
                        }
                    }
                } else if let Some((arr, len, elem)) = seq_var.as_seq() {
                    // Primitive seq: bind `var` to the element directly.
                    let n = len.simplify().as_i64()?;
                    for i in 0..n {
                        let mut env2 = env_clone(env);
                        let idx = Int::from_i64(ctx, i);
                        let cell = arr.select(&idx);
                        let v = match elem {
                            SeqElem::Int  => cell.as_int().map(Var::IntVar),
                            SeqElem::Bool => cell.as_bool().map(Var::BoolVar),
                            SeqElem::Str  => cell.as_string().map(Var::StrVar),
                        };
                        let v = v?;
                        env2.insert(var.clone(), v);
                        if let Some(b) = translate_bool(body, ctx, &env2, schemas) {
                            clauses.push(b);
                        }
                    }
                } else {
                    // Identifier in scope but not a seq — can't iterate.
                    return None;
                }
            } else {
                // Range expression we don't recognize.
                return None;
            }

            let refs: Vec<&Bool> = clauses.iter().collect();
            if matches!(e, Expr::Forall(..)) {
                Some(Bool::and(ctx, &refs))
            } else {
                if refs.is_empty() { Some(Bool::from_bool(ctx, false)) }
                else                { Some(Bool::or(ctx, &refs)) }
            }
        }
        Expr::Binary(op, lhs, rhs) => match op {
            // Boolean combinators
            BinOp::And => {
                let l = translate_bool(lhs, ctx, env, schemas)?;
                let r = translate_bool(rhs, ctx, env, schemas)?;
                Some(Bool::and(ctx, &[&l, &r]))
            }
            BinOp::Or => {
                let l = translate_bool(lhs, ctx, env, schemas)?;
                let r = translate_bool(rhs, ctx, env, schemas)?;
                Some(Bool::or(ctx, &[&l, &r]))
            }
            BinOp::Implies => {
                let l = translate_bool(lhs, ctx, env, schemas)?;
                let r = translate_bool(rhs, ctx, env, schemas)?;
                Some(l.implies(&r))
            }
            // Eq/Neq work over Bool, Int, or String. Try in that order.
            BinOp::Eq | BinOp::Neq => {
                // First: handle `seq_var = ⟨e1, e2, …⟩` (sequence literal
                // assignment). This pins both length and per-element values
                // and lives outside the Bool/Int/Str scalar paths because
                // it produces a conjunction over the elements rather than
                // a single _eq.
                if let Some(b) = translate_seq_lit_eq(lhs, rhs, ctx, env, schemas) {
                    return Some(match op {
                        BinOp::Eq  => b,
                        BinOp::Neq => b.not(),
                        _ => unreachable!(),
                    });
                }
                if let Some(b) = translate_seq_lit_eq(rhs, lhs, ctx, env, schemas) {
                    return Some(match op {
                        BinOp::Eq  => b,
                        BinOp::Neq => b.not(),
                        _ => unreachable!(),
                    });
                }
                // `seq[i] = composite_var` (single-element composite-seq
                // assignment). Try both orientations.
                if let Some(b) = translate_seq_index_assign(lhs, rhs, ctx, env) {
                    return Some(match op {
                        BinOp::Eq  => b,
                        BinOp::Neq => b.not(),
                        _ => unreachable!(),
                    });
                }
                if let Some(b) = translate_seq_index_assign(rhs, lhs, ctx, env) {
                    return Some(match op {
                        BinOp::Eq  => b,
                        BinOp::Neq => b.not(),
                        _ => unreachable!(),
                    });
                }
                if let (Some(l), Some(r)) =
                    (translate_bool(lhs, ctx, env, schemas), translate_bool(rhs, ctx, env, schemas))
                {
                    return Some(match op {
                        BinOp::Eq  => l._eq(&r),
                        BinOp::Neq => l._eq(&r).not(),
                        _ => unreachable!(),
                    });
                }
                if let (Some(l), Some(r)) =
                    (translate_int(lhs, ctx, env), translate_int(rhs, ctx, env))
                {
                    return Some(match op {
                        BinOp::Eq  => l._eq(&r),
                        BinOp::Neq => l._eq(&r).not(),
                        _ => unreachable!(),
                    });
                }
                // Real path: at least one side is Real (RealVar or Real
                // literal); the other side may be Int and gets coerced.
                if let (Some(l), Some(r)) =
                    (translate_real(lhs, ctx, env), translate_real(rhs, ctx, env))
                {
                    return Some(match op {
                        BinOp::Eq  => l._eq(&r),
                        BinOp::Neq => l._eq(&r).not(),
                        _ => unreachable!(),
                    });
                }
                if let (Some(l), Some(r)) =
                    (translate_str(lhs, ctx, env), translate_str(rhs, ctx, env))
                {
                    return Some(match op {
                        BinOp::Eq  => l._eq(&r),
                        BinOp::Neq => l._eq(&r).not(),
                        _ => unreachable!(),
                    });
                }
                // Enum equality: `today = Mon` where `today` is an
                // EnumVar and `Mon` is an EnumValue (or vice versa, or
                // both EnumValues). Both sides must reference enum-
                // typed identifiers in env. Different enums on the two
                // sides aren't allowed — caller has a type error.
                if let (Some(l), Some(r)) =
                    (resolve_enum_ast(lhs, ctx, env, schemas),
                     resolve_enum_ast(rhs, ctx, env, schemas))
                {
                    return Some(match op {
                        BinOp::Eq  => l._eq(&r),
                        BinOp::Neq => l._eq(&r).not(),
                        _ => unreachable!(),
                    });
                }
                // Record-op broadcast: handles `=`, `≠` between
                // record-typed expressions on either side, including
                // arithmetic (`vec_lo = vec - offset`).
                lift_record_op(op, lhs, rhs, ctx, env, schemas)
            }
            // Numeric comparisons. Try Int first; fall back to Real
            // (with Int→Real coercion) so `realvar < 3` and
            // `realvar < 3.14` both work.
            BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
                if let (Some(l), Some(r)) =
                    (translate_int(lhs, ctx, env), translate_int(rhs, ctx, env))
                {
                    return Some(match op {
                        BinOp::Lt => l.lt(&r),
                        BinOp::Le => l.le(&r),
                        BinOp::Gt => l.gt(&r),
                        BinOp::Ge => l.ge(&r),
                        _ => unreachable!(),
                    });
                }
                if let (Some(l), Some(r)) =
                    (translate_real(lhs, ctx, env), translate_real(rhs, ctx, env))
                {
                    return Some(match op {
                        BinOp::Lt => l.lt(&r),
                        BinOp::Le => l.le(&r),
                        BinOp::Gt => l.gt(&r),
                        BinOp::Ge => l.ge(&r),
                        _ => unreachable!(),
                    });
                }
                // Record-op broadcast: `<`, `≤`, `>`, `≥` between
                // record-typed expressions are componentwise. Same
                // helper as Eq/Neq — operator threads through.
                // Handles `vec_lo ≤ vec` and arithmetic-laden forms
                // like `dot.pos - offset_lo ≤ player.pos`.
                lift_record_op(op, lhs, rhs, ctx, env, schemas)
            }
            _ => None,
        }
        _ => None,
    }
}
