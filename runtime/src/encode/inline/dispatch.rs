//! Inliner support: ident-prefixing rewrite, recursion-depth frames, guard
//! composition, forall-unroll, and call-dispatch resolution (plain claim vs
//! receiver-prefix vs subschema). inline_subschema_call recurses back into
//! the main inliner (super::inline_body_items_guarded).

use std::collections::{HashMap, HashSet};
use z3::{Context, SatResult, Solver};
use z3::ast::{Ast, Bool};

use crate::core::ast::*;
use crate::core::{DatatypeRegistry, EnumRegistry, Var};
use super::super::declare::{declare_var_named, next_call_id};
use super::super::exprs::resolve_mapping;
use super::inline_body_items_guarded;

pub(super) fn rewrite_idents_with_prefix(
    e: &Expr,
    prefix: &str,
    field_set: &HashSet<String>,
) -> Expr {
    let r = |x: &Expr| Box::new(rewrite_idents_with_prefix(x, prefix, field_set));
    let rv = |xs: &Vec<Expr>| xs.iter()
        .map(|x| rewrite_idents_with_prefix(x, prefix, field_set))
        .collect();
    match e {
        Expr::Identifier(name) => {
            let first_seg = name.split('.').next().unwrap_or("");
            if field_set.contains(first_seg) {
                Expr::Identifier(format!("{}.{}", prefix, name))
            } else {
                e.clone()
            }
        }
        Expr::Int(_) | Expr::Real(_) | Expr::Bool(_) | Expr::Str(_) => e.clone(),
        Expr::SetLit(xs)  => Expr::SetLit(rv(xs)),
        Expr::SeqLit(xs)  => Expr::SeqLit(rv(xs)),
        Expr::Tuple(xs)   => Expr::Tuple(rv(xs)),
        Expr::Range(a, b) => Expr::Range(r(a), r(b)),
        Expr::InExpr(a, b) => Expr::InExpr(r(a), r(b)),
        Expr::Forall(vars, range, body) => {

            let inner_set: HashSet<String> = field_set.iter()
                .filter(|f| !vars.contains(f))
                .cloned()
                .collect();
            Expr::Forall(
                vars.clone(),
                Box::new(rewrite_idents_with_prefix(range, prefix, field_set)),
                Box::new(rewrite_idents_with_prefix(body,  prefix, &inner_set)),
            )
        }
        Expr::Exists(vars, range, body) => {
            let inner_set: HashSet<String> = field_set.iter()
                .filter(|f| !vars.contains(f))
                .cloned()
                .collect();
            Expr::Exists(
                vars.clone(),
                Box::new(rewrite_idents_with_prefix(range, prefix, field_set)),
                Box::new(rewrite_idents_with_prefix(body,  prefix, &inner_set)),
            )
        }

        Expr::Call(name, args) => Expr::Call(name.clone(), rv(args)),
        Expr::Cardinality(x) => Expr::Cardinality(r(x)),
        Expr::Index(a, b)    => Expr::Index(r(a), r(b)),
        Expr::Field(recv, f) => Expr::Field(r(recv), f.clone()),
        Expr::Binary(op, a, b) => Expr::Binary(op.clone(), r(a), r(b)),
        Expr::Not(x)           => Expr::Not(r(x)),
        Expr::Delta(x)         => Expr::Delta(r(x)),
        Expr::Ternary(c, a, b) => Expr::Ternary(r(c), r(a), r(b)),
        Expr::Match(scr, arms) => {
            let new_arms: Vec<MatchArm> = arms.iter().map(|arm| {

                let shadowed: HashSet<String> = match &arm.pattern {
                    MatchPattern::Ctor { binds, .. } => binds.iter()
                        .filter_map(|b| b.clone())
                        .collect(),
                    MatchPattern::Wildcard => HashSet::new(),
                };
                let inner: HashSet<String> = field_set.iter()
                    .filter(|n| !shadowed.contains(*n))
                    .cloned()
                    .collect();
                MatchArm {
                    pattern: arm.pattern.clone(),
                    body: Box::new(rewrite_idents_with_prefix(&arm.body, prefix, &inner)),
                }
            }).collect();
            Expr::Match(r(scr), new_arms)
        }
        Expr::Matches(x, p) => Expr::Matches(r(x), p.clone()),
    }
}

const MAX_INLINE_DEPTH: usize = 64;

pub(super) fn try_enter(visited: &mut HashMap<String, usize>, name: &str) -> Option<usize> {
    let cnt = visited.entry(name.to_string()).or_insert(0);
    if *cnt >= MAX_INLINE_DEPTH {
        None
    } else {
        *cnt += 1;
        Some(*cnt)
    }
}

pub(super) fn exit_frame(visited: &mut HashMap<String, usize>, name: &str) {
    if let Some(cnt) = visited.get_mut(name) {
        *cnt -= 1;
        if *cnt == 0 { visited.remove(name); }
    }
}

pub(super) fn isolate_helper_locals(
    body: &[BodyItem],
    inner: &mut HashMap<String, Var<'static>>,
    param_count: usize,
) {

    if param_count == 0 { return; }
    for (i, item) in body.iter().enumerate() {
        if i < param_count { continue; }
        if let BodyItem::Membership { name, .. } = item {
            inner.remove(name);
            let prefix = format!("{}.", name);
            let dotted: Vec<String> = inner.keys()
                .filter(|k| k.starts_with(&prefix)).cloned().collect();
            for k in dotted { inner.remove(&k); }
        }
    }
}

pub(super) fn guard_is_satisfiable(
    solver: &Solver<'static>,
    guard: &Option<Bool<'static>>,
) -> bool {
    let g = match guard {
        None => return true,
        Some(g) => g,
    };
    solver.push();
    solver.assert(g);
    let result = solver.check();
    solver.pop(1);
    !matches!(result, SatResult::Unsat)
}

pub(super) fn guarded_bool<'ctx>(b: Bool<'ctx>, guard: &Option<Bool<'ctx>>) -> Bool<'ctx> {
    match guard {
        None => b,
        Some(g) => g.implies(&b),
    }
}

pub(super) fn compose_guards<'ctx>(
    ctx: &'ctx z3::Context,
    outer: &Option<Bool<'ctx>>,
    inner: Bool<'ctx>,
) -> Option<Bool<'ctx>> {
    match outer {
        None => Some(inner),
        Some(o) => Some(Bool::and(ctx, &[o, &inner])),
    }
}

pub(super) enum CallDispatch {
    Subschema { recv: String, type_name: String, claim_name: String },
    ReceiverPrefix { claim_name: String, recv: String },
    Plain { claim_name: String },
}

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

pub(super) fn substitute_bound_var(e: &Expr, bound: &str, elem: &Expr) -> Expr {
    let r = |x: &Expr| Box::new(substitute_bound_var(x, bound, elem));
    let rv = |xs: &Vec<Expr>| xs.iter()
        .map(|x| substitute_bound_var(x, bound, elem))
        .collect();
    match e {
        Expr::Identifier(name) => {
            if name == bound { return elem.clone(); }
            let prefix = format!("{}.", bound);
            if let Some(suffix) = name.strip_prefix(&prefix) {

                let mut out = elem.clone();
                for seg in suffix.split('.') {
                    out = Expr::Field(Box::new(out), seg.to_string());
                }
                return out;
            }
            e.clone()
        }
        Expr::Int(_) | Expr::Real(_) | Expr::Bool(_) | Expr::Str(_) => e.clone(),
        Expr::SetLit(xs)  => Expr::SetLit(rv(xs)),
        Expr::SeqLit(xs)  => Expr::SeqLit(rv(xs)),
        Expr::Tuple(xs)   => Expr::Tuple(rv(xs)),
        Expr::Range(a, b) => Expr::Range(r(a), r(b)),
        Expr::InExpr(a, b) => Expr::InExpr(r(a), r(b)),
        Expr::Forall(vars, range, body) => {

            if vars.iter().any(|v| v == bound) {
                Expr::Forall(vars.clone(), r(range), body.clone())
            } else {
                Expr::Forall(vars.clone(), r(range), r(body))
            }
        }
        Expr::Exists(vars, range, body) => {
            if vars.iter().any(|v| v == bound) {
                Expr::Exists(vars.clone(), r(range), body.clone())
            } else {
                Expr::Exists(vars.clone(), r(range), r(body))
            }
        }
        Expr::Call(n, args)    => Expr::Call(n.clone(), rv(args)),
        Expr::Cardinality(x)   => Expr::Cardinality(r(x)),
        Expr::Index(a, b)      => Expr::Index(r(a), r(b)),
        Expr::Field(recv, f)   => Expr::Field(r(recv), f.clone()),
        Expr::Binary(op, a, b) => Expr::Binary(op.clone(), r(a), r(b)),
        Expr::Not(x)           => Expr::Not(r(x)),
        Expr::Delta(x)         => Expr::Delta(r(x)),
        Expr::Ternary(c, a, b) => Expr::Ternary(r(c), r(a), r(b)),
        Expr::Match(scr, arms) => {
            let new_arms: Vec<MatchArm> = arms.iter().map(|arm| MatchArm {
                pattern: arm.pattern.clone(),
                body: Box::new(substitute_bound_var(&arm.body, bound, elem)),
            }).collect();
            Expr::Match(r(scr), new_arms)
        }
        Expr::Matches(x, p) => Expr::Matches(r(x), p.clone()),
    }
}

pub(super) fn resolve_forall_unroll(
    vars: &[String],
    range: &Expr,
    env: &HashMap<String, Var<'static>>,
) -> Option<Vec<Vec<(String, Expr)>>> {

    if let Expr::Call(name, args) = range {
        if name == "coindexed" && args.len() == vars.len() && !args.is_empty() {

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

    if schemas.contains_key(suffix) {
        return Some(CallDispatch::ReceiverPrefix {
            claim_name: suffix.to_string(),
            recv: prefix.to_string(),
        });
    }
    None
}

pub(super) fn resolve_call_name(
    rhs: &Expr,
    body_items: &[BodyItem],
    schemas: &HashMap<String, SchemaDecl>,
) -> Option<CallDispatch> {
    let Expr::Identifier(n) = rhs else { return None; };
    resolve_call(n, body_items, schemas)
}

pub(super) fn method_dispatch_call_compat(
    name: &str,
    body_items: &[BodyItem],
    schemas: &HashMap<String, SchemaDecl>,
) -> Option<(String, Option<String>)> {
    match resolve_call(name, body_items, schemas)? {
        CallDispatch::Plain { claim_name } => Some((claim_name, None)),
        CallDispatch::ReceiverPrefix { claim_name, recv } => Some((claim_name, Some(recv))),
        CallDispatch::Subschema { .. } => None,
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

#[allow(clippy::too_many_arguments)]
pub(super) fn inline_subschema_call(
    recv: &str,
    type_name: &str,
    claim_name: &str,
    args: &[Expr],
    env: &mut HashMap<String, Var<'static>>,
    solver: &Solver<'static>,
    schemas: &HashMap<String, SchemaDecl>,
    ctx: &'static Context,
    registry: &DatatypeRegistry,
    enums: Option<&EnumRegistry>,
    visited: &mut HashMap<String, usize>,
    guard: &Option<Bool<'static>>,
    lenient: bool,
) {

    let Some(type_decl) = schemas.get(type_name) else { return; };
    let mut subclaim: Option<&SchemaDecl> = None;
    for item in &type_decl.body {
        if let BodyItem::SubclaimDecl(s) = item {
            if s.name == claim_name { subclaim = Some(s); break; }
        }
    }
    let Some(subclaim) = subclaim else { return; };

    let qualified = format!("{}.{}", type_name, claim_name);
    let Some(_depth) = try_enter(visited, &qualified) else { return; };

    let mut inner = env.clone();

    let prefix = format!("{recv}.");
    let outer_keys: Vec<(String, String)> = env.keys()
        .filter_map(|k| k.strip_prefix(&prefix).map(|rest|
            (k.clone(), rest.to_string())))
        .collect();
    for (full_key, bare) in &outer_keys {
        if let Some(v) = env.get(full_key) {
            inner.insert(bare.clone(), v.clone());
        }
    }

    let slot_info: Vec<(String, String)> = subclaim.body.iter()
        .filter_map(|i| if let BodyItem::Membership { name, type_name, .. } = i {
            Some((name.clone(), type_name.clone()))
        } else { None })
        .take(args.len())
        .collect();
    if slot_info.len() != args.len() {
        eprintln!(
            "warning: subschema call `{}.{}` got {} args but the \
             subclaim has only {} param Memberships",
            recv, claim_name, args.len(), slot_info.len()
        );
        exit_frame(visited, &qualified);
        return;
    }

    let mappings: Vec<crate::core::ast::Mapping> = slot_info.iter()
        .zip(args.iter())
        .map(|((slot, slot_type), value)| {
            let coerced = match value {
                Expr::Tuple(items) if schemas.contains_key(slot_type) =>
                    Expr::Call(slot_type.clone(), items.clone()),
                _ => value.clone(),
            };
            crate::core::ast::Mapping { slot: slot.clone(), value: coerced }
        })
        .collect();

    isolate_helper_locals(&subclaim.body, &mut inner, subclaim.param_count);
    let slot_set: std::collections::HashSet<String> =
        mappings.iter().map(|m| m.slot.clone()).collect();
    for m in &mappings {
        let bound = resolve_mapping(&m.slot, &m.value, ctx, env, schemas);
        if bound.is_empty() {
            eprintln!("warning: subschema arg didn't resolve: {:?}", m.value);
        }
        for (k, v) in bound { inner.insert(k, v); }
    }
    let call_id = next_call_id();
    for sub in &subclaim.body {
        if let BodyItem::Membership { name: vname, type_name: vty, .. } = sub {
            if slot_set.contains(vname) { continue; }
            if inner.contains_key(vname) { continue; }
            let z3_name = format!("{}__{}__call{}", claim_name, vname, call_id);
            let post = declare_var_named(ctx, &mut inner, vname, &z3_name,
                              vty, schemas, Some(registry), enums);
            for c in &post { solver.assert(c); }
        }
    }
    inline_body_items_guarded(
        &subclaim.body, &mut inner, solver, schemas, ctx, registry, enums, visited, guard, lenient,
    );
    exit_frame(visited, &qualified);
}
