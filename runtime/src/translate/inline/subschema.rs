//! Subclaim-of-type invocation: `recv.subclaim(args)` and `∀ … : recv.subclaim(args)`.
//! Rebinds receiver fields onto bare-name fields so the subclaim body resolves correctly.

use std::collections::HashMap;

use z3::{Context, Solver};
use z3::ast::Bool;

use crate::core::ast::*;
use crate::core::{DatatypeRegistry, EnumRegistry, Var};
use crate::translate::declare::{declare_var_named, next_call_id};
use crate::translate::exprs::{resolve_mapping, translate_bool};
use super::dispatch::{CallDispatch, resolve_call, resolve_forall_unroll};
use super::guards::{guard_is_satisfiable, guarded_bool, track_assert};
use super::recursion::{exit_frame, isolate_helper_locals, try_enter};
use super::rewrite::substitute_bound_var;
use super::walk::inline_body_items_guarded;

/// Inline `recv.subclaim(args)`: mirrors receiver's fields into bare names so the subclaim body resolves.
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
    tracker: Option<&Bool<'static>>,
) {
    // Look up the subclaim inside the type's body.
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

    // Clone env, then mirror `recv.*` keys as bare names so the subclaim body sees `renderer` not `recv.renderer`.
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

    // Subclaims have no first-line params today; use leading body Memberships as slots.
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
    // Coerce Tuple args to record literals for matching slot types.
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
            for c in &post { track_assert(solver, c, tracker); }
        }
    }
    inline_body_items_guarded(
        &subclaim.body, &mut inner, solver, schemas, ctx, registry, enums, visited, guard, tracker,
    );
    exit_frame(visited, &qualified);
}

/// `∀ vars ∈ range : body` where body contains a subclaim call.
/// translate_bool has no solver access so subclaim assertions drop (COUNTEREXAMPLES #26); expand statically instead.
#[allow(clippy::too_many_arguments)]
pub(super) fn inline_forall_subschema(
    vars: &[String],
    range: &Expr,
    body: &Expr,
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
    let Some(iterations) =
        resolve_forall_unroll(vars, range, env)
    else {
        // Range not statically unrollable; fall through. Subclaim assertions will drop.
        let e = Expr::Forall(
            vars.to_vec(), Box::new(range.clone()), Box::new(body.clone()));
        if let Some(b) = translate_bool(&e, ctx, env, schemas) {
            track_assert(solver, &guarded_bool(b, guard), tracker);
        }
        return;
    };
    for binds in iterations {
        let mut item_body: Expr = body.clone();
        for (bound, elem) in &binds {
            item_body = substitute_bound_var(&item_body, bound, elem);
        }
        // Append to outer `items` so resolve_call can find the receiver's type Membership.
        let item = BodyItem::Constraint(item_body);
        let mut expanded = items.to_vec();
        expanded.push(item);
        // Dispatch just the new item; can't slice without borrow-checker issues.
        if let BodyItem::Constraint(ref e) = expanded[expanded.len() - 1] {
            if let Expr::Call(name, args) = e {
                if let Some(CallDispatch::Subschema { recv, type_name, claim_name }) =
                    resolve_call(name, items, schemas)
                {
                    inline_subschema_call(
                        &recv, &type_name, &claim_name, args,
                        env, solver, schemas, ctx, registry,
                        enums, visited, guard, tracker,
                    );
                    continue;
                }
            }
        }
        // Fall back to regular Constraint translation.
        if let BodyItem::Constraint(e) = &expanded[expanded.len() - 1] {
            if let Some(b) = translate_bool(e, ctx, env, schemas) {
                track_assert(solver, &guarded_bool(b, guard), tracker);
            }
        }
    }
}
