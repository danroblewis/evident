//! AST `Expr` → Z3 expression translators (Int / Bool / String) and
//! the helpers they share. Also `resolve_mapping` / `expr_as_var` for
//! `ClaimCall` mapping resolution; `translate_seq_lit_eq` and
//! `translate_seq_index_assign` for the two seq-equality shapes that
//! aren't pure scalar `_eq`.
//!
//! File layout (top-to-bottom):
//!   1. Thread-local context — active EnumRegistry pointer, target
//!      enum hint for SeqLit-as-Cons-chain lowering, RAII guards.
//!   2. ClaimCall mapping resolution — `resolve_mapping`, `expr_as_var`.
//!   3. Enum / Cons-chain helpers — `resolve_enum_ast`, `build_cons_chain`.
//!   4. Seq field resolution — `resolve_seq_field`.
//!   5. Per-sort translators — `translate_str`, `translate_int`,
//!      `translate_real`.
//!   6. Real-literal helpers — `real_from_f64` and friends.
//!   7. Record / vector lifting — `lift_record_op`, leaf enumeration,
//!      record-ref substitution. The plumbing for componentwise
//!      operations on `IVec2`-style record types.
//!   8. Seq-equality translation — `translate_seq_lit_eq`,
//!      `translate_seq_index_assign`, composite Seq plumbing.
//!   9. `translate_bool` — the big Bool dispatcher (~500 LOC).
//!  10. Match-expression translator — `translate_match_arms`,
//!      `fold_arms_to_ite`.
//!  11. Literal-range folder — `literal_range`, used by
//!      `translate_bool`'s quantifier-unroll path.

use std::collections::HashMap;
use z3::ast::{Ast, Bool, Int, Real, String as Z3Str};
use z3::{Context, DatatypeSort};

use crate::ast::*;
use super::types::{env_clone, EnumRegistry, FieldKind, SeqElem, Value, Var};

// ── Section 1: Thread-local context (active enums + target hint) ─────

thread_local! {
    /// Active EnumRegistry for the current translation. Set by
    /// `with_enums(...)` (called from each `evaluate*` entry point in
    /// eval.rs) and restored on drop. Read by `translate_match_arms`
    /// to look up the DatatypeSort of a payload field whose declared
    /// type is itself an enum (so the binding can become a proper
    /// `Var::EnumVar` for further pattern matching).
    ///
    /// Stored as a raw `*const EnumRegistry` because the registry's
    /// lifetime is tied to `EvidentRuntime` (which lives for the whole
    /// translation), but we can't carry a `'static` reference through
    /// thread-locals. The pointer is set/cleared via the RAII guard
    /// `EnumRegistryGuard`; readers borrow it back as `&EnumRegistry`
    /// inside the guard's lifetime.
    static ACTIVE_ENUMS: std::cell::Cell<Option<*const EnumRegistry>> =
        const { std::cell::Cell::new(None) };
}

/// RAII guard: stash an EnumRegistry pointer in thread-local for the
/// duration of a translation. Restores the previous value on drop so
/// nested calls compose correctly.
pub struct EnumRegistryGuard {
    prev: Option<*const EnumRegistry>,
}

impl EnumRegistryGuard {
    pub fn new(enums: Option<&EnumRegistry>) -> Self {
        let new_ptr = enums.map(|r| r as *const EnumRegistry);
        let prev = ACTIVE_ENUMS.with(|c| {
            let was = c.get();
            c.set(new_ptr);
            was
        });
        Self { prev }
    }
}

impl Drop for EnumRegistryGuard {
    fn drop(&mut self) {
        ACTIVE_ENUMS.with(|c| c.set(self.prev));
    }
}

/// Run `f` with the active EnumRegistry borrowed if one is set.
fn with_active_enums<R>(f: impl FnOnce(Option<&EnumRegistry>) -> R) -> R {
    let ptr = ACTIVE_ENUMS.with(|c| c.get());
    // SAFETY: `ptr` was set by an EnumRegistryGuard whose Drop hasn't
    // run yet (translation is single-threaded, the guard outlives the
    // call stack that uses it).
    let opt = ptr.map(|p| unsafe { &*p });
    f(opt)
}

thread_local! {
    /// Currently expected enum type for SeqLit-as-Cons-chain lowering
    /// inside enum-typed contexts. Set by `translate_bool`'s Eq path
    /// when the LHS is enum-typed; read by `resolve_enum_ast`'s
    /// SeqLit arm. Holds (enum_name, dt).
    static TARGET_ENUM_HINT: std::cell::RefCell<Option<(String, &'static DatatypeSort<'static>)>> =
        const { std::cell::RefCell::new(None) };
}

/// Run `f` with `target` as the current SeqLit-target hint. Restores
/// the previous value on return so nested calls compose.
fn with_target_enum_hint<R>(
    target: Option<(String, &'static DatatypeSort<'static>)>,
    f: impl FnOnce() -> R,
) -> R {
    let prev = TARGET_ENUM_HINT.with(|c| c.replace(target));
    let r = f();
    TARGET_ENUM_HINT.with(|c| { *c.borrow_mut() = prev; });
    r
}

fn current_target_enum() -> Option<(String, &'static DatatypeSort<'static>)> {
    TARGET_ENUM_HINT.with(|c| c.borrow().clone())
}

// ── Section 2: ClaimCall mapping resolution ──────────────────────────

/// Resolve a mapping-value expression to one-or-more `(env-key, Var)`
/// bindings to install in the inner env when entering a ClaimCall.
///
/// Three resolution paths, tried in order:
///   1. Sub-schema mapping: the value is a dotted identifier (e.g.
///      `state.player`) AND no env binding exists for that exact name,
///      but multiple env keys share it as a prefix (`state.player.x`,
///      `state.player.y`, …). Each matched leaf is bound under
///      `slot.field`. This matches the Python translator's behavior
///      for `state mapsto state.player`.
///   2. Leaf identifier or literal: `expr_as_var` produces a single
///      `Var`, bound to `slot` directly.
///   3. Otherwise → empty (caller logs a warning).
pub(super) fn resolve_mapping<'ctx>(
    slot: &str,
    value: &Expr,
    ctx: &'ctx Context,
    env: &HashMap<String, Var<'ctx>>,
    schemas: &HashMap<String, SchemaDecl>,
) -> Vec<(String, Var<'ctx>)> {
    if let Expr::Identifier(name) = value {
        // If the exact name is in env, prefer leaf binding.
        if env.contains_key(name) {
            return vec![(slot.to_string(), env[name].clone())];
        }
        // Otherwise try sub-schema expansion: gather every env key
        // beginning with `name.` and re-key under `slot.field`.
        let prefix = format!("{}.", name);
        let mut out = Vec::new();
        for (k, v) in env {
            if let Some(field) = k.strip_prefix(&prefix) {
                out.push((format!("{}.{}", slot, field), v.clone()));
            }
        }
        if !out.is_empty() {
            return out;
        }
    }
    // Inline record literal: `Type(arg1, arg2, …)` where `Type` is a
    // known schema (a record type). Expand per-field, binding each
    // arg to `slot.field_name`. Unspecified fields stay free — same
    // partial-pinning semantics as `name ∈ Type(args)` declarations.
    //
    // Without this branch, `set_draw_color(ren, Color(220, 40, 60), eff)`
    // would warn "positional arg didn't resolve" and leave the claim's
    // `color.*` fields unconstrained. Same fix applies whether the
    // call site uses positional invocation or `mapsto` (`color ↦
    // Color(220, 40, 60)`).
    if let Expr::Call(type_name, args) = value {
        if let Some(schema) = schemas.get(type_name) {
            let fields: Vec<(String, String)> = schema.body.iter()
                .filter_map(|i| if let BodyItem::Membership { name, type_name, .. } = i {
                    Some((name.clone(), type_name.clone()))
                } else { None })
                .collect();
            if args.len() <= fields.len() {
                let mut out = Vec::new();
                let mut ok = true;
                for (arg, (field_name, field_type)) in args.iter().zip(fields.iter()) {
                    let key = format!("{}.{}", slot, field_name);
                    // Tuple → sub-record coercion. When the arg is a
                    // bare `(a, b, c)` and the field's type is a known
                    // record schema, treat the tuple as positional
                    // args for that schema. Same rule applies inside
                    // record literals as for top-level claim args.
                    let coerced_storage: Expr;
                    let arg_ref: &Expr = match arg {
                        Expr::Tuple(items) if schemas.contains_key(field_type) => {
                            coerced_storage = Expr::Call(
                                field_type.clone(), items.clone());
                            &coerced_storage
                        }
                        other => other,
                    };
                    let v: Option<Var<'ctx>> = match field_type.as_str() {
                        "Int" | "Nat" | "Pos" =>
                            translate_int(arg_ref, ctx, env).map(Var::IntVar),
                        "Bool" =>
                            translate_bool(arg_ref, ctx, env, schemas).map(Var::BoolVar),
                        "String" =>
                            translate_str(arg_ref, ctx, env).map(Var::StrVar),
                        "Real" =>
                            translate_real(arg_ref, ctx, env).map(Var::RealVar),
                        _ => {
                            // Composite field — recurse. Handles both
                            // sub-record literals (`Foo(Bar(1, 2), 3)`)
                            // and identifier passthrough by sub-schema
                            // expansion (handled by the Identifier
                            // branch above).
                            let nested = resolve_mapping(&key, arg_ref, ctx, env, schemas);
                            if !nested.is_empty() {
                                out.extend(nested);
                                continue;
                            }
                            None
                        }
                    };
                    if let Some(var) = v {
                        out.push((key, var));
                    } else {
                        ok = false;
                        break;
                    }
                }
                if ok && !out.is_empty() {
                    return out;
                }
            }
        }
    }
    if let Some(v) = expr_as_var(value, ctx, env) {
        return vec![(slot.to_string(), v)];
    }
    Vec::new()
}

/// Resolve a leaf expression to a single `Var`. Used both for ClaimCall
/// scalar mappings and as the tail-case of `resolve_mapping`.
fn expr_as_var<'ctx>(
    e: &Expr,
    ctx: &'ctx Context,
    env: &HashMap<String, Var<'ctx>>,
) -> Option<Var<'ctx>> {
    match e {
        Expr::Identifier(name) => env.get(name).cloned(),
        Expr::Int(n)  => Some(Var::IntVar(Int::from_i64(ctx, *n))),
        Expr::Bool(b) => Some(Var::BoolVar(Bool::from_bool(ctx, *b))),
        Expr::Real(f) => Some(Var::RealVar(real_from_f64(ctx, *f))),
        Expr::Str(s)  => Z3Str::from_str(ctx, s).ok().map(Var::StrVar),
        _ => None,
    }
}

// ── Section 3: Enum / Cons-chain helpers ─────────────────────────────

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
            // Seq(enum) indexing: `body[i]` where body is a
            // DatatypeSeqVar with empty fields (Seq-of-enum marker).
            let Expr::Identifier(seq_name) = base.as_ref() else { return None };
            let var = env.get(seq_name)?;
            let Var::DatatypeSeqVar { arr, fields, .. } = var else { return None };
            if !fields.is_empty() { return None; } // record-style seq, handled elsewhere
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
                if let Some(inner) = crate::runtime::parse_seq_type(field_type) {
                    // Internal-Cons backing? Look up the helper enum
                    // in the registry; if it exists, the field is a
                    // single Datatype slot, not (arr, len). Build the
                    // Cons chain via build_cons_chain targeted at
                    // __SeqOf_<inner>.
                    let helper_name = crate::runtime::internal_cons_helper_name(inner);
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
    use z3::ast::{Array, Ast as _, Bool, Int, String as Z3Str};

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

fn build_cons_chain<'ctx>(
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

// ── Section 4: Seq field resolution ──────────────────────────────────

/// Resolve a (possibly-nested) field access chain against a
/// `DatatypeSeqVar` in the env. Two shapes:
///
///   `Field(Index(Identifier(seq_name), idx_expr), field_name)` —
///       direct primitive field of a `Seq(UserType)` element.
///       Returns the field's primitive `Dynamic` and its type name.
///
///   `Field(Field(... , inner_field), leaf_field)` (recursively) —
///       nested field of a composite element field. Walks the chain
///       outward-in: bottom of the chain is the same Index pattern,
///       each enclosing `Field` peels another level by applying the
///       nested type's accessor and threading the new (dt, fields)
///       pair down the recursion.
///
/// Returns the raw `Dynamic` for the final leaf field plus the
/// primitive type name ("Int" / "Nat" / "Pos" / "Bool" / "String") so
/// the caller can route through `as_int` / `as_bool` / `as_string`.
fn resolve_seq_field<'ctx>(
    field_expr: &Expr,
    ctx: &'ctx Context,
    env: &HashMap<String, Var<'ctx>>,
) -> Option<(z3::ast::Dynamic<'ctx>, String)> {
    // Decompose the chain. `outer_path` is the leaf-to-root list of
    // field names; the receiver at the bottom of the chain must be
    // the `Index(Identifier(seq_name), idx_expr)` pattern.
    let mut path: Vec<&str> = Vec::new();
    let mut cur = field_expr;
    let (seq_name, idx_expr) = loop {
        match cur {
            Expr::Field(receiver, field_name) => {
                path.push(field_name.as_str());
                cur = receiver.as_ref();
            }
            Expr::Index(seq_expr, idx_expr) => {
                let Expr::Identifier(seq_name) = seq_expr.as_ref() else { return None };
                break (seq_name.as_str(), idx_expr.as_ref());
            }
            _ => return None,
        }
    };
    // path is leaf-first; reverse to get root-to-leaf so we can apply
    // accessors in forward order (outer composite → inner field → ...).
    path.reverse();
    if path.is_empty() { return None; }

    let var = env.get(seq_name)?;
    let (arr, _, _, root_dt, root_fields) = var.as_datatype_seq()?;
    let i = translate_int(idx_expr, ctx, env)?;
    let elem_dyn = arr.select(&i);
    let mut cur_dyn = elem_dyn;

    // Walk the field chain. At each step we're at a Datatype value
    // (`cur_dyn`); look up the field in the current `(dt, fields)`
    // pair, apply its accessor, and either return (if it's a
    // primitive leaf) or recurse with the nested `(dt, sub_fields)`.
    let mut cur_dt: &DatatypeSort = root_dt;
    let mut cur_fields: &[FieldKind] = root_fields;
    for (depth, fname) in path.iter().enumerate() {
        let field_idx = cur_fields.iter().position(|fk| fk.name() == *fname)?;
        if field_idx >= cur_dt.variants[0].accessors.len() { return None; }
        let elem = cur_dyn.as_datatype()?;
        let raw = cur_dt.variants[0].accessors[field_idx].apply(&[&elem]);
        let is_last = depth == path.len() - 1;
        match &cur_fields[field_idx] {
            FieldKind::Primitive { prim_type, .. } => {
                if !is_last {
                    // Trying to index into a primitive — programmer error.
                    return None;
                }
                return Some((raw, prim_type.clone()));
            }
            FieldKind::Nested { dt: nested_dt, sub_fields, .. } => {
                if is_last {
                    // The chain ends on a composite — translators only
                    // know how to consume primitive leaves, so signal
                    // "no leaf primitive" by returning None. Composite
                    // values aren't first-class in Evident expressions
                    // (you always reach into one of their fields).
                    return None;
                }
                cur_dt = nested_dt;
                cur_fields = sub_fields.as_slice();
                cur_dyn = raw;
            }
        }
    }
    None
}

// ── Section 5: Per-sort translators (str / int / real) ───────────────

pub(super) fn translate_str<'ctx>(e: &Expr, ctx: &'ctx Context, env: &HashMap<String, Var<'ctx>>) -> Option<Z3Str<'ctx>> {
    match e {
        Expr::Str(s) => Z3Str::from_str(ctx, s).ok(),
        Expr::Identifier(name) => env.get(name).and_then(|v| v.as_str().cloned()),
        // `lhs ++ rhs` — string concatenation. Both operands must translate
        // as strings; the result is a Z3 string concat.
        Expr::Binary(BinOp::Concat, lhs, rhs) => {
            let l = translate_str(lhs, ctx, env)?;
            let r = translate_str(rhs, ctx, env)?;
            Some(Z3Str::concat(ctx, &[&l, &r]))
        }
        // `seq[i]` where seq holds String elements.
        Expr::Index(seq_expr, idx_expr) => {
            let name = match seq_expr.as_ref() {
                Expr::Identifier(n) => n,
                _ => return None,
            };
            let (arr, _, elem) = env.get(name)?.as_seq()?;
            if elem != SeqElem::Str { return None; }
            let i = translate_int(idx_expr, ctx, env)?;
            arr.select(&i).as_string()
        }
        // `pts[i].name` where pts is Seq(UserType) and `name` is a
        // String field of UserType.
        Expr::Field(_, _) => {
            let (raw, ftype) = resolve_seq_field(e, ctx, env)?;
            if ftype == "String" {
                raw.as_string()
            } else {
                None
            }
        }
        // `cond ? a : b` — String-typed branches via Z3 ITE.
        Expr::Ternary(c, a, b) => {
            let cond = translate_bool(c, ctx, env, &HashMap::new())?;
            let then_v = translate_str(a, ctx, env)?;
            let else_v = translate_str(b, ctx, env)?;
            Some(cond.ite(&then_v, &else_v))
        }
        Expr::Match(scr, arms) => {
            let compiled = translate_match_arms(scr, arms, ctx, env,
                |body, e| translate_str(body, ctx, e))?;
            fold_arms_to_ite(compiled)
        }
        _ => None,
    }
}

pub(super) fn translate_int<'ctx>(e: &Expr, ctx: &'ctx Context, env: &HashMap<String, Var<'ctx>>) -> Option<Int<'ctx>> {
    // Int-typed builtins: `min`, `max`, `abs`, `mod`, `clamp`.
    // All lower to Z3 ITE compositions over translated args, so
    // they share `translate_int`'s recursion and play with the
    // rest of integer arithmetic transparently.
    if let Expr::Call(name, args) = e {
        match (name.as_str(), args.len()) {
            ("min", 2) => {
                let a = translate_int(&args[0], ctx, env)?;
                let b = translate_int(&args[1], ctx, env)?;
                return Some(a.le(&b).ite(&a, &b));
            }
            ("max", 2) => {
                let a = translate_int(&args[0], ctx, env)?;
                let b = translate_int(&args[1], ctx, env)?;
                return Some(a.ge(&b).ite(&a, &b));
            }
            ("abs", 1) => {
                let x = translate_int(&args[0], ctx, env)?;
                let zero = Int::from_i64(ctx, 0);
                let neg = Int::sub(ctx, &[&zero, &x]);
                return Some(x.ge(&zero).ite(&x, &neg));
            }
            ("mod", 2) => {
                let a = translate_int(&args[0], ctx, env)?;
                let b = translate_int(&args[1], ctx, env)?;
                return Some(a.modulo(&b));
            }
            ("clamp", 3) => {
                let x  = translate_int(&args[0], ctx, env)?;
                let lo = translate_int(&args[1], ctx, env)?;
                let hi = translate_int(&args[2], ctx, env)?;
                // max(lo, min(x, hi))
                let inner = x.le(&hi).ite(&x, &hi);
                return Some(inner.ge(&lo).ite(&inner, &lo));
            }
            // `position_of(seq, x)` — index of `x` in `seq` for the
            // first match, or -1 if not present. Implemented as a
            // chained ITE over the seq's pinned-length positions:
            //
            //     seq[0] = x ? 0 : (seq[1] = x ? 1 : … : -1)
            //
            // No side effects, no fresh constants — just an
            // expression Z3 can fold. For distinct-valued seqs the
            // result is the unique position. For Seqs with the
            // element appearing multiple times, returns the lowest
            // index (well-defined; mirrors Z3 / Python semantics).
            //
            // Primitive Seq path only in v1; Datatype-Seq element
            // types fall through.
            ("position_of", 2) => {
                let Expr::Identifier(sname) = &args[0] else { return None };
                let var = env.get(sname)?;
                let (arr, len, elem) = var.as_seq()?;
                let n = len.simplify().as_i64()?;
                let mut result = Int::from_i64(ctx, -1);
                for i in (0..n).rev() {
                    let idx = Int::from_i64(ctx, i);
                    let cell = arr.select(&idx);
                    let eq = match elem {
                        SeqElem::Int => {
                            let v = translate_int(&args[1], ctx, env)?;
                            cell.as_int()?._eq(&v)
                        }
                        SeqElem::Bool => {
                            let v = match &args[1] {
                                Expr::Bool(b) => Bool::from_bool(ctx, *b),
                                Expr::Identifier(n) => env.get(n)?.as_bool()?.clone(),
                                _ => return None,
                            };
                            cell.as_bool()?._eq(&v)
                        }
                        SeqElem::Str => {
                            let v = translate_str(&args[1], ctx, env)?;
                            cell.as_string()?._eq(&v)
                        }
                    };
                    result = eq.ite(&idx, &result);
                }
                return Some(result);
            }
            _ => {}
        }
    }
    match e {
        Expr::Int(n) => Some(Int::from_i64(ctx, *n)),
        Expr::Identifier(name) => match env.get(name) {
            Some(Var::IntVar(i)) => Some(i.clone()),
            Some(Var::PinnedInt(v)) => Some(Int::from_i64(ctx, *v)),
            _ => None,
        },
        Expr::Binary(op, lhs, rhs) => {
            let l = translate_int(lhs, ctx, env)?;
            let r = translate_int(rhs, ctx, env)?;
            Some(match op {
                BinOp::Add => Int::add(ctx, &[&l, &r]),
                BinOp::Sub => Int::sub(ctx, &[&l, &r]),
                BinOp::Mul => Int::mul(ctx, &[&l, &r]),
                BinOp::Div => l.div(&r),
                _ => return None,
            })
        }
        // `#seq` → the seq's length variable. Both primitive Seq and
        // composite-element Seq (DatatypeSeqVar) expose a length.
        // For Sets (both flavors), Z3 has no native cardinality — we
        // return the recorded candidates count if the Set was pinned
        // via `S = {…}`; otherwise drop (silent, same as today for
        // unpinned Set extraction).
        Expr::Cardinality(inner) => {
            if let Expr::Identifier(name) = inner.as_ref() {
                if let Some(var) = env.get(name) {
                    if let Some((_, len, _)) = var.as_seq() {
                        return Some(len.clone());
                    }
                    if let Some((_, len, _, _, _)) = var.as_datatype_seq() {
                        return Some(len.clone());
                    }
                    if let Some((_, _, candidates)) = var.as_set_with_candidates() {
                        if let Some(cands) = candidates.borrow().as_ref() {
                            return Some(Int::from_i64(ctx, cands.len() as i64));
                        }
                    }
                    if let Some((_, _, _, _, candidates)) = var.as_datatype_set() {
                        if let Some(cands) = candidates.borrow().as_ref() {
                            return Some(Int::from_i64(ctx, cands.len() as i64));
                        }
                    }
                }
            }
            None
        }
        // `seq[i]` where seq holds Int elements → Array.select(i) → Int.
        Expr::Index(seq_expr, idx_expr) => {
            let name = match seq_expr.as_ref() {
                Expr::Identifier(n) => n,
                _ => return None,
            };
            let (arr, _, elem) = env.get(name)?.as_seq()?;
            if elem != SeqElem::Int { return None; }
            let i = translate_int(idx_expr, ctx, env)?;
            arr.select(&i).as_int()
        }
        // `pts[i].x` where pts is Seq(UserType) and `x` is an Int field.
        Expr::Field(_, _) => {
            let (raw, ftype) = resolve_seq_field(e, ctx, env)?;
            if matches!(ftype.as_str(), "Int" | "Nat" | "Pos") {
                raw.as_int()
            } else {
                None
            }
        }
        // `cond ? a : b` — ternary conditional. Both branches must
        // translate as Int; lifted to Z3's ITE.
        Expr::Ternary(c, a, b) => {
            let cond = translate_bool(c, ctx, env, &HashMap::new())?;
            let then_v = translate_int(a, ctx, env)?;
            let else_v = translate_int(b, ctx, env)?;
            Some(cond.ite(&then_v, &else_v))
        }
        // `match scrutinee { Ctor(b) ⇒ body | _ ⇒ fallback }` with
        // Int-typed arm bodies → nested ITE.
        Expr::Match(scr, arms) => {
            let compiled = translate_match_arms(scr, arms, ctx, env,
                |body, e| translate_int(body, ctx, e))?;
            fold_arms_to_ite(compiled)
        }
        _ => None,
    }
}

/// Translate an Expr that should evaluate to a Z3 Real. Mirrors
/// `translate_int` for the Real domain. Supports:
///   - Real literals (`3.14`)
///   - Identifier resolving to `Var::RealVar`
///   - Binary arithmetic (`+`, `-`, `*`, `/`) with operands that
///     translate as Real OR can be coerced from Int (Z3 supports
///     mixed Int/Real arithmetic by lifting Int to Real).
///   - Unary minus via `0 - e` desugaring (parser does this already).
/// Returns None if the expression doesn't fit any of these patterns —
/// caller (typically `translate_bool`'s Eq/comparison arms) tries
/// other type paths.
pub(super) fn translate_real<'ctx>(e: &Expr, ctx: &'ctx Context, env: &HashMap<String, Var<'ctx>>) -> Option<Real<'ctx>> {
    match e {
        Expr::Real(f) => Some(real_from_f64(ctx, *f)),
        Expr::Int(n)  => Some(Real::from_int(&Int::from_i64(ctx, *n))),  // numeric literal coercion
        Expr::Identifier(name) => match env.get(name) {
            Some(Var::RealVar(r)) => Some(r.clone()),
            Some(Var::IntVar(i))  => Some(Real::from_int(i)),     // promote int var
            Some(Var::PinnedInt(v)) => Some(Real::from_int(&Int::from_i64(ctx, *v))),
            _ => None,
        },
        Expr::Binary(op, lhs, rhs) => {
            let l = translate_real(lhs, ctx, env)?;
            let r = translate_real(rhs, ctx, env)?;
            Some(match op {
                BinOp::Add => Real::add(ctx, &[&l, &r]),
                BinOp::Sub => Real::sub(ctx, &[&l, &r]),
                BinOp::Mul => Real::mul(ctx, &[&l, &r]),
                BinOp::Div => l.div(&r),
                _ => return None,
            })
        }
        // `cond ? a : b` — Real-typed branches via Z3 ITE. The condition
        // is a boolean expression; we don't have a `schemas` table here,
        // so claim-call conditions in ternary aren't supported in Real
        // context (use a Bool intermediate variable instead).
        Expr::Ternary(c, a, b) => {
            let cond = translate_bool(c, ctx, env, &HashMap::new())?;
            let then_v = translate_real(a, ctx, env)?;
            let else_v = translate_real(b, ctx, env)?;
            Some(cond.ite(&then_v, &else_v))
        }
        Expr::Match(scr, arms) => {
            let compiled = translate_match_arms(scr, arms, ctx, env,
                |body, e| translate_real(body, ctx, e))?;
            fold_arms_to_ite(compiled)
        }
        _ => None,
    }
}

// ── Section 6: Real-literal helpers ──────────────────────────────────

/// Local copy of the Real-from-f64 helper. Same shape as the one in
/// `eval.rs` (private there); duplicated to avoid a cross-module
/// dependency for one tiny helper.
///
/// Splits f64's Display form (`"3.14"`) into pure-integer num/den
/// (`"314" / "100"`) so Z3's numeral parser only sees integers.
/// Z3's parser is finicky about decimals embedded in `"num/den"`.
fn real_from_f64<'ctx>(ctx: &'ctx Context, f: f64) -> Real<'ctx> {
    if f.is_nan() || f.is_infinite() {
        return Real::from_real(ctx, 0, 1);
    }
    let s = f.to_string();
    let (num, den) = if let Some(dot) = s.find('.') {
        let (int_part, frac_with_dot) = s.split_at(dot);
        let frac = &frac_with_dot[1..];
        (format!("{}{}", int_part, frac),
         format!("1{}", "0".repeat(frac.len())))
    } else {
        (s, "1".to_string())
    };
    Real::from_real_str(ctx, &num, &den)
        .unwrap_or_else(|| Real::from_real(ctx, 0, 1))
}

// ── Section 7: Record / vector lifting ───────────────────────────────

/// Field-wise broadcast for `lhs OP rhs` where at least one side is a
/// record reference (Identifier or Field-of-Index) and the operator is
/// any of `=`, `≠`, `<`, `≤`, `>`, `≥`. Either side may be an
/// arithmetic expression involving record references and scalars.
///
/// For each leaf field path of the record's type, we substitute *both*
/// sides by extending every record sub-expression with that leaf path,
/// then translate the per-leaf op. Results fold with `Or` for `≠`
/// (some-field-differs) and `And` for the others (componentwise).
///
/// Supported record reference shapes (anywhere in the expression):
///   - `Identifier(name)` where `name.*` keys exist in env
///   - `Field(Index(Identifier(seq), idx), name)` where `seq` is a
///     `DatatypeSeqVar` whose element type has `name` as Nested
///
/// Other sub-expressions (literals, scalar identifiers like `input.dt`,
/// scalar arithmetic, primitive Seq indexing) pass through unchanged.
///
/// Guards:
///   - At least one side must contain a record reference. `vec = 5`
///     (scalar-only RHS) would otherwise silently broadcast the
///     scalar to every axis, which is almost always a bug.
///   - All record references must have the *same* leaf set
///     (bidirectional shape check). `vec2 = vec3` returns None
///     so the constraint drops with a translator error rather than
///     producing a partial-overlap conjunction.
fn lift_record_op<'ctx>(
    op: &BinOp,
    lhs: &Expr,
    rhs: &Expr,
    ctx: &'ctx Context,
    env: &HashMap<String, Var<'ctx>>,
    schemas: &HashMap<String, SchemaDecl>,
) -> Option<Bool<'ctx>> {
    if !matches!(op,
        BinOp::Eq | BinOp::Neq | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge
    ) {
        return None;
    }
    // Each side must contribute at least one record reference. Without
    // this, `vec = 5` (or `vec ≤ 100`) would broadcast the scalar to
    // every leaf — almost always a bug. Per-side counts also make it
    // clear we're operating on records "all the way through" rather
    // than mixing a record with a scalar at the top level.
    let mut lhs_records = Vec::new();
    let mut rhs_records = Vec::new();
    collect_record_refs(lhs, env, schemas, &mut lhs_records);
    collect_record_refs(rhs, env, schemas, &mut rhs_records);
    if lhs_records.is_empty() || rhs_records.is_empty() { return None; }
    let mut all_records = lhs_records;
    all_records.extend(rhs_records);

    // All record references must share the same leaf shape.
    let leaves = lhs_record_leaves(&all_records[0], env, schemas)?;
    for rec in all_records.iter().skip(1) {
        let rec_leaves = lhs_record_leaves(rec, env, schemas)?;
        if rec_leaves != leaves { return None; }
    }

    let mut clauses = Vec::with_capacity(leaves.len());
    for leaf in &leaves {
        let lhs_leaf = substitute_record_refs(lhs, leaf, env, schemas)?;
        let rhs_leaf = substitute_record_refs(rhs, leaf, env, schemas)?;
        let leaf_op = Expr::Binary(
            op.clone(),
            Box::new(lhs_leaf),
            Box::new(rhs_leaf),
        );
        clauses.push(translate_bool(&leaf_op, ctx, env, schemas)?);
    }
    let refs: Vec<&Bool> = clauses.iter().collect();
    Some(match op {
        // Two records "differ" iff at least one field differs.
        BinOp::Neq => Bool::or(ctx, &refs),
        // =, <, ≤, >, ≥ are all componentwise (all axes must satisfy).
        _ => Bool::and(ctx, &refs),
    })
}

/// Enumerate the leaf field paths of an expression representing a
/// record. Single-level paths (`["x", "y"]` for an IVec2) for
/// flat records; dotted paths (`["pos.x", "pos.y", "color.r", …]`)
/// for records containing sub-records.
fn lhs_record_leaves<'ctx>(
    lhs: &Expr,
    env: &HashMap<String, Var<'ctx>>,
    schemas: &HashMap<String, SchemaDecl>,
) -> Option<Vec<String>> {
    match lhs {
        // Record-literal expression `IVec2(380, 280)` — the leaves come
        // from the type's SchemaDecl, walked recursively for any nested
        // fields the type might have.
        Expr::Call(type_name, _args) => {
            let schema = schemas.get(type_name)?;
            let mut leaves = schema_leaf_paths(schema, schemas);
            if leaves.is_empty() { return None; }
            leaves.sort();
            Some(leaves)
        }
        Expr::Identifier(name) => {
            if env.contains_key(name) { return None; }   // not a record (already a primitive)
            let prefix = format!("{}.", name);
            let mut leaves: Vec<String> = env.keys()
                .filter_map(|k| k.strip_prefix(&prefix).map(String::from))
                .collect();
            if leaves.is_empty() { return None; }
            leaves.sort();
            Some(leaves)
        }
        Expr::Field(receiver, field) => {
            // Field-of-Index path: `seq[i].pos` where pos is a Nested
            // record sub-field. Enumerate the Nested's sub-leaves from
            // the DatatypeSeqVar's field metadata.
            let Expr::Index(seq_expr, _) = receiver.as_ref() else { return None };
            let Expr::Identifier(seq_name) = seq_expr.as_ref() else { return None };
            let Some(Var::DatatypeSeqVar { fields, .. }) = env.get(seq_name) else { return None };
            let nested_sub = fields.iter().find_map(|f| match f {
                FieldKind::Nested { name, sub_fields, .. } if name == field => Some(sub_fields),
                _ => None,
            })?;
            let mut leaves = enumerate_nested_leaves(nested_sub);
            if leaves.is_empty() { return None; }
            leaves.sort();
            Some(leaves)
        }
        Expr::Index(receiver, _) => {
            // Direct Seq-element record: `output.rects[4] = player_rect`.
            // The element type is the entire DatatypeSeqVar's field
            // shape — every leaf, including those reached through
            // Nested sub-records.
            let Expr::Identifier(seq_name) = receiver.as_ref() else { return None };
            let Some(Var::DatatypeSeqVar { fields, .. }) = env.get(seq_name) else { return None };
            let mut leaves = enumerate_nested_leaves(fields);
            if leaves.is_empty() { return None; }
            leaves.sort();
            Some(leaves)
        }
        _ => None,
    }
}

/// Recursively walk a `FieldKind` list and produce flat leaf paths.
/// Primitive fields yield their name; Nested fields yield
/// `name.<sub-leaf>` for each sub-leaf in their `sub_fields`.
/// Walk a SchemaDecl and produce flat leaf paths the same way
/// `enumerate_nested_leaves` does for `FieldKind`. Used for
/// `lhs_record_leaves` on `Expr::Call(type, args)` (record literals
/// in expression position) where we don't have the Z3 Datatype yet —
/// just need leaf NAMES for the lift's positional substitution.
///
/// A field whose type appears in `schemas` is treated as nested
/// (recurse into its body). Anything else (primitives, compound
/// types like `Seq(T)`) is treated as a primitive leaf.
fn schema_leaf_paths(
    schema: &SchemaDecl,
    schemas: &HashMap<String, SchemaDecl>,
) -> Vec<String> {
    let mut out = Vec::new();
    for item in &schema.body {
        if let BodyItem::Membership { name, type_name, .. } = item {
            if let Some(sub) = schemas.get(type_name) {
                for leaf in schema_leaf_paths(sub, schemas) {
                    out.push(format!("{}.{}", name, leaf));
                }
            } else {
                out.push(name.clone());
            }
        }
    }
    out
}

fn enumerate_nested_leaves(fields: &[FieldKind]) -> Vec<String> {
    let mut out = Vec::new();
    for f in fields {
        match f {
            FieldKind::Primitive { name, .. } => out.push(name.clone()),
            FieldKind::Nested { name, sub_fields, .. } => {
                for sub in enumerate_nested_leaves(sub_fields) {
                    out.push(format!("{}.{}", name, sub));
                }
            }
        }
    }
    out
}

/// Walk an expression and substitute each record reference with its
/// `.leaf` extension. Scalars and non-record expressions pass through.
/// Returns None on shape mismatch (record reference whose `.leaf`
/// component doesn't exist in env).
fn substitute_record_refs<'ctx>(
    expr: &Expr,
    leaf: &str,
    env: &HashMap<String, Var<'ctx>>,
    schemas: &HashMap<String, SchemaDecl>,
) -> Option<Expr> {
    match expr {
        // Record-literal expression: `IVec2(380, 280)` → 380 for leaf x,
        // 280 for leaf y. Looks up the type's field declaration order
        // in schemas, finds the leaf's first segment in that order,
        // then either returns the matching arg directly (single-level
        // leaf) or recurses into it (multi-level leaf for a nested
        // record).
        Expr::Call(type_name, args) => {
            let schema = schemas.get(type_name)?;
            // Field name + type in declaration order — for nested
            // sub-records this is the field ("pos"), not the sub-leaves.
            let fields: Vec<(&str, &str)> = schema.body.iter()
                .filter_map(|item| match item {
                    BodyItem::Membership { name, type_name, .. } =>
                        Some((name.as_str(), type_name.as_str())),
                    _ => None,
                })
                .collect();
            // Split the leaf into its first segment (which is one of
            // `fields`) and the remainder.
            let (first, rest) = match leaf.split_once('.') {
                Some((a, b)) => (a, Some(b)),
                None => (leaf, None),
            };
            let pos = fields.iter().position(|(n, _)| *n == first)?;
            if pos >= args.len() { return None; }
            // Tuple → sub-record coercion: if the arg is a bare
            // `(a, b, c)` AND the field's declared type is a known
            // record schema, treat the tuple as positional args for
            // that schema. Lets the caller write
            //     Rect((220, 40, 40, 255), (0, 432), (640, 48))
            // instead of fully spelling out each ctor.
            let coerced: Expr;
            let arg_ref: &Expr = match &args[pos] {
                Expr::Tuple(items) if schemas.contains_key(fields[pos].1) => {
                    coerced = Expr::Call(fields[pos].1.to_string(), items.clone());
                    &coerced
                }
                other => other,
            };
            match rest {
                None => Some(arg_ref.clone()),
                // Nested leaf: recurse into the arg with the rest of
                // the path. Works for `SDLRect(IVec2(...), IVec2(...),
                // Color(...))` accessed at leaf "pos.x".
                Some(rest_path) => substitute_record_refs(arg_ref, rest_path, env, schemas),
            }
        }
        Expr::Identifier(name) => {
            if env.contains_key(name) {
                // Scalar identifier — leave as-is.
                return Some(expr.clone());
            }
            let prefix = format!("{}.", name);
            if env.keys().any(|k| k.starts_with(&prefix)) {
                // Record identifier — extend with leaf path. Verify
                // the resulting key actually exists; else shape
                // mismatch (e.g. `vec2.r` for a Color leaf).
                let mut extended = name.clone();
                for p in leaf.split('.') {
                    extended.push('.');
                    extended.push_str(p);
                }
                if env.contains_key(&extended) { Some(Expr::Identifier(extended)) }
                else { None }
            } else {
                // Unknown identifier — leave; later translation
                // either resolves it or fails on its own.
                Some(expr.clone())
            }
        }
        Expr::Field(receiver, field) => {
            // Field-of-Index record sub-field? If so, wrap in Fields.
            if is_field_of_index_record(receiver, field, env) {
                let mut result = expr.clone();
                for p in leaf.split('.') {
                    result = Expr::Field(Box::new(result), p.to_string());
                }
                return Some(result);
            }
            // Primitive Field access — leave as-is.
            Some(expr.clone())
        }
        Expr::Index(receiver, _) => {
            // Direct Seq-element record (`output.rects[4] = player_rect`):
            // the indexed element IS a Datatype value. Wrap with Field
            // accesses for each leaf path component so the existing
            // `resolve_seq_field` chain reaches the leaf.
            if is_seq_element_record(receiver, env) {
                let mut result = expr.clone();
                for p in leaf.split('.') {
                    result = Expr::Field(Box::new(result), p.to_string());
                }
                return Some(result);
            }
            // Primitive Seq indexing (e.g. effective_vy[i]) — leave as-is.
            Some(expr.clone())
        }
        Expr::Binary(op, a, b) => {
            let a2 = substitute_record_refs(a, leaf, env, schemas)?;
            let b2 = substitute_record_refs(b, leaf, env, schemas)?;
            Some(Expr::Binary(op.clone(), Box::new(a2), Box::new(b2)))
        }
        Expr::Not(x) => substitute_record_refs(x, leaf, env, schemas).map(|y| Expr::Not(Box::new(y))),
        // Literals, etc.: scalar values, leave as-is.
        _ => Some(expr.clone()),
    }
}

/// True if `Field(receiver, field)` resolves to a record-typed
/// sub-field of a Seq element (e.g. `state.dots[i].pos`). Drives both
/// LHS leaf enumeration and RHS substitution.
fn is_field_of_index_record<'ctx>(
    receiver: &Expr,
    field: &str,
    env: &HashMap<String, Var<'ctx>>,
) -> bool {
    let Expr::Index(seq_expr, _) = receiver else { return false };
    let Expr::Identifier(seq_name) = seq_expr.as_ref() else { return false };
    let Some(Var::DatatypeSeqVar { fields, .. }) = env.get(seq_name) else { return false };
    fields.iter().any(|f| matches!(f, FieldKind::Nested { name, .. } if name == field))
}

/// True if `Index(receiver, _)` indexes into a `Seq(UserType)` whose
/// element is a Datatype record (e.g. `output.rects[4]` returns an
/// SDLRect value). Drives `output.rects[4] = player_rect` lifting.
fn is_seq_element_record<'ctx>(
    receiver: &Expr,
    env: &HashMap<String, Var<'ctx>>,
) -> bool {
    let Expr::Identifier(seq_name) = receiver else { return false };
    matches!(env.get(seq_name), Some(Var::DatatypeSeqVar { .. }))
}

/// Walk `expr` and collect every record-reference sub-expression
/// (bare-identifier records and Field-of-Index records). Used by
/// `lift_record_assignment` to verify each RHS record has the same
/// leaf shape as the LHS — without this check, `vec2 = vec3` would
/// produce a partial-overlap broadcast over the LHS's leaves only.
fn collect_record_refs<'ctx>(
    expr: &Expr,
    env: &HashMap<String, Var<'ctx>>,
    schemas: &HashMap<String, SchemaDecl>,
    out: &mut Vec<Expr>,
) {
    match expr {
        // Record literal `IVec2(380, 280)` IS a record reference.
        Expr::Call(type_name, _) if schemas.contains_key(type_name) => {
            out.push(expr.clone());
        }
        Expr::Identifier(name) => {
            if !env.contains_key(name)
                && env.keys().any(|k| k.starts_with(&format!("{}.", name)))
            {
                out.push(expr.clone());
            }
        }
        Expr::Field(receiver, field) => {
            if is_field_of_index_record(receiver, field, env) {
                out.push(expr.clone());
            }
        }
        Expr::Index(receiver, _) => {
            if is_seq_element_record(receiver, env) {
                out.push(expr.clone());
            }
        }
        Expr::Binary(_, a, b) => {
            collect_record_refs(a, env, schemas, out);
            collect_record_refs(b, env, schemas, out);
        }
        Expr::Not(x) => collect_record_refs(x, env, schemas, out),
        _ => {}
    }
}

// ── Section 8: Seq-equality translation ──────────────────────────────

/// Handle `enum_var = ⟨a, b, c⟩` where `enum_var` is a Cons/Nil-shaped
/// enum (one variant with 0 fields = "Nil", one variant with 2 fields
/// where the second field's declared type matches the enum itself =
/// "Cons"). `EffectList` (pending Phase 6.4 migration to Seq),
/// user-defined `LinkedList`, etc. all qualify. The literal is lowered to nested
/// constructor calls: `Cons(a, Cons(b, Cons(c, Nil)))`.
///
/// `⟨⟩` (empty) lowers to just the Nil constructor.
///
/// Returns None if the LHS isn't an enum-typed Identifier, the RHS
/// isn't a SeqLit, or the enum lacks the Nil/Cons shape.
fn translate_cons_chain_eq<'ctx>(
    lhs: &Expr,
    rhs: &Expr,
    ctx: &'ctx Context,
    env: &HashMap<String, Var<'ctx>>,
    schemas: &HashMap<String, SchemaDecl>,
) -> Option<Bool<'ctx>> {
    let items = match rhs { Expr::SeqLit(items) => items, _ => return None };
    let lhs_name = match lhs { Expr::Identifier(n) => n, _ => return None };
    let var = env.get(lhs_name)?;
    let (lhs_ast, enum_name, dt) = match var {
        Var::EnumVar { ast, enum_name, dt } => (ast.clone(), enum_name.clone(), *dt),
        _ => return None,
    };
    let acc = build_cons_chain(items, &enum_name, dt, ctx, env, schemas)?;
    Some(lhs_ast._eq(&acc))
}

/// Handle `seq_var = ⟨e1, e2, …⟩` (sequence-literal assignment).
///
/// Returns the conjunction `len == items.len() ∧ ∀i: arr[i] == translated(e_i)`
/// when `lhs` is an `Identifier(name)` resolving to a `Var::SeqVar` (primitive
/// element) or `Var::DatatypeSeqVar` (composite element), and `rhs` is an
/// `Expr::SeqLit(items)`. Returns `None` otherwise — caller then falls back
/// through the Bool/Int/Str equality paths.
fn translate_seq_lit_eq<'ctx>(
    lhs: &Expr,
    rhs: &Expr,
    ctx: &'ctx Context,
    env: &HashMap<String, Var<'ctx>>,
    schemas: &HashMap<String, SchemaDecl>,
) -> Option<Bool<'ctx>> {
    let name = match lhs {
        Expr::Identifier(n) => n,
        _ => return None,
    };
    if env.get(name).is_none() { return None; }
    translate_seq_rhs_eq(name, rhs, ctx, env, schemas)
}

/// Translate `seq_name = <rhs>` where `rhs` is a Seq-valued expression
/// — a SeqLit, a `cond ? a : b` ternary whose branches are Seq-valued,
/// or a `match scrutinee | arm ⇒ body` whose arm bodies are Seq-valued.
///
/// The result is a Bool conjunction: each arm/branch contributes a
/// guarded equality `(arm_guard ⇒ seq_name = arm_body)`. For wildcard
/// arms the guard is the negation of all prior arms' guards.
fn translate_seq_rhs_eq<'ctx>(
    name: &str,
    rhs: &Expr,
    ctx: &'ctx Context,
    env: &HashMap<String, Var<'ctx>>,
    schemas: &HashMap<String, SchemaDecl>,
) -> Option<Bool<'ctx>> {
    match rhs {
        Expr::SeqLit(items) =>
            translate_seq_lit_for_var(name, items, ctx, env, schemas),
        Expr::Ternary(c, a, b) => {
            let cond = translate_bool(c, ctx, env, schemas)?;
            let then_eq = translate_seq_rhs_eq(name, a, ctx, env, schemas)?;
            let else_eq = translate_seq_rhs_eq(name, b, ctx, env, schemas)?;
            Some(Bool::and(ctx, &[
                &cond.implies(&then_eq),
                &cond.not().implies(&else_eq),
            ]))
        }
        Expr::Match(scr, arms) => {
            // Body translator: produces `seq_name = arm_body` for each arm.
            let owned_name = name.to_string();
            let compiled = translate_match_arms(scr, arms, ctx, env, |body, e| {
                translate_seq_rhs_eq(&owned_name, body, ctx, e, schemas)
            })?;
            // Fold: each arm contributes a guarded equality. Wildcard
            // arms fire when no prior tester matched (¬OR(priors)).
            let mut clauses: Vec<Bool<'ctx>> = Vec::with_capacity(compiled.len());
            let mut prior_testers: Vec<Bool<'ctx>> = Vec::new();
            for (tester_opt, body_eq) in compiled {
                let guard = match &tester_opt {
                    Some(t) => t.clone(),
                    None => {
                        let nots: Vec<Bool<'ctx>> =
                            prior_testers.iter().map(|p| p.not()).collect();
                        let refs: Vec<&Bool<'ctx>> = nots.iter().collect();
                        Bool::and(ctx, &refs)
                    }
                };
                clauses.push(guard.implies(&body_eq));
                if let Some(t) = tester_opt { prior_testers.push(t); }
            }
            let refs: Vec<&Bool<'ctx>> = clauses.iter().collect();
            Some(Bool::and(ctx, &refs))
        }
        _ => None,
    }
}

/// Core: assert `seq_name = ⟨items[0], items[1], …⟩` — pins length and
/// per-index equality. Handles primitive, enum-element, and composite-
/// record Seq element kinds. Returns None if `seq_name` doesn't resolve
/// to a Seq-shaped Var or any item doesn't translate.
fn translate_seq_lit_for_var<'ctx>(
    name: &str,
    items: &[Expr],
    ctx: &'ctx Context,
    env: &HashMap<String, Var<'ctx>>,
    schemas: &HashMap<String, SchemaDecl>,
) -> Option<Bool<'ctx>> {
    let var = env.get(name)?;

    // Primitive-element Seq: pin length, then per-element equality on the
    // underlying Z3 array.
    if let Some((arr, len, elem)) = var.as_seq() {
        let n = items.len() as i64;
        let mut clauses: Vec<Bool<'ctx>> = Vec::with_capacity(items.len() + 1);
        clauses.push(len._eq(&Int::from_i64(ctx, n)));
        for (i, item) in items.iter().enumerate() {
            let idx = Int::from_i64(ctx, i as i64);
            let cell = arr.select(&idx);
            let eq = match elem {
                SeqElem::Int => {
                    let z = cell.as_int()?;
                    let v = translate_int(item, ctx, env)?;
                    z._eq(&v)
                }
                SeqElem::Bool => {
                    let z = cell.as_bool()?;
                    let v = translate_bool(item, ctx, env, schemas)?;
                    z._eq(&v)
                }
                SeqElem::Str => {
                    let z = cell.as_string()?;
                    let v = translate_str(item, ctx, env)?;
                    z._eq(&v)
                }
            };
            clauses.push(eq);
        }
        let refs: Vec<&Bool<'ctx>> = clauses.iter().collect();
        return Some(Bool::and(ctx, &refs));
    }

    // Enum-element Seq (`Seq(EnumType)` — DatatypeSeqVar with empty
    // fields). Each item is an enum constructor call like `IntResult(42)`
    // (or a bare nullary variant identifier). Translate to a Datatype
    // value via the existing enum-aware path and assert per-index
    // equality. `last_results = ⟨IntResult(42)⟩` is the headline use.
    if let Some((arr, len, _, dt, fields)) = var.as_datatype_seq() {
        if fields.is_empty() {
            let enum_name = match var {
                Var::DatatypeSeqVar { type_name, .. } => type_name.clone(),
                _ => unreachable!(),
            };
            let mut clauses: Vec<Bool<'ctx>> = Vec::with_capacity(items.len() + 1);
            clauses.push(len._eq(&Int::from_i64(ctx, items.len() as i64)));
            // Hint so that nested ⟨...⟩ items lower against this enum.
            let elems: Option<Vec<Bool<'ctx>>> = with_target_enum_hint(
                Some((enum_name, dt)),
                || {
                    let mut tmp: Vec<Bool<'ctx>> = Vec::with_capacity(items.len());
                    for (i, item) in items.iter().enumerate() {
                        let v = resolve_enum_ast(item, ctx, env, schemas)?;
                        let idx = Int::from_i64(ctx, i as i64);
                        let cell = arr.select(&idx).as_datatype()?;
                        tmp.push(cell._eq(&v));
                    }
                    Some(tmp)
                },
            );
            clauses.extend(elems?);
            let refs: Vec<&Bool<'ctx>> = clauses.iter().collect();
            return Some(Bool::and(ctx, &refs));
        }
        // Composite-element Seq: each item must be a bare Identifier referring to
        // flat sub-schema fields (e.g. `ball_rect`). Walk the Datatype's FieldKind
        // list and assemble a constructor application from `env["ident.field"]`
        // lookups, recursing for nested composites (e.g. `ball_rect.color.r`).
        let n = items.len() as i64;
        let mut clauses: Vec<Bool<'ctx>> = Vec::with_capacity(items.len() + 1);
        clauses.push(len._eq(&Int::from_i64(ctx, n)));
        for (i, item) in items.iter().enumerate() {
            let ident = match item {
                Expr::Identifier(s) => s,
                _ => return None,
            };
            let elem_dyn = build_composite_dynamic(ident, dt, fields, ctx, env)?;
            let idx = Int::from_i64(ctx, i as i64);
            let cell = arr.select(&idx);
            clauses.push(cell._eq(&elem_dyn));
        }
        let refs: Vec<&Bool<'ctx>> = clauses.iter().collect();
        return Some(Bool::and(ctx, &refs));
    }
    None
}

/// Resolve an expression to a compile-time `Value` if it's a literal
/// (or an identifier bound to a known constant). Used by
/// `translate_set_lit_eq` to record the Set's members for later
/// extraction without needing to model-evaluate at extract time.
///
/// Returns None for expressions whose value depends on the model
/// (free variables, arithmetic over them, etc.). v1 supports this
/// statically-resolvable subset because every Set use site in the
/// FFI surface is expected to be `S = {literal_constants…}`. The
/// dynamic case is a Phase 6.6+ extension.
fn expr_to_const_value(e: &Expr, env: &HashMap<String, Var>) -> Option<Value> {
    match e {
        Expr::Int(n) => Some(Value::Int(*n)),
        Expr::Bool(b) => Some(Value::Bool(*b)),
        Expr::Str(s) => Some(Value::Str(s.clone())),
        Expr::Identifier(name) => match env.get(name)? {
            Var::PinnedInt(v) => Some(Value::Int(*v)),
            _ => None,
        },
        _ => None,
    }
}

/// Recognize `∀ x ∈ A : x ∈ B` — the subset pattern. Returns the
/// Z3 Set handle for `B` (the superset) if `body` is `Expr::InExpr`
/// whose LHS is exactly the bound name `var` and whose RHS is an
/// Identifier resolving to a SetVar / DatatypeSetVar. Used by the
/// quantifier translator to emit Z3 native `set_subset` instead of
/// trying to unroll a free Set (which has no candidates to iterate).
fn match_set_subset_body<'a, 'ctx>(
    body: &Expr,
    var: &str,
    env: &'a HashMap<String, Var<'ctx>>,
) -> Option<&'a z3::ast::Set<'ctx>> {
    let Expr::InExpr(lhs, rhs) = body else { return None };
    match lhs.as_ref() {
        Expr::Identifier(n) if n == var => {}
        _ => return None,
    }
    let Expr::Identifier(set_name) = rhs.as_ref() else { return None };
    let v = env.get(set_name)?;
    if let Some((set, _)) = v.as_set() { return Some(set); }
    if let Some((set, _, _, _, _)) = v.as_datatype_set() { return Some(set); }
    None
}

/// Translate `S = {a, b, c}` where S is a SetVar and the RHS is a
/// SetLit. Builds a Z3 literal set by add'ing each element to
/// `Set::empty`, then asserts set-equality against the variable —
/// this gives EXACT membership semantics (S contains a, b, c and
/// nothing else). Also records the literal items in S's `candidates`
/// cell so `extract_set` can recover the members from the model
/// without needing general Z3-set enumeration.
///
/// Returns None when LHS isn't a SetVar or RHS isn't a SetLit, or
/// when the SetLit elements can't all be translated as the Set's
/// element type — caller falls through to the regular Eq path.
fn translate_set_lit_eq<'ctx>(
    lhs: &Expr,
    rhs: &Expr,
    ctx: &'ctx Context,
    env: &HashMap<String, Var<'ctx>>,
    schemas: &HashMap<String, SchemaDecl>,
) -> Option<Bool<'ctx>> {
    use z3::ast::Set as Z3Set;
    use z3::Sort;

    let items = match rhs {
        Expr::SetLit(items) => items,
        _ => return None,
    };
    let name = match lhs {
        Expr::Identifier(n) => n,
        _ => return None,
    };

    // Composite-element Set: items must be bare Identifiers referring
    // to flat-expanded composites (same shape as composite SeqLit).
    // Build each element as a Datatype Dynamic via `build_composite_dynamic`,
    // assemble a literal Z3 Set, and assert set-equality against the var.
    // We record one `Value::Composite{}` placeholder per literal item so
    // `#s` (cardinality) can return the count; per-element field values
    // are left empty in v1 — extracting Set(Composite) into a populated
    // `Value` is deferred until there's a concrete consumer.
    if let Some((set, _, dt, fields, candidates_cell)) =
        env.get(name).and_then(|v| v.as_datatype_set())
    {
        let mut lit = Z3Set::empty(ctx, &dt.sort);
        for item in items {
            let ident = match item {
                Expr::Identifier(s) => s.as_str(),
                _ => return None,
            };
            let dyn_val = build_composite_dynamic(ident, dt, fields, ctx, env)?;
            lit = lit.add(&dyn_val);
        }
        let placeholders: Vec<Value> = items.iter()
            .map(|_| Value::Composite(HashMap::new()))
            .collect();
        *candidates_cell.borrow_mut() = Some(placeholders);
        return Some(set._eq(&lit));
    }

    let (set_var, elem, candidates_cell) = env.get(name)?.as_set_with_candidates()?;

    // Build the Z3 literal set by add'ing each translated item.
    let domain = match elem {
        SeqElem::Int  => Sort::int(ctx),
        SeqElem::Bool => Sort::bool(ctx),
        SeqElem::Str  => Sort::string(ctx),
    };
    let mut lit = Z3Set::empty(ctx, &domain);
    for item in items {
        match elem {
            SeqElem::Int  => { let z = translate_int(item, ctx, env)?; lit = lit.add(&z); }
            SeqElem::Bool => {
                let z = translate_bool(item, ctx, env, schemas)?;
                lit = lit.add(&z);
            }
            SeqElem::Str  => { let z = translate_str(item, ctx, env)?; lit = lit.add(&z); }
        }
    }

    // Best-effort: record statically-resolvable candidates for the
    // extract path. If any item isn't a compile-time constant, leave
    // candidates as None — extraction silently omits the binding,
    // matching the pre-Phase-6.1 behavior for that case.
    let mut static_cands: Option<Vec<Value>> = Some(Vec::with_capacity(items.len()));
    for item in items {
        match (&mut static_cands, expr_to_const_value(item, env)) {
            (Some(acc), Some(v)) => acc.push(v),
            _ => { static_cands = None; break; }
        }
    }
    if let Some(cands) = static_cands {
        *candidates_cell.borrow_mut() = Some(cands);
    }

    Some(set_var._eq(&lit))
}

/// Build a single Datatype value (`Dynamic`) by applying `dt.variants[0]
/// .constructor` to one Dynamic per `FieldKind`. Each primitive field is
/// resolved via `env.get(&format!("{prefix}.{field_name}"))`; each nested
/// composite is resolved by recursing with prefix
/// `format!("{prefix}.{field_name}")`.
///
/// Used by `translate_seq_lit_eq` to translate `seq = ⟨ident1, ident2, …⟩`
/// when seq is a `Seq(UserType)` and each `identK` names a flat-expanded
/// sub-schema instance whose fields already exist in env as
/// `identK.field…` Z3 consts.
fn build_composite_dynamic<'ctx>(
    prefix: &str,
    dt: &'static DatatypeSort<'static>,
    fields: &[FieldKind],
    ctx: &'ctx Context,
    env: &HashMap<String, Var<'ctx>>,
) -> Option<z3::ast::Dynamic<'ctx>> {
    let mut field_dyns: Vec<z3::ast::Dynamic<'ctx>> = Vec::with_capacity(fields.len());
    for fk in fields.iter() {
        let dynamic = match fk {
            FieldKind::Primitive { name, prim_type } => {
                let key = format!("{}.{}", prefix, name);
                let var = env.get(&key)?;
                match (prim_type.as_str(), var) {
                    ("Int" | "Nat" | "Pos", Var::IntVar(i)) =>
                        z3::ast::Dynamic::from_ast(i),
                    ("Int" | "Nat" | "Pos", Var::PinnedInt(v)) =>
                        z3::ast::Dynamic::from_ast(&Int::from_i64(ctx, *v)),
                    ("Bool", Var::BoolVar(b)) =>
                        z3::ast::Dynamic::from_ast(b),
                    ("String", Var::StrVar(s)) =>
                        z3::ast::Dynamic::from_ast(s),
                    _ => return None,
                }
            }
            FieldKind::Nested { name, dt: nested_dt, sub_fields, .. } => {
                let sub_prefix = format!("{}.{}", prefix, name);
                build_composite_dynamic(&sub_prefix, nested_dt, sub_fields, ctx, env)?
            }
        };
        field_dyns.push(dynamic);
    }
    let dyn_refs: Vec<&dyn Ast> = field_dyns.iter().map(|d| d as &dyn Ast).collect();
    Some(dt.variants[0].constructor.apply(&dyn_refs))
}

/// Handle `seq[i] = composite_var` (single-element composite assignment
/// against a `Seq(UserType)`). Used by `output.rects[#state.dots] = player_rect`
/// in the dot-collect engine: assign one composite value into one slot of a
/// composite-element seq.
///
/// LHS must be `Index(Identifier(seq_name), idx_expr)` where `seq_name`
/// resolves to a `Var::DatatypeSeqVar`. RHS must be `Identifier(comp_name)`
/// where `comp_name.*` keys exist in env (flat-expanded composite from a
/// sub-schema membership). Builds the per-element Datatype value via
/// `build_composite_dynamic` and asserts `arr.select(idx) == composite`.
fn translate_seq_index_assign<'ctx>(
    lhs: &Expr,
    rhs: &Expr,
    ctx: &'ctx Context,
    env: &HashMap<String, Var<'ctx>>,
) -> Option<Bool<'ctx>> {
    let (seq_name, idx_expr) = match lhs {
        Expr::Index(seq_expr, idx_expr) => {
            let Expr::Identifier(name) = seq_expr.as_ref() else { return None };
            (name.as_str(), idx_expr.as_ref())
        }
        _ => return None,
    };
    let comp_name = match rhs {
        Expr::Identifier(n) => n.as_str(),
        _ => return None,
    };
    let var = env.get(seq_name)?;
    let (arr, _, _, dt, fields) = var.as_datatype_seq()?;
    // The composite must be flat-expanded — verify by checking at least one
    // expected leaf exists in env. Without this, `output.rects[i] = player_rect`
    // would silently match `player_rect ∈ Bool` and translate wrong.
    let first_field = fields.first().map(|f| f.name())?;
    if !env.contains_key(&format!("{}.{}", comp_name, first_field)) {
        return None;
    }
    let idx = translate_int(idx_expr, ctx, env)?;
    let composite = build_composite_dynamic(comp_name, dt, fields, ctx, env)?;
    let elem = arr.select(&idx);
    Some(elem._eq(&composite))
}

/// Walk a composite seq element and bind each declared field as
/// `<prefix>.<field_name>` in env, with the field's Z3 expression
/// extracted via the Datatype's accessor. Used by `∀ var ∈ <seq>`
/// composite iteration: for each iteration index i, the body
/// references `var.field1`, `var.field2`, etc. — those resolve via
/// env-key lookup, so we populate env with the right per-iteration
/// values before translating the body.
///
/// Recurses for `FieldKind::Nested` (e.g. `dot.color.r` where
/// `color ∈ Color`). Returns false on shape mismatch (caller
/// should fail the whole quantifier rather than silently produce
/// a wrong model).
fn bind_composite_fields<'ctx>(
    env: &mut HashMap<String, Var<'ctx>>,
    elem_dyn: &z3::ast::Dynamic<'ctx>,
    fields: &[FieldKind],
    dt: &DatatypeSort<'ctx>,
    prefix: &str,
) -> bool {
    let Some(elem) = elem_dyn.as_datatype() else { return false };
    for (fi, fk) in fields.iter().enumerate() {
        if fi >= dt.variants[0].accessors.len() { return false; }
        let raw = dt.variants[0].accessors[fi].apply(&[&elem]);
        match fk {
            FieldKind::Primitive { name, prim_type } => {
                let key = format!("{}.{}", prefix, name);
                let var = match prim_type.as_str() {
                    "Int" | "Nat" | "Pos" => raw.as_int().map(Var::IntVar),
                    "Bool"   => raw.as_bool().map(Var::BoolVar),
                    "String" => raw.as_string().map(Var::StrVar),
                    _ => None,
                };
                let Some(v) = var else { return false };
                env.insert(key, v);
            }
            FieldKind::Nested { name, dt: nested_dt, sub_fields, .. } => {
                let sub_prefix = format!("{}.{}", prefix, name);
                if !bind_composite_fields(env, &raw, sub_fields, nested_dt, &sub_prefix) {
                    return false;
                }
            }
        }
    }
    true
}

/// Whole-Seq equality: `A = B` where both `A` and `B` resolve to Seq
/// vars (primitive `SeqVar` or `DatatypeSeqVar`). Desugars to
/// element-wise `∀ i ∈ {0..n-1} : A[i] = B[i]` plus a length match.
///
/// Returns None if either side isn't a Seq, the element kinds don't
/// match, or either length isn't a literal int (we need a pinned
/// length to unroll the element-wise conjunction).
fn translate_seq_eq<'ctx>(
    lhs: &Expr,
    rhs: &Expr,
    ctx: &'ctx Context,
    env: &HashMap<String, Var<'ctx>>,
) -> Option<Bool<'ctx>> {
    let Expr::Identifier(l_name) = lhs else { return None };
    let Expr::Identifier(r_name) = rhs else { return None };
    let l = env.get(l_name)?;
    let r = env.get(r_name)?;
    match (l, r) {
        (Var::SeqVar { arr: la, len: ll, elem: le },
         Var::SeqVar { arr: ra, len: lr, elem: re }) => {
            if le != re { return None; }
            let ln = ll.simplify().as_i64()?;
            let rn = lr.simplify().as_i64()?;
            if ln != rn { return None; }
            let mut clauses: Vec<Bool> = Vec::with_capacity(ln as usize);
            for i in 0..ln {
                let idx = Int::from_i64(ctx, i);
                let l_elem = la.select(&idx);
                let r_elem = ra.select(&idx);
                clauses.push(l_elem._eq(&r_elem));
            }
            let refs: Vec<&Bool> = clauses.iter().collect();
            Some(Bool::and(ctx, &refs))
        }
        (Var::DatatypeSeqVar { arr: la, len: ll, type_name: lt, .. },
         Var::DatatypeSeqVar { arr: ra, len: lr, type_name: rt, .. }) => {
            if lt != rt { return None; }
            let ln = ll.simplify().as_i64()?;
            let rn = lr.simplify().as_i64()?;
            if ln != rn { return None; }
            let mut clauses: Vec<Bool> = Vec::with_capacity(ln as usize);
            for i in 0..ln {
                let idx = Int::from_i64(ctx, i);
                let l_elem = la.select(&idx);
                let r_elem = ra.select(&idx);
                clauses.push(l_elem._eq(&r_elem));
            }
            let refs: Vec<&Bool> = clauses.iter().collect();
            Some(Bool::and(ctx, &refs))
        }
        _ => None,
    }
}

// ── Section 9: translate_bool — the Bool dispatcher ──────────────────

pub(super) fn translate_bool<'ctx>(
    e: &Expr,
    ctx: &'ctx Context,
    env: &HashMap<String, Var<'ctx>>,
    schemas: &HashMap<String, SchemaDecl>,
) -> Option<Bool<'ctx>> {
    // `distinct(a, b, c, …)` — Z3's all-different primitive. Two
    // call shapes:
    //   * Variadic over scalar args: `distinct(a, b, c)`. All args
    //     translate to the same Z3 sort. v1 supports Int / Bool /
    //     String; picks the first sort that translates every arg.
    //   * Single Seq arg with pinned length: `distinct(seq)`.
    //     Unrolls to `distinct(seq[0], seq[1], …, seq[n-1])`
    //     and recurses through the variadic path.
    // 0 or 1 args is trivially true.
    // `contains(seq, x)` — true if x ∈ seq. The `x ∈ seq` infix
    // form is silently dropped today for element-in-Seq; this
    // builtin makes the operation explicit and translates. For a
    // pinned-length Seq, unrolls to a disjunction of element
    // equalities `seq[0] = x ∨ seq[1] = x ∨ … ∨ seq[n-1] = x`.
    if let Expr::Call(name, args) = e {
        if name == "contains" && args.len() == 2 {
            let Expr::Identifier(seq_name) = &args[0] else { return None };
            let var = env.get(seq_name)?;
            // Primitive Seq path (SeqInt / SeqBool / SeqStr).
            if let Some((arr, len, elem)) = var.as_seq() {
                let n = len.simplify().as_i64()?;
                let mut clauses: Vec<Bool> = Vec::with_capacity(n as usize);
                for i in 0..n {
                    let idx = Int::from_i64(ctx, i);
                    let cell = arr.select(&idx);
                    let eq = match elem {
                        SeqElem::Int => {
                            let v = translate_int(&args[1], ctx, env)?;
                            cell.as_int()?._eq(&v)
                        }
                        SeqElem::Bool => {
                            let v = translate_bool(&args[1], ctx, env, schemas)?;
                            cell.as_bool()?._eq(&v)
                        }
                        SeqElem::Str => {
                            let v = translate_str(&args[1], ctx, env)?;
                            cell.as_string()?._eq(&v)
                        }
                    };
                    clauses.push(eq);
                }
                let refs: Vec<&Bool> = clauses.iter().collect();
                return Some(if refs.is_empty() {
                    Bool::from_bool(ctx, false)
                } else {
                    Bool::or(ctx, &refs)
                });
            }
            // Datatype Seq path (Seq(UserType) or Seq(EnumType)).
            if let Some((arr, len, _, _, _)) = var.as_datatype_seq() {
                let n = len.simplify().as_i64()?;
                // Translate x as a Call/Identifier that resolves to a
                // datatype value via the existing seq-element handling.
                // For simplicity: build seq[i] = x for each i.
                let mut clauses: Vec<Bool> = Vec::with_capacity(n as usize);
                for i in 0..n {
                    let idx = Int::from_i64(ctx, i);
                    let cell = arr.select(&idx);
                    // Compare via the cell's _eq against translated x.
                    // For datatype types, we need translate_x_as_datatype;
                    // best-effort via the existing translate_bool's Eq path
                    // by constructing `cell_value = arg`.
                    let arg = args[1].clone();
                    let eq_expr = Expr::Binary(
                        crate::ast::BinOp::Eq,
                        Box::new(Expr::Index(
                            Box::new(args[0].clone()),
                            Box::new(Expr::Int(i)),
                        )),
                        Box::new(arg),
                    );
                    if let Some(b) = translate_bool(&eq_expr, ctx, env, schemas) {
                        clauses.push(b);
                    } else {
                        let _ = cell; // silence unused
                        return None;
                    }
                }
                let refs: Vec<&Bool> = clauses.iter().collect();
                return Some(if refs.is_empty() {
                    Bool::from_bool(ctx, false)
                } else {
                    Bool::or(ctx, &refs)
                });
            }
            return None;
        }
        if name == "distinct" {
            // 0 args: trivially true (no pair to differ).
            if args.is_empty() { return Some(Bool::from_bool(ctx, true)); }
            // 1 arg: must be a pinned-length Seq variable.
            // Returning None on failure (not vacuous true) so a
            // `distinct(s)` over an unpinned Seq surfaces as a
            // dropped constraint instead of silently passing.
            if args.len() == 1 {
                let Expr::Identifier(sname) = &args[0] else { return None };
                let var = env.get(sname)?;
                let (_, len, _) = var.as_seq()?;
                let n = len.simplify().as_i64()?;
                if n <= 1 { return Some(Bool::from_bool(ctx, true)); }
                let exploded: Vec<Expr> = (0..n).map(|i|
                    Expr::Index(
                        Box::new(Expr::Identifier(sname.clone())),
                        Box::new(Expr::Int(i)))).collect();
                return translate_bool(
                    &Expr::Call("distinct".into(), exploded),
                    ctx, env, schemas);
            }
            if let Some(ints) = args.iter()
                .map(|a| translate_int(a, ctx, env))
                .collect::<Option<Vec<_>>>()
            {
                let refs: Vec<&Int> = ints.iter().collect();
                return Some(Int::distinct(ctx, &refs));
            }
            if let Some(bools) = args.iter()
                .map(|a| translate_bool(a, ctx, env, schemas))
                .collect::<Option<Vec<_>>>()
            {
                let refs: Vec<&Bool> = bools.iter().collect();
                return Some(Bool::distinct(ctx, &refs));
            }
            if let Some(strs) = args.iter()
                .map(|a| translate_str(a, ctx, env))
                .collect::<Option<Vec<_>>>()
            {
                let refs: Vec<&Z3Str> = strs.iter().collect();
                return Some(Z3Str::distinct(ctx, &refs));
            }
            return None;
        }
    }
    match e {
        Expr::Bool(b) => Some(Bool::from_bool(ctx, *b)),
        Expr::Identifier(name) => env.get(name).and_then(|v| v.as_bool().cloned()),
        Expr::Not(inner) => Some(translate_bool(inner, ctx, env, schemas)?.not()),

        // `cond ? a : b` with Bool branches → Z3 ITE.
        Expr::Ternary(c, a, b) => {
            let cond = translate_bool(c, ctx, env, schemas)?;
            let then_v = translate_bool(a, ctx, env, schemas)?;
            let else_v = translate_bool(b, ctx, env, schemas)?;
            Some(cond.ite(&then_v, &else_v))
        }
        Expr::Match(scr, arms) => {
            let compiled = translate_match_arms(scr, arms, ctx, env,
                |body, e| translate_bool(body, ctx, e, schemas))?;
            fold_arms_to_ite(compiled)
        }
        // `e matches Pattern` — constructor recognizer. Returns Bool.
        // Wildcard pattern → always true. Ctor pattern → is_Ctor(e).
        // Payload binds in the pattern are IGNORED (use `match` to
        // bind, or `e = Ctor(literal)` to compare payload values).
        Expr::Matches(e, pattern) => {
            use crate::ast::MatchPattern;
            match pattern {
                MatchPattern::Wildcard => Some(Bool::from_bool(ctx, true)),
                MatchPattern::Ctor { name, .. } => {
                    let scr_name = match e.as_ref() {
                        Expr::Identifier(n) if !n.contains('.') => n,
                        _ => return None,
                    };
                    let (scr_dt, dt) = match env.get(scr_name)? {
                        Var::EnumVar { ast, dt, .. } => (ast.clone(), *dt),
                        _ => return None,
                    };
                    let var_idx = dt.variants.iter()
                        .position(|v| v.constructor.name() == *name)?;
                    dt.variants[var_idx].tester.apply(&[&scr_dt]).as_bool()
                }
            }
        }

        // `seq[i]` where seq holds Bool elements.
        Expr::Index(seq_expr, idx_expr) => {
            let name = match seq_expr.as_ref() {
                Expr::Identifier(n) => n,
                _ => return None,
            };
            let (arr, _, elem) = env.get(name)?.as_seq()?;
            if elem != SeqElem::Bool { return None; }
            let i = translate_int(idx_expr, ctx, env)?;
            arr.select(&i).as_bool()
        }
        // `pts[i].active` where pts is Seq(UserType) and `active` is a
        // Bool field.
        Expr::Field(_, _) => {
            let (raw, ftype) = resolve_seq_field(e, ctx, env)?;
            if ftype == "Bool" { raw.as_bool() } else { None }
        }

        // `x ∈ {a, b, c}` → x = a ∨ x = b ∨ x = c.
        // `x ∈ s` where s is a Set var → s.member(x).
        Expr::InExpr(lhs, rhs) => {
            // Set-var RHS (Identifier whose env entry is SetVar): use Z3's
            // native set membership.
            if let Expr::Identifier(name) = rhs.as_ref() {
                if let Some((set, elem)) = env.get(name).and_then(|v| v.as_set()) {
                    return match elem {
                        SeqElem::Int => {
                            let x = translate_int(lhs, ctx, env)?;
                            Some(set.member(&x))
                        }
                        SeqElem::Bool => {
                            let x = translate_bool(lhs, ctx, env, schemas)?;
                            Some(set.member(&x))
                        }
                        SeqElem::Str => {
                            let x = translate_str(lhs, ctx, env)?;
                            Some(set.member(&x))
                        }
                    };
                }
                // Composite-element Set: LHS must be an Identifier whose
                // flat-expanded fields exist in env (same shape as for
                // `Seq(Composite)` element references). Build the
                // composite Dynamic and use Z3 native set.member.
                if let Some((set, _, dt, fields, _)) =
                    env.get(name).and_then(|v| v.as_datatype_set())
                {
                    if let Expr::Identifier(ident) = lhs.as_ref() {
                        let dyn_val = build_composite_dynamic(ident, dt, fields, ctx, env)?;
                        return Some(set.member(&dyn_val));
                    }
                }
            }
            // Set-literal RHS: reduce to OR of equalities.
            let items = match rhs.as_ref() {
                Expr::SetLit(items) => items.clone(),
                _ => return None,
            };
            let mut clauses: Vec<Bool> = Vec::with_capacity(items.len());
            for it in &items {
                let eq = Expr::Binary(BinOp::Eq, lhs.clone(), Box::new(it.clone()));
                if let Some(b) = translate_bool(&eq, ctx, env, schemas) {
                    clauses.push(b);
                }
            }
            if clauses.is_empty() { return Some(Bool::from_bool(ctx, false)); }
            let refs: Vec<&Bool> = clauses.iter().collect();
            Some(Bool::or(ctx, &refs))
        }

        // `∀ vars ∈ <range> : body` / `∃ …`. Range shapes:
        //
        //   1. Integer range `{lo..hi}` — unrolls lo..=hi, binds the
        //      single var to each Int. Single-var binding only.
        //   2. Composite seq `state.dots` (Seq(UserType)) — unrolls
        //      0..len, binds `var.field` to each leaf of state.dots[i].
        //      Single-var only.
        //   3. Primitive seq `s` (Seq(Int|Bool|String)) — unrolls
        //      0..len, binds the single var to each element.
        //   4. `coindexed(A, B, C)` — N-arity zip. Tuple binding required;
        //      each iteration binds vars[k] to seqs[k][i] (positionally
        //      across all sequences).
        //   5. `edges(seq)` — consecutive-pair iteration. 2-tuple binding;
        //      each iteration binds vars[0] to seq[i], vars[1] to seq[i+1].
        Expr::Forall(vars, range, body) | Expr::Exists(vars, range, body) => {
            let mut clauses: Vec<Bool> = Vec::new();

            // Form 4: coindexed(A, B, …) — tuple-binding required.
            if let Expr::Call(name, args) = range.as_ref() {
                match (name.as_str(), args.len()) {
                    ("coindexed", n_seqs) if n_seqs >= 1 => {
                        if vars.len() != n_seqs {
                            return None; // arity mismatch — let the caller's
                                         // dropped-constraint path surface it
                        }
                        // All sequences must have the same pinned length.
                        // Build the (Var-handle, length) per sequence so we
                        // can iterate and bind each var per index.
                        let mut seq_lens: Vec<i64> = Vec::with_capacity(n_seqs);
                        for arg in args {
                            let Expr::Identifier(seq_name) = arg else { return None };
                            let seq_var = env.get(seq_name)?;
                            let len = if let Some((_, len, _, _, _)) = seq_var.as_datatype_seq() {
                                len.simplify().as_i64()?
                            } else if let Some((_, len, _)) = seq_var.as_seq() {
                                len.simplify().as_i64()?
                            } else {
                                return None;
                            };
                            seq_lens.push(len);
                        }
                        let n = *seq_lens.iter().min()?;
                        for i in 0..n {
                            let mut env2 = env_clone(env);
                            for (var, arg) in vars.iter().zip(args.iter()) {
                                let Expr::Identifier(seq_name) = arg else { return None };
                                let seq_var = env.get(seq_name)?;
                                let idx = Int::from_i64(ctx, i);
                                if let Some((arr, _, _, dt, fields)) = seq_var.as_datatype_seq() {
                                    let elem_dyn = arr.select(&idx);
                                    if !bind_composite_fields(&mut env2, &elem_dyn, fields, dt, var) {
                                        return None;
                                    }
                                } else if let Some((arr, _, elem)) = seq_var.as_seq() {
                                    let cell = arr.select(&idx);
                                    let v = match elem {
                                        SeqElem::Int  => cell.as_int().map(Var::IntVar),
                                        SeqElem::Bool => cell.as_bool().map(Var::BoolVar),
                                        SeqElem::Str  => cell.as_string().map(Var::StrVar),
                                    };
                                    env2.insert(var.clone(), v?);
                                } else {
                                    return None;
                                }
                            }
                            if let Some(b) = translate_bool(body, ctx, &env2, schemas) {
                                clauses.push(b);
                            }
                        }
                        let refs: Vec<&Bool> = clauses.iter().collect();
                        return Some(if matches!(e, Expr::Forall(..)) {
                            Bool::and(ctx, &refs)
                        } else if refs.is_empty() {
                            Bool::from_bool(ctx, false)
                        } else {
                            Bool::or(ctx, &refs)
                        });
                    }
                    ("edges", 1) => {
                        // edges(seq) — adjacent-pair iteration, requires
                        // a 2-tuple binding. Each step binds vars[0] to
                        // seq[i] and vars[1] to seq[i+1] for i in 0..n-1.
                        if vars.len() != 2 { return None; }
                        let arg = &args[0];
                        let Expr::Identifier(seq_name) = arg else { return None };
                        let seq_var = env.get(seq_name)?;
                        let (n, bind): (i64, Box<dyn Fn(&mut HashMap<String, Var<'ctx>>, i64, &str) -> bool>) =
                            if let Some((arr, len, _, dt, fields)) = seq_var.as_datatype_seq() {
                                let arr = arr.clone(); let fields = fields.to_vec();
                                let n = len.simplify().as_i64()?;
                                (n, Box::new(move |env2, i, var| {
                                    let idx = Int::from_i64(ctx, i);
                                    let elem_dyn = arr.select(&idx);
                                    bind_composite_fields(env2, &elem_dyn, &fields, dt, var)
                                }))
                            } else if let Some((arr, len, elem)) = seq_var.as_seq() {
                                let arr = arr.clone();
                                let n = len.simplify().as_i64()?;
                                (n, Box::new(move |env2, i, var| {
                                    let idx = Int::from_i64(ctx, i);
                                    let cell = arr.select(&idx);
                                    let v = match elem {
                                        SeqElem::Int  => cell.as_int().map(Var::IntVar),
                                        SeqElem::Bool => cell.as_bool().map(Var::BoolVar),
                                        SeqElem::Str  => cell.as_string().map(Var::StrVar),
                                    };
                                    match v {
                                        Some(v) => { env2.insert(var.to_string(), v); true }
                                        None => false,
                                    }
                                }))
                            } else {
                                return None;
                            };
                        for i in 0..(n - 1) {
                            let mut env2 = env_clone(env);
                            if !bind(&mut env2, i,     &vars[0]) { return None; }
                            if !bind(&mut env2, i + 1, &vars[1]) { return None; }
                            if let Some(b) = translate_bool(body, ctx, &env2, schemas) {
                                clauses.push(b);
                            }
                        }
                        let refs: Vec<&Bool> = clauses.iter().collect();
                        return Some(if matches!(e, Expr::Forall(..)) {
                            Bool::and(ctx, &refs)
                        } else if refs.is_empty() {
                            Bool::from_bool(ctx, false)
                        } else {
                            Bool::or(ctx, &refs)
                        });
                    }
                    _ => return None,    // unknown function in quantifier range
                }
            }

            // Forms 1–3 require a single-name binding.
            if vars.len() != 1 { return None; }
            let var = &vars[0];

            // Form 1: integer range.
            if let Some((lo, hi)) = literal_range(range, ctx, env) {
                for i in lo..=hi {
                    let mut env2 = env_clone(env);
                    env2.insert(var.clone(), Var::IntVar(Int::from_i64(ctx, i)));
                    if let Some(b) = translate_bool(body, ctx, &env2, schemas) {
                        clauses.push(b);
                    }
                }
            // Form 2 / 3: iterate over a Seq variable.
            } else if let Expr::Identifier(seq_name) = range.as_ref() {
                let seq_var = env.get(seq_name)?;
                if let Some((arr, len, _, dt, fields)) = seq_var.as_datatype_seq() {
                    // Composite seq: iterate elements, bind <var>.<field>
                    // for each declared field in env on each iteration.
                    let n = len.simplify().as_i64()?;
                    for i in 0..n {
                        let mut env2 = env_clone(env);
                        let idx = Int::from_i64(ctx, i);
                        let elem_dyn = arr.select(&idx);
                        if !bind_composite_fields(&mut env2, &elem_dyn, fields, dt, var) {
                            return None; // shape mismatch — fail loudly
                        }
                        if let Some(b) = translate_bool(body, ctx, &env2, schemas) {
                            clauses.push(b);
                        }
                    }
                } else if let Some((arr, len, elem)) = seq_var.as_seq() {
                    // Primitive seq: bind `var` to the element directly.
                    let n = len.simplify().as_i64()?;
                    for i in 0..n {
                        let mut env2 = env_clone(env);
                        let idx = Int::from_i64(ctx, i);
                        let cell = arr.select(&idx);
                        let v = match elem {
                            SeqElem::Int  => cell.as_int().map(Var::IntVar),
                            SeqElem::Bool => cell.as_bool().map(Var::BoolVar),
                            SeqElem::Str  => cell.as_string().map(Var::StrVar),
                        };
                        let v = v?;
                        env2.insert(var.clone(), v);
                        if let Some(b) = translate_bool(body, ctx, &env2, schemas) {
                            clauses.push(b);
                        }
                    }
                } else if let Some((set, _elem)) = seq_var.as_set() {
                    // Primitive-element Set: detect the subset pattern
                    // `∀ x ∈ a : x ∈ b` and emit Z3 native set_subset.
                    // Used for both pinned and free Sets — works without
                    // iteration. Anything else over a primitive Set is
                    // unsupported in v1.
                    if let Some(other_set) = match_set_subset_body(body, var, env) {
                        let b = set.set_subset(other_set);
                        return Some(if matches!(e, Expr::Forall(..)) {
                            b
                        } else {
                            b.not().not()    // ∃ x ∈ a : x ∈ b is "a ∩ b ≠ ∅"
                                              // — different semantics; we don't
                                              // model existence here.
                        });
                    }
                    return None;
                } else if let Some((set, _, _, _, _)) = seq_var.as_datatype_set() {
                    // Composite-element Set: same subset pattern as the
                    // primitive case. The pattern is `∀ e ∈ a : e ∈ b`
                    // where the body's `e` was a flat-expanded composite;
                    // both `a` and `b` must be DatatypeSetVars over the
                    // same datatype.
                    if let Some(other_set) = match_set_subset_body(body, var, env) {
                        let b = set.set_subset(other_set);
                        return Some(if matches!(e, Expr::Forall(..)) { b } else { b });
                    }
                    return None;
                } else {
                    // Identifier in scope but not a seq — can't iterate.
                    return None;
                }
            } else {
                // Range expression we don't recognize.
                return None;
            }

            let refs: Vec<&Bool> = clauses.iter().collect();
            if matches!(e, Expr::Forall(..)) {
                Some(Bool::and(ctx, &refs))
            } else {
                if refs.is_empty() { Some(Bool::from_bool(ctx, false)) }
                else                { Some(Bool::or(ctx, &refs)) }
            }
        }
        Expr::Binary(op, lhs, rhs) => match op {
            // Boolean combinators
            BinOp::And => {
                let l = translate_bool(lhs, ctx, env, schemas)?;
                let r = translate_bool(rhs, ctx, env, schemas)?;
                Some(Bool::and(ctx, &[&l, &r]))
            }
            BinOp::Or => {
                let l = translate_bool(lhs, ctx, env, schemas)?;
                let r = translate_bool(rhs, ctx, env, schemas)?;
                Some(Bool::or(ctx, &[&l, &r]))
            }
            BinOp::Implies => {
                let l = translate_bool(lhs, ctx, env, schemas)?;
                let r = translate_bool(rhs, ctx, env, schemas)?;
                Some(l.implies(&r))
            }
            // Eq/Neq work over Bool, Int, or String. Try in that order.
            BinOp::Eq | BinOp::Neq => {
                // Cons/Nil-shaped enum SeqLit: `effs = ⟨a, b, c⟩` where
                // `effs` is e.g. EffectList (any enum with a 0-arity
                // variant + a 2-arity self-recursive variant).
                if let Some(b) = translate_cons_chain_eq(lhs, rhs, ctx, env, schemas) {
                    return Some(match op {
                        BinOp::Eq  => b,
                        BinOp::Neq => b.not(),
                        _ => unreachable!(),
                    });
                }
                if let Some(b) = translate_cons_chain_eq(rhs, lhs, ctx, env, schemas) {
                    return Some(match op {
                        BinOp::Eq  => b,
                        BinOp::Neq => b.not(),
                        _ => unreachable!(),
                    });
                }
                // First: handle `seq_var = ⟨e1, e2, …⟩` (sequence literal
                // assignment). This pins both length and per-element values
                // and lives outside the Bool/Int/Str scalar paths because
                // it produces a conjunction over the elements rather than
                // a single _eq.
                if let Some(b) = translate_seq_lit_eq(lhs, rhs, ctx, env, schemas) {
                    return Some(match op {
                        BinOp::Eq  => b,
                        BinOp::Neq => b.not(),
                        _ => unreachable!(),
                    });
                }
                if let Some(b) = translate_seq_lit_eq(rhs, lhs, ctx, env, schemas) {
                    return Some(match op {
                        BinOp::Eq  => b,
                        BinOp::Neq => b.not(),
                        _ => unreachable!(),
                    });
                }
                // `set_var = {a, b, c}` — exact set membership. Mirror of
                // translate_seq_lit_eq but for SetVar + SetLit. Records
                // candidates for the extract path.
                if let Some(b) = translate_set_lit_eq(lhs, rhs, ctx, env, schemas) {
                    return Some(match op {
                        BinOp::Eq  => b,
                        BinOp::Neq => b.not(),
                        _ => unreachable!(),
                    });
                }
                if let Some(b) = translate_set_lit_eq(rhs, lhs, ctx, env, schemas) {
                    return Some(match op {
                        BinOp::Eq  => b,
                        BinOp::Neq => b.not(),
                        _ => unreachable!(),
                    });
                }
                // `A = B` (whole-Seq equality between two named Seq
                // vars). Desugars to element-wise equality + length
                // match. Try lhs/rhs in only one direction since the
                // helper is symmetric in operand roles.
                if let Some(b) = translate_seq_eq(lhs, rhs, ctx, env) {
                    return Some(match op {
                        BinOp::Eq  => b,
                        BinOp::Neq => b.not(),
                        _ => unreachable!(),
                    });
                }
                // `seq[i] = composite_var` (single-element composite-seq
                // assignment). Try both orientations.
                if let Some(b) = translate_seq_index_assign(lhs, rhs, ctx, env) {
                    return Some(match op {
                        BinOp::Eq  => b,
                        BinOp::Neq => b.not(),
                        _ => unreachable!(),
                    });
                }
                if let Some(b) = translate_seq_index_assign(rhs, lhs, ctx, env) {
                    return Some(match op {
                        BinOp::Eq  => b,
                        BinOp::Neq => b.not(),
                        _ => unreachable!(),
                    });
                }
                if let (Some(l), Some(r)) =
                    (translate_bool(lhs, ctx, env, schemas), translate_bool(rhs, ctx, env, schemas))
                {
                    return Some(match op {
                        BinOp::Eq  => l._eq(&r),
                        BinOp::Neq => l._eq(&r).not(),
                        _ => unreachable!(),
                    });
                }
                if let (Some(l), Some(r)) =
                    (translate_int(lhs, ctx, env), translate_int(rhs, ctx, env))
                {
                    return Some(match op {
                        BinOp::Eq  => l._eq(&r),
                        BinOp::Neq => l._eq(&r).not(),
                        _ => unreachable!(),
                    });
                }
                // Real path: at least one side is Real (RealVar or Real
                // literal); the other side may be Int and gets coerced.
                if let (Some(l), Some(r)) =
                    (translate_real(lhs, ctx, env), translate_real(rhs, ctx, env))
                {
                    return Some(match op {
                        BinOp::Eq  => l._eq(&r),
                        BinOp::Neq => l._eq(&r).not(),
                        _ => unreachable!(),
                    });
                }
                if let (Some(l), Some(r)) =
                    (translate_str(lhs, ctx, env), translate_str(rhs, ctx, env))
                {
                    return Some(match op {
                        BinOp::Eq  => l._eq(&r),
                        BinOp::Neq => l._eq(&r).not(),
                        _ => unreachable!(),
                    });
                }
                // Enum equality: `today = Mon` where `today` is an
                // EnumVar and `Mon` is an EnumValue (or vice versa, or
                // both EnumValues). Both sides must reference enum-
                // typed identifiers in env. Different enums on the two
                // sides aren't allowed — caller has a type error.
                //
                // If LHS is an enum-typed Identifier, set it as the
                // SeqLit-target hint so any ⟨…⟩ inside RHS (including
                // inside match arm bodies) lowers to the correct
                // Cons/Nil chain.
                let target_hint = match lhs.as_ref() {
                    Expr::Identifier(n) => env.get(n).and_then(|v| match v {
                        Var::EnumVar { enum_name, dt, .. } => Some((enum_name.clone(), *dt)),
                        _ => None,
                    }),
                    _ => None,
                };
                let pair = with_target_enum_hint(target_hint.clone(), || {
                    let l = resolve_enum_ast(lhs, ctx, env, schemas);
                    let r = resolve_enum_ast(rhs, ctx, env, schemas);
                    (l, r)
                });
                if let (Some(l), Some(r)) = pair {
                    return Some(match op {
                        BinOp::Eq  => l._eq(&r),
                        BinOp::Neq => l._eq(&r).not(),
                        _ => unreachable!(),
                    });
                }
                // Record-op broadcast: handles `=`, `≠` between
                // record-typed expressions on either side, including
                // arithmetic (`vec_lo = vec - offset`).
                lift_record_op(op, lhs, rhs, ctx, env, schemas)
            }
            // Numeric comparisons. Try Int first; fall back to Real
            // (with Int→Real coercion) so `realvar < 3` and
            // `realvar < 3.14` both work.
            BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
                if let (Some(l), Some(r)) =
                    (translate_int(lhs, ctx, env), translate_int(rhs, ctx, env))
                {
                    return Some(match op {
                        BinOp::Lt => l.lt(&r),
                        BinOp::Le => l.le(&r),
                        BinOp::Gt => l.gt(&r),
                        BinOp::Ge => l.ge(&r),
                        _ => unreachable!(),
                    });
                }
                if let (Some(l), Some(r)) =
                    (translate_real(lhs, ctx, env), translate_real(rhs, ctx, env))
                {
                    return Some(match op {
                        BinOp::Lt => l.lt(&r),
                        BinOp::Le => l.le(&r),
                        BinOp::Gt => l.gt(&r),
                        BinOp::Ge => l.ge(&r),
                        _ => unreachable!(),
                    });
                }
                // Record-op broadcast: `<`, `≤`, `>`, `≥` between
                // record-typed expressions are componentwise. Same
                // helper as Eq/Neq — operator threads through.
                // Handles `vec_lo ≤ vec` and arithmetic-laden forms
                // like `dot.pos - offset_lo ≤ player.pos`.
                lift_record_op(op, lhs, rhs, ctx, env, schemas)
            }
            _ => None,
        }
        _ => None,
    }
}

// ── Section 10: Match-expression translator ──────────────────────────
//
// `match scrutinee
//      Ctor(b1, ...) ⇒ body
//      _             ⇒ fallback`
//
// translates to a nested Z3 `Bool::ite(...)` chain over the
// constructor-recognizer (tester) booleans. Each non-wildcard arm's
// body is translated with payload bindings extended into a cloned env.
//
// v1 limitations:
//   - Scrutinee must be a bare Identifier (Var::EnumVar in env).
//   - Payload bindings are restricted to Int / Bool / String / Real
//     fields. Enum-typed payloads can use `_` to discard but not bind.
//   - Exhaustiveness isn't enforced — if no arm matches at runtime,
//     the last arm's body is used as the trailing else (which may
//     fire incorrectly if the user omitted variants).

/// One compiled arm: an optional tester boolean (None = wildcard) and
/// the translated body in a per-arm extended env. Type T is the body's
/// Z3 sort (Int / Bool / Z3Str / Real / Datatype).
type CompiledArm<'ctx, T> = (Option<Bool<'ctx>>, T);

/// Resolve the scrutinee + walk arms, returning a Vec of (tester, body).
/// Body translation is delegated to `body_translator` so the same
/// machinery serves Int / Bool / Str / Real / Enum match results.
fn translate_match_arms<'ctx, T>(
    scr: &Expr,
    arms: &[crate::ast::MatchArm],
    ctx: &'ctx Context,
    env: &HashMap<String, Var<'ctx>>,
    body_translator: impl Fn(&Expr, &HashMap<String, Var<'ctx>>) -> Option<T>,
) -> Option<Vec<CompiledArm<'ctx, T>>> {
    use crate::ast::MatchPattern;
    // Scrutinee shapes supported:
    //   * Bare Identifier resolving to Var::EnumVar.
    //   * Index(Identifier(seq), idx) where `seq` is a Var::DatatypeSeqVar
    //     with empty fields (i.e. Seq(EnumType)) — element pulled via
    //     arr.select(idx). Lets `match last_results[0]` reach the same
    //     arm machinery as bare-identifier matches.
    let (scr_dt, dt, scr_enum_name) = match scr {
        Expr::Identifier(n) if !n.contains('.') => {
            match env.get(n)? {
                Var::EnumVar { ast, dt, enum_name } =>
                    (ast.clone(), *dt, enum_name.clone()),
                Var::EnumValue { .. } => return None,
                _ => return None,
            }
        }
        Expr::Index(seq_expr, idx_expr) => {
            let Expr::Identifier(seq_name) = seq_expr.as_ref() else { return None };
            if seq_name.contains('.') { return None; }
            let (arr, dt, type_name) = match env.get(seq_name)? {
                Var::DatatypeSeqVar { arr, dt, type_name, fields, .. }
                    if fields.is_empty() =>
                        (arr.clone(), *dt, type_name.clone()),
                _ => return None,
            };
            let idx = translate_int(idx_expr, ctx, env)?;
            let elem_dt = arr.select(&idx).as_datatype()?;
            (elem_dt, dt, type_name)
        }
        _ => return None,
    };
    let mut compiled: Vec<CompiledArm<T>> = Vec::new();
    for arm in arms {
        match &arm.pattern {
            MatchPattern::Wildcard => {
                let body = body_translator(&arm.body, env)?;
                compiled.push((None, body));
            }
            MatchPattern::Ctor { name, binds } => {
                let var_idx = dt.variants.iter()
                    .position(|v| v.constructor.name() == *name)?;
                let z3_var = &dt.variants[var_idx];
                if binds.len() != z3_var.accessors.len() { return None; }
                let tester = z3_var.tester.apply(&[&scr_dt]).as_bool()?;
                let mut env2 = env_clone(env);
                let scr_enum_name = scr_enum_name.clone();
                let field_decls: Vec<crate::ast::EnumField> = with_active_enums(|enums| {
                    enums.and_then(|er| {
                        er.by_name.borrow().get(&scr_enum_name)
                            .and_then(|(_, variants)| {
                                variants.iter()
                                    .find(|v| v.name == *name)
                                    .map(|v| v.fields.clone())
                            })
                    }).unwrap_or_default()
                });
                for (j, bind_opt) in binds.iter().enumerate() {
                    let Some(bind_name) = bind_opt else { continue };
                    let acc = &z3_var.accessors[j];
                    let raw = acc.apply(&[&scr_dt]);
                    // Try each primitive sort first.
                    let var = if let Some(i) = raw.as_int() { Var::IntVar(i) }
                        else if let Some(b) = raw.as_bool() { Var::BoolVar(b) }
                        else if let Some(s) = raw.as_string() { Var::StrVar(s) }
                        else if let Some(r) = raw.as_real() { Var::RealVar(r) }
                        else if let Some(payload_dt) = raw.as_datatype() {
                            // Enum-typed payload. The field's type name
                            // comes from the EnumField list we looked up
                            // above. For self-recursion the type matches
                            // the scrutinee; for cross-enum we look up
                            // the field's type in the EnumRegistry.
                            let field_type = field_decls.get(j)
                                .map(|f| f.type_name.clone())
                                .unwrap_or_else(|| scr_enum_name.clone());
                            let payload_dt_sort: &'static DatatypeSort<'static> =
                                with_active_enums(|enums| {
                                    enums.and_then(|er| {
                                        er.by_name.borrow().get(&field_type)
                                            .map(|(d, _)| *d)
                                    })
                                }).unwrap_or(dt);  // fall back to scrutinee's dt
                            Var::EnumVar {
                                ast: payload_dt,
                                enum_name: field_type,
                                dt: payload_dt_sort,
                            }
                        }
                        else { return None; };
                    env2.insert(bind_name.clone(), var);
                }
                let body = body_translator(&arm.body, &env2)?;
                compiled.push((Some(tester), body));
            }
        }
    }
    Some(compiled)
}

/// Fold compiled arms bottom-up into a nested ITE. Last arm's body
/// becomes the trailing else; any earlier wildcard arm short-circuits
/// (its body becomes the new accumulator).
fn fold_arms_to_ite<'ctx, T>(
    mut compiled: Vec<CompiledArm<'ctx, T>>,
) -> Option<T>
where
    T: z3::ast::Ast<'ctx>,
{
    if compiled.is_empty() { return None; }
    let (_, last_body) = compiled.pop()?;
    let mut acc = last_body;
    for (tester_opt, body) in compiled.into_iter().rev() {
        match tester_opt {
            None       => { acc = body; }
            Some(tester) => { acc = tester.ite(&body, &acc); }
        }
    }
    Some(acc)
}

// ── Section 11: Literal-range folder ─────────────────────────────────

/// Resolve `Range(lo, hi)` to a `(lo, hi)` literal pair.
///
/// Both bounds are evaluated through `translate_int` (so identifiers
/// bound to `Var::PinnedInt` resolve to literal `IntVal`s and arithmetic
/// over them folds), then Z3 `simplify` reduces to a literal that
/// `as_i64` can extract. Returns None if either bound stays symbolic
/// (no PinnedInt for it) or the simplified form isn't a literal.
///
/// This is what enables `∀ i ∈ {0..n - 1}` when `n` is bound to a
/// concrete value via `n = #seq` length propagation, `n = 4` pinning,
/// or a `given` value.
///
/// Lives in `exprs` because it builds Z3 expressions (calls
/// `translate_int`) — the prior home in `preprocess` was a layering
/// inversion (preprocess is AST→AST only) AND created a cycle
/// (preprocess → exprs for `translate_int`, exprs → preprocess for
/// `literal_range`).
pub(super) fn literal_range<'ctx>(
    e: &Expr,
    ctx: &'ctx Context,
    env: &HashMap<String, Var<'ctx>>,
) -> Option<(i64, i64)> {
    if let Expr::Range(lo, hi) = e {
        let lo_z3 = translate_int(lo, ctx, env)?;
        let hi_z3 = translate_int(hi, ctx, env)?;
        let lo_v = lo_z3.simplify().as_i64()?;
        let hi_v = hi_z3.simplify().as_i64()?;
        return Some((lo_v, hi_v));
    }
    None
}
