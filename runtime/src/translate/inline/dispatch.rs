//! Call-name resolution: dotted names → `CallDispatch` flavors, plus `∀`-unroll
//! analysis for method-style subclaim calls in quantifier bodies.

use std::collections::HashMap;

use z3::ast::Ast;

use crate::core::ast::*;
use crate::core::Var;

/// Dotted call resolved to: `Subschema` (recv has the subclaim on its type),
/// `ReceiverPrefix` (suffix is a known claim), or `Plain` (whole name is known).
pub(super) enum CallDispatch {
    Subschema { recv: String, type_name: String, claim_name: String },
    ReceiverPrefix { claim_name: String, recv: String },
    Plain { claim_name: String },
}

/// True if `e` has a method-style subclaim call, meaning the `∀` body needs
/// AST-expansion to reach the inline pass (translate_bool lacks solver access).
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

/// Per-iteration element bindings for `∀` unrolling; None when not statically
/// unrollable. Supports `coindexed(seqs…)` and bare `Identifier(seq)`.
pub(super) fn resolve_forall_unroll(
    vars: &[String],
    range: &Expr,
    env: &HashMap<String, Var<'static>>,
) -> Option<Vec<Vec<(String, Expr)>>> {
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

/// Find the declared type of a body Membership by name.
fn find_membership_type(items: &[BodyItem], name: &str) -> Option<String> {
    for item in items {
        if let BodyItem::Membership { name: n, type_name, .. } = item {
            if n == name { return Some(type_name.clone()); }
        }
    }
    None
}

fn type_has_subclaim(type_decl: &SchemaDecl, name: &str) -> bool {
    type_decl.body.iter().any(|item| matches!(item,
        BodyItem::SubclaimDecl(s) if s.name == name))
}

/// Resolve a call name to a `CallDispatch` flavor; `body_items` provides
/// receiver type context.
pub(super) fn resolve_call(
    name: &str,
    body_items: &[BodyItem],
    schemas: &HashMap<String, SchemaDecl>,
) -> Option<CallDispatch> {
    if !name.contains('.') {
        if schemas.contains_key(name) {
            return Some(CallDispatch::Plain { claim_name: name.to_string() });
        }
        return None;
    }
    let (prefix, suffix) = name.rsplit_once('.')?;
    // Subschema path: prefix is a body var of record type T with SubclaimDecl `suffix`.
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
    // Receiver-prefix fallback: suffix is a known claim; prefix becomes first arg.
    if schemas.contains_key(suffix) {
        return Some(CallDispatch::ReceiverPrefix {
            claim_name: suffix.to_string(),
            recv: prefix.to_string(),
        });
    }
    None
}

/// Resolve `(args) ∈ rhs` where rhs is an Identifier.
pub(super) fn resolve_call_name(
    rhs: &Expr,
    body_items: &[BodyItem],
    schemas: &HashMap<String, SchemaDecl>,
) -> Option<CallDispatch> {
    let Expr::Identifier(n) = rhs else { return None; };
    resolve_call(n, body_items, schemas)
}

/// Back-compat: collapse to `(claim_name, Option<recv>)`; Subschema → None
/// (handled by a dedicated arm before these are reached).
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
