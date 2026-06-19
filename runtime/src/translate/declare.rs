use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use z3::ast::{Array, Bool, Int, Real, Set, String as Z3Str};
use z3::{Context, DatatypeAccessor, DatatypeBuilder, DatatypeSort, Sort};

use crate::core::ast::*;
use crate::core::{DatatypeRegistry, EnumRegistry, FieldKind, SeqElem, SeqFieldElem, Var};

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

pub(super) fn apply_seq_lengths<'ctx>(
    env: &mut HashMap<String, Var<'ctx>>,
    seq_lengths: &HashMap<String, i64>,
    ctx: &'ctx Context,
) {
    for (name, n) in seq_lengths {
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
        env.insert(name.clone(), new_var);
    }
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
