//! Resolving env bindings: claim-arg mapping, enum-value construction, and
//! the seq "handle" abstraction (primitive vs composite element arrays).

use std::collections::HashMap;
use z3::ast::{Bool, Int, String as Z3Str};
use z3::{Context, DatatypeSort};

use crate::core::{FieldKind, SeqElem, Var};

use super::*;

pub(in crate::encode) fn resolve_mapping<'ctx>(
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
                            encode_int(arg_ref, ctx, env).map(Var::IntVar),
                        "Bool" =>
                            encode_bool(arg_ref, ctx, env, schemas).map(Var::BoolVar),
                        "String" =>
                            encode_str(arg_ref, ctx, env).map(Var::StrVar),
                        "Real" =>
                            encode_real(arg_ref, ctx, env).map(Var::RealVar),
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
                    if let Some(i) = encode_int(idx_expr, ctx, env) {
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

pub(super) fn resolve_field_chain_to_bindings<'ctx>(
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
    let i = encode_int(idx_expr, ctx, env)?;
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

pub(super) fn expr_as_var<'ctx>(
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
            let i = encode_int(idx, ctx, env)?;
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
                        encode_seq_arg_for_ctor(arg_expr, inner, ctx, env, schemas)?;
                    owned_args.push(arr_dyn);
                    owned_args.push(len_dyn);
                    continue;
                }
                let v: Box<dyn z3::ast::Ast<'ctx>> = match field_type.as_str() {
                    "Int" | "Nat" | "Pos" =>
                        Box::new(encode_int(arg_expr, ctx, env)?),
                    "Bool" =>
                        Box::new(encode_bool(arg_expr, ctx, env, schemas)?),
                    "String" =>
                        Box::new(encode_str(arg_expr, ctx, env)?),
                    "Real" =>
                        Box::new(encode_real(arg_expr, ctx, env)?),
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
            let cond = encode_bool(c, ctx, env, schemas)?;
            let then_v = resolve_enum_ast(a, ctx, env, schemas)?;
            let else_v = resolve_enum_ast(b, ctx, env, schemas)?;
            Some(cond.ite(&then_v, &else_v))
        }
        Expr::Match(scr, arms) => {
            let compiled = encode_match_arms(scr, arms, ctx, env,
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

pub(super) fn encode_seq_arg_for_ctor<'ctx>(
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
                    let v = encode_int(item, ctx, env)?;
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
                    let v = encode_bool(item, ctx, env, schemas)?;
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
                    let v = encode_str(item, ctx, env)?;
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
            "Int" | "Nat" | "Pos" => encode_int(item, ctx, env)?.into(),
            "Bool"                => encode_bool(item, ctx, env, schemas)?.into(),
            "String"              => encode_str(item, ctx, env)?.into(),
            "Real"                => encode_real(item, ctx, env)?.into(),
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
    let i = encode_int(idx_expr, ctx, env)?;
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

pub(super) fn resolve_seq_field<'ctx>(
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
    let i = encode_int(idx_expr, ctx, env)?;
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
