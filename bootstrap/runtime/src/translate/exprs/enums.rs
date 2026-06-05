//! Enum / Cons-chain helpers: `resolve_enum_ast` → enum-typed Z3 Datatype;
//! `build_cons_chain` → Cons/Nil literal; `translate_seq_arg_for_ctor` → `(Array,Int)` payload.

use std::collections::HashMap;
use z3::{Context, DatatypeSort};

use crate::core::ast::*;
use crate::core::Var;

use super::bool::translate_bool;
use super::match_expr::{fold_arms_to_ite, translate_match_arms};
use super::scalar::{translate_int, translate_real, translate_str};
use super::seq_field::{resolve_seq_handle, SeqHandleRef};
use super::{current_target_enum, with_active_enums};

/// Resolve an expression to an enum-typed Z3 Datatype AST: EnumVar/EnumValue/EnumCtor,
/// Seq(enum) index, Ternary, Match, and SeqLit Cons-chains.
pub(super) fn resolve_enum_ast<'ctx>(
    e: &Expr,
    ctx: &'ctx Context,
    env: &HashMap<String, Var<'ctx>>,
    schemas: &HashMap<String, SchemaDecl>,
) -> Option<z3::ast::Datatype<'ctx>> {
    match e {
        Expr::Identifier(name) => match env.get(name)? {
            Var::EnumVar { ast, .. }   => Some(ast.clone()),
            Var::EnumValue { ast, .. } => Some(ast.clone()),
            _ => None,
        },
        Expr::Index(base, idx) => {
            // Seq(enum) indexing via resolve_seq_handle; Composite handle with empty fields = Seq(enum).
            let handle = resolve_seq_handle(base.as_ref(), ctx, env)?;
            let SeqHandleRef::Composite { arr, fields, .. } = handle else { return None };
            if !fields.is_empty() { return None; }
            let i = translate_int(idx, ctx, env)?;
            arr.select(&i).as_datatype()
        }
        Expr::Call(name, args) => {
            let ctor_info = env.get(name)?;
            let (dt, variant_idx, field_types) = match ctor_info {
                Var::EnumCtor { dt, variant_idx, field_types, .. } =>
                    (*dt, *variant_idx, field_types.clone()),
                _ => return None,
            };
            if args.len() != field_types.len() { return None; }
            let ctor = &dt.variants[variant_idx].constructor;
            // Seq(T) fields are two-accessor-expanded (arr, len); push both.
            let mut owned_args: Vec<Box<dyn z3::ast::Ast<'ctx>>> = Vec::new();
            for (arg_expr, field_type) in args.iter().zip(field_types.iter()) {
                if let Some(inner) = crate::core::parse_seq_type(field_type) {
                    // Internal-Cons backed field: single Datatype slot, not (arr, len).
                    let helper_name = crate::core::internal_cons_helper_name(inner);
                    let helper_dt: Option<&'static DatatypeSort<'static>> =
                        with_active_enums(|opt| opt.and_then(|er|
                            er.by_name.borrow().get(&helper_name).map(|(d, _)| *d)));
                    if let Some(helper_dt) = helper_dt {
                        let cons_val = build_cons_chain_from_items(
                            arg_expr, &helper_name, helper_dt, ctx, env, schemas)?;
                        owned_args.push(Box::new(cons_val) as Box<dyn z3::ast::Ast<'ctx>>);
                        continue;
                    }
                    let (arr_dyn, len_dyn) =
                        translate_seq_arg_for_ctor(arg_expr, inner, ctx, env, schemas)?;
                    owned_args.push(arr_dyn);
                    owned_args.push(len_dyn);
                    continue;
                }
                let v: Box<dyn z3::ast::Ast<'ctx>> = match field_type.as_str() {
                    "Int" | "Nat" | "Pos" => Box::new(translate_int(arg_expr, ctx, env)?),
                    "Bool" => Box::new(translate_bool(arg_expr, ctx, env, schemas)?),
                    "String" => Box::new(translate_str(arg_expr, ctx, env)?),
                    "Real" => Box::new(translate_real(arg_expr, ctx, env)?),
                    _ => Box::new(resolve_enum_ast(arg_expr, ctx, env, schemas)?),
                };
                owned_args.push(v);
            }
            let arg_refs: Vec<&dyn z3::ast::Ast<'ctx>> =
                owned_args.iter().map(|b| b.as_ref()).collect();
            ctor.apply(&arg_refs).as_datatype()
        }
        Expr::Ternary(c, a, b) => {
            let cond = translate_bool(c, ctx, env, schemas)?;
            let then_v = resolve_enum_ast(a, ctx, env, schemas)?;
            let else_v = resolve_enum_ast(b, ctx, env, schemas)?;
            Some(cond.ite(&then_v, &else_v))
        }
        Expr::Match(scr, arms) => {
            let compiled = translate_match_arms(scr, arms, ctx, env,
                |body, e| resolve_enum_ast(body, ctx, e, schemas))?;
            fold_arms_to_ite(compiled)
        }
        // ⟨a,b,c⟩ → Cons-chain; hint set by translate_bool Eq path when LHS is enum-typed.
        Expr::SeqLit(items) => {
            let (enum_name, dt) = current_target_enum()?;
            build_cons_chain(items, &enum_name, dt, ctx, env, schemas)
        }
        _ => None,
    }
}

/// Build `(Array, Int)` for a constructor's Seq-typed payload field.
/// Identifier → pull from SeqVar; SeqLit → build Z3 Array via successive stores.
fn translate_seq_arg_for_ctor<'ctx>(
    arg_expr: &Expr,
    inner_type: &str,
    ctx: &'ctx Context,
    env: &HashMap<String, Var<'ctx>>,
    schemas: &HashMap<String, SchemaDecl>,
) -> Option<(Box<dyn z3::ast::Ast<'ctx> + 'ctx>, Box<dyn z3::ast::Ast<'ctx> + 'ctx>)> {
    use z3::Sort;
    use z3::ast::{Array, Bool, Int};

    if let Expr::Identifier(name) = arg_expr {
        if let Some(var) = env.get(name) {
            if let Some((arr, len, _elem)) = var.as_seq() {
                return Some((
                    Box::new(arr.clone()) as Box<dyn z3::ast::Ast<'ctx>>,
                    Box::new(len.clone()) as Box<dyn z3::ast::Ast<'ctx>>,
                ));
            }
            if let Some((arr, len, _name, _dt, _fields)) = var.as_datatype_seq() {
                return Some((
                    Box::new(arr.clone()) as Box<dyn z3::ast::Ast<'ctx>>,
                    Box::new(len.clone()) as Box<dyn z3::ast::Ast<'ctx>>,
                ));
            }
        }
    }

    if let Expr::SeqLit(items) = arg_expr {
        let n = items.len() as i64;
        let len_int = Int::from_i64(ctx, n);
        match inner_type {
            "Int" | "Nat" | "Pos" => {
                let mut arr = Array::const_array(
                    ctx, &Sort::int(ctx), &Int::from_i64(ctx, 0));
                for (i, item) in items.iter().enumerate() {
                    let v = translate_int(item, ctx, env)?;
                    arr = arr.store(&Int::from_i64(ctx, i as i64), &v);
                }
                return Some((
                    Box::new(arr) as Box<dyn z3::ast::Ast<'ctx>>,
                    Box::new(len_int) as Box<dyn z3::ast::Ast<'ctx>>,
                ));
            }
            "Bool" => {
                let mut arr = Array::const_array(
                    ctx, &Sort::int(ctx), &Bool::from_bool(ctx, false));
                for (i, item) in items.iter().enumerate() {
                    let v = translate_bool(item, ctx, env, schemas)?;
                    arr = arr.store(&Int::from_i64(ctx, i as i64), &v);
                }
                return Some((
                    Box::new(arr) as Box<dyn z3::ast::Ast<'ctx>>,
                    Box::new(len_int) as Box<dyn z3::ast::Ast<'ctx>>,
                ));
            }
            "String" => {
                let default = crate::translate::z3_string(ctx, "").ok()?;
                let mut arr = Array::const_array(ctx, &Sort::int(ctx), &default);
                for (i, item) in items.iter().enumerate() {
                    let v = translate_str(item, ctx, env)?;
                    arr = arr.store(&Int::from_i64(ctx, i as i64), &v);
                }
                return Some((
                    Box::new(arr) as Box<dyn z3::ast::Ast<'ctx>>,
                    Box::new(len_int) as Box<dyn z3::ast::Ast<'ctx>>,
                ));
            }
            // Enum element: fresh_const base array; values past len are unconstrained.
            enum_type => {
                let dt: &'static z3::DatatypeSort<'static> = with_active_enums(|opt| {
                    let reg = opt?;
                    reg.by_name.borrow().get(enum_type).map(|(d, _)| *d)
                })?;
                let mut arr = z3::ast::Array::fresh_const(
                    ctx, "__seq_payload", &Sort::int(ctx), &dt.sort);
                for (i, item) in items.iter().enumerate() {
                    let v = resolve_enum_ast(item, ctx, env, schemas)?;
                    arr = arr.store(&Int::from_i64(ctx, i as i64), &v);
                }
                return Some((
                    Box::new(arr) as Box<dyn z3::ast::Ast<'ctx> + 'ctx>,
                    Box::new(len_int) as Box<dyn z3::ast::Ast<'ctx> + 'ctx>,
                ));
            }
        }
    }

    None
}

/// Build a Cons-chain from a SeqLit or pass through an existing Cons-shaped identifier.
/// Used for `Seq(T)` constructor fields backed by `__SeqOf_T` helper.
pub(super) fn build_cons_chain_from_items<'ctx>(
    arg: &Expr,
    enum_name: &str,
    dt: &'static DatatypeSort<'static>,
    ctx: &'ctx Context,
    env: &HashMap<String, Var<'ctx>>,
    schemas: &HashMap<String, SchemaDecl>,
) -> Option<z3::ast::Datatype<'ctx>> {
    match arg {
        Expr::SeqLit(items) =>
            build_cons_chain(items, enum_name, dt, ctx, env, schemas),
        Expr::Identifier(name) => {
            match env.get(name)? {
                Var::EnumVar { ast, .. } => Some(ast.clone()),
                Var::EnumValue { ast, .. } => Some(ast.clone()),
                // DatatypeSeqVar = don't-care Cons arg; materialize fresh const.
                Var::DatatypeSeqVar { .. } =>
                    Some(z3::ast::Datatype::fresh_const(ctx, "__cons_view", &dt.sort)),
                _ => None,
            }
        }
        _ => resolve_enum_ast(arg, ctx, env, schemas),
    }
}

pub(super) fn build_cons_chain<'ctx>(
    items: &[Expr],
    enum_name: &str,
    dt: &'static DatatypeSort<'static>,
    ctx: &'ctx Context,
    env: &HashMap<String, Var<'ctx>>,
    schemas: &HashMap<String, SchemaDecl>,
) -> Option<z3::ast::Datatype<'ctx>> {
    let (nil_idx, cons_idx, elem_type) = with_active_enums(|enums_opt| {
        let enums = enums_opt?;
        let by_name = enums.by_name.borrow();
        let (_, decl_variants) = by_name.get(enum_name)?;
        let nil_idx = decl_variants.iter().position(|v| v.fields.is_empty())?;
        let cons_idx = decl_variants.iter().position(|v|
            v.fields.len() == 2 && v.fields[1].type_name == enum_name)?;
        let elem_type = decl_variants[cons_idx].fields[0].type_name.clone();
        Some((nil_idx, cons_idx, elem_type))
    })?;

    let mut acc = dt.variants[nil_idx].constructor.apply(&[]).as_datatype()?;
    for item in items.iter().rev() {
        let elem_dyn: z3::ast::Dynamic<'ctx> = match elem_type.as_str() {
            "Int" | "Nat" | "Pos" => translate_int(item, ctx, env)?.into(),
            "Bool"                => translate_bool(item, ctx, env, schemas)?.into(),
            "String"              => translate_str(item, ctx, env)?.into(),
            "Real"                => translate_real(item, ctx, env)?.into(),
            _                     => resolve_enum_ast(item, ctx, env, schemas)?.into(),
        };
        acc = dt.variants[cons_idx].constructor
            .apply(&[&elem_dyn, &acc])
            .as_datatype()?;
    }
    Some(acc)
}
