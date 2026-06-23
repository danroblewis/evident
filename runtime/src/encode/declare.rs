use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use z3::ast::{Array, Ast, Bool, Int, Real, Set, String as Z3Str};
use z3::{Context, DatatypeAccessor, DatatypeBuilder, DatatypeSort, Sort};

use crate::core::ast::*;
use crate::core::{DatatypeRegistry, EnumRegistry, FieldKind, SeqElem, SeqFieldElem, Value, Var};

static CLAIM_CALL_COUNTER: AtomicU64 = AtomicU64::new(0);

pub(super) fn next_call_id() -> u64 {
    CLAIM_CALL_COUNTER.fetch_add(1, Ordering::Relaxed)
}

#[must_use]
pub(super) fn declare_var(
    ctx: &'static Context,
    env: &mut HashMap<String, Var<'static>>,
    prefix: &str,
    type_name: &str,
    schemas: &HashMap<String, SchemaDecl>,
    registry: Option<&DatatypeRegistry>,
    enums: Option<&EnumRegistry>,
) -> Vec<Bool<'static>> {
    declare_var_named(ctx, env, prefix, prefix, type_name, schemas, registry, enums)
}

#[must_use]
pub(super) fn declare_var_named(
    ctx: &'static Context,
    env: &mut HashMap<String, Var<'static>>,
    env_key: &str,
    z3_name: &str,
    type_name: &str,
    schemas: &HashMap<String, SchemaDecl>,
    registry: Option<&DatatypeRegistry>,
    enums: Option<&EnumRegistry>,
) -> Vec<Bool<'static>> {
    let mut post: Vec<Bool<'static>> = Vec::new();

    if env.contains_key(env_key) { return post; }
    let prefix = z3_name;
    match type_name {
        "Int" => {
            env.insert(env_key.to_string(), Var::IntVar(Int::new_const(ctx, prefix)));
        }
        "Nat" => {
            let v = Int::new_const(ctx, prefix);
            post.push(v.ge(&Int::from_i64(ctx, 0)));
            env.insert(env_key.to_string(), Var::IntVar(v));
        }
        "Pos" => {
            let v = Int::new_const(ctx, prefix);
            post.push(v.gt(&Int::from_i64(ctx, 0)));
            env.insert(env_key.to_string(), Var::IntVar(v));
        }
        "Bool" => {
            env.insert(env_key.to_string(), Var::BoolVar(Bool::new_const(ctx, prefix)));
        }
        "Real" => {
            env.insert(env_key.to_string(), Var::RealVar(Real::new_const(ctx, prefix)));
        }
        "String" => {
            env.insert(env_key.to_string(), Var::StrVar(Z3Str::new_const(ctx, prefix)));
        }

        s if s.starts_with("Seq(") && s.ends_with(')') => {
            let inner = &s[4..s.len() - 1];
            match inner {
                "Int" | "Bool" | "String" => {
                    let (range, elem) = match inner {
                        "Int"    => (Sort::int(ctx),    SeqElem::Int),
                        "Bool"   => (Sort::bool(ctx),   SeqElem::Bool),
                        "String" => (Sort::string(ctx), SeqElem::Str),
                        _ => unreachable!(),
                    };
                    let arr = Array::new_const(ctx, prefix, &Sort::int(ctx), &range);
                    let len = Int::new_const(ctx, format!("{}__len", prefix).as_str());
                    post.push(len.ge(&Int::from_i64(ctx, 0)));
                    env.insert(env_key.to_string(), Var::SeqVar { arr, len, elem });
                }
                user_type if schemas.contains_key(user_type) => {
                    let Some(reg) = registry else {
                        eprintln!(
                            "warning: Seq({}) requires a DatatypeRegistry; \
                             skipping declaration of {}",
                            user_type, prefix
                        );
                        return post;
                    };
                    let Some((dt, fields)) = get_or_build_datatype(user_type, ctx, schemas, reg, enums) else {
                        return post;
                    };
                    let arr = Array::new_const(ctx, prefix, &Sort::int(ctx), &dt.sort);
                    let len = Int::new_const(ctx, format!("{}__len", prefix).as_str());
                    post.push(len.ge(&Int::from_i64(ctx, 0)));
                    env.insert(env_key.to_string(), Var::DatatypeSeqVar {
                        arr, len,
                        type_name: user_type.to_string(),
                        dt,
                        fields,
                    });
                }

                enum_type if enums.is_some()
                    && enums.unwrap().by_name.borrow().contains_key(enum_type) => {
                    let er = enums.unwrap();
                    let dts = er.by_name.borrow();
                    let (dt, _variants) = dts.get(enum_type).unwrap();
                    let arr = Array::new_const(ctx, prefix, &Sort::int(ctx), &dt.sort);
                    let len = Int::new_const(ctx, format!("{}__len", prefix).as_str());
                    post.push(len.ge(&Int::from_i64(ctx, 0)));
                    env.insert(env_key.to_string(), Var::DatatypeSeqVar {
                        arr, len,
                        type_name: enum_type.to_string(),
                        dt: *dt,
                        fields: Vec::new(),
                    });
                }
                other => {
                    eprintln!("warning: unsupported Seq element type {} for {}", other, prefix);
                }
            }
        }
        s if s.starts_with("Set(") && s.ends_with(')') => {
            let inner = &s[4..s.len() - 1];
            match inner {
                "Int" | "Bool" | "String" => {
                    let (eltype, elem) = match inner {
                        "Int"    => (Sort::int(ctx),    SeqElem::Int),
                        "Bool"   => (Sort::bool(ctx),   SeqElem::Bool),
                        "String" => (Sort::string(ctx), SeqElem::Str),
                        _ => unreachable!(),
                    };
                    let set = Set::new_const(ctx, prefix, &eltype);
                    env.insert(env_key.to_string(), Var::SetVar {
                        set,
                        elem,
                        candidates: std::rc::Rc::new(std::cell::RefCell::new(None)),
                    });
                }
                user_type if schemas.contains_key(user_type) => {
                    let Some(reg) = registry else {
                        eprintln!(
                            "warning: Set({}) requires a DatatypeRegistry; \
                             skipping declaration of {}",
                            user_type, prefix
                        );
                        return post;
                    };
                    let Some((dt, fields)) = get_or_build_datatype(user_type, ctx, schemas, reg, enums) else {
                        return post;
                    };
                    let set = Set::new_const(ctx, prefix, &dt.sort);
                    env.insert(env_key.to_string(), Var::DatatypeSetVar {
                        set,
                        type_name: user_type.to_string(),
                        dt,
                        fields,
                        candidates: std::rc::Rc::new(std::cell::RefCell::new(None)),
                    });
                }
                other => {
                    eprintln!("warning: unsupported Set element type {} for {}", other, prefix);
                    return post;
                }
            }
        }
        _ => {

            if let Some(er) = enums {
                if let Some((dt, _variants)) = er.by_name.borrow().get(type_name) {
                    let ast = z3::ast::Datatype::new_const(ctx, prefix, &dt.sort);
                    env.insert(env_key.to_string(), Var::EnumVar {
                        ast,
                        enum_name: type_name.to_string(),
                        dt: *dt,
                    });
                    return post;
                }
            }
            if let Some(schema) = schemas.get(type_name) {

                for item in &schema.body {
                    if let BodyItem::Membership { name: field, type_name: ftype, .. } = item {
                        let dotted_env = format!("{}.{}", env_key, field);
                        let dotted_z3  = format!("{}.{}", prefix, field);
                        post.extend(declare_var_named(ctx, env, &dotted_env, &dotted_z3,
                                          ftype, schemas, registry, enums));
                    }
                }
            } else {
                eprintln!("warning: unknown type {} for {}", type_name, prefix);
            }
        }
    }
    post
}

/// Pin each Seq var's length to its source-declared `N`, and return the
/// `<seq>__len = N` equalities so callers can ASSERT them into the solver.
///
/// Two distinct mechanisms, both required:
///   1. The in-memory `len` field is REPLACED with a literal `Int::from_i64(N)`.
///      The `coindexed` unroller reads `len.simplify().as_i64()` and needs a
///      concrete value to unroll the per-element transition — without the
///      literal the ongoing transition is silently dropped.
///   2. The symbolic `<seq>__len` const (made in `declare_var_named`, only
///      asserted `≥ 0`) is left ORPHANED by step 1, and the source's `#xs = N`
///      lowers against the literal to the tautology `(= N N)`. So the EXPORTED
///      SMT-LIB never pins the length — a downstream z3 user pasting it can pick
///      any `cells__len` and a `∀ i` property reads out-of-range cells. We
///      reconstruct that const (Z3 interns by name+sort, so `Int::new_const`
///      with the same `<name>__len` recovers the identical handle) and return
///      `<seq>__len = N`. The caller asserts it, so the export is self-contained:
///      satisfiable only for length-N sequences.
#[must_use]
pub(super) fn apply_seq_lengths<'ctx>(
    env: &mut HashMap<String, Var<'ctx>>,
    seq_lengths: &HashMap<String, i64>,
    ctx: &'ctx Context,
) -> Vec<Bool<'ctx>> {
    // A previous-tick Seq twin (`_xs`, `__xs`) carries the SAME length as its
    // base (`xs`): the trampoline copies the whole `Value::Seq*` across ticks,
    // so `#_xs = #xs`. The length-pin collector only sees the base's `#xs = N`,
    // never an explicit `#_xs = N`, so propagate the base length to every
    // `_`-prefixed twin present in env. Without this, `coindexed(_xs, xs)` can't
    // resolve `_xs`'s length and the ongoing transition is silently dropped.
    let mut effective: HashMap<String, i64> = seq_lengths.clone();
    for (name, n) in seq_lengths {
        for depth in 1..=2 {
            let twin = format!("{}{name}", "_".repeat(depth));
            if env.contains_key(&twin) {
                effective.entry(twin).or_insert(*n);
            }
        }
    }

    let mut len_eqs: Vec<Bool<'ctx>> = Vec::new();
    for (name, n) in &effective {
        let Some(var) = env.get(name) else { continue };
        let new_len = Int::from_i64(ctx, *n);
        let new_var = match var {
            Var::SeqVar { arr, elem, .. } => {
                Var::SeqVar { arr: arr.clone(), len: new_len, elem: *elem }
            }
            Var::DatatypeSeqVar { arr, type_name, dt, fields, .. } => {
                Var::DatatypeSeqVar {
                    arr: arr.clone(),
                    len: new_len,
                    type_name: type_name.clone(),
                    dt: *dt,
                    fields: fields.clone(),
                }
            }
            _ => continue,
        };
        // Pin the symbolic `<name>__len` const so the EXPORTED encoding is
        // self-contained (the literal `len` above is for the unroll only and
        // never lands in the smt2). The env key == the z3 var prefix for every
        // declared Seq leaf, so this reconstructs the const made in declare.
        let len_const = Int::new_const(ctx, format!("{name}__len").as_str());
        len_eqs.push(len_const._eq(&Int::from_i64(ctx, *n)));
        env.insert(name.clone(), new_var);
    }
    len_eqs
}

pub(super) fn apply_set_candidates<'ctx>(
    env: &HashMap<String, Var<'ctx>>,
    given: &HashMap<String, crate::core::Value>,
) {
    use crate::core::Value;
    for (name, value) in given {
        let Some(var) = env.get(name) else { continue };
        if let Var::SetVar { candidates, .. } = var {
            match value {
                Value::SetInt(items) => {
                    *candidates.borrow_mut() =
                        Some(items.iter().map(|n| Value::Int(*n)).collect());
                }
                Value::SetBool(items) => {
                    *candidates.borrow_mut() =
                        Some(items.iter().map(|b| Value::Bool(*b)).collect());
                }
                Value::SetStr(items) => {
                    *candidates.borrow_mut() =
                        Some(items.iter().map(|s| Value::Str(s.clone())).collect());
                }
                _ => {}
            }
        }
    }
}

// ───────────────────────── struct → Z3 Datatype sort builder ─────────────────────────

pub(super) fn get_or_build_datatype(
    type_name: &str,
    ctx: &'static Context,
    schemas: &HashMap<String, SchemaDecl>,
    registry: &DatatypeRegistry,
    enums: Option<&EnumRegistry>,
) -> Option<(&'static DatatypeSort<'static>, Vec<FieldKind>)> {

    if let Some((dt, fields)) = registry.borrow().get(type_name) {
        return Some((*dt, fields.clone()));
    }
    let schema = schemas.get(type_name)?;

    let mut fields: Vec<FieldKind> = Vec::new();
    let mut field_sorts: Vec<(String, Sort<'static>)> = Vec::new();
    for item in &schema.body {
        if let BodyItem::Membership { name, type_name: ftype, .. } = item {
            match ftype.as_str() {
                "Int" | "Nat" | "Pos" => {
                    fields.push(FieldKind::Primitive {
                        name: name.clone(),
                        prim_type: ftype.clone(),
                    });
                    field_sorts.push((name.clone(), Sort::int(ctx)));
                }
                "Bool" => {
                    fields.push(FieldKind::Primitive {
                        name: name.clone(),
                        prim_type: ftype.clone(),
                    });
                    field_sorts.push((name.clone(), Sort::bool(ctx)));
                }
                "String" => {
                    fields.push(FieldKind::Primitive {
                        name: name.clone(),
                        prim_type: ftype.clone(),
                    });
                    field_sorts.push((name.clone(), Sort::string(ctx)));
                }

                s if s.starts_with("Seq(") && s.ends_with(')') => {
                    let inner = &s[4..s.len() - 1];
                    let (elem_sort, seq_elem): (Sort<'static>, SeqFieldElem) = match inner {
                        "Int" | "Nat" | "Pos" =>
                            (Sort::int(ctx), SeqFieldElem::Primitive(SeqElem::Int)),
                        "Bool" =>
                            (Sort::bool(ctx), SeqFieldElem::Primitive(SeqElem::Bool)),
                        "String" =>
                            (Sort::string(ctx), SeqFieldElem::Primitive(SeqElem::Str)),
                        enum_name if enums
                            .map(|er| er.by_name.borrow().contains_key(enum_name))
                            .unwrap_or(false) =>
                        {
                            let er = enums.unwrap();
                            let dts = er.by_name.borrow();
                            let (dt, _variants) = dts.get(enum_name).unwrap();
                            (dt.sort.clone(), SeqFieldElem::Enum {
                                enum_name: enum_name.to_string(),
                                dt: *dt,
                            })
                        }
                        user_type if schemas.contains_key(user_type) => {
                            let Some((nested_dt, sub_fields)) = get_or_build_datatype(
                                user_type, ctx, schemas, registry, enums)
                            else { return None; };
                            (nested_dt.sort.clone(), SeqFieldElem::Composite {
                                type_name: user_type.to_string(),
                                dt: nested_dt,
                                sub_fields,
                            })
                        }
                        _ => {
                            eprintln!(
                                "warning: unsupported Seq element {} in Datatype \
                                 field `{}` for {}; supported: Int/Nat/Pos/Bool/\
                                 String, enums, user structs",
                                inner, name, type_name
                            );
                            return None;
                        }
                    };
                    let arr_idx = fields.len();
                    let arr_sort = Sort::array(ctx, &Sort::int(ctx), &elem_sort);
                    field_sorts.push((format!("{}__arr", name), arr_sort));
                    field_sorts.push((format!("{}__len", name), Sort::int(ctx)));
                    fields.push(FieldKind::SeqField {
                        name: name.clone(),
                        arr_idx,
                        len_idx: arr_idx + 1,
                        elem_type_name: inner.to_string(),
                        elem: seq_elem,
                    });
                }

                user_type if schemas.contains_key(user_type) => {
                    let Some((nested_dt, sub_fields)) =
                        get_or_build_datatype(user_type, ctx, schemas, registry, enums)
                    else {

                        return None;
                    };
                    field_sorts.push((name.clone(), nested_dt.sort.clone()));
                    fields.push(FieldKind::Nested {
                        name: name.clone(),
                        type_name: user_type.to_string(),
                        dt: nested_dt,
                        sub_fields,
                    });
                }
                _ => {
                    eprintln!(
                        "warning: unsupported field type {} in Datatype for {}; \
                         supported: Int/Nat/Pos/Bool/String, Seq(...), enums, \
                         user struct types",
                        ftype, type_name
                    );
                    return None;
                }
            }
        }

    }
    if fields.is_empty() {
        eprintln!("warning: type {} has no fields; can't build Datatype", type_name);
        return None;
    }

    let ctor_name = format!("mk_{}", type_name);
    let field_refs: Vec<(&str, DatatypeAccessor<'static>)> = field_sorts
        .iter()
        .map(|(n, s)| (n.as_str(), DatatypeAccessor::Sort(s.clone())))
        .collect();
    let dt: DatatypeSort<'static> = DatatypeBuilder::new(ctx, type_name)
        .variant(&ctor_name, field_refs)
        .finish();
    let leaked: &'static DatatypeSort<'static> = Box::leak(Box::new(dt));
    registry.borrow_mut().insert(type_name.to_string(), (leaked, fields.clone()));
    Some((leaked, fields))
}

// ===========================================================================
// Pre-translate analysis passes — collect referenced names, pinned ints, and
// seq lengths from the AST; the collected maps feed apply_seq_lengths /
// apply_pinned_ints above.  (was preprocess.rs)
// ===========================================================================

pub fn collect_referenced_names(e: &Expr, out: &mut HashSet<String>) {
    match e {
        Expr::Identifier(n) => { out.insert(n.clone()); }
        Expr::Cardinality(inner) => {

            if let Expr::Identifier(name) = inner.as_ref() {
                out.insert(name.clone());
            }
            collect_referenced_names(inner, out);
        }
        Expr::Binary(_, lhs, rhs) => {
            collect_referenced_names(lhs, out);
            collect_referenced_names(rhs, out);
        }
        Expr::Not(inner) => collect_referenced_names(inner, out),
        Expr::Range(lo, hi) => {
            collect_referenced_names(lo, out);
            collect_referenced_names(hi, out);
        }
        Expr::Index(s, i) => {
            collect_referenced_names(s, out);
            collect_referenced_names(i, out);
        }
        Expr::Field(r, _) => collect_referenced_names(r, out),
        Expr::InExpr(lhs, rhs) => {
            collect_referenced_names(lhs, out);
            collect_referenced_names(rhs, out);
        }
        Expr::SetLit(items) | Expr::SeqLit(items) => {
            for it in items { collect_referenced_names(it, out); }
        }
        Expr::Forall(_, range, body) | Expr::Exists(_, range, body) => {
            collect_referenced_names(range, out);
            collect_referenced_names(body, out);
        }
        Expr::Call(_, args) => {
            for a in args { collect_referenced_names(a, out); }
        }
        _ => {}
    }
}

pub(super) fn collect_pinned_ints(
    body: &[BodyItem],
    given: &HashMap<String, Value>,
    seq_lengths: &HashMap<String, i64>,
) -> HashMap<String, i64> {
    let mut pinned: HashMap<String, i64> = HashMap::new();
    for (k, v) in given {
        if let Value::Int(n) = v { pinned.insert(k.clone(), *n); }
    }
    let mut changed = true;
    while changed {
        changed = false;
        for item in body {
            if let BodyItem::Constraint(Expr::Binary(BinOp::Eq, lhs, rhs)) = item {
                for (a, b) in [(lhs, rhs), (rhs, lhs)] {
                    if let Expr::Identifier(name) = a.as_ref() {
                        if !pinned.contains_key(name) {

                            if let Some(v) = eval_pure_int(b, &pinned, seq_lengths) {
                                pinned.insert(name.clone(), v);
                                changed = true;
                            }
                        }
                    }
                }
            }
        }
    }
    pinned
}

fn eval_pure_int(
    e: &Expr,
    pinned: &HashMap<String, i64>,
    seq_lengths: &HashMap<String, i64>,
) -> Option<i64> {
    match e {
        Expr::Int(n) => Some(*n),
        Expr::Identifier(name) => pinned.get(name).copied(),
        Expr::Cardinality(inner) => match inner.as_ref() {
            Expr::Identifier(name) => seq_lengths.get(name).copied(),
            _ => None,
        },
        Expr::Binary(op, lhs, rhs) => {
            let l = eval_pure_int(lhs, pinned, seq_lengths)?;
            let r = eval_pure_int(rhs, pinned, seq_lengths)?;
            Some(match op {
                BinOp::Add => l.checked_add(r)?,
                BinOp::Sub => l.checked_sub(r)?,
                BinOp::Mul => l.checked_mul(r)?,
                // Match Z3/SMT-LIB integer `div` (remainder always non-negative)
                // via Rust's div_euclid — NOT `/` (truncation toward zero). Using
                // `/` here disagreed with the solver on negative inexact division
                // (e.g. -5/3: `/` gives -1, Z3 gives -2), so when both the folder
                // and the solver constrained one value the result was UNSAT.
                BinOp::Div => if r == 0 { return None } else { l.checked_div_euclid(r)? },
                _ => return None,
            })
        }
        _ => None,
    }
}

pub(super) fn collect_seq_lengths_with_schemas(
    body: &[BodyItem],
    given: &HashMap<String, Value>,
    schemas: Option<&HashMap<String, SchemaDecl>>,
) -> HashMap<String, i64> {
    let mut out = HashMap::new();

    for (k, v) in given {
        let len = match v {
            Value::SeqInt(v)       => v.len() as i64,
            Value::SeqBool(v)      => v.len() as i64,
            Value::SeqStr(v)       => v.len() as i64,
            Value::SeqComposite(v) => v.len() as i64,
            Value::SeqEnum(v)      => v.len() as i64,
            Value::SetInt(v)       => v.len() as i64,
            Value::SetBool(v)      => v.len() as i64,
            Value::SetStr(v)       => v.len() as i64,
            _ => continue,
        };
        out.insert(k.clone(), len);
    }

    let mut pinned: HashMap<String, i64> = HashMap::new();
    for (k, v) in given {
        if let Value::Int(n) = v { pinned.insert(k.clone(), *n); }
    }

    let mut changed = true;
    while changed {
        changed = false;
        walk_constraints(body, schemas, &pinned, &mut out, &mut changed);

        scan_int_pins(body, schemas, &mut pinned, &out, &mut changed);
    }
    out
}

fn scan_int_pins(
    body: &[BodyItem],
    schemas: Option<&HashMap<String, SchemaDecl>>,
    pinned: &mut HashMap<String, i64>,
    seq_lens: &HashMap<String, i64>,
    changed: &mut bool,
) {
    for item in body {
        match item {
            BodyItem::Constraint(Expr::Binary(BinOp::Eq, lhs, rhs)) => {
                for (a, b) in [(lhs, rhs), (rhs, lhs)] {
                    if let Expr::Identifier(name) = a.as_ref() {
                        if !pinned.contains_key(name) {
                            if let Some(v) = eval_pure_int(b, pinned, seq_lens) {
                                pinned.insert(name.clone(), v);
                                *changed = true;
                            }
                        }
                    }
                }
            }
            BodyItem::Passthrough(claim_name) => {
                if let Some(schemas) = schemas {
                    if let Some(claim) = schemas.get(claim_name) {
                        scan_int_pins(&claim.body, Some(schemas), pinned, seq_lens, changed);
                    }
                }
            }
            _ => {}
        }
    }
}

fn walk_constraints(
    body: &[BodyItem],
    schemas: Option<&HashMap<String, SchemaDecl>>,
    no_pinned: &HashMap<String, i64>,
    out: &mut HashMap<String, i64>,
    changed: &mut bool,
) {
    for item in body {
        match item {
            BodyItem::Constraint(Expr::Binary(BinOp::Eq, lhs, rhs)) => {
                for (a, b) in [(lhs, rhs), (rhs, lhs)] {

                    if let Expr::Cardinality(inner) = a.as_ref() {
                        if let Expr::Identifier(name) = inner.as_ref() {
                            if !out.contains_key(name) {
                                if let Some(v) = eval_pure_int(b, no_pinned, out) {
                                    out.insert(name.clone(), v);
                                    *changed = true;
                                }
                            }
                        }
                    }

                    if let (Expr::Identifier(name), Expr::SeqLit(items)) =
                        (a.as_ref(), b.as_ref())
                    {
                        if !out.contains_key(name) {
                            out.insert(name.clone(), items.len() as i64);
                            *changed = true;
                        }
                    }
                }
            }
            BodyItem::Passthrough(claim_name) => {
                if let Some(schemas) = schemas {
                    if let Some(claim) = schemas.get(claim_name) {
                        walk_constraints(&claim.body, Some(schemas), no_pinned, out, changed);
                    }
                }
            }

            BodyItem::Membership { name: inst_name, type_name, .. } => {
                if let Some(schemas) = schemas {
                    if let Some(ty) = schemas.get(type_name) {
                        let field_set: std::collections::HashSet<String> = ty.body.iter()
                            .filter_map(|it| match it {
                                BodyItem::Membership { name, .. } => Some(name.clone()),
                                _ => None,
                            })
                            .collect();
                        walk_constraints_with_prefix(
                            &ty.body, Some(schemas), no_pinned, out, changed,
                            inst_name, &field_set);
                    }
                }
            }
            _ => {}
        }
    }
}

fn walk_constraints_with_prefix(
    body: &[BodyItem],
    schemas: Option<&HashMap<String, SchemaDecl>>,
    no_pinned: &HashMap<String, i64>,
    out: &mut HashMap<String, i64>,
    changed: &mut bool,
    prefix: &str,
    field_set: &std::collections::HashSet<String>,
) {
    for item in body {
        if let BodyItem::Constraint(Expr::Binary(BinOp::Eq, lhs, rhs)) = item {
            for (a, b) in [(lhs, rhs), (rhs, lhs)] {

                if let Expr::Cardinality(inner) = a.as_ref() {
                    if let Expr::Identifier(name) = inner.as_ref() {
                        let first_seg = name.split('.').next().unwrap_or("");
                        if field_set.contains(first_seg) {
                            let dotted = format!("{}.{}", prefix, name);
                            if !out.contains_key(&dotted) {
                                if let Some(v) = eval_pure_int(b, no_pinned, out) {
                                    out.insert(dotted, v);
                                    *changed = true;
                                }
                            }
                        }
                    }
                }

                if let (Expr::Identifier(name), Expr::SeqLit(items)) =
                    (a.as_ref(), b.as_ref())
                {
                    let first_seg = name.split('.').next().unwrap_or("");
                    if field_set.contains(first_seg) {
                        let dotted = format!("{}.{}", prefix, name);
                        if !out.contains_key(&dotted) {
                            out.insert(dotted, items.len() as i64);
                            *changed = true;
                        }
                    }
                }
            }
        }
    }

    let _ = schemas;
}

pub(super) fn apply_pinned_ints<'ctx>(
    env: &mut HashMap<String, Var<'ctx>>,
    pinned: &HashMap<String, i64>,
) {
    for (name, value) in pinned {
        if env.contains_key(name) {
            env.insert(name.clone(), Var::PinnedInt(*value));
        }
    }
}
