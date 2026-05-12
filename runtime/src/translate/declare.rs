//! Variable declaration: building Z3 consts for typed memberships and
//! recursing into sub-schemas. Also owns `CLAIM_CALL_COUNTER` (and
//! `next_call_id`) used to generate per-invocation suffixes for the
//! Z3 names of claim-internal parameters — see `declare_var_named` for
//! why.
//!
//! Declaration ALLOCATES typed Z3 constants and inserts them into the
//! caller's `env`. It does NOT assert constraints on a Solver — the
//! type-implied invariants for `Nat`, `Pos`, and Seq-length fields
//! (non-negativity / strict positivity) are returned as a `Vec<Bool>`
//! the caller must issue itself. Keeping the boundary clean: this
//! file is "name binding"; the inline / eval orchestrators are where
//! constraint assertion lives.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use z3::ast::{Array, Bool, Int, Real, Set, String as Z3Str};
use z3::{Context, Sort};

use crate::ast::*;
use super::types::{DatatypeRegistry, EnumRegistry, SeqElem, Var};
use super::datatypes::get_or_build_datatype;

/// Monotonic counter used by `inline_body_items` to give each
/// `ClaimCall` invocation a unique suffix on its Z3 const names.
/// Without this, two invocations of the same claim share Z3 vars
/// for the claim's internal parameters and end up contradicting.
static CLAIM_CALL_COUNTER: AtomicU64 = AtomicU64::new(0);

pub(super) fn next_call_id() -> u64 {
    CLAIM_CALL_COUNTER.fetch_add(1, Ordering::Relaxed)
}

/// Declare one variable into env. For built-in types (Int, Nat, Pos,
/// Bool, String) this allocates a single Z3 const. For user-defined
/// schemas it recurses into the schema body, declaring one const per
/// field under the dotted prefix `prefix.field`.
///
/// The optional `registry` enables `Seq(UserType)` declarations: if
/// present, a Z3 Datatype for the user type is built (or reused) and
/// the variable is bound as `Var::DatatypeSeqVar`. Without it, the
/// branch logs a warning and skips.
///
/// Returns a list of `Bool` constraints the caller MUST assert on the
/// solver after declaration completes. These are the type-implied
/// non-negativity invariants for `Nat`, `Pos`, and Seq-length fields:
/// the values are facts about the typed binding, but issuing them on
/// the solver is the consumer's responsibility (declare's single
/// concern is allocation; the inline / eval orchestrators decide when
/// and how to assert — including whether to attach an unsat-core
/// tracker, etc.).
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

/// Like `declare_var`, but the Z3 const name is decoupled from the env
/// key. Used by `ClaimCall` to give each invocation its own fresh Z3
/// constants for the claim's *unmapped internal* variables (e.g.
/// `AxisPhysics.intended`), so two parallel invocations don't collide.
///
/// Two `Int::new_const(ctx, "intended")` calls return the **same** Z3
/// constant — the API treats names as identifiers, not tags. So when
/// `PlayerPhysics` invokes `AxisPhysics` twice (once per axis) and
/// each invocation declared `intended` with name `"intended"`, both
/// shared one Z3 var; the x-axis branch and y-axis branch each tried
/// to constrain that same var to different values → UNSAT. Passing a
/// per-call suffix (`intended__call7`) makes the Z3 vars distinct
/// while keeping the env key stable so the claim body's references
/// resolve correctly.
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
    // Idempotence guard: if the leaf is already declared (Int/Bool/Seq/
    // Set/composite — anything that lands in env at this exact key),
    // don't re-declare. Sub-schemas (`state ∈ DotCollectState`) never
    // store the bare name (`state`) in env — only their flat-expanded
    // leaves (`state.player.x`, …) — so this guard is a no-op there
    // and the recursion proceeds to the leaves, which DO get the guard.
    //
    // Without this, when `inline_body_items` walks a passthrough's
    // Memberships and calls declare_var(state, DotCollectState), the
    // user-type recursion blindly re-declares `state.dots` — wiping
    // the literal `len` that `apply_seq_lengths` just installed and
    // breaking quantifier unrolling over `#state.dots`.
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
        // Seq sorts: model as Array(Int → T) + a separate length
        // variable. Three element-type families:
        //   - primitives (Int, Bool, String): handled inline.
        //   - user types in `schemas`: build a Z3 Datatype on the fly
        //     and use that as the array's range sort. Field access on
        //     `arr[i]` routes through the Datatype's accessors.
        //   - anything else: warn and skip.
        // See the Var::SeqVar / Var::DatatypeSeqVar comments for why
        // we don't use Z3's native Seq sort.
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
                    let Some((dt, fields)) = get_or_build_datatype(user_type, ctx, schemas, reg) else {
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
                // Stage 5: enum element type — `Seq(BodyItem)`,
                // `Seq(SchemaDecl)`, etc. The enum's DatatypeSort is
                // already in the EnumRegistry from load time. Reuse
                // `Var::DatatypeSeqVar` with `fields = []` to signal
                // "enum-typed seq, no record fields." Indexing
                // returns the datatype value directly; equality with
                // a constructor-applied value drives the typical
                // pattern-matching workflow.
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
            let (eltype, elem) = match inner {
                "Int"    => (Sort::int(ctx),    SeqElem::Int),
                "Bool"   => (Sort::bool(ctx),   SeqElem::Bool),
                "String" => (Sort::string(ctx), SeqElem::Str),
                other => {
                    eprintln!("warning: unsupported Set element type {} for {}", other, prefix);
                    return post;
                }
            };
            let set = Set::new_const(ctx, prefix, &eltype);
            env.insert(env_key.to_string(), Var::SetVar {
                set,
                elem,
                candidates: std::rc::Rc::new(std::cell::RefCell::new(None)),
            });
        }
        _ => {
            // Enum type? Look up in the EnumRegistry, build a Z3 const
            // of the enum's DatatypeSort, store as EnumVar.
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
                // Expand each membership in the sub-schema's body. Both
                // env key and Z3 name extend with the same field name —
                // for sub-schemas, leaf-level isolation is what matters
                // (e.g. `state.player.x`); whether `state` itself uses a
                // fresh-suffixed Z3 name is irrelevant since the bare
                // `state` never gets a Z3 const of its own.
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

/// Substitute a literal `Int::from_i64(n)` for the symbolic `len`
/// field of every `Var::SeqVar` / `Var::DatatypeSeqVar` whose name
/// has a known length in `seq_lengths`. Without this,
/// `translate_int(Cardinality(seq))` returns the free `len` symbol,
/// so `literal_range` can't fold `Range(0, #seq - 1)` to a concrete
/// pair and the quantifier silently drops.
///
/// Lives here (rather than in `preprocess`) because mutating typed
/// Z3 bindings is part of the "name binding" concern this module
/// owns — preprocess is AST→AST only and must not allocate Z3
/// values. Idempotent and safe to run after `apply_pinned_ints`
/// (different var kinds, no overlap).
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
