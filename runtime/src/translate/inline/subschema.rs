//! Subclaim-of-type ("use a field of a schema as a subschema")
//! invocation inlining:
//!
//!   * `inline_subschema_call`   — `recv.subclaim(args)` / `(args) ∈ recv.subclaim`
//!   * `inline_forall_subschema` — `∀ … : recv.subclaim(args)` (AST unroll)
//!
//! Both rebind the receiver's record fields onto the subclaim's
//! bare-name fields so the subclaim body's references resolve to the
//! receiver's leaves, then recurse into `walk::inline_body_items_guarded`.

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

/// Inline a subclaim-of-type invocation `recv.subclaim(args)`.
///
/// The "use a field of a schema as a subschema" form: the
/// receiver's record fields get rebound onto T's bare-name
/// fields so the subclaim body's references resolve to the
/// receiver's leaves. Caller has already confirmed that `recv`
/// is a body Membership of `type_name` and that `claim_name`
/// is a subclaim inside `type_name`'s body.
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

    // Recursion guard via the standard visited map.
    let qualified = format!("{}.{}", type_name, claim_name);
    let Some(_depth) = try_enter(visited, &qualified) else { return; };

    // Build inner env starting from outer. The Z3 vars themselves
    // are shared (same constants); we just add bare-name aliases
    // for each parent-type field so the subclaim's body sees
    // `renderer` and resolves to `recv.renderer`.
    let mut inner = env.clone();
    // Parent-type fields = top-level Memberships inside the type's
    // body (NOT subclaim decls or constraints). For each leaf key
    // in env starting with `recv.`, mirror it without the prefix.
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

    // Slot info: the subclaim's first-line params (its leading
    // body Memberships up to `args.len()` if needed). Subclaims
    // don't have first-line params today, so this is just the
    // leading body Memberships.
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
    // Tuple-as-record-literal coercion (same as positional Call arm).
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

/// `∀ vars ∈ range : body` where `body` contains a method-style
/// subclaim invocation (`recv.subclaim(args)`).
///
/// translate_bool's ∀ translator runs the body through
/// translate_bool, which has no solver access and can't
/// fire the subclaim's per-iteration assertions (the
/// `out = ⟨…⟩` pin inside the subclaim body never lands,
/// leaving outputs free; see COUNTEREXAMPLES #26).
///
/// Fix: expand the ∀ at AST level for known-length ranges
/// (coindexed of pinned-length Seqs, or a bare pinned Seq).
/// Each iteration becomes a regular BodyItem the inline
/// pass can dispatch — subclaim calls get full solver
/// access via inline_subschema_call as usual.
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
        // Range shape not statically unrollable —
        // fall through to the regular Constraint
        // translation path. The subclaim assertions
        // will drop, but the user gets the same
        // behavior as before this fix.
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
        // Append the substituted body as the next item of
        // the OUTER `items` slice and dispatch through
        // inline_body_items_guarded again. Keeping the
        // outer items in scope is important: `resolve_call`
        // walks `items` to find the receiver's type (e.g.
        // `win ∈ SDL_Window`) for method-style subclaim
        // dispatch. Passing only the substituted item
        // would lose those Memberships.
        let item = BodyItem::Constraint(item_body);
        let mut expanded = items.to_vec();
        expanded.push(item);
        let single_slice = &expanded[expanded.len() - 1 ..];
        // ↑ a slice of just the new item, but its
        // `items` siblings (passed via the outer `items`
        // arg below) include all of the surrounding body.
        let _ = single_slice;
        // We can't easily slice the merged vec without
        // hitting borrow-checker issues; instead, dispatch
        // the single item directly here without recursion.
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
        // Fall back: regular Constraint translation.
        if let BodyItem::Constraint(e) = &expanded[expanded.len() - 1] {
            if let Some(b) = translate_bool(e, ctx, env, schemas) {
                track_assert(solver, &guarded_bool(b, guard), tracker);
            }
        }
    }
}
