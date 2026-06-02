//! `Membership` body-item handler: declare, fire type-use pins, and inherit
//! the type's body Constraints (including per-element for `Seq(SomeType)`).

use std::collections::HashMap;

use z3::{Context, Solver};
use z3::ast::{Ast, Bool};

use crate::core::ast::*;
use crate::core::{DatatypeRegistry, EnumRegistry, Var};
use crate::translate::declare::declare_var;
use crate::translate::exprs::translate_bool;
use super::guards::{guarded_bool, track_assert};
use super::rewrite::{rewrite_idents_with_prefix, substitute_bound_var};

#[allow(clippy::too_many_arguments)]
pub(super) fn inline_membership(
    name: &str,
    type_name: &str,
    pins: &Pins,
    env: &mut HashMap<String, Var<'static>>,
    solver: &Solver<'static>,
    schemas: &HashMap<String, SchemaDecl>,
    ctx: &'static Context,
    registry: &DatatypeRegistry,
    enums: Option<&EnumRegistry>,
    guard: &Option<Bool<'static>>,
    tracker: Option<&Bool<'static>>,
) {
    // Pass-1 pre-declares top-level Memberships; this no-ops there, but
    // is needed for passthrough bodies with vars not yet in env.
    if !env.contains_key(name) {
        let post = declare_var(ctx, env, name, type_name, schemas, Some(registry), enums);
        for c in &post { track_assert(solver, c, tracker); }
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
            // Partial ok (leading fields); too many args is an error.
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
        if let Some(b) = translate_bool(&eq, ctx, env, schemas) {
            track_assert(solver, &guarded_bool(b, guard), tracker);
        } else {
            let pretty = format!("{eq:?}");
            {
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

    // Inherit type body Constraints onto this instance (prefix field refs with
    // `name.`). Skips builtins (Int/Nat/Seq/…) which have no body.
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
                if let Some(b) = translate_bool(&rewritten, ctx, env, schemas) {
                    track_assert(solver, &guarded_bool(b, guard), tracker);
                }
                // Silently skip on translation failure (passthrough shapes etc);
                // hard-fail applies only to direct body items of the calling schema.
            }
        }
    }

    // Inherit SomeType's body Constraints per element of `Seq(SomeType)`,
    // substituting bare field refs → `Field(Index(name, i), field_name)`.
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
                                        Box::new(Expr::Identifier(name.to_string())),
                                        Box::new(Expr::Int(i)),
                                    )),
                                    fname.clone(),
                                );
                                substituted = substitute_bound_var(
                                    &substituted, fname, &elem);
                            }
                            if let Some(b) = translate_bool(
                                &substituted, ctx, env, schemas)
                            {
                                track_assert(solver, &guarded_bool(b, guard), tracker);
                            }
                        }
                    }
                }
            }
        }
    }
}
