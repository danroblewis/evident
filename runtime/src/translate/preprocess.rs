//! Pre-translation: pin literal-int vars, propagate Seq lengths, fold quantifier bounds.
//! Pure AST + Value map → updated env; no Z3 Context or Solver involved.

use std::collections::{HashMap, HashSet};

use crate::core::ast::*;
use crate::core::{Value, Var};

/// Names that appear in a quantifier bound (∀/∃ `range`). Changing these invalidates the cached
/// constraint set; pure value givens (never in a bound) are NOT structural.
pub fn structural_names(body: &[BodyItem]) -> HashSet<String> {
    let mut out = HashSet::new();
    for item in body {
        if let BodyItem::Constraint(e) = item {
            walk_for_quantifier_bounds(e, &mut out);
        }
        // ClaimCall/Passthrough/SubclaimDecl bodies not walked — they rarely reference top-level givens as bounds.
    }
    out
}

fn walk_for_quantifier_bounds(e: &Expr, out: &mut HashSet<String>) {
    match e {
        Expr::Forall(_, range, body) | Expr::Exists(_, range, body) => {
            collect_referenced_names(range, out);
            walk_for_quantifier_bounds(body, out);
        }
        Expr::Binary(_, lhs, rhs) => {
            walk_for_quantifier_bounds(lhs, out);
            walk_for_quantifier_bounds(rhs, out);
        }
        Expr::Not(inner) => walk_for_quantifier_bounds(inner, out),
        Expr::InExpr(lhs, rhs) => {
            walk_for_quantifier_bounds(lhs, out);
            walk_for_quantifier_bounds(rhs, out);
        }
        // `#name` outside a quantifier range is still structural: drives cardinality-chain propagation.
        Expr::Cardinality(inner) => {
            if let Expr::Identifier(name) = inner.as_ref() {
                out.insert(name.clone());
            }
        }
        _ => {}
    }
}

pub fn collect_referenced_names(e: &Expr, out: &mut HashSet<String>) {
    match e {
        Expr::Identifier(n) => { out.insert(n.clone()); }
        Expr::Cardinality(inner) => {
            // The seq name itself is structural — a `given` Seq value
            // for it determines the length, which the bound consumes.
            if let Expr::Identifier(name) = inner.as_ref() {
                out.insert(name.clone());
            }
            collect_referenced_names(inner, out);
        }
        Expr::Binary(_, lhs, rhs) => {
            collect_referenced_names(lhs, out);
            collect_referenced_names(rhs, out);
        }
        Expr::Not(inner) => collect_referenced_names(inner, out),
        Expr::Range(lo, hi) => {
            collect_referenced_names(lo, out);
            collect_referenced_names(hi, out);
        }
        Expr::Index(s, i) => {
            collect_referenced_names(s, out);
            collect_referenced_names(i, out);
        }
        Expr::Field(r, _) => collect_referenced_names(r, out),
        Expr::InExpr(lhs, rhs) => {
            collect_referenced_names(lhs, out);
            collect_referenced_names(rhs, out);
        }
        Expr::SetLit(items) | Expr::SeqLit(items) => {
            for it in items { collect_referenced_names(it, out); }
        }
        Expr::Forall(_, range, body) | Expr::Exists(_, range, body) => {
            collect_referenced_names(range, out);
            collect_referenced_names(body, out);
        }
        Expr::Call(_, args) => {
            for a in args { collect_referenced_names(a, out); }
        }
        _ => {}
    }
}

/// Cache-invalidation key: `(filtered_pinned, filtered_seq_lens)` restricted to structural names.
/// Seq length (not value) matters — equal-length Seqs share the same constraint shape.
pub type StructuralSignature = (HashMap<String, i64>, HashMap<String, i64>);

pub fn structural_signature(
    body: &[BodyItem],
    given: &HashMap<String, Value>,
) -> StructuralSignature {
    let names = structural_names(body);
    let seq_lens = collect_seq_lengths(body, given);
    let pinned = collect_pinned_ints(body, given, &seq_lens);
    let pinned_filtered: HashMap<String, i64> = pinned.into_iter()
        .filter(|(k, _)| names.contains(k.as_str()))
        .collect();
    let seq_lens_filtered: HashMap<String, i64> = seq_lens.into_iter()
        .filter(|(k, _)| names.contains(k.as_str()))
        .collect();
    (pinned_filtered, seq_lens_filtered)
}

/// Collect literal-int pins from `given` and body constraints (`name = expr`, `name = #seq`).
/// Fixed-point so chains like `n = #s ∧ #s = 4 ∧ k = n - 1` fully resolve.
pub(super) fn collect_pinned_ints(
    body: &[BodyItem],
    given: &HashMap<String, Value>,
    seq_lengths: &HashMap<String, i64>,
) -> HashMap<String, i64> {
    let mut pinned: HashMap<String, i64> = HashMap::new();
    for (k, v) in given {
        if let Value::Int(n) = v { pinned.insert(k.clone(), *n); }
    }
    let mut changed = true;
    while changed {
        changed = false;
        for item in body {
            if let BodyItem::Constraint(Expr::Binary(BinOp::Eq, lhs, rhs)) = item {
                for (a, b) in [(lhs, rhs), (rhs, lhs)] {
                    if let Expr::Identifier(name) = a.as_ref() {
                        if !pinned.contains_key(name) {
                            if let Some(v) = eval_pure_int(b, &pinned, seq_lengths) {
                                pinned.insert(name.clone(), v);
                                changed = true;
                            }
                        }
                    }
                }
            }
        }
    }
    pinned
}

/// Constant-fold an Int expression over pinned names, literals, and `#seq` lengths.
fn eval_pure_int(
    e: &Expr,
    pinned: &HashMap<String, i64>,
    seq_lengths: &HashMap<String, i64>,
) -> Option<i64> {
    match e {
        Expr::Int(n) => Some(*n),
        Expr::Identifier(name) => pinned.get(name).copied(),
        Expr::Cardinality(inner) => match inner.as_ref() {
            Expr::Identifier(name) => seq_lengths.get(name).copied(),
            _ => None,
        },
        Expr::Binary(op, lhs, rhs) => {
            let l = eval_pure_int(lhs, pinned, seq_lengths)?;
            let r = eval_pure_int(rhs, pinned, seq_lengths)?;
            Some(match op {
                BinOp::Add => l.checked_add(r)?,
                BinOp::Sub => l.checked_sub(r)?,
                BinOp::Mul => l.checked_mul(r)?,
                BinOp::Div => if r == 0 { return None } else { l / r },
                _ => return None,
            })
        }
        _ => None,
    }
}

/// Collect Seq lengths from `given` values, `seq = ⟨…⟩` literals, and `#seq = expr` constraints.
/// Fixed-point so chains like `#state_next.cells = #state.cells` propagate fully.
pub(super) fn collect_seq_lengths(
    body: &[BodyItem],
    given: &HashMap<String, Value>,
) -> HashMap<String, i64> {
    collect_seq_lengths_with_schemas(body, given, None)
}

/// Like `collect_seq_lengths` but recurses into `..Passthrough` bodies via `schemas`.
pub(super) fn collect_seq_lengths_with_schemas(
    body: &[BodyItem],
    given: &HashMap<String, Value>,
    schemas: Option<&HashMap<String, SchemaDecl>>,
) -> HashMap<String, i64> {
    let mut out = HashMap::new();
    // Seed lengths from `given` Seq/Set values (Set cardinalities feed `#s = #p` chains too).
    for (k, v) in given {
        let len = match v {
            Value::SeqInt(v)       => v.len() as i64,
            Value::SeqBool(v)      => v.len() as i64,
            Value::SeqStr(v)       => v.len() as i64,
            Value::SeqComposite(v) => v.len() as i64,
            Value::SeqEnum(v)      => v.len() as i64,
            Value::SetInt(v)       => v.len() as i64,
            Value::SetBool(v)      => v.len() as i64,
            Value::SetStr(v)       => v.len() as i64,
            _ => continue,
        };
        out.insert(k.clone(), len);
    }
    // Seed pinned ints from `given` so `#position = n` chains resolve.
    let mut pinned: HashMap<String, i64> = HashMap::new();
    for (k, v) in given {
        if let Value::Int(n) = v { pinned.insert(k.clone(), *n); }
    }
    let mut changed = true;
    while changed {
        changed = false;
        walk_constraints(body, schemas, &pinned, &mut out, &mut changed);
        // Also scan for `name = literal_int_expr` and add to `pinned`
        // so `#seq = name` in a later pass resolves.
        scan_int_pins(body, schemas, &mut pinned, &out, &mut changed);
    }
    out
}

/// Find `name = literal_int_expr` body constraints and add to `pinned`.
/// Recurses into passthroughs so chains like `n = 3; #position = n` resolve.
fn scan_int_pins(
    body: &[BodyItem],
    schemas: Option<&HashMap<String, SchemaDecl>>,
    pinned: &mut HashMap<String, i64>,
    seq_lens: &HashMap<String, i64>,
    changed: &mut bool,
) {
    for item in body {
        match item {
            BodyItem::Constraint(Expr::Binary(BinOp::Eq, lhs, rhs)) => {
                for (a, b) in [(lhs, rhs), (rhs, lhs)] {
                    if let Expr::Identifier(name) = a.as_ref() {
                        if !pinned.contains_key(name) {
                            if let Some(v) = eval_pure_int(b, pinned, seq_lens) {
                                pinned.insert(name.clone(), v);
                                *changed = true;
                            }
                        }
                    }
                }
            }
            BodyItem::Passthrough(claim_name) => {
                if let Some(schemas) = schemas {
                    if let Some(claim) = schemas.get(claim_name) {
                        scan_int_pins(&claim.body, Some(schemas), pinned, seq_lens, changed);
                    }
                }
            }
            _ => {}
        }
    }
}

/// Walk body Eq constraints for cardinality pins, recursing into `..Passthrough` bodies.
fn walk_constraints(
    body: &[BodyItem],
    schemas: Option<&HashMap<String, SchemaDecl>>,
    no_pinned: &HashMap<String, i64>,
    out: &mut HashMap<String, i64>,
    changed: &mut bool,
) {
    for item in body {
        match item {
            BodyItem::Constraint(Expr::Binary(BinOp::Eq, lhs, rhs)) => {
                for (a, b) in [(lhs, rhs), (rhs, lhs)] {
                    if let Expr::Cardinality(inner) = a.as_ref() {
                        if let Expr::Identifier(name) = inner.as_ref() {
                            if !out.contains_key(name) {
                                if let Some(v) = eval_pure_int(b, no_pinned, out) {
                                    out.insert(name.clone(), v);
                                    *changed = true;
                                }
                            }
                        }
                    }
                    if let (Expr::Identifier(name), Expr::SeqLit(items)) =
                        (a.as_ref(), b.as_ref())
                    {
                        if !out.contains_key(name) {
                            out.insert(name.clone(), items.len() as i64);
                            *changed = true;
                        }
                    }
                }
            }
            BodyItem::Passthrough(claim_name) => {
                if let Some(schemas) = schemas {
                    if let Some(claim) = schemas.get(claim_name) {
                        walk_constraints(&claim.body, Some(schemas), no_pinned, out, changed);
                    }
                }
            }
            // Sub-schema Membership: walk the type's body for length pins with `inst_name.` prefix.
            BodyItem::Membership { name: inst_name, type_name, .. } => {
                if let Some(schemas) = schemas {
                    if let Some(ty) = schemas.get(type_name) {
                        let field_set: std::collections::HashSet<String> = ty.body.iter()
                            .filter_map(|it| match it {
                                BodyItem::Membership { name, .. } => Some(name.clone()),
                                _ => None,
                            })
                            .collect();
                        walk_constraints_with_prefix(
                            &ty.body, Some(schemas), no_pinned, out, changed,
                            inst_name, &field_set);
                    }
                }
            }
            _ => {}
        }
    }
}

/// Like `walk_constraints` but prefixes field identifiers with `prefix.` for sub-schema instances.
fn walk_constraints_with_prefix(
    body: &[BodyItem],
    schemas: Option<&HashMap<String, SchemaDecl>>,
    no_pinned: &HashMap<String, i64>,
    out: &mut HashMap<String, i64>,
    changed: &mut bool,
    prefix: &str,
    field_set: &std::collections::HashSet<String>,
) {
    for item in body {
        if let BodyItem::Constraint(Expr::Binary(BinOp::Eq, lhs, rhs)) = item {
            for (a, b) in [(lhs, rhs), (rhs, lhs)] {
                if let Expr::Cardinality(inner) = a.as_ref() {
                    if let Expr::Identifier(name) = inner.as_ref() {
                        let first_seg = name.split('.').next().unwrap_or("");
                        if field_set.contains(first_seg) {
                            let dotted = format!("{}.{}", prefix, name);
                            if !out.contains_key(&dotted) {
                                if let Some(v) = eval_pure_int(b, no_pinned, out) {
                                    out.insert(dotted, v);
                                    *changed = true;
                                }
                            }
                        }
                    }
                }
                if let (Expr::Identifier(name), Expr::SeqLit(items)) =
                    (a.as_ref(), b.as_ref())
                {
                    let first_seg = name.split('.').next().unwrap_or("");
                    if field_set.contains(first_seg) {
                        let dotted = format!("{}.{}", prefix, name);
                        if !out.contains_key(&dotted) {
                            out.insert(dotted, items.len() as i64);
                            *changed = true;
                        }
                    }
                }
            }
        }
    }
    // No further recursion: nested Memberships are flat-expanded by declare_var, not here.
    let _ = schemas;
}

/// Upgrade env entries for pinned names to `Var::PinnedInt`. No-op if the name is not in env.
pub(super) fn apply_pinned_ints<'ctx>(
    env: &mut HashMap<String, Var<'ctx>>,
    pinned: &HashMap<String, i64>,
) {
    for (name, value) in pinned {
        if env.contains_key(name) {
            env.insert(name.clone(), Var::PinnedInt(*value));
        }
    }
}


