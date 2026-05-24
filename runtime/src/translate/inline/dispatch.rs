//! Call-name resolution for the inline walker: turn a (possibly
//! dotted) call name into a `CallDispatch` flavor, plus the static
//! `∀`-unroll analysis that decides whether a quantifier body needs
//! AST-level expansion so method-style subclaim calls reach the
//! solver-aware inline pass.

use std::collections::HashMap;

use z3::ast::Ast;

use crate::core::ast::*;
use crate::core::Var;

/// Resolution result for a (possibly dotted) call name like
/// `recv.subclaim_name`. Three flavors, tried in priority order:
///
///   * `Subschema { recv, type, subclaim }` — `recv` is a body
///     Membership of record type T and `subclaim` is declared as
///     `subclaim … ` inside T. Dispatch rebinds T's fields onto
///     `recv.field` so the subclaim body's bare references resolve
///     to the receiver's leaves.
///
///   * `ReceiverPrefix { claim_name, recv }` — `recv` is anything
///     (an Int, a dotted field) and the SUFFIX is a known claim.
///     The receiver becomes the first positional arg. Fallback when
///     the subschema path doesn't apply.
///
///   * `Plain { claim_name }` — the whole name is a known schema;
///     no receiver involved.
pub(super) enum CallDispatch {
    Subschema { recv: String, type_name: String, claim_name: String },
    ReceiverPrefix { claim_name: String, recv: String },
    Plain { claim_name: String },
}

/// True if `e` contains a subexpression that's a method-style
/// subclaim call (`recv.subclaim(args)` resolving to a SubschemaDecl
/// on `recv`'s type). Used to decide whether a `∀` body needs to
/// be AST-expanded into per-iteration body items so each subclaim
/// invocation reaches the inline pass (which has solver access)
/// instead of going through translate_bool (which doesn't).
pub(super) fn body_contains_subschema_call(
    e: &Expr,
    body_items: &[BodyItem],
    schemas: &HashMap<String, SchemaDecl>,
) -> bool {
    match e {
        Expr::Call(name, _) => matches!(
            resolve_call(name, body_items, schemas),
            Some(CallDispatch::Subschema { .. })),
        Expr::Binary(_, l, r) =>
            body_contains_subschema_call(l, body_items, schemas)
                || body_contains_subschema_call(r, body_items, schemas),
        Expr::Not(x) | Expr::Cardinality(x) =>
            body_contains_subschema_call(x, body_items, schemas),
        Expr::Ternary(c, a, b) =>
            body_contains_subschema_call(c, body_items, schemas)
                || body_contains_subschema_call(a, body_items, schemas)
                || body_contains_subschema_call(b, body_items, schemas),
        Expr::SeqLit(items) | Expr::SetLit(items) | Expr::Tuple(items) =>
            items.iter().any(|x| body_contains_subschema_call(x, body_items, schemas)),
        Expr::Forall(_, r, b) | Expr::Exists(_, r, b) =>
            body_contains_subschema_call(r, body_items, schemas)
                || body_contains_subschema_call(b, body_items, schemas),
        Expr::Index(a, b) | Expr::InExpr(a, b) | Expr::Range(a, b) =>
            body_contains_subschema_call(a, body_items, schemas)
                || body_contains_subschema_call(b, body_items, schemas),
        Expr::Field(recv, _) => body_contains_subschema_call(recv, body_items, schemas),
        Expr::Match(scr, arms) =>
            body_contains_subschema_call(scr, body_items, schemas)
                || arms.iter().any(|a| body_contains_subschema_call(&a.body, body_items, schemas)),
        Expr::Matches(x, _) => body_contains_subschema_call(x, body_items, schemas),
        _ => false,
    }
}

/// Resolve the per-iteration element exprs for each bound variable
/// in a `∀ … : body`. Returns `Some(Vec<(bound_var, element_expr_for_iter_i)>)`
/// per iteration, OR None if the range shape isn't statically
/// unrollable (length unknown / unsupported range form).
///
/// Supported ranges:
///   * `coindexed(seq1, seq2, …)` with tuple binding `(a, b, …)` —
///     element_i for bound k is `Index(Identifier(seq_k), Int(i))`.
///   * Bare `Identifier(seq_name)` with single binding `a` —
///     element_i is `Index(Identifier(seq_name), Int(i))`.
pub(super) fn resolve_forall_unroll(
    vars: &[String],
    range: &Expr,
    env: &HashMap<String, Var<'static>>,
) -> Option<Vec<Vec<(String, Expr)>>> {
    // coindexed(seq1, …) — tuple binding.
    if let Expr::Call(name, args) = range {
        if name == "coindexed" && args.len() == vars.len() && !args.is_empty() {
            // Collect each seq's pinned length.
            let mut seq_names: Vec<String> = Vec::with_capacity(args.len());
            let mut lens: Vec<i64> = Vec::with_capacity(args.len());
            for arg in args {
                let Expr::Identifier(seq_name) = arg else { return None };
                let var = env.get(seq_name)?;
                let len = if let Some((_, len, _)) = var.as_seq() {
                    len.simplify().as_i64()?
                } else if let Some((_, len, _, _, _)) = var.as_datatype_seq() {
                    len.simplify().as_i64()?
                } else {
                    return None;
                };
                seq_names.push(seq_name.clone());
                lens.push(len);
            }
            let n = *lens.iter().min()?;
            let mut iters: Vec<Vec<(String, Expr)>> = Vec::with_capacity(n as usize);
            for i in 0..n {
                let mut binds: Vec<(String, Expr)> = Vec::with_capacity(vars.len());
                for (v, seq) in vars.iter().zip(seq_names.iter()) {
                    let elem = Expr::Index(
                        Box::new(Expr::Identifier(seq.clone())),
                        Box::new(Expr::Int(i)),
                    );
                    binds.push((v.clone(), elem));
                }
                iters.push(binds);
            }
            return Some(iters);
        }
    }
    // Bare Identifier(seq_name) — single-name binding.
    if let Expr::Identifier(seq_name) = range {
        if vars.len() != 1 { return None; }
        let var = env.get(seq_name)?;
        let n = if let Some((_, len, _)) = var.as_seq() {
            len.simplify().as_i64()?
        } else if let Some((_, len, _, _, _)) = var.as_datatype_seq() {
            len.simplify().as_i64()?
        } else {
            return None;
        };
        let v = &vars[0];
        let iters: Vec<Vec<(String, Expr)>> = (0..n).map(|i| {
            let elem = Expr::Index(
                Box::new(Expr::Identifier(seq_name.clone())),
                Box::new(Expr::Int(i)),
            );
            vec![(v.clone(), elem)]
        }).collect();
        return Some(iters);
    }
    None
}

/// Walk the current body slice for a Membership matching `name`,
/// return its declared type_name. Used to find a receiver's type
/// when dispatching `recv.subclaim(args)`.
fn find_membership_type(items: &[BodyItem], name: &str) -> Option<String> {
    for item in items {
        if let BodyItem::Membership { name: n, type_name, .. } = item {
            if n == name { return Some(type_name.clone()); }
        }
    }
    None
}

/// Walk a type's body for a SubclaimDecl matching `name`.
fn type_has_subclaim(type_decl: &SchemaDecl, name: &str) -> bool {
    type_decl.body.iter().any(|item| matches!(item,
        BodyItem::SubclaimDecl(s) if s.name == name))
}

/// Resolve a call name with full receiver awareness. `body_items`
/// is the surrounding body slice (used to look up the receiver's
/// declared type).
pub(super) fn resolve_call(
    name: &str,
    body_items: &[BodyItem],
    schemas: &HashMap<String, SchemaDecl>,
) -> Option<CallDispatch> {
    // No dots → plain claim invocation.
    if !name.contains('.') {
        if schemas.contains_key(name) {
            return Some(CallDispatch::Plain { claim_name: name.to_string() });
        }
        return None;
    }
    let (prefix, suffix) = name.rsplit_once('.')?;
    // (1) Subschema path: prefix is a bare body var of a record type
    //     T, and T has a SubclaimDecl `suffix`. This is the
    //     "use a field of a schema as a subschema" form.
    if !prefix.contains('.') {
        if let Some(type_name) = find_membership_type(body_items, prefix) {
            if let Some(type_decl) = schemas.get(&type_name) {
                if type_has_subclaim(type_decl, suffix) {
                    return Some(CallDispatch::Subschema {
                        recv: prefix.to_string(),
                        type_name,
                        claim_name: suffix.to_string(),
                    });
                }
            }
        }
    }
    // (2) Receiver-prefix fallback: suffix is a known claim and the
    //     prefix gets prepended as the first positional arg. Works
    //     even with multi-segment prefixes (`win.renderer.foo`).
    if schemas.contains_key(suffix) {
        return Some(CallDispatch::ReceiverPrefix {
            claim_name: suffix.to_string(),
            recv: prefix.to_string(),
        });
    }
    None
}

/// Resolve for the `(args) ∈ rhs` form where rhs is an Identifier.
pub(super) fn resolve_call_name(
    rhs: &Expr,
    body_items: &[BodyItem],
    schemas: &HashMap<String, SchemaDecl>,
) -> Option<CallDispatch> {
    let Expr::Identifier(n) = rhs else { return None; };
    resolve_call(n, body_items, schemas)
}

/// Back-compat wrappers — the existing Plain / ReceiverPrefix
/// dispatch arms below want the old `(claim_name, Option<recv>)`
/// shape. These collapse Subschema cases out so those arms only
/// see Plain / ReceiverPrefix (the Subschema arm above catches
/// the rest first).
pub(super) fn method_dispatch_call_compat(
    name: &str,
    body_items: &[BodyItem],
    schemas: &HashMap<String, SchemaDecl>,
) -> Option<(String, Option<String>)> {
    match resolve_call(name, body_items, schemas)? {
        CallDispatch::Plain { claim_name } => Some((claim_name, None)),
        CallDispatch::ReceiverPrefix { claim_name, recv } => Some((claim_name, Some(recv))),
        CallDispatch::Subschema { .. } => None,  // handled by dedicated arm
    }
}

pub(super) fn method_dispatch_name_compat(
    rhs: &Expr,
    body_items: &[BodyItem],
    schemas: &HashMap<String, SchemaDecl>,
) -> Option<(String, Option<String>)> {
    let Expr::Identifier(n) = rhs else { return None; };
    method_dispatch_call_compat(n, body_items, schemas)
}
