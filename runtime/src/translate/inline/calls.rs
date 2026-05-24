//! Top-level claim invocation inlining. Each function here is one
//! body-item dispatch arm extracted from the walker:
//!
//!   * `inline_tuple_in_claim`   — `(args) ∈ recv.claim`
//!   * `inline_positional_call`  — `claim(args)` / `recv.claim(args)`
//!   * `inline_guarded_claim`    — `cond ⇒ ClaimName`
//!   * `inline_claim_call`       — `ClaimName(slot ↦ value, …)`
//!
//! (Subclaim-of-type invocations — `recv.subclaim(args)` and the
//! `∀`-unrolled form — live in the sibling `subschema` module.)
//!
//! All of them clone the caller env, isolate the helper's locals,
//! declare per-call fresh Z3 consts for unmapped body Memberships,
//! then recurse into `walk::inline_body_items_guarded`.

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

/// `(args) ∈ recv.claim_name` — the relational reading of a positional
/// claim call. Method-style: the receiver is prepended as the first
/// positional arg.
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
    // Method-style: `(args) ∈ recv.claim_name` prepends
    // `Identifier(recv)` as the first positional arg.
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
    // Tuple-as-record-literal coercion (same rule as the
    // positional-Call arm above) for nested `(a, b, c)`
    // args whose slot is a known record type.
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

/// Positional claim invocation `claim(args)` (or `recv.claim(args)`,
/// where the receiver is prepended as the first positional arg).
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
    // Method-style: `recv.claim_name(args)` prepends
    // `Identifier(recv)` to the positional args.
    let mut owned_args: Vec<Expr> = args.to_vec();
    if let Some(recv) = receiver {
        owned_args.insert(0, Expr::Identifier(recv));
    }
    let args = &owned_args;
    let name = &claim_name;
    let Some(depth) = try_enter(visited, name) else { return };
    let Some(claim) = schemas.get(name) else { exit_frame(visited, name); return };

    // Pair positional args with the claim's first N Membership
    // body items (which include first-line params, since
    // those desugar to Memberships at the head of the body).
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
    // Tuple-as-record-literal coercion: when an arg is a
    // bare `(a, b, c)` Tuple AND the slot's type names a
    // known record schema, rewrite to `Call(type, items)`
    // — the existing record-literal-in-expression-position
    // path. Lets the user write
    //   set_draw_color((220, 40, 40, 255), out)
    // instead of `Color(220, 40, 40, 255)`.
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

    // Pre-isolate: the called claim's body Memberships
    // are LOCAL to this invocation. Any caller-scope vars
    // sharing those names are removed from `inner` so the
    // body either gets the slot-mapped value (if the name
    // is a slot param) or a per-call fresh declaration.
    // Without this, recursive helpers (transpiler-style)
    // that share body-Membership names with the caller's
    // scope collapse Z3 consts across invocations and go
    // UNSAT for inputs that disagree on the shared field.
    let _ = depth;
    let mut inner = env.clone();
    // Slot params (the first param_count body items) are
    // explicit inputs — caller's value goes here. Body
    // Memberships past that index are helper-locals; clear
    // any same-named entries inherited from the caller's
    // scope so they don't collide across nested helper
    // invocations.
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

/// Guarded claim invocation: `cond ⇒ ClaimName` inlines the
/// claim's body but wraps each constraint in `cond ⇒ …`.
/// Declarations from the claim fire unconditionally; the
/// guard only narrows what the constraints assert. Composes
/// with an outer guard if we're already inside one.
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
    // Names-match invocation: clone env, isolate names
    // that clash with the helper's body Memberships,
    // declare those fresh per-call. Each sibling/nested
    // invocation thus gets its own Z3 consts for body
    // locals, rather than collapsing to the caller's
    // same-named entries (see comment in the positional
    // ClaimCall arm above for the recursive transpiler
    // case that drove this design).
    //
    // Note we DON'T cherry-pick "input slot" names from
    // outer — body Memberships that reference outer
    // values via names-match already exist in `env` (the
    // caller's scope), and the constraints they generate
    // assert against the FRESH per-call consts. Those
    // constraints then form an equivalence (the helper
    // sets `out = ...` where `out` is the fresh local;
    // upstream callers explicitly pass their `out` via
    // the surrounding positional ClaimCall's slot
    // mapping, which equates them through the assertion
    // chain). Sharing the Z3 const directly is an
    // optimization that breaks recursion, so we drop it.
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

/// `ClaimName(slot ↦ value, …)` — the explicit-mapping claim call.
/// Each invocation gets a per-call suffix on the Z3 name for the
/// claim's unmapped internal variables so two invocations of the
/// same claim get distinct Z3 constants.
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
    // Declare any of the claim's own variables that weren't
    // bound by a mapping (the claim's "internal" parameters,
    // like AxisPhysics's `intended` / `target`). Each
    // invocation gets a per-call suffix on the Z3 name so
    // two invocations of the same claim get distinct Z3
    // constants — without this, both AxisPhysics calls in
    // PlayerPhysics share one `intended` Z3 var and the
    // x-axis vs. y-axis branches contradict → UNSAT.
    let call_id = next_call_id();
    for sub in &claim.body {
        if let BodyItem::Membership { name: vname, type_name, .. } = sub {
            let slot_prefix = format!("{}.", vname);
            let already_bound = inner.contains_key(vname)
                || inner.keys().any(|k| k.starts_with(&slot_prefix));
            // See positional-call arm: recursive frames
            // shadow body-internal Memberships so each
            // invocation gets a fresh Z3 const.
            let force_fresh = depth > 1 && !slot_set.contains(vname);
            if force_fresh {
                // declare_var_named's idempotence guard
                // would skip the re-declaration; pop the
                // inherited entry first so the fresh decl
                // sticks.
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
