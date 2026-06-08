//! Claim invocation inlining: `(args) ∈ claim`, `claim(args)`, `cond ⇒ Claim`,
//! `Claim(slot ↦ val)`. Clones env, isolates locals, recurses into walker.

use std::collections::HashMap;

use z3::{Context, Solver};
use z3::ast::Bool;

use crate::core::ast::*;
use crate::core::{DatatypeRegistry, EnumRegistry, Var};
use crate::translate::declare::{declare_var_named, next_call_id};
use crate::translate::exprs::{resolve_mapping, translate_bool};
use super::dispatch::{method_dispatch_call_compat, method_dispatch_name_compat};
use super::guards::{compose_guards, guard_is_satisfiable, track_assert};
use super::recursion::{exit_frame, isolate_helper_locals, try_enter};
use super::walk::inline_body_items_guarded;

/// `(args) ∈ recv.claim_name` — receiver prepended as first positional arg.
#[allow(clippy::too_many_arguments)]
pub(super) fn inline_tuple_in_claim(
    lhs: &Expr,
    rhs: &Expr,
    items: &[BodyItem],
    env: &mut HashMap<String, Var<'static>>,
    solver: &Solver<'static>,
    schemas: &HashMap<String, SchemaDecl>,
    ctx: &'static Context,
    registry: &DatatypeRegistry,
    enums: Option<&EnumRegistry>,
    visited: &mut HashMap<String, usize>,
    guard: &Option<Bool<'static>>,
    tracker: Option<&Bool<'static>>,
) {
    if !guard_is_satisfiable(solver, guard) { return; }
    let (name, receiver) = method_dispatch_name_compat(rhs, items, schemas)
        .expect("guarded above");
    let mut args: Vec<Expr> = match lhs {
        Expr::Tuple(items) => items.clone(),
        _ => unreachable!(),
    };
    if let Some(recv) = receiver {
        args.insert(0, Expr::Identifier(recv));
    }
    let Some(depth) = try_enter(visited, &name) else { return };
    let Some(claim) = schemas.get(&name) else {
        exit_frame(visited, &name); return
    };
    let slot_info: Vec<(String, String)> = claim.body.iter()
        .filter_map(|i| if let BodyItem::Membership { name, type_name, .. } = i {
            Some((name.clone(), type_name.clone()))
        } else { None })
        .take(args.len())
        .collect();
    if slot_info.len() != args.len() {
        eprintln!(
            "warning: tuple-in-claim `(...) ∈ {}` got {} args but \
             the claim has only {} param Memberships",
            name, args.len(), slot_info.len()
        );
        exit_frame(visited, &name);
        return;
    }
    // Tuple-as-record coercion: `(a,b)` → `RecordType(a,b)` when slot type is known.
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
    let _ = depth;
    let mut inner = env.clone();
    isolate_helper_locals(&claim.body, &mut inner, claim.param_count);
    let slot_set: std::collections::HashSet<String> =
        mappings.iter().map(|m| m.slot.clone()).collect();
    for m in &mappings {
        let bound = resolve_mapping(&m.slot, &m.value, ctx, env, schemas);
        if bound.is_empty() {
            eprintln!("warning: tuple-in-claim arg didn't resolve: {:?}", m.value);
        }
        for (k, v) in bound {
            inner.insert(k, v);
        }
    }
    let call_id = next_call_id();
    for sub in &claim.body {
        if let BodyItem::Membership { name: vname, type_name, .. } = sub {
            if slot_set.contains(vname) { continue; }
            if inner.contains_key(vname) { continue; }
            let z3_name = format!("{}__{}__call{}", name, vname, call_id);
            let post = declare_var_named(ctx, &mut inner, vname, &z3_name,
                              type_name, schemas, Some(registry), enums);
            for c in &post { track_assert(solver, c, tracker); }
        }
    }
    inline_body_items_guarded(
        &claim.body, &mut inner, solver, schemas, ctx, registry, enums, visited, guard, tracker
    );
    exit_frame(visited, &name);
}

/// Positional claim invocation `claim(args)` / `recv.claim(args)`.
#[allow(clippy::too_many_arguments)]
pub(super) fn inline_positional_call(
    name: &str,
    args: &[Expr],
    items: &[BodyItem],
    env: &mut HashMap<String, Var<'static>>,
    solver: &Solver<'static>,
    schemas: &HashMap<String, SchemaDecl>,
    ctx: &'static Context,
    registry: &DatatypeRegistry,
    enums: Option<&EnumRegistry>,
    visited: &mut HashMap<String, usize>,
    guard: &Option<Bool<'static>>,
    tracker: Option<&Bool<'static>>,
) {
    if !guard_is_satisfiable(solver, guard) { return; }
    let (claim_name, receiver) = method_dispatch_call_compat(name, items, schemas)
        .expect("guarded above");
    let mut owned_args: Vec<Expr> = args.to_vec();
    if let Some(recv) = receiver {
        owned_args.insert(0, Expr::Identifier(recv));
    }
    let args = &owned_args;
    let name = &claim_name;
    let Some(depth) = try_enter(visited, name) else { return };
    let Some(claim) = schemas.get(name) else { exit_frame(visited, name); return };

    // Pair args with first N Membership items (first-line params desugar to head Memberships).
    let slot_info: Vec<(String, String)> = claim.body.iter()
        .filter_map(|i| if let BodyItem::Membership { name, type_name, .. } = i {
            Some((name.clone(), type_name.clone()))
        } else { None })
        .take(args.len())
        .collect();
    if slot_info.len() != args.len() {
        eprintln!(
            "warning: positional ClaimCall to `{}` got {} args but \
             the claim has only {} param Memberships",
            name, args.len(), slot_info.len()
        );
        exit_frame(visited, name);
        return;
    }
    // Tuple-as-record coercion: `(a,b)` → `RecordType(a,b)` when slot type is known.
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

    // Isolate helper locals so each invocation gets fresh Z3 consts;
    // without this, recursive helpers collapse consts across calls → UNSAT.
    let _ = depth;
    let mut inner = env.clone();
    isolate_helper_locals(&claim.body, &mut inner, claim.param_count);
    let slot_set: std::collections::HashSet<String> =
        mappings.iter().map(|m| m.slot.clone()).collect();
    for m in &mappings {
        let bound = resolve_mapping(&m.slot, &m.value, ctx, env, schemas);
        if bound.is_empty() {
            eprintln!("warning: positional arg didn't resolve: {:?}", m.value);
        }
        for (k, v) in bound {
            inner.insert(k, v);
        }
    }
    let call_id = next_call_id();
    for sub in &claim.body {
        if let BodyItem::Membership { name: vname, type_name, .. } = sub {
            if slot_set.contains(vname) { continue; }
            if inner.contains_key(vname) { continue; }
            let z3_name = format!("{}__{}__call{}", name, vname, call_id);
            let post = declare_var_named(ctx, &mut inner, vname, &z3_name,
                              type_name, schemas, Some(registry), enums);
            for c in &post { track_assert(solver, c, tracker); }
        }
    }
    inline_body_items_guarded(
        &claim.body, &mut inner, solver, schemas, ctx, registry, enums, visited, guard, tracker
    );
    exit_frame(visited, name);
}

/// `cond ⇒ ClaimName` — inlines the body with each constraint wrapped in
/// `cond ⇒ …`; declarations fire unconditionally. Composes with outer guard.
#[allow(clippy::too_many_arguments)]
pub(super) fn inline_guarded_claim(
    ant: &Expr,
    cons: &Expr,
    env: &mut HashMap<String, Var<'static>>,
    solver: &Solver<'static>,
    schemas: &HashMap<String, SchemaDecl>,
    ctx: &'static Context,
    registry: &DatatypeRegistry,
    enums: Option<&EnumRegistry>,
    visited: &mut HashMap<String, usize>,
    guard: &Option<Bool<'static>>,
    tracker: Option<&Bool<'static>>,
) {
    let claim_name = match cons {
        Expr::Identifier(n) => n,
        _ => unreachable!(),
    };
    let Some(ant_bool) = translate_bool(ant, ctx, env, schemas) else {
        return;
    };
    let new_guard = compose_guards(ctx, guard, ant_bool);
    if !guard_is_satisfiable(solver, &new_guard) { return; }
    if try_enter(visited, claim_name).is_none() { return; }
    let Some(claim) = schemas.get(claim_name) else {
        exit_frame(visited, claim_name); return
    };
    // Fresh per-call locals for names-match: sharing Z3 consts across
    // recursive invocations breaks correctness, so we always isolate.
    let mut inner = env.clone();
    isolate_helper_locals(&claim.body, &mut inner, claim.param_count);
    let call_id = next_call_id();
    for sub in &claim.body {
        if let BodyItem::Membership { name: vname, type_name, .. } = sub {
            if inner.contains_key(vname) { continue; }
            let z3_name = format!("{}__{}__call{}", claim_name, vname, call_id);
            let post = declare_var_named(ctx, &mut inner, vname, &z3_name,
                              type_name, schemas, Some(registry), enums);
            for c in &post { track_assert(solver, c, tracker); }
        }
    }
    inline_body_items_guarded(
        &claim.body, &mut inner, solver, schemas, ctx, registry, enums, visited, &new_guard, tracker
    );
    exit_frame(visited, claim_name);
}

/// `ClaimName(slot ↦ value, …)` — explicit-mapping call. Each invocation
/// gets per-call suffixes on unmapped internal vars to avoid Z3 const collisions.
#[allow(clippy::too_many_arguments)]
pub(super) fn inline_claim_call(
    name: &str,
    mappings: &[Mapping],
    env: &mut HashMap<String, Var<'static>>,
    solver: &Solver<'static>,
    schemas: &HashMap<String, SchemaDecl>,
    ctx: &'static Context,
    registry: &DatatypeRegistry,
    enums: Option<&EnumRegistry>,
    visited: &mut HashMap<String, usize>,
    guard: &Option<Bool<'static>>,
    tracker: Option<&Bool<'static>>,
) {
    if !guard_is_satisfiable(solver, guard) { return; }
    let Some(depth) = try_enter(visited, name) else { return };
    let Some(claim) = schemas.get(name) else {
        eprintln!("warning: ClaimCall to unknown claim {}", name);
        exit_frame(visited, name);
        return;
    };
    let mut inner = env.clone();
    let slot_set: std::collections::HashSet<String> =
        mappings.iter().map(|m| m.slot.clone()).collect();
    for m in mappings {
        let bound = resolve_mapping(&m.slot, &m.value, ctx, env, schemas);
        if bound.is_empty() {
            eprintln!("warning: mapping value didn't resolve: {:?}", m.value);
        }
        for (k, v) in bound {
            inner.insert(k, v);
        }
    }
    // Per-call suffix ensures two invocations of the same claim get distinct
    // Z3 consts (e.g. two AxisPhysics calls share no `intended` var).
    let call_id = next_call_id();
    for sub in &claim.body {
        if let BodyItem::Membership { name: vname, type_name, .. } = sub {
            let slot_prefix = format!("{}.", vname);
            let already_bound = inner.contains_key(vname)
                || inner.keys().any(|k| k.starts_with(&slot_prefix));
            let force_fresh = depth > 1 && !slot_set.contains(vname);
            if force_fresh {
                // Pop inherited entry so the fresh decl isn't skipped by idempotence.
                inner.remove(vname);
                let dotted: Vec<String> = inner.keys()
                    .filter(|k| k.starts_with(&slot_prefix))
                    .cloned().collect();
                for k in dotted { inner.remove(&k); }
            }
            if !already_bound || force_fresh {
                let z3_name = format!("{}__{}__call{}", name, vname, call_id);
                let post = declare_var_named(ctx, &mut inner, vname, &z3_name,
                                  type_name, schemas, Some(registry), enums);
                for c in &post { track_assert(solver, c, tracker); }
            }
        }
    }
    inline_body_items_guarded(
        &claim.body, &mut inner, solver, schemas, ctx, registry, enums, visited, guard, tracker
    );
    exit_frame(visited, name);
}
