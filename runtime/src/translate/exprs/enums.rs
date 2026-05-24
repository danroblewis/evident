//! Enum / Cons-chain helpers. `resolve_enum_ast` translates an
//! expression to an enum-typed Z3 Datatype value; `build_cons_chain`
//! lowers a `⟨a, b, c⟩` literal to `Cons(a, Cons(b, …, Nil))` for a
//! hinted Cons/Nil-shaped enum; `translate_seq_arg_for_ctor` builds the
//! `(Array, Int)` pair for a constructor's `Seq(T)`-typed payload field.

use std::collections::HashMap;
use z3::{Context, DatatypeSort};

use crate::core::ast::*;
use crate::core::Var;

use super::bool::translate_bool;
use super::match_expr::{fold_arms_to_ite, translate_match_arms};
use super::scalar::{translate_int, translate_real, translate_str};
use super::seq_field::{resolve_seq_handle, SeqHandleRef};
use super::{current_target_enum, with_active_enums};

/// Resolve an expression to an enum-typed Z3 Datatype AST. Four shapes:
///
///   * `Identifier(name)` where env has `EnumVar` — the user's `today`
///   * `Identifier(name)` where env has `EnumValue` — bare nullary
///     variant identifier like `Mon`
///   * `Call(name, args)` where env has `EnumCtor` — payload variant
///     constructor application like `Ok(5)` or `Cons(7, Nil)`
///   * `Index(Identifier(seq), idx)` where seq is `Seq(SomeEnum)` —
///     pulls the i-th datatype value out of the seq's underlying
///     Array. Detected via `DatatypeSeqVar` with empty `fields` (the
///     marker we use for Seq(enum) — see declare.rs).
///
/// For Call: each arg is translated against the constructor's declared
/// field type. Recursive payloads (a field whose type is the enum
/// itself, e.g. `LinkedList`) recurse through `resolve_enum_ast` again.
/// Arity mismatches and per-field type translation failures return None
/// (the calling expression then drops as untranslatable).
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
            // Seq(enum) indexing — both the bare Identifier case
            // (`body[i]` for a top-level Seq(EnumType) binding) and
            // the SeqField-of-composite case (`outer[i].field[j]`
            // for a Seq(Composite-with-Seq-EnumType-field) binding).
            // resolve_seq_handle unifies both shapes; we then check
            // the handle is enum-element-shaped (Composite variant
            // with empty `fields` = the Seq-of-enum marker, same
            // as DatatypeSeqVar's empty `fields`).
            let handle = resolve_seq_handle(base.as_ref(), ctx, env)?;
            let SeqHandleRef::Composite { arr, fields, .. } = handle else { return None };
            if !fields.is_empty() { return None; }   // record-style seq, handled elsewhere
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
            // Translate each arg against its declared field type. We
            // need a Vec<Box<dyn Ast>> kind of structure to call
            // ctor.apply, but z3-rs uses `&[&dyn Ast]`. Build the
            // typed Vec then borrow.
            //
            // Seq(T) payload fields are two-accessor-expanded in the
            // Z3 datatype: one logical arg becomes two Z3 values
            // (arr, len). We push both here so the constructor call
            // sees the right physical arg count.
            let mut owned_args: Vec<Box<dyn z3::ast::Ast<'ctx>>> = Vec::new();
            for (arg_expr, field_type) in args.iter().zip(field_types.iter()) {
                if let Some(inner) = crate::core::parse_seq_type(field_type) {
                    // Internal-Cons backing? Look up the helper enum
                    // in the registry; if it exists, the field is a
                    // single Datatype slot, not (arr, len). Build the
                    // Cons chain via build_cons_chain targeted at
                    // __SeqOf_<inner>.
                    let helper_name = crate::core::internal_cons_helper_name(inner);
                    let helper_dt: Option<&'static DatatypeSort<'static>> =
                        with_active_enums(|opt| opt.and_then(|er|
                            er.by_name.borrow().get(&helper_name).map(|(d, _)| *d)));
                    if let Some(helper_dt) = helper_dt {
                        let cons_val = build_cons_chain_from_items(
                            arg_expr, &helper_name, helper_dt, ctx, env, schemas)?;
                        owned_args.push(
                            Box::new(cons_val) as Box<dyn z3::ast::Ast<'ctx>>);
                        continue;
                    }
                    let (arr_dyn, len_dyn) =
                        translate_seq_arg_for_ctor(arg_expr, inner, ctx, env, schemas)?;
                    owned_args.push(arr_dyn);
                    owned_args.push(len_dyn);
                    continue;
                }
                let v: Box<dyn z3::ast::Ast<'ctx>> = match field_type.as_str() {
                    "Int" | "Nat" | "Pos" =>
                        Box::new(translate_int(arg_expr, ctx, env)?),
                    "Bool" =>
                        Box::new(translate_bool(arg_expr, ctx, env, schemas)?),
                    "String" =>
                        Box::new(translate_str(arg_expr, ctx, env)?),
                    "Real" =>
                        Box::new(translate_real(arg_expr, ctx, env)?),
                    _ => {
                        // Either a self-reference or another enum.
                        // Recurse via resolve_enum_ast.
                        Box::new(resolve_enum_ast(arg_expr, ctx, env, schemas)?)
                    }
                };
                owned_args.push(v);
            }
            let arg_refs: Vec<&dyn z3::ast::Ast<'ctx>> =
                owned_args.iter().map(|b| b.as_ref()).collect();
            ctor.apply(&arg_refs).as_datatype()
        }
        // `cond ? a : b` with enum-typed branches → Z3 ITE on Datatype.
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
        // ⟨a, b, c⟩ as a Cons-chain over a hinted enum (set by
        // translate_bool's Eq path when the LHS is enum-typed).
        // Hint flows through Match arms via the body translator.
        Expr::SeqLit(items) => {
            let (enum_name, dt) = current_target_enum()?;
            build_cons_chain(items, &enum_name, dt, ctx, env, schemas)
        }
        _ => None,
    }
}

/// Build a (Array, Int) pair for an enum-constructor's Seq-typed
/// payload field. Two source shapes:
///
///   * `Identifier(name)` resolving to `Var::SeqVar` /
///     `Var::DatatypeSeqVar` — pull (arr, len) out directly.
///   * `Expr::SeqLit(items)` — build a Z3 Array literal by
///     starting from a constant-array (default value) and
///     storing each item at its index. Length is the item count.
///
/// Used by the `Call`-case constructor-application path when a
/// variant field's declared type is `Seq(T)`. The two-accessor
/// expansion in the enum loader means the underlying Z3
/// constructor expects two args (arr_sort, Int) for this slot.
fn translate_seq_arg_for_ctor<'ctx>(
    arg_expr: &Expr,
    inner_type: &str,
    ctx: &'ctx Context,
    env: &HashMap<String, Var<'ctx>>,
    schemas: &HashMap<String, SchemaDecl>,
) -> Option<(Box<dyn z3::ast::Ast<'ctx> + 'ctx>, Box<dyn z3::ast::Ast<'ctx> + 'ctx>)> {
    use z3::Sort;
    use z3::ast::{Array, Bool, Int, String as Z3Str};

    // Identifier: pull (arr, len) out of an existing Seq variable.
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

    // SeqLit: build an Array literal via successive `store`s on a
    // constant-array seeded with a default value of the right sort.
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
                let default = Z3Str::from_str(ctx, "").ok()?;
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
            // Enum element: use Array::fresh_const (unconstrained Z3
            // array of the right sort) as the base, then store each
            // translated enum constructor at its index. Values past
            // `len` are unconstrained — extract_seq truncates at len.
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

/// Build `Cons(items[0], Cons(items[1], ..., Nil))` for a hinted
/// Cons/Nil-shaped enum. Returns the resulting Datatype value.
/// Build a Cons-chain Datatype value from an `Expr` argument that
/// can be either a SeqLit (build it from items) or an Identifier
/// (already a Cons-shaped variable in env — return its value).
/// Used by enum-constructor-call translation for `Seq(T)` fields
/// that the runtime backs with an internal `__SeqOf_T` helper.
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
            // Three identifier shapes we accept here:
            //   * Var::EnumVar of the helper's sort — already Cons-
            //     shaped, return its ast directly.
            //   * Var::DatatypeSeqVar (top-level `Seq(T)` Array+Int
            //     representation) — the user is passing it as a
            //     "don't-care" Cons-field arg (typical literal_types.
            //     ev existential pattern). Materialize a FRESH Cons
            //     constant of the helper's sort; Z3 picks freely.
            //     The Array+Int value and this Cons constant are
            //     independent; if the user needs them linked,
            //     they'd express that constraint explicitly.
            //   * Anything else — None.
            match env.get(name)? {
                Var::EnumVar { ast, .. } => Some(ast.clone()),
                // Nullary variant identifier (e.g. `__Empty_SchemaDecl`,
                // or a user-named empty list value) — already a
                // pre-applied constructor of the helper's sort.
                Var::EnumValue { ast, .. } => Some(ast.clone()),
                Var::DatatypeSeqVar { .. } => {
                    Some(z3::ast::Datatype::fresh_const(
                        ctx, "__cons_view", &dt.sort))
                }
                _ => None,
            }
        }
        // Cons-call like `Cell_Tree(head, tail)` — resolve as enum.
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
