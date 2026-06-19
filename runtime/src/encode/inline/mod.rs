use std::collections::HashMap;
use z3::{Context, Solver};
use z3::ast::{Ast, Bool};

use crate::core::ast::*;
use crate::core::{DatatypeRegistry, EnumRegistry, Var};
use super::declare::{declare_var, declare_var_named, next_call_id};
use super::exprs::{resolve_mapping, encode_bool};

mod dispatch;
use dispatch::*;

pub(super) fn inline_body_items(
    items: &[BodyItem],
    env: &mut HashMap<String, Var<'static>>,
    solver: &Solver<'static>,
    schemas: &HashMap<String, SchemaDecl>,
    ctx: &'static Context,
    registry: &DatatypeRegistry,
    enums: Option<&EnumRegistry>,
    visited: &mut HashMap<String, usize>,
    lenient: bool,
) {
    inline_body_items_guarded(items, env, solver, schemas, ctx, registry, enums, visited, &None, lenient)
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
    lenient: bool,
) {
    for item in items {
        match item {
            BodyItem::Membership { name, type_name, pins } => {

                if !env.contains_key(name) {
                    let post = declare_var(ctx, env, name, type_name, schemas, Some(registry), enums);
                    for c in &post { solver.assert(c); }
                }

                let resolved_pins: Vec<(String, Expr)> = match pins {
                    crate::core::ast::Pins::None => Vec::new(),
                    crate::core::ast::Pins::Named(maps) => maps.iter()
                        .map(|m| (m.slot.clone(), m.value.clone())).collect(),
                    crate::core::ast::Pins::Positional(args) => {

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

                for (slot, value) in resolved_pins {
                    let lhs = Expr::Identifier(format!("{}.{}", name, slot));
                    let eq = Expr::Binary(
                        crate::core::ast::BinOp::Eq,
                        Box::new(lhs),
                        Box::new(value.clone()),
                    );
                    if let Some(b) = encode_bool(&eq, ctx, env, schemas) {
                        solver.assert(&guarded_bool(b, guard));
                    } else {
                        let pretty = eq.to_string();
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
                            std::process::exit(1);
                        }
                    }
                }

                if let Some(type_schema) = schemas.get(type_name) {
                    let field_set: std::collections::HashSet<String> = type_schema
                        .body
                        .iter()
                        .filter_map(|item| match item {
                            BodyItem::Membership { name: n, .. } => Some(n.clone()),
                            _ => None,
                        })
                        .collect();
                    for item in &type_schema.body {
                        if let BodyItem::Constraint(e) = item {
                            let rewritten = rewrite_idents_with_prefix(e, name, &field_set);
                            if let Some(b) = encode_bool(&rewritten, ctx, env, schemas) {
                                solver.assert(&guarded_bool(b, guard));
                            }

                        }
                    }
                }

                if let Some(inner) = type_name.strip_prefix("Seq(")
                    .and_then(|s| s.strip_suffix(')'))
                {
                    if let Some(inner_schema) = schemas.get(inner) {
                        let len_opt = env.get(name).and_then(|v| {
                            if let Some((_, len, _, _, _)) = v.as_datatype_seq() {
                                len.simplify().as_i64()
                            } else if let Some((_, len, _)) = v.as_seq() {
                                len.simplify().as_i64()
                            } else { None }
                        });
                        if let Some(n) = len_opt {
                            let field_set: std::collections::HashSet<String> =
                                inner_schema.body.iter()
                                    .filter_map(|item| match item {
                                        BodyItem::Membership { name: n, .. } => Some(n.clone()),
                                        _ => None,
                                    })
                                    .collect();
                            for i in 0..n {
                                for item in &inner_schema.body {
                                    if let BodyItem::Constraint(e) = item {

                                        let mut substituted = e.clone();
                                        for fname in &field_set {
                                            let elem = Expr::Field(
                                                Box::new(Expr::Index(
                                                    Box::new(Expr::Identifier(name.clone())),
                                                    Box::new(Expr::Int(i)),
                                                )),
                                                fname.clone(),
                                            );
                                            substituted = substitute_bound_var(
                                                &substituted, fname, &elem);
                                        }
                                        if let Some(b) = encode_bool(
                                            &substituted, ctx, env, schemas)
                                        {
                                            solver.assert(&guarded_bool(b, guard));
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            BodyItem::Constraint(Expr::InExpr(lhs, rhs))
                if matches!(resolve_call_name(rhs.as_ref(), items, schemas),
                    Some(CallDispatch::Subschema { .. }))
                && matches!(lhs.as_ref(), Expr::Tuple(_)) =>
            {
                if !guard_is_satisfiable(solver, guard) { continue; }
                let Some(CallDispatch::Subschema { recv, type_name, claim_name }) =
                    resolve_call_name(rhs.as_ref(), items, schemas) else { unreachable!() };
                let args: Vec<Expr> = match lhs.as_ref() {
                    Expr::Tuple(items) => items.clone(),
                    _ => unreachable!(),
                };
                inline_subschema_call(
                    &recv, &type_name, &claim_name, &args,
                    env, solver, schemas, ctx, registry, enums, visited, guard, lenient,
                );
            }
            BodyItem::Constraint(Expr::InExpr(lhs, rhs))
                if method_dispatch_name_compat(rhs.as_ref(), items, schemas).is_some()
                && matches!(lhs.as_ref(), Expr::Tuple(_)) =>
            {
                if !guard_is_satisfiable(solver, guard) { continue; }
                let (name, receiver) = method_dispatch_name_compat(rhs.as_ref(), items, schemas)
                    .expect("guarded above");
                let mut args: Vec<Expr> = match lhs.as_ref() {
                    Expr::Tuple(items) => items.clone(),
                    _ => unreachable!(),
                };

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
                        for c in &post { solver.assert(c); }
                    }
                }
                inline_body_items_guarded(
                    &claim.body, &mut inner, solver, schemas, ctx, registry, enums, visited, guard, lenient
                );
                exit_frame(visited, &name);
            }

            BodyItem::Constraint(Expr::Call(name, args))
                if matches!(resolve_call(name, items, schemas),
                    Some(CallDispatch::Subschema { .. })) =>
            {
                if !guard_is_satisfiable(solver, guard) { continue; }
                let Some(CallDispatch::Subschema { recv, type_name, claim_name }) =
                    resolve_call(name, items, schemas) else { unreachable!() };
                inline_subschema_call(
                    &recv, &type_name, &claim_name, args,
                    env, solver, schemas, ctx, registry, enums, visited, guard, lenient,
                );
            }
            BodyItem::Constraint(Expr::Call(name, args))
                if method_dispatch_call_compat(name, items, schemas).is_some() =>
            {
                if !guard_is_satisfiable(solver, guard) { continue; }
                let (claim_name, receiver) = method_dispatch_call_compat(name, items, schemas)
                    .expect("guarded above");

                let mut owned_args: Vec<Expr> = args.clone();
                if let Some(recv) = receiver {
                    owned_args.insert(0, Expr::Identifier(recv));
                }
                let args = &owned_args;
                let name = &claim_name;
                let Some(depth) = try_enter(visited, name) else { continue };
                let Some(claim) = schemas.get(name) else { exit_frame(visited, name); continue };

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
                        for c in &post { solver.assert(c); }
                    }
                }
                inline_body_items_guarded(
                    &claim.body, &mut inner, solver, schemas, ctx, registry, enums, visited, guard, lenient
                );
                exit_frame(visited, name);
            }

            BodyItem::Constraint(Expr::Binary(crate::core::ast::BinOp::Implies, ant, cons))
                if matches!(cons.as_ref(),
                    Expr::Identifier(n) if schemas.contains_key(n)) =>
            {
                let claim_name = match cons.as_ref() {
                    Expr::Identifier(n) => n,
                    _ => unreachable!(),
                };
                let Some(ant_bool) = encode_bool(ant, ctx, env, schemas) else {
                    continue;
                };
                let new_guard = compose_guards(ctx, guard, ant_bool);
                if !guard_is_satisfiable(solver, &new_guard) { continue; }
                if try_enter(visited, claim_name).is_none() { continue; }
                let Some(claim) = schemas.get(claim_name) else {
                    exit_frame(visited, claim_name); continue
                };

                let mut inner = env.clone();
                isolate_helper_locals(&claim.body, &mut inner, claim.param_count);
                let call_id = next_call_id();
                for sub in &claim.body {
                    if let BodyItem::Membership { name: vname, type_name, .. } = sub {
                        if inner.contains_key(vname) { continue; }
                        let z3_name = format!("{}__{}__call{}", claim_name, vname, call_id);
                        let post = declare_var_named(ctx, &mut inner, vname, &z3_name,
                                          type_name, schemas, Some(registry), enums);
                        for c in &post { solver.assert(c); }
                    }
                }
                inline_body_items_guarded(
                    &claim.body, &mut inner, solver, schemas, ctx, registry, enums, visited, &new_guard, lenient
                );
                exit_frame(visited, claim_name);
            }

            BodyItem::Constraint(Expr::Forall(vars, range, body))
                if body_contains_subschema_call(body, items, schemas) =>
            {
                if !guard_is_satisfiable(solver, guard) { continue; }
                let Some(iterations) =
                    resolve_forall_unroll(vars, range, env)
                else {

                    let e = Expr::Forall(
                        vars.clone(), range.clone(), body.clone());
                    if let Some(b) = encode_bool(&e, ctx, env, schemas) {
                        solver.assert(&guarded_bool(b, guard));
                    }
                    continue;
                };
                for binds in iterations {
                    let mut item_body: Expr = (**body).clone();
                    for (bound, elem) in &binds {
                        item_body = substitute_bound_var(&item_body, bound, elem);
                    }

                    let item = BodyItem::Constraint(item_body);
                    let mut expanded = items.to_vec();
                    expanded.push(item);
                    let single_slice = &expanded[expanded.len() - 1 ..];

                    let _ = single_slice;

                    if let BodyItem::Constraint(ref e) = expanded[expanded.len() - 1] {
                        if let Expr::Call(name, args) = e {
                            if let Some(CallDispatch::Subschema { recv, type_name, claim_name }) =
                                resolve_call(name, items, schemas)
                            {
                                inline_subschema_call(
                                    &recv, &type_name, &claim_name, args,
                                    env, solver, schemas, ctx, registry,
                                    enums, visited, guard, lenient,
                                );
                                continue;
                            }
                        }
                    }

                    if let BodyItem::Constraint(e) = &expanded[expanded.len() - 1] {
                        if let Some(b) = encode_bool(e, ctx, env, schemas) {
                            solver.assert(&guarded_bool(b, guard));
                        }
                    }
                }
            }

            BodyItem::Constraint(Expr::Identifier(name))
                if schemas.contains_key(name)
                    && !crate::core::ast::BODY_MARKERS.contains(&name.as_str()) =>
            {
                if !guard_is_satisfiable(solver, guard) { continue; }
                if try_enter(visited, name).is_none() { continue; }
                let Some(claim) = schemas.get(name) else {
                    exit_frame(visited, name);
                    continue;
                };
                inline_body_items_guarded(
                    &claim.body, env, solver, schemas, ctx, registry, enums, visited, guard, lenient
                );
                exit_frame(visited, name);
            }
            BodyItem::Constraint(e) => {

                if let crate::core::ast::Expr::Identifier(s) = e {
                    if crate::core::ast::BODY_MARKERS.contains(&s.as_str()) { continue; }
                }
                if let Some(b) = encode_bool(e, ctx, env, schemas) {
                    solver.assert(&guarded_bool(b, guard));
                } else {
                    let pretty = e.to_string();
                    if lenient {
                        eprintln!("warning: dropped constraint (couldn't translate to Bool): {pretty}");
                    } else {
                        eprintln!("error: dropped constraint (couldn't translate to Bool):");
                        eprintln!("       {pretty}");
                        eprintln!();
                        eprintln!("This constraint can't be expressed as a Z3 Bool with the");
                        eprintln!("current translator — almost certainly a translator gap.");
                        eprintln!("Rewrite the constraint to a supported shape.");
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
                    &claim.body, env, solver, schemas, ctx, registry, enums, visited, guard, lenient
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

                let call_id = next_call_id();
                for sub in &claim.body {
                    if let BodyItem::Membership { name: vname, type_name, .. } = sub {
                        let slot_prefix = format!("{}.", vname);
                        let already_bound = inner.contains_key(vname)
                            || inner.keys().any(|k| k.starts_with(&slot_prefix));

                        let force_fresh = depth > 1 && !slot_set.contains(vname);
                        if force_fresh {

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
                            for c in &post { solver.assert(c); }
                        }
                    }
                }
                inline_body_items_guarded(
                    &claim.body, &mut inner, solver, schemas, ctx, registry, enums, visited, guard, lenient
                );
                exit_frame(visited, name);
            }
            BodyItem::SubclaimDecl(_) => {}
        }
    }
}
