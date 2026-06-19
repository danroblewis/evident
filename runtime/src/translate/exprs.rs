use std::collections::HashMap;
use z3::ast::{Ast, Bool, Int, Real, String as Z3Str};
use z3::{Context, DatatypeSort};

use crate::core::ast::*;
use crate::core::{EnumRegistry, FieldKind, SeqElem, Value, Var};

thread_local! {

    static ACTIVE_ENUMS: std::cell::Cell<Option<*const EnumRegistry>> =
        const { std::cell::Cell::new(None) };
}

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

fn with_active_enums<R>(f: impl FnOnce(Option<&EnumRegistry>) -> R) -> R {
    let ptr = ACTIVE_ENUMS.with(|c| c.get());

    let opt = ptr.map(|p| unsafe { &*p });
    f(opt)
}

thread_local! {

    static TARGET_ENUM_HINT: std::cell::RefCell<Option<(String, &'static DatatypeSort<'static>)>> =
        const { std::cell::RefCell::new(None) };
}

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

pub(super) fn resolve_mapping<'ctx>(
    slot: &str,
    value: &Expr,
    ctx: &'ctx Context,
    env: &HashMap<String, Var<'ctx>>,
    schemas: &HashMap<String, SchemaDecl>,
) -> Vec<(String, Var<'ctx>)> {
    if let Expr::Identifier(name) = value {

        if env.contains_key(name) {
            return vec![(slot.to_string(), env[name].clone())];
        }

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

    if let Expr::Index(seq_expr, idx_expr) = value {
        if let Expr::Identifier(seq_name) = seq_expr.as_ref() {
            if let Some(var) = env.get(seq_name) {
                if let Some((arr, _, _, dt, fields)) = var.as_datatype_seq() {
                    if let Some(i) = translate_int(idx_expr, ctx, env) {
                        let elem_dyn = arr.select(&i);

                        let mut tmp: HashMap<String, Var<'ctx>> = HashMap::new();
                        if bind_composite_fields(&mut tmp, &elem_dyn, fields, dt, slot) {
                            return tmp.into_iter().collect();
                        }
                    }
                }
            }
        }
    }

    if matches!(value, Expr::Field(_, _)) {
        let mut path: Vec<String> = Vec::new();
        let mut cur = value;
        let (seq_name, idx_expr) = loop {
            match cur {
                Expr::Field(recv, fname) => {
                    path.push(fname.clone());
                    cur = recv.as_ref();
                }
                Expr::Index(seq, idx) => {
                    if let Expr::Identifier(name) = seq.as_ref() {
                        break (name.clone(), idx.clone());
                    }
                    return Vec::new();
                }
                _ => return Vec::new(),
            }
        };
        path.reverse();
        let Some(out) = resolve_field_chain_to_bindings(
            &seq_name, &idx_expr, &path, slot, ctx, env) else {
            return Vec::new();
        };
        return out;
    }
    if let Some(v) = expr_as_var(value, ctx, env) {
        return vec![(slot.to_string(), v)];
    }
    Vec::new()
}

fn resolve_field_chain_to_bindings<'ctx>(
    seq_name: &str,
    idx_expr: &Expr,
    path: &[String],
    slot: &str,
    ctx: &'ctx Context,
    env: &HashMap<String, Var<'ctx>>,
) -> Option<Vec<(String, Var<'ctx>)>> {
    use crate::core::SeqFieldElem;
    let var = env.get(seq_name)?;
    let (arr, _, _, root_dt, root_fields) = var.as_datatype_seq()?;
    let i = translate_int(idx_expr, ctx, env)?;
    let elem_dyn = arr.select(&i);

    let mut cur_dyn = elem_dyn;
    let mut cur_dt: &DatatypeSort = root_dt;
    let mut cur_fields: &[FieldKind] = root_fields;
    for (depth, fname) in path.iter().enumerate() {
        let pos = cur_fields.iter().position(|fk| fk.name() == fname)?;
        let fk = &cur_fields[pos];
        let is_last = depth == path.len() - 1;
        match fk {
            FieldKind::Primitive { prim_type, .. } => {
                if !is_last { return None; }
                if pos >= cur_dt.variants[0].accessors.len() { return None; }
                let raw = cur_dt.variants[0].accessors[pos].apply(
                    &[&cur_dyn.as_datatype()?]);
                let var: Option<Var<'ctx>> = match prim_type.as_str() {
                    "Int" | "Nat" | "Pos" => raw.as_int().map(Var::IntVar),
                    "Bool" => raw.as_bool().map(Var::BoolVar),
                    "String" => raw.as_string().map(Var::StrVar),
                    _ => None,
                };
                return Some(vec![(slot.to_string(), var?)]);
            }
            FieldKind::Nested { dt: nested_dt, sub_fields, .. } => {
                if pos >= cur_dt.variants[0].accessors.len() { return None; }
                let raw = cur_dt.variants[0].accessors[pos].apply(
                    &[&cur_dyn.as_datatype()?]);
                if is_last {

                    let mut tmp: HashMap<String, Var<'ctx>> = HashMap::new();
                    if !bind_composite_fields(&mut tmp, &raw, sub_fields, nested_dt, slot) {
                        return None;
                    }
                    return Some(tmp.into_iter().collect());
                }
                cur_dyn = raw;
                cur_dt = nested_dt;
                cur_fields = sub_fields;
            }
            FieldKind::SeqField { arr_idx, len_idx, elem: seq_elem, .. } => {
                if !is_last { return None; }
                if *len_idx >= cur_dt.variants[0].accessors.len() { return None; }
                let elem_d = cur_dyn.as_datatype()?;
                let arr_d = cur_dt.variants[0].accessors[*arr_idx].apply(&[&elem_d]);
                let len_d = cur_dt.variants[0].accessors[*len_idx].apply(&[&elem_d]);
                let inner_arr = arr_d.as_array()?;
                let inner_len = len_d.as_int()?;
                let var = match seq_elem {
                    SeqFieldElem::Primitive(e) => Var::SeqVar {
                        arr: inner_arr, len: inner_len, elem: *e,
                    },
                    SeqFieldElem::Enum { dt, enum_name } => Var::DatatypeSeqVar {
                        arr: inner_arr, len: inner_len,
                        type_name: enum_name.clone(),
                        dt: *dt, fields: Vec::new(),
                    },
                    SeqFieldElem::Composite { dt, type_name, sub_fields } => Var::DatatypeSeqVar {
                        arr: inner_arr, len: inner_len,
                        type_name: type_name.clone(),
                        dt: *dt, fields: sub_fields.clone(),
                    },
                };
                return Some(vec![(slot.to_string(), var)]);
            }
        }
    }
    None
}

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

            let mut owned_args: Vec<Box<dyn z3::ast::Ast<'ctx>>> = Vec::new();
            for (arg_expr, field_type) in args.iter().zip(field_types.iter()) {
                if let Some(inner) = crate::core::parse_seq_type(field_type) {

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

                        Box::new(resolve_enum_ast(arg_expr, ctx, env, schemas)?)
                    }
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

        Expr::SeqLit(items) => {
            let (enum_name, dt) = current_target_enum()?;
            build_cons_chain(items, &enum_name, dt, ctx, env, schemas)
        }
        _ => None,
    }
}

fn translate_seq_arg_for_ctor<'ctx>(
    arg_expr: &Expr,
    inner_type: &str,
    ctx: &'ctx Context,
    env: &HashMap<String, Var<'ctx>>,
    schemas: &HashMap<String, SchemaDecl>,
) -> Option<(Box<dyn z3::ast::Ast<'ctx> + 'ctx>, Box<dyn z3::ast::Ast<'ctx> + 'ctx>)> {
    use z3::Sort;
    use z3::ast::{Array, Bool, Int, String as Z3Str};

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
                Var::DatatypeSeqVar { .. } => {
                    Some(z3::ast::Datatype::fresh_const(
                        ctx, "__cons_view", &dt.sort))
                }
                _ => None,
            }
        }

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

pub(super) enum SeqHandleRef<'ctx> {
    Primitive {
        arr: z3::ast::Array<'ctx>,
        len: Int<'ctx>,
        elem: SeqElem,
    },
    Composite {
        arr: z3::ast::Array<'ctx>,
        len: Int<'ctx>,
        #[allow(dead_code)]
        type_name: String,
        dt: &'static DatatypeSort<'static>,
        fields: Vec<FieldKind>,
    },
}

impl<'ctx> SeqHandleRef<'ctx> {
    pub(super) fn len(&self) -> &Int<'ctx> {
        match self {
            SeqHandleRef::Primitive { len, .. } => len,
            SeqHandleRef::Composite { len, .. } => len,
        }
    }
}

pub(super) fn resolve_seq_handle<'ctx>(
    expr: &Expr,
    ctx: &'ctx Context,
    env: &HashMap<String, Var<'ctx>>,
) -> Option<SeqHandleRef<'ctx>> {
    use crate::core::SeqFieldElem;

    if let Expr::Identifier(name) = expr {
        if let Some(var) = env.get(name) {
            if let Some((arr, len, elem)) = var.as_seq() {
                return Some(SeqHandleRef::Primitive {
                    arr: arr.clone(), len: len.clone(), elem,
                });
            }
            if let Some((arr, len, type_name, dt, fields)) = var.as_datatype_seq() {
                return Some(SeqHandleRef::Composite {
                    arr: arr.clone(), len: len.clone(),
                    type_name: type_name.to_string(),
                    dt, fields: fields.to_vec(),
                });
            }
        }
        return None;
    }

    let Expr::Field(receiver, field_name) = expr else { return None };
    let Expr::Index(seq_expr, idx_expr) = receiver.as_ref() else { return None };
    let Expr::Identifier(outer_name) = seq_expr.as_ref() else { return None };
    let var = env.get(outer_name)?;
    let (arr, _, _, dt, fields) = var.as_datatype_seq()?;
    let i = translate_int(idx_expr, ctx, env)?;
    let elem_dyn = arr.select(&i);
    let elem = elem_dyn.as_datatype()?;

    let fk = fields.iter().find(|f| f.name() == field_name)?;
    let FieldKind::SeqField { arr_idx, len_idx, elem: seq_elem, .. } = fk else {
        return None;
    };
    if *len_idx >= dt.variants[0].accessors.len() { return None; }
    let inner_arr_dyn = dt.variants[0].accessors[*arr_idx].apply(&[&elem]);
    let inner_len_dyn = dt.variants[0].accessors[*len_idx].apply(&[&elem]);
    let inner_arr = inner_arr_dyn.as_array()?;
    let inner_len = inner_len_dyn.as_int()?;
    match seq_elem {
        SeqFieldElem::Primitive(e) => Some(SeqHandleRef::Primitive {
            arr: inner_arr, len: inner_len, elem: *e,
        }),
        SeqFieldElem::Enum { dt, enum_name } => Some(SeqHandleRef::Composite {
            arr: inner_arr, len: inner_len,
            type_name: enum_name.clone(),
            dt: *dt, fields: Vec::new(),
        }),
        SeqFieldElem::Composite { dt, type_name, sub_fields } => Some(SeqHandleRef::Composite {
            arr: inner_arr, len: inner_len,
            type_name: type_name.clone(),
            dt: *dt, fields: sub_fields.clone(),
        }),
    }
}

fn resolve_seq_field<'ctx>(
    field_expr: &Expr,
    ctx: &'ctx Context,
    env: &HashMap<String, Var<'ctx>>,
) -> Option<(z3::ast::Dynamic<'ctx>, String)> {

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

    path.reverse();
    if path.is_empty() { return None; }

    let var = env.get(seq_name)?;
    let (arr, _, _, root_dt, root_fields) = var.as_datatype_seq()?;
    let i = translate_int(idx_expr, ctx, env)?;
    let elem_dyn = arr.select(&i);
    let mut cur_dyn = elem_dyn;

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

                    return None;
                }
                return Some((raw, prim_type.clone()));
            }
            FieldKind::Nested { dt: nested_dt, sub_fields, .. } => {
                if is_last {

                    return None;
                }
                cur_dt = nested_dt;
                cur_fields = sub_fields.as_slice();
                cur_dyn = raw;
            }
            FieldKind::SeqField { .. } => {

                return None;
            }
        }
    }
    None
}

pub(super) fn translate_str<'ctx>(e: &Expr, ctx: &'ctx Context, env: &HashMap<String, Var<'ctx>>) -> Option<Z3Str<'ctx>> {
    match e {
        Expr::Str(s) => Z3Str::from_str(ctx, s).ok(),
        Expr::Identifier(name) => env.get(name).and_then(|v| v.as_str().cloned()),

        Expr::Binary(BinOp::Concat, lhs, rhs) => {
            let l = translate_str(lhs, ctx, env)?;
            let r = translate_str(rhs, ctx, env)?;
            Some(Z3Str::concat(ctx, &[&l, &r]))
        }

        Expr::Index(seq_expr, idx_expr) => {
            let handle = resolve_seq_handle(seq_expr.as_ref(), ctx, env)?;
            let SeqHandleRef::Primitive { arr, elem, .. } = handle else { return None };
            if elem != SeqElem::Str { return None; }
            let i = translate_int(idx_expr, ctx, env)?;
            arr.select(&i).as_string()
        }

        Expr::Field(_, _) => {
            let (raw, ftype) = resolve_seq_field(e, ctx, env)?;
            if ftype == "String" {
                raw.as_string()
            } else {
                None
            }
        }

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

                let inner = x.le(&hi).ite(&x, &hi);
                return Some(inner.ge(&lo).ite(&inner, &lo));
            }

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

        Expr::Cardinality(inner) => {
            if let Some(handle) = resolve_seq_handle(inner.as_ref(), ctx, env) {
                return Some(handle.len().clone());
            }
            if let Expr::Identifier(name) = inner.as_ref() {
                if let Some(var) = env.get(name) {
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

        Expr::Index(seq_expr, idx_expr) => {
            let handle = resolve_seq_handle(seq_expr.as_ref(), ctx, env)?;
            let SeqHandleRef::Primitive { arr, elem, .. } = handle else { return None };
            if elem != SeqElem::Int { return None; }
            let i = translate_int(idx_expr, ctx, env)?;
            arr.select(&i).as_int()
        }

        Expr::Field(_, _) => {
            let (raw, ftype) = resolve_seq_field(e, ctx, env)?;
            if matches!(ftype.as_str(), "Int" | "Nat" | "Pos") {
                raw.as_int()
            } else {
                None
            }
        }

        Expr::Ternary(c, a, b) => {
            let cond = translate_bool(c, ctx, env, &HashMap::new())?;
            let then_v = translate_int(a, ctx, env)?;
            let else_v = translate_int(b, ctx, env)?;
            Some(cond.ite(&then_v, &else_v))
        }

        Expr::Match(scr, arms) => {
            let compiled = translate_match_arms(scr, arms, ctx, env,
                |body, e| translate_int(body, ctx, e))?;
            fold_arms_to_ite(compiled)
        }
        _ => None,
    }
}

pub(super) fn translate_real<'ctx>(e: &Expr, ctx: &'ctx Context, env: &HashMap<String, Var<'ctx>>) -> Option<Real<'ctx>> {
    match e {
        Expr::Real(f) => Some(real_from_f64(ctx, *f)),
        Expr::Int(n)  => Some(Real::from_int(&Int::from_i64(ctx, *n))),
        Expr::Identifier(name) => match env.get(name) {
            Some(Var::RealVar(r)) => Some(r.clone()),
            Some(Var::IntVar(i))  => Some(Real::from_int(i)),
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

    let mut lhs_records = Vec::new();
    let mut rhs_records = Vec::new();
    collect_record_refs(lhs, env, schemas, &mut lhs_records);
    collect_record_refs(rhs, env, schemas, &mut rhs_records);
    if lhs_records.is_empty() || rhs_records.is_empty() { return None; }
    let mut all_records = lhs_records;
    all_records.extend(rhs_records);

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

        BinOp::Neq => Bool::or(ctx, &refs),

        _ => Bool::and(ctx, &refs),
    })
}

fn lhs_record_leaves<'ctx>(
    lhs: &Expr,
    env: &HashMap<String, Var<'ctx>>,
    schemas: &HashMap<String, SchemaDecl>,
) -> Option<Vec<String>> {
    match lhs {

        Expr::Call(type_name, _args) => {
            let schema = schemas.get(type_name)?;
            let mut leaves = schema_leaf_paths(schema, schemas);
            if leaves.is_empty() { return None; }
            leaves.sort();
            Some(leaves)
        }
        Expr::Identifier(name) => {
            if env.contains_key(name) { return None; }
            let prefix = format!("{}.", name);
            let mut leaves: Vec<String> = env.keys()
                .filter_map(|k| k.strip_prefix(&prefix).map(String::from))
                .collect();
            if leaves.is_empty() { return None; }
            leaves.sort();
            Some(leaves)
        }
        Expr::Field(receiver, field) => {

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
            FieldKind::SeqField { name, .. } => {

                out.push(name.clone());
            }
        }
    }
    out
}

fn substitute_record_refs<'ctx>(
    expr: &Expr,
    leaf: &str,
    env: &HashMap<String, Var<'ctx>>,
    schemas: &HashMap<String, SchemaDecl>,
) -> Option<Expr> {
    match expr {

        Expr::Call(type_name, args) => {
            let schema = schemas.get(type_name)?;

            let fields: Vec<(&str, &str)> = schema.body.iter()
                .filter_map(|item| match item {
                    BodyItem::Membership { name, type_name, .. } =>
                        Some((name.as_str(), type_name.as_str())),
                    _ => None,
                })
                .collect();

            let (first, rest) = match leaf.split_once('.') {
                Some((a, b)) => (a, Some(b)),
                None => (leaf, None),
            };
            let pos = fields.iter().position(|(n, _)| *n == first)?;
            if pos >= args.len() { return None; }

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

                Some(rest_path) => substitute_record_refs(arg_ref, rest_path, env, schemas),
            }
        }
        Expr::Identifier(name) => {
            if env.contains_key(name) {

                return Some(expr.clone());
            }
            let prefix = format!("{}.", name);
            if env.keys().any(|k| k.starts_with(&prefix)) {

                let mut extended = name.clone();
                for p in leaf.split('.') {
                    extended.push('.');
                    extended.push_str(p);
                }
                if env.contains_key(&extended) { Some(Expr::Identifier(extended)) }
                else { None }
            } else {

                Some(expr.clone())
            }
        }
        Expr::Field(receiver, field) => {

            if is_field_of_index_record(receiver, field, env) {
                let mut result = expr.clone();
                for p in leaf.split('.') {
                    result = Expr::Field(Box::new(result), p.to_string());
                }
                return Some(result);
            }

            Some(expr.clone())
        }
        Expr::Index(receiver, _) => {

            if is_seq_element_record(receiver, env) {
                let mut result = expr.clone();
                for p in leaf.split('.') {
                    result = Expr::Field(Box::new(result), p.to_string());
                }
                return Some(result);
            }

            Some(expr.clone())
        }
        Expr::Binary(op, a, b) => {
            let a2 = substitute_record_refs(a, leaf, env, schemas)?;
            let b2 = substitute_record_refs(b, leaf, env, schemas)?;
            Some(Expr::Binary(op.clone(), Box::new(a2), Box::new(b2)))
        }
        Expr::Not(x) => substitute_record_refs(x, leaf, env, schemas).map(|y| Expr::Not(Box::new(y))),

        _ => Some(expr.clone()),
    }
}

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

fn is_seq_element_record<'ctx>(
    receiver: &Expr,
    env: &HashMap<String, Var<'ctx>>,
) -> bool {
    let Expr::Identifier(seq_name) = receiver else { return false };
    matches!(env.get(seq_name), Some(Var::DatatypeSeqVar { .. }))
}

fn collect_record_refs<'ctx>(
    expr: &Expr,
    env: &HashMap<String, Var<'ctx>>,
    schemas: &HashMap<String, SchemaDecl>,
    out: &mut Vec<Expr>,
) {
    match expr {

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

            let owned_name = name.to_string();
            let compiled = translate_match_arms(scr, arms, ctx, env, |body, e| {
                translate_seq_rhs_eq(&owned_name, body, ctx, e, schemas)
            })?;

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

fn translate_seq_lit_for_var<'ctx>(
    name: &str,
    items: &[Expr],
    ctx: &'ctx Context,
    env: &HashMap<String, Var<'ctx>>,
    schemas: &HashMap<String, SchemaDecl>,
) -> Option<Bool<'ctx>> {
    let var = env.get(name)?;

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

    if let Some((arr, len, _, dt, fields)) = var.as_datatype_seq() {
        if fields.is_empty() {
            let enum_name = match var {
                Var::DatatypeSeqVar { type_name, .. } => type_name.clone(),
                _ => unreachable!(),
            };
            let mut clauses: Vec<Bool<'ctx>> = Vec::with_capacity(items.len() + 1);
            clauses.push(len._eq(&Int::from_i64(ctx, items.len() as i64)));

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
            FieldKind::SeqField { .. } => {

                return None;
            }
        };
        field_dyns.push(dynamic);
    }
    let dyn_refs: Vec<&dyn Ast> = field_dyns.iter().map(|d| d as &dyn Ast).collect();
    Some(dt.variants[0].constructor.apply(&dyn_refs))
}

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

    let first_field = fields.first().map(|f| f.name())?;
    if !env.contains_key(&format!("{}.{}", comp_name, first_field)) {
        return None;
    }
    let idx = translate_int(idx_expr, ctx, env)?;
    let composite = build_composite_dynamic(comp_name, dt, fields, ctx, env)?;
    let elem = arr.select(&idx);
    Some(elem._eq(&composite))
}

fn bind_composite_fields<'ctx>(
    env: &mut HashMap<String, Var<'ctx>>,
    elem_dyn: &z3::ast::Dynamic<'ctx>,
    fields: &[FieldKind],
    dt: &DatatypeSort<'ctx>,
    prefix: &str,
) -> bool {
    use crate::core::SeqFieldElem;
    let Some(elem) = elem_dyn.as_datatype() else { return false };

    let mut acc_pos: usize = 0;
    for fk in fields.iter() {
        match fk {
            FieldKind::Primitive { name, prim_type } => {
                if acc_pos >= dt.variants[0].accessors.len() { return false; }
                let raw = dt.variants[0].accessors[acc_pos].apply(&[&elem]);
                acc_pos += 1;
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
                if acc_pos >= dt.variants[0].accessors.len() { return false; }
                let raw = dt.variants[0].accessors[acc_pos].apply(&[&elem]);
                acc_pos += 1;
                let sub_prefix = format!("{}.{}", prefix, name);
                if !bind_composite_fields(env, &raw, sub_fields, nested_dt, &sub_prefix) {
                    return false;
                }
            }
            FieldKind::SeqField { name, arr_idx, len_idx, elem: seq_elem, .. } => {
                if *len_idx >= dt.variants[0].accessors.len() { return false; }
                let arr_dyn = dt.variants[0].accessors[*arr_idx].apply(&[&elem]);
                let len_dyn = dt.variants[0].accessors[*len_idx].apply(&[&elem]);
                acc_pos = *len_idx + 1;
                let arr = arr_dyn.as_array();
                let len = len_dyn.as_int();
                let (Some(arr), Some(len)) = (arr, len) else { return false; };
                let key = format!("{}.{}", prefix, name);
                match seq_elem {
                    SeqFieldElem::Primitive(elem) => {
                        env.insert(key, Var::SeqVar { arr, len, elem: *elem });
                    }
                    SeqFieldElem::Enum { dt, enum_name } => {
                        env.insert(key, Var::DatatypeSeqVar {
                            arr, len, type_name: enum_name.clone(),
                            dt: *dt, fields: Vec::new(),
                        });
                    }
                    SeqFieldElem::Composite { dt, type_name, sub_fields } => {
                        env.insert(key, Var::DatatypeSeqVar {
                            arr, len, type_name: type_name.clone(),
                            dt: *dt, fields: sub_fields.clone(),
                        });
                    }
                }
            }
        }
    }
    true
}

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

pub(super) fn translate_bool<'ctx>(
    e: &Expr,
    ctx: &'ctx Context,
    env: &HashMap<String, Var<'ctx>>,
    schemas: &HashMap<String, SchemaDecl>,
) -> Option<Bool<'ctx>> {

    if let Expr::Call(name, args) = e {
        if name == "contains" && args.len() == 2 {
            let Expr::Identifier(seq_name) = &args[0] else { return None };
            let var = env.get(seq_name)?;

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

            if let Some((arr, len, _, _, _)) = var.as_datatype_seq() {
                let n = len.simplify().as_i64()?;

                let mut clauses: Vec<Bool> = Vec::with_capacity(n as usize);
                for i in 0..n {
                    let idx = Int::from_i64(ctx, i);
                    let cell = arr.select(&idx);

                    let arg = args[1].clone();
                    let eq_expr = Expr::Binary(
                        crate::core::ast::BinOp::Eq,
                        Box::new(Expr::Index(
                            Box::new(args[0].clone()),
                            Box::new(Expr::Int(i)),
                        )),
                        Box::new(arg),
                    );
                    if let Some(b) = translate_bool(&eq_expr, ctx, env, schemas) {
                        clauses.push(b);
                    } else {
                        let _ = cell;
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

            if args.is_empty() { return Some(Bool::from_bool(ctx, true)); }

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

        Expr::Matches(e, pattern) => {
            use crate::core::ast::MatchPattern;
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

        Expr::Index(seq_expr, idx_expr) => {
            let handle = resolve_seq_handle(seq_expr.as_ref(), ctx, env)?;
            let SeqHandleRef::Primitive { arr, elem, .. } = handle else { return None };
            if elem != SeqElem::Bool { return None; }
            let i = translate_int(idx_expr, ctx, env)?;
            arr.select(&i).as_bool()
        }

        Expr::Field(_, _) => {
            let (raw, ftype) = resolve_seq_field(e, ctx, env)?;
            if ftype == "Bool" { raw.as_bool() } else { None }
        }

        Expr::InExpr(lhs, rhs) => {

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

                if let Some((set, _, dt, fields, _)) =
                    env.get(name).and_then(|v| v.as_datatype_set())
                {
                    if let Expr::Identifier(ident) = lhs.as_ref() {
                        let dyn_val = build_composite_dynamic(ident, dt, fields, ctx, env)?;
                        return Some(set.member(&dyn_val));
                    }
                }
            }

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

        Expr::Forall(vars, range, body) | Expr::Exists(vars, range, body) => {
            let mut clauses: Vec<Bool> = Vec::new();

            if let Expr::Call(name, args) = range.as_ref() {
                match (name.as_str(), args.len()) {
                    ("coindexed", n_seqs) if n_seqs >= 1 => {
                        if vars.len() != n_seqs {
                            return None;

                        }

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
                            let mut env2 = env.clone();
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
                            let mut env2 = env.clone();
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
                    _ => return None,
                }
            }

            if vars.len() != 1 { return None; }
            let var = &vars[0];

            if let Some((lo, hi)) = literal_range(range, ctx, env) {
                for i in lo..=hi {
                    let mut env2 = env.clone();
                    env2.insert(var.clone(), Var::IntVar(Int::from_i64(ctx, i)));
                    if let Some(b) = translate_bool(body, ctx, &env2, schemas) {
                        clauses.push(b);
                    }
                }

            } else if let Some(handle) = (!matches!(range.as_ref(), Expr::Identifier(_)))
                .then(|| resolve_seq_handle(range.as_ref(), ctx, env))
                .flatten()
            {

                let n = handle.len().simplify().as_i64()?;
                match &handle {
                    SeqHandleRef::Composite { arr, dt, fields, .. } => {
                        for i in 0..n {
                            let mut env2 = env.clone();
                            let idx = Int::from_i64(ctx, i);
                            let elem_dyn = arr.select(&idx);
                            if !bind_composite_fields(&mut env2, &elem_dyn, fields, dt, var) {
                                return None;
                            }
                            if let Some(b) = translate_bool(body, ctx, &env2, schemas) {
                                clauses.push(b);
                            }
                        }
                    }
                    SeqHandleRef::Primitive { arr, elem, .. } => {
                        for i in 0..n {
                            let mut env2 = env.clone();
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
                    }
                }
            } else if let Expr::Identifier(seq_name) = range.as_ref() {
                let seq_var = env.get(seq_name)?;
                if let Some((arr, len, _, dt, fields)) = seq_var.as_datatype_seq() {

                    let n = len.simplify().as_i64()?;
                    for i in 0..n {
                        let mut env2 = env.clone();
                        let idx = Int::from_i64(ctx, i);
                        let elem_dyn = arr.select(&idx);
                        if !bind_composite_fields(&mut env2, &elem_dyn, fields, dt, var) {
                            return None;
                        }
                        if let Some(b) = translate_bool(body, ctx, &env2, schemas) {
                            clauses.push(b);
                        }
                    }
                } else if let Some((arr, len, elem)) = seq_var.as_seq() {

                    let n = len.simplify().as_i64()?;
                    for i in 0..n {
                        let mut env2 = env.clone();
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

                    if let Some(other_set) = match_set_subset_body(body, var, env) {
                        let b = set.set_subset(other_set);
                        return Some(if matches!(e, Expr::Forall(..)) {
                            b
                        } else {
                            b.not().not()

                        });
                    }
                    return None;
                } else if let Some((set, _, _, _, _)) = seq_var.as_datatype_set() {

                    if let Some(other_set) = match_set_subset_body(body, var, env) {
                        let b = set.set_subset(other_set);
                        return Some(if matches!(e, Expr::Forall(..)) { b } else { b });
                    }
                    return None;
                } else {

                    return None;
                }
            } else {

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

            BinOp::Eq | BinOp::Neq => {

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

                if let Some(b) = translate_seq_eq(lhs, rhs, ctx, env) {
                    return Some(match op {
                        BinOp::Eq  => b,
                        BinOp::Neq => b.not(),
                        _ => unreachable!(),
                    });
                }

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

                lift_record_op(op, lhs, rhs, ctx, env, schemas)
            }

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

                lift_record_op(op, lhs, rhs, ctx, env, schemas)
            }
            _ => None,
        }
        _ => None,
    }
}

type CompiledArm<'ctx, T> = (Option<Bool<'ctx>>, T);

fn translate_match_arms<'ctx, T>(
    scr: &Expr,
    arms: &[crate::core::ast::MatchArm],
    ctx: &'ctx Context,
    env: &HashMap<String, Var<'ctx>>,
    body_translator: impl Fn(&Expr, &HashMap<String, Var<'ctx>>) -> Option<T>,
) -> Option<Vec<CompiledArm<'ctx, T>>> {
    use crate::core::ast::MatchPattern;

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
                let mut env2 = env.clone();
                let scr_enum_name = scr_enum_name.clone();
                let field_decls: Vec<crate::core::ast::EnumField> = with_active_enums(|enums| {
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

                    let var = if let Some(i) = raw.as_int() { Var::IntVar(i) }
                        else if let Some(b) = raw.as_bool() { Var::BoolVar(b) }
                        else if let Some(s) = raw.as_string() { Var::StrVar(s) }
                        else if let Some(r) = raw.as_real() { Var::RealVar(r) }
                        else if let Some(payload_dt) = raw.as_datatype() {

                            let field_type = field_decls.get(j)
                                .map(|f| f.type_name.clone())
                                .unwrap_or_else(|| scr_enum_name.clone());
                            let payload_dt_sort: &'static DatatypeSort<'static> =
                                with_active_enums(|enums| {
                                    enums.and_then(|er| {
                                        er.by_name.borrow().get(&field_type)
                                            .map(|(d, _)| *d)
                                    })
                                }).unwrap_or(dt);
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
