//! `inline_body_items` — the recursive constraint-translation walker.
//! Handles `Membership` (declare-if-new), `Constraint` (translate +
//! assert), `Passthrough` (`..ClaimName`), and `ClaimCall` (with
//! mappings + per-call fresh Z3 names for the claim's unmapped
//! internals).
//!
//! Bare-identifier-as-passthrough (`Constraint(Identifier(name))`
//! where `name` is a known claim) is handled BEFORE this walker
//! runs by a self-hosted desugar pass —
//! `stdlib/passes/desugar_passthrough.ev` paired with
//! `commands/desugar.rs::auto_apply_desugar`, wired into every CLI
//! subcommand. By the time the AST arrives here, the rewrite has
//! already turned the bare form into an explicit `Passthrough` node.

use std::collections::{HashMap, HashSet};
use z3::{Context, SatResult, Solver};
use z3::ast::Bool;

use crate::ast::*;
use crate::pretty;
use super::types::{DatatypeRegistry, EnumRegistry, Var};
use super::declare::{declare_var, declare_var_named, next_call_id};
use super::exprs::{resolve_mapping, translate_bool};

/// Add `b` to the solver. With a tracker, use `assert_and_track` so
/// the constraint joins the unsat-core machinery; otherwise plain
/// `assert`. The tracker stays the same across every assertion derived
/// from one top-level body item, so the entire item shows up as one
/// entry in the core.
fn track_assert(solver: &Solver<'static>, b: &Bool<'static>, tracker: Option<&Bool<'static>>) {
    match tracker {
        Some(t) => solver.assert_and_track(b, t),
        None    => solver.assert(b),
    }
}

/// Recursively translate a list of body items into the solver. Used by
/// the constraint-translation pass of both `evaluate` and `build_cache`,
/// and also called recursively when a `Passthrough`, bare-identifier
/// passthrough, or `ClaimCall` references another claim's body.
///
/// Without this, passthroughs only inlined `Constraint` items — any
/// `ClaimCall` (e.g. `PlayerPhysics(state mapsto state.player, …)`)
/// inside a passthrough was silently dropped. Same problem inside a
/// `ClaimCall`: nested claim calls in the called claim's body were
/// dropped. That broke `..DotCollectGameEngine` (no player, no physics,
/// no background — black screen).
///
/// `visited` is a per-claim depth counter that bounds inlining
/// recursion. Each entry maps a claim name to how many frames of
/// it are currently on the inlining stack. A frame can re-enter the
/// same claim up to `MAX_INLINE_DEPTH` times — enough to walk a
/// recursive AST (transpilers, list emitters, etc.) but bounded so
/// pathological self-passthrough cycles don't OOM. Without unrolling
/// at all, the transpiler-as-recursive-claims pattern doesn't work
/// (Z3 invents arbitrary string values for un-asserted `tail_out`
/// bindings). The depth bound is overridable via
/// `EVIDENT_MAX_INLINE_DEPTH` for ASTs deeper than the default.

/// Default cap — large enough for any realistic shader/transpiler AST,
/// small enough that a self-passthrough loop trips it before the
/// translation context blows out.
const DEFAULT_MAX_INLINE_DEPTH: usize = 64;

fn max_inline_depth() -> usize {
    std::env::var("EVIDENT_MAX_INLINE_DEPTH")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_MAX_INLINE_DEPTH)
}

/// Try to enter a frame of `name` on the inlining stack. Returns
/// `Some(depth)` (the post-increment count) on success, `None` if
/// we'd exceed the depth cap. `depth > 1` ⇒ this is a recursive
/// frame; callers use that to force fresh per-call declarations
/// for body-internal Memberships, otherwise the env-clone would
/// shadow them with outer-scope vars and recursive claims would
/// self-reference (e.g. `out = "x " ++ tail_out` where `tail_out`
/// is the SAME Z3 const as the outer call's `tail_out`).
fn try_enter(visited: &mut HashMap<String, usize>, name: &str) -> Option<usize> {
    let max = max_inline_depth();
    let cnt = visited.entry(name.to_string()).or_insert(0);
    if *cnt >= max {
        None
    } else {
        *cnt += 1;
        Some(*cnt)
    }
}

/// Counterpart to `try_enter` — call after the inlined body has been
/// translated. Removes the entry entirely when its count hits zero
/// so subsequent same-name lookups don't see stale state.
fn exit_frame(visited: &mut HashMap<String, usize>, name: &str) {
    if let Some(cnt) = visited.get_mut(name) {
        *cnt -= 1;
        if *cnt == 0 { visited.remove(name); }
    }
}

/// Pre-isolate helper-local Z3 consts: when entering a ClaimCall, any
/// caller-scope vars whose names match the called claim's body
/// Memberships PAST `param_count` (i.e. the helper's internal locals,
/// not its first-line input/output slots) are removed from the cloned
/// inner env. This prevents recursive helper invocations from
/// accidentally sharing locals via the env-clone chain — without it,
/// nested `emit_ternary(...)` would reuse the OUTER `emit_ternary`'s
/// `cnd`/`thn`/`els` Z3 consts, collapsing distinct AST values to one
/// const and going UNSAT.
///
/// Slot params (the leading body Memberships up to `param_count`) are
/// PRESERVED in the clone so the helper's body can reach the
/// outer-supplied values via names-match composition.
fn isolate_helper_locals(
    body: &[BodyItem],
    inner: &mut HashMap<String, Var<'static>>,
    param_count: usize,
) {
    // When the claim has no first-line params (param_count == 0), we
    // can't tell input slots from helper-locals — fall back to the
    // legacy names-match behavior: keep everything in the cloned env
    // so body Memberships that match outer scope re-use those Z3
    // consts. Helpers that NEED isolation (transpiler-style recursive
    // claims) must use first-line params to declare which body
    // Memberships are inputs.
    if param_count == 0 { return; }
    for (i, item) in body.iter().enumerate() {
        if i < param_count { continue; } // input/output slot — keep.
        if let BodyItem::Membership { name, .. } = item {
            inner.remove(name);
            let prefix = format!("{}.", name);
            let dotted: Vec<String> = inner.keys()
                .filter(|k| k.starts_with(&prefix)).cloned().collect();
            for k in dotted { inner.remove(&k); }
        }
    }
}

/// Returns true if the active inlining guard is satisfiable (or there
/// is no guard). Used to PRUNE recursive ClaimCall expansion when the
/// guard is provably false — the body would generate only dead
/// constraints (Z3 would prove them vacuously true), so skipping the
/// inline saves the translation cost.
///
/// Without this prune, recursive transpiler-style claims (e.g.
/// `e_is_binary ⇒ emit_binary` where `emit_binary` calls `emit_expr`
/// on subexpressions) cascade unconditionally — each level multiplies
/// the inlined body count even though most branches won't fire.
///
/// Implementation: push the guard into the solver, ask Z3 if it's
/// satisfiable in the current scope, pop. Z3 prunes propositional
/// contradictions in microseconds; this lets the depth bound do its
/// real job (cutting genuine cycles) instead of bounding work-per-node.
fn guard_is_satisfiable(
    solver: &Solver<'static>,
    guard: &Option<Bool<'static>>,
) -> bool {
    let g = match guard {
        None => return true,
        Some(g) => g,
    };
    let trace = std::env::var("EVIDENT_INLINE_TRACE").is_ok();
    let t0 = if trace { Some(std::time::Instant::now()) } else { None };
    solver.push();
    solver.assert(g);
    let result = solver.check();
    solver.pop(1);
    if let Some(t0) = t0 {
        eprintln!("[inline] sat-check {:?} in {:?}", result, t0.elapsed());
    }
    !matches!(result, SatResult::Unsat)
}
/// Combine guard + body Bool: `guard ⇒ body` if guarded, else just
/// the body. Operates on already-translated Z3 Bool asts so the guard's
/// resolution is FROZEN at the point a guarded claim was entered —
/// subsequent shadowing in deeper recursive frames can't accidentally
/// rebind the guard's identifiers to fresh per-frame consts of the
/// same name. (That bug used to silently make recursive transpilers
/// emit unconstrained outputs because the depth-1 `e_is_unaryneg`
/// guard, when consumed at depth-2, resolved to depth-2's freshly
/// shadowed `e_is_unaryneg` — which Z3 had constrained to `false`
/// because the inner expression isn't a UnaryNeg.)
fn guarded_bool<'ctx>(b: Bool<'ctx>, guard: &Option<Bool<'ctx>>) -> Bool<'ctx> {
    match guard {
        None => b,
        Some(g) => g.implies(&b),
    }
}

/// Compose two pre-translated guards: `outer ∧ inner`.
fn compose_guards<'ctx>(
    ctx: &'ctx z3::Context,
    outer: &Option<Bool<'ctx>>,
    inner: Bool<'ctx>,
) -> Option<Bool<'ctx>> {
    match outer {
        None => Some(inner),
        Some(o) => Some(Bool::and(ctx, &[o, &inner])),
    }
}

/// Method-call dispatch: split a (possibly dotted) call name on its
/// last `.` and check whether the suffix is a known schema. Returns
/// `Some((claim_name, Some(receiver)))` for method-style
/// `recv.claim(args)`; `Some((claim_name, None))` for plain
/// `claim(args)`; `None` when no segment matches a schema.
///
/// The receiver string keeps its dots intact (`win.renderer` for
/// `win.renderer.set_draw_color(...)`) so it can be re-emitted as
/// an `Identifier` and resolved through env's dotted leaf keys
/// (`win.renderer` lives in env as an IntVar from the SDL_Window
/// FTI expansion).
fn method_dispatch_call(
    name: &str,
    schemas: &HashMap<String, SchemaDecl>,
) -> Option<(String, Option<String>)> {
    if schemas.contains_key(name) {
        return Some((name.to_string(), None));
    }
    let (prefix, suffix) = name.rsplit_once('.')?;
    if schemas.contains_key(suffix) {
        return Some((suffix.to_string(), Some(prefix.to_string())));
    }
    None
}

/// Same dispatch logic for the `(args) ∈ recv.claim_name` form,
/// where the RHS is an `Identifier`. Returns
/// `Some((claim_name, Option<receiver>))` or `None`.
fn method_dispatch_name(
    rhs: &Expr,
    schemas: &HashMap<String, SchemaDecl>,
) -> Option<(String, Option<String>)> {
    let Expr::Identifier(n) = rhs else { return None; };
    method_dispatch_call(n, schemas)
}

pub(super) fn inline_body_items(
    items: &[BodyItem],
    env: &mut HashMap<String, Var<'static>>,
    solver: &Solver<'static>,
    schemas: &HashMap<String, SchemaDecl>,
    ctx: &'static Context,
    registry: &DatatypeRegistry,
    enums: Option<&EnumRegistry>,
    visited: &mut HashMap<String, usize>,
) {
    inline_body_items_guarded(items, env, solver, schemas, ctx, registry, enums, visited, &None, None)
}

/// Translate `items` and assert each derived constraint into the
/// solver, additionally tagging every assertion with one of `trackers`
/// so a later `solver.get_unsat_core()` can name the offending
/// top-level body item. `trackers[i]` corresponds to `items[i]` —
/// passing fewer trackers than items means tail items go untracked.
/// Used by `evaluate_with_core` to surface unsat-cores back to the
/// test runner; the regular `evaluate` path passes `None` for
/// zero overhead.
pub(super) fn inline_body_items_tracked(
    items: &[BodyItem],
    env: &mut HashMap<String, Var<'static>>,
    solver: &Solver<'static>,
    schemas: &HashMap<String, SchemaDecl>,
    ctx: &'static Context,
    registry: &DatatypeRegistry,
    enums: Option<&EnumRegistry>,
    visited: &mut HashMap<String, usize>,
    trackers: &[Bool<'static>],
) {
    for (idx, item) in items.iter().enumerate() {
        let tracker = trackers.get(idx);
        let slice = std::slice::from_ref(item);
        inline_body_items_guarded(
            slice, env, solver, schemas, ctx, registry, enums, visited, &None, tracker,
        );
    }
}

fn inline_body_items_guarded(
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
    for item in items {
        match item {
            BodyItem::Membership { name, type_name, pins } => {
                // Top-level Memberships are pre-declared by pass 1, so the
                // declare_var call is a no-op there. Useful when the helper
                // recurses into a passthrough's body that introduces
                // variables not yet in env (e.g. a nested claim's locals).
                if !env.contains_key(name) {
                    let post = declare_var(ctx, env, name, type_name, schemas, Some(registry), enums);
                    for c in &post { track_assert(solver, c, tracker); }
                }
                // Resolve `pins` to a list of (field-name, value-expr)
                // pairs. Named is direct; Positional looks up the type's
                // body Membership order to map positions to field names.
                let resolved_pins: Vec<(String, Expr)> = match pins {
                    crate::ast::Pins::None => Vec::new(),
                    crate::ast::Pins::Named(maps) => maps.iter()
                        .map(|m| (m.slot.clone(), m.value.clone())).collect(),
                    crate::ast::Pins::Positional(args) => {
                        // Look up the type's field order from its
                        // SchemaDecl. Strict count match required.
                        let Some(schema) = schemas.get(type_name) else {
                            eprintln!(
                                "error: positional pin on unknown type `{}`",
                                type_name
                            );
                            std::process::exit(1);
                        };
                        let field_order: Vec<String> = schema.body.iter()
                            .filter_map(|item| match item {
                                BodyItem::Membership { name, .. } => Some(name.clone()),
                                _ => None,
                            })
                            .collect();
                        // Partial allowed: too few args pin the leading
                        // fields and leave the rest free. Too many is
                        // a real error — the user is asking to pin
                        // fields that don't exist.
                        if args.len() > field_order.len() {
                            eprintln!(
                                "error: too many positional pins on `{}`: \
                                 type declares {} fields, got {} args",
                                type_name, field_order.len(), args.len()
                            );
                            std::process::exit(1);
                        }
                        field_order.into_iter()
                            .zip(args.iter().cloned())
                            .collect()
                    }
                };
                // Fire each pin as `name.field = value`. Same machinery
                // and same dropped-constraint policy as a regular
                // Constraint — a pin to a non-existent field is the
                // same kind of silent error as a generic dropped
                // translation, so it shares the hard-fail behavior.
                for (slot, value) in resolved_pins {
                    let lhs = Expr::Identifier(format!("{}.{}", name, slot));
                    let eq = Expr::Binary(
                        crate::ast::BinOp::Eq,
                        Box::new(lhs),
                        Box::new(value.clone()),
                    );
                    if let Some(b) = translate_bool(&eq, ctx, env, schemas) {
                        track_assert(solver, &guarded_bool(b, guard), tracker);
                    } else {
                        let lenient = std::env::var("EVIDENT_LENIENT")
                            .map(|v| !v.is_empty() && v != "0")
                            .unwrap_or(false);
                        let pretty = pretty::expr(&eq);
                        if lenient {
                            eprintln!(
                                "warning: type-use pin didn't translate: {}",
                                pretty
                            );
                        } else {
                            eprintln!(
                                "error: type-use pin didn't translate: {}",
                                pretty
                            );
                            eprintln!();
                            eprintln!(
                                "The field `{}` probably doesn't exist on type `{}`,",
                                slot, type_name
                            );
                            eprintln!(
                                "or its type doesn't accept the pinned value's shape."
                            );
                            eprintln!(
                                "Set EVIDENT_LENIENT=1 to demote this to a warning."
                            );
                            std::process::exit(1);
                        }
                    }
                }
            }
            // Bare-identifier-as-passthrough handling moved to the
            // self-hosted desugar pass (`stdlib/passes/desugar_passthrough.ev`
            // + `commands/desugar.rs::auto_apply_desugar`). By the time
            // any constraint arrives here, the rewrite has already turned
            // `Constraint(Identifier(name))` (when `name` is a known
            // claim) into `Passthrough(name)`. Bare-identifier
            // constraints whose name does NOT match a claim fall through
            // to the regular Constraint arm below, same as before.

            // Positional claim invocation: `Constraint(Call(name, args))`
            // whose `name` matches a known claim is treated as a
            // ClaimCall with positional args bound by index to the
            // claim's first-line params (or first N Memberships in
            // declaration order). Encourages the
            //   claim Foo(items ∈ Seq, keys ∈ Seq, asc ∈ Bool)
            //       …body using items/keys/asc…
            //   ⋮
            //   Foo(my_items, my_keys, true)
            // pattern over the longer mapsto form.
            // Tuple-in-claim invocation: `(a, b, c) ∈ claim_name` is
            // the relational reading of a positional claim call —
            // "this tuple is in the set of satisfying assignments."
            // Desugars to the same positional ClaimCall path as
            // `claim_name(a, b, c)`. The dispatch is here (not in
            // exprs.rs's InExpr handler) because it has to fire at
            // BodyItem position, not as a value-producing
            // expression — a claim invocation contributes constraints,
            // not a Bool.
            BodyItem::Constraint(Expr::InExpr(lhs, rhs))
                if method_dispatch_name(rhs.as_ref(), schemas).is_some()
                && matches!(lhs.as_ref(), Expr::Tuple(_)) =>
            {
                if !guard_is_satisfiable(solver, guard) { continue; }
                let (name, receiver) = method_dispatch_name(rhs.as_ref(), schemas)
                    .expect("guarded above");
                let mut args: Vec<Expr> = match lhs.as_ref() {
                    Expr::Tuple(items) => items.clone(),
                    _ => unreachable!(),
                };
                // Method-style: `(args) ∈ recv.claim_name` prepends
                // `Identifier(recv)` as the first positional arg.
                if let Some(recv) = receiver {
                    args.insert(0, Expr::Identifier(recv));
                }
                let Some(depth) = try_enter(visited, &name) else { continue };
                let Some(claim) = schemas.get(&name) else {
                    exit_frame(visited, &name); continue
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
                    continue;
                }
                // Tuple-as-record-literal coercion (same rule as the
                // positional-Call arm above) for nested `(a, b, c)`
                // args whose slot is a known record type.
                let mappings: Vec<crate::ast::Mapping> = slot_info.iter()
                    .zip(args.iter())
                    .map(|((slot, slot_type), value)| {
                        let coerced = match value {
                            Expr::Tuple(items) if schemas.contains_key(slot_type) =>
                                Expr::Call(slot_type.clone(), items.clone()),
                            _ => value.clone(),
                        };
                        crate::ast::Mapping { slot: slot.clone(), value: coerced }
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
            BodyItem::Constraint(Expr::Call(name, args))
                if method_dispatch_call(name, schemas).is_some() =>
            {
                if !guard_is_satisfiable(solver, guard) { continue; }
                let (claim_name, receiver) = method_dispatch_call(name, schemas)
                    .expect("guarded above");
                // Method-style: `recv.claim_name(args)` prepends
                // `Identifier(recv)` to the positional args.
                let mut owned_args: Vec<Expr> = args.clone();
                if let Some(recv) = receiver {
                    owned_args.insert(0, Expr::Identifier(recv));
                }
                let args = &owned_args;
                let name = &claim_name;
                let Some(depth) = try_enter(visited, name) else { continue };
                let Some(claim) = schemas.get(name) else { exit_frame(visited, name); continue };

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
                    continue;
                }
                // Tuple-as-record-literal coercion: when an arg is a
                // bare `(a, b, c)` Tuple AND the slot's type names a
                // known record schema, rewrite to `Call(type, items)`
                // — the existing record-literal-in-expression-position
                // path. Lets the user write
                //   set_draw_color((220, 40, 40, 255), out)
                // instead of `Color(220, 40, 40, 255)`.
                let mappings: Vec<crate::ast::Mapping> = slot_info.iter()
                    .zip(args.iter())
                    .map(|((slot, slot_type), value)| {
                        let coerced = match value {
                            Expr::Tuple(items) if schemas.contains_key(slot_type) =>
                                Expr::Call(slot_type.clone(), items.clone()),
                            _ => value.clone(),
                        };
                        crate::ast::Mapping { slot: slot.clone(), value: coerced }
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
            // Guarded claim invocation: `cond ⇒ ClaimName` inlines the
            // claim's body but wraps each constraint in `cond ⇒ …`.
            // Declarations from the claim fire unconditionally; the
            // guard only narrows what the constraints assert. Composes
            // with an outer guard if we're already inside one.
            BodyItem::Constraint(Expr::Binary(crate::ast::BinOp::Implies, ant, cons))
                if matches!(cons.as_ref(),
                    Expr::Identifier(n) if schemas.contains_key(n)) =>
            {
                let claim_name = match cons.as_ref() {
                    Expr::Identifier(n) => n,
                    _ => unreachable!(),
                };
                let Some(ant_bool) = translate_bool(ant, ctx, env, schemas) else {
                    continue;
                };
                let new_guard = compose_guards(ctx, guard, ant_bool);
                if !guard_is_satisfiable(solver, &new_guard) { continue; }
                if try_enter(visited, claim_name).is_none() { continue; }
                let Some(claim) = schemas.get(claim_name) else {
                    exit_frame(visited, claim_name); continue
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
            BodyItem::Constraint(e) => {
                // Recognized runtime markers (declared in
                // `crate::ast::BODY_MARKERS`) are bare identifiers
                // that carry metadata for some other runtime layer
                // — they have no Bool translation. Skip silently
                // so they don't trip the dropped-constraint diagnostic.
                if let crate::ast::Expr::Identifier(s) = e {
                    if crate::ast::BODY_MARKERS.contains(&s.as_str()) { continue; }
                }
                if let Some(b) = translate_bool(e, ctx, env, schemas) {
                    track_assert(solver, &guarded_bool(b, guard), tracker);
                } else {
                    let lenient = std::env::var("EVIDENT_LENIENT")
                        .map(|v| !v.is_empty() && v != "0")
                        .unwrap_or(false);
                    let pretty = pretty::expr(e);
                    if lenient {
                        eprintln!("warning: dropped constraint (couldn't translate to Bool): {pretty}");
                    } else {
                        eprintln!("error: dropped constraint (couldn't translate to Bool):");
                        eprintln!("       {pretty}");
                        eprintln!();
                        eprintln!("This constraint can't be expressed as a Z3 Bool with the");
                        eprintln!("current translator — almost certainly a translator gap.");
                        eprintln!("Either rewrite the constraint to a supported shape, or");
                        eprintln!("set EVIDENT_LENIENT=1 to demote this to a warning.");
                        std::process::exit(1);
                    }
                }
            }
            BodyItem::Passthrough(claim_name) => {
                if !guard_is_satisfiable(solver, guard) { continue; }
                if try_enter(visited, claim_name).is_none() { continue; }
                let Some(claim) = schemas.get(claim_name) else {
                    eprintln!("warning: ..{} references unknown claim", claim_name);
                    exit_frame(visited, claim_name);
                    continue;
                };
                inline_body_items_guarded(
                    &claim.body, env, solver, schemas, ctx, registry, enums, visited, guard, tracker
                );
                exit_frame(visited, claim_name);
            }
            BodyItem::ClaimCall { name, mappings } => {
                if !guard_is_satisfiable(solver, guard) { continue; }
                let Some(depth) = try_enter(visited, name) else { continue };
                let Some(claim) = schemas.get(name) else {
                    eprintln!("warning: ClaimCall to unknown claim {}", name);
                    exit_frame(visited, name);
                    continue;
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
            BodyItem::SubclaimDecl(_) => {}
        }
    }
}
