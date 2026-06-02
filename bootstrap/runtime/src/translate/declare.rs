//! Variable declaration: build Z3 consts for typed memberships; recurse into sub-schemas.
//! Returns non-negativity constraints (Nat/Pos/Seq-len) for the caller to assert.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use z3::ast::{Array, Bool, Int, Real, Set, String as Z3Str};
use z3::{Context, Sort};

use crate::core::ast::*;
use crate::core::{DatatypeRegistry, EnumRegistry, SeqElem, Var};
use super::datatypes::get_or_build_datatype;

/// Unique suffix for each `ClaimCall` invocation's Z3 const names; prevents
/// two parallel invocations of the same claim from sharing internal vars.
static CLAIM_CALL_COUNTER: AtomicU64 = AtomicU64::new(0);

pub(super) fn next_call_id() -> u64 {
    CLAIM_CALL_COUNTER.fetch_add(1, Ordering::Relaxed)
}

/// Declare one variable into env (primitives: one Z3 const; schemas: recurse per-field).
/// Returns non-negativity constraints (Nat/Pos/Seq-len) the caller MUST assert.
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

/// Like `declare_var` but decouples the Z3 const name from the env key.
/// Parallel claim invocations must use distinct Z3 names (`x__call7`) — same name = same var.
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
    // Idempotence guard: re-declaring `state.dots` would wipe the literal len from
    // `apply_seq_lengths`, breaking quantifier unrolling over `#state.dots`.
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
        // Seq: Array(Int→T) + length var. Primitives handled inline; user types get a
        // DatatypeSort (built or reused); anything else warns and skips.
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
                        return post; // warning already emitted by get_or_build_datatype
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
                // Enum element: reuse `DatatypeSeqVar` with `fields = []` (the enum marker).
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
                        fields: Vec::new(),  // no record fields — enum elements
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
                        return post; // warning already emitted by get_or_build_datatype
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

/// Pin the `len` of each Seq var in `seq_lengths` to a literal. Without this, `Cardinality`
/// returns a free symbol and quantifiers over `{0..#seq-1}` silently drop.
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

/// Populate `candidates` on Set vars from `given` before body translation.
/// `Expr::Cardinality` reads `candidates.len()`; membership assertions happen later in `run_cached`.
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
