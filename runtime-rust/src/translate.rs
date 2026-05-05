//! AST → Z3 expressions. v0.1: Int/Bool only, flat declarations,
//! arithmetic + boolean + comparisons.

use crate::ast::*;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use z3::ast::{Array, Ast, Bool, Int, Set, String as Z3Str};
use z3::{Context, DatatypeAccessor, DatatypeBuilder, DatatypeSort, SatResult, Solver, Sort};

/// Cache of Z3 Datatype sorts built for user types referenced as the
/// element of `Seq(UserType)`. Built lazily on first reference. The
/// runtime owns this and passes a reference into `evaluate` /
/// `build_cache` the same way `schemas` is passed.
///
/// The `'static` lifetime mirrors the runtime's leaked `Context` —
/// the runtime already leaks its Context, so leaking the per-type
/// `DatatypeSort` (which borrows from that Context) is consistent.
/// See `EvidentRuntime::new` for why the Context is leaked.
///
/// Each entry caches both the DatatypeSort and the parallel
/// `Vec<FieldKind>` that describes how to walk the type's fields
/// (leaf primitives + nested sub-structs). Sharing the field list
/// across siblings (e.g. SDLRect.color and SDLOutput.bg both use
/// Color) avoids re-walking the schema body on every reference.
pub type DatatypeRegistry =
    RefCell<HashMap<String, (&'static DatatypeSort<'static>, Vec<FieldKind>)>>;

/// Result of running one query.
#[derive(Debug, Clone)]
pub struct EvalResult {
    pub satisfied: bool,
    pub bindings: HashMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Int(i64),
    Bool(bool),
    Str(String),
    /// Sequence values returned in the model. The variant tracks which
    /// element type was declared so callers don't have to. Length is
    /// implicit in the Vec's len().
    SeqInt(Vec<i64>),
    SeqBool(Vec<bool>),
    SeqStr(Vec<String>),
    /// A single struct value — one entry per declared field, mapping
    /// field name to its primitive Value. Used as the element of
    /// `SeqComposite`. Not currently produced as a top-level binding
    /// (sub-schema field expansion still creates one leaf per field).
    Composite(HashMap<String, Value>),
    /// `Seq(UserType)` — one map per element. Each map keys a flat
    /// field name to the field's primitive Value.
    SeqComposite(Vec<HashMap<String, Value>>),
}

/// What primitive a Seq holds. Lets `SeqVar` stay homogeneous while
/// still letting model extraction pick the right path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SeqElem { Int, Bool, Str }

/// One field of a user type stored as the element of `Seq(UserType)`.
/// Two flavors: leaf primitives (Int/Nat/Pos/Bool/String), and nested
/// composite fields whose own type is itself a user struct.
///
/// The accessor in the parent Datatype always returns a `Dynamic` of
/// the field's sort. For primitives that's an Int/Bool/String; for
/// nested composites it's another Datatype value, which has its own
/// list of accessors (the `sub_fields` here, plus the `dt` pointer).
///
/// v1 still rejects fields that are themselves Seqs / Sets — the
/// recursion only handles user-defined struct types.
#[derive(Clone, Debug)]
pub enum FieldKind {
    Primitive {
        name: String,
        /// "Int" | "Nat" | "Pos" | "Bool" | "String" — routes the
        /// extracted Dynamic through the right `as_int` / `as_bool`
        /// / `as_string` extractor and tells callers what sort it
        /// translates to.
        prim_type: String,
    },
    Nested {
        name: String,
        /// User type's name, kept for diagnostics + cache key parity
        /// with what `get_or_build_datatype` registers.
        #[allow(dead_code)]
        type_name: String,
        /// Z3 Datatype for this nested type. Same `'static` lifetime
        /// trick as the outer DatatypeSeqVar's `dt` — the runtime
        /// already leaks its Context, so leaking the per-type sort
        /// is consistent.
        dt: &'static DatatypeSort<'static>,
        /// Recursive: the nested type's own field list. Could itself
        /// contain another `Nested` for two-deep composition (e.g.
        /// SDLOutput.bg.color, if Color had another nested field).
        sub_fields: Vec<FieldKind>,
    },
}

impl FieldKind {
    fn name(&self) -> &str {
        match self {
            FieldKind::Primitive { name, .. } => name,
            FieldKind::Nested { name, .. } => name,
        }
    }
}

/// Z3 binding for a declared variable. Keep a typed handle so we know
/// which AST kind to translate against.
///
/// Seq values are modeled as a Z3 Array(Int → T) plus an explicit
/// length variable. Z3's native Seq sort would work via `Z3_mk_seq_sort`
/// but the safe `z3` crate doesn't expose `Z3_mk_seq_nth` (only
/// `Z3_mk_seq_at` which returns a length-1 sub-sequence with no way
/// to extract the element). The Array+Length encoding is simpler and
/// gives us cardinality + indexing for free; the only downside is the
/// Array has values at all indices, not just 0..len, but we just don't
/// read past `len` during model extraction.
#[derive(Clone)]
enum Var<'ctx> {
    IntVar(Int<'ctx>),
    BoolVar(Bool<'ctx>),
    StrVar(Z3Str<'ctx>),
    SeqVar { arr: Array<'ctx>, len: Int<'ctx>, elem: SeqElem },
    /// `Seq(UserType)` — element sort is a Z3 Datatype whose
    /// constructor + accessors live in the shared `DatatypeRegistry`.
    /// Modeled the same as primitive Seqs: `Array(Int → DatatypeSort)
    /// + length`. The `dt` pointer is duplicated here so translators
    /// can resolve `pts[i].field` without threading the registry
    /// through every translate-* call. The `'static` lifetime on
    /// `dt` mirrors the leaked Context; this variant is only ever
    /// constructed from the cached path with `'ctx = 'static`.
    DatatypeSeqVar {
        arr: Array<'ctx>,
        len: Int<'ctx>,
        type_name: String,
        dt: &'static DatatypeSort<'static>,
        /// Per-field metadata in declaration order — the same order as
        /// `dt.variants[0].accessors`. Each entry is a `FieldKind`,
        /// either a leaf primitive (which routes through `as_int` /
        /// `as_bool` / `as_string`) or a `Nested` sub-struct (which
        /// holds its own DatatypeSort + `sub_fields` for further
        /// recursion).
        fields: Vec<FieldKind>,
    },
    /// Z3 Set — characteristic function over an element sort. Supports
    /// `x ∈ s` membership; we don't expose cardinality / iteration
    /// because Z3 sets are functions over infinite domains, not finite
    /// containers. Model extraction returns no binding for SetVars.
    SetVar { set: Set<'ctx>, elem: SeqElem },
    /// Compile-time literal int. Mirrors Python's "value pre-bound in env"
    /// pattern: certain names are known to equal a specific integer
    /// before the solver runs (from `given` + literal-equality body
    /// constraints + length propagation `n = #seq` where #seq is also
    /// pinned). Translating an Identifier bound to PinnedInt yields a
    /// Z3 IntVal, which lets `literal_range` recover the value via
    /// simplify+as_i64. Without this, `∀ i ∈ {0..n - 1}` couldn't unroll.
    PinnedInt(i64),
}

impl<'ctx> Var<'ctx> {
    fn as_bool(&self) -> Option<&Bool<'ctx>> {
        match self { Var::BoolVar(b) => Some(b), _ => None }
    }
    fn as_str(&self) -> Option<&Z3Str<'ctx>> {
        match self { Var::StrVar(s) => Some(s), _ => None }
    }
    fn as_seq(&self) -> Option<(&Array<'ctx>, &Int<'ctx>, SeqElem)> {
        match self { Var::SeqVar { arr, len, elem } => Some((arr, len, *elem)), _ => None }
    }
    fn as_set(&self) -> Option<(&Set<'ctx>, SeqElem)> {
        match self { Var::SetVar { set, elem } => Some((set, *elem)), _ => None }
    }
    fn as_datatype_seq(&self) -> Option<(&Array<'ctx>, &Int<'ctx>, &str,
                                         &'static DatatypeSort<'static>,
                                         &[FieldKind])> {
        match self {
            Var::DatatypeSeqVar { arr, len, type_name, dt, fields } =>
                Some((arr, len, type_name.as_str(), *dt, fields.as_slice())),
            _ => None,
        }
    }
}

/// Read a Seq value out of the model: read the length, then read each
/// `arr.select(i)` for i ∈ 0..length and assemble into the right
/// `Value::Seq*` variant. Indices past the length are unconstrained
/// in Z3 (Arrays are total functions); we just don't read them.
fn extract_seq<'ctx>(
    arr: &Array<'ctx>,
    len: &Int<'ctx>,
    elem: SeqElem,
    model: &z3::Model<'ctx>,
    ctx: &'ctx Context,
) -> Option<Value> {
    let n = model.eval(len, true)?.as_i64()?;
    if n < 0 { return None; }
    match elem {
        SeqElem::Int => {
            let mut out = Vec::with_capacity(n as usize);
            for i in 0..n {
                let idx = Int::from_i64(ctx, i);
                let v = arr.select(&idx).as_int()?;
                out.push(model.eval(&v, true)?.as_i64()?);
            }
            Some(Value::SeqInt(out))
        }
        SeqElem::Bool => {
            let mut out = Vec::with_capacity(n as usize);
            for i in 0..n {
                let idx = Int::from_i64(ctx, i);
                let v = arr.select(&idx).as_bool()?;
                out.push(model.eval(&v, true)?.as_bool()?);
            }
            Some(Value::SeqBool(out))
        }
        SeqElem::Str => {
            let mut out = Vec::with_capacity(n as usize);
            for i in 0..n {
                let idx = Int::from_i64(ctx, i);
                let v = arr.select(&idx).as_string()?;
                out.push(model.eval(&v, true)?.as_string()?);
            }
            Some(Value::SeqStr(out))
        }
    }
}

/// Get or build a Z3 `DatatypeSort` for a user type referenced as the
/// element of `Seq(UserType)`. Walks the type's body for `Membership`
/// items, building a parallel `Vec<FieldKind>` and a list of Z3 sorts
/// suitable for `DatatypeBuilder::variant`.
///
/// Recurses for nested user-type fields: a field declared `c ∈ Color`
/// where Color is itself a struct triggers a nested
/// `get_or_build_datatype` call, and the resulting Datatype's sort
/// becomes the field's `DatatypeAccessor::Sort(...)`. Both the outer
/// and inner Datatypes land in the shared `DatatypeRegistry` so
/// siblings using the same nested type (e.g. SDLRect.color and
/// SDLOutput.bg both pointing at Color) share the same Z3 sort.
///
/// v1 limitation: nested fields can only be other user structs (or
/// the same set of leaf primitives — Int/Nat/Pos/Bool/String). Fields
/// of type `Seq(...)` / `Set(...)` are still rejected with a warning
/// (would need different element-array handling that's out of scope
/// for this slice).
///
/// The returned references have a `'static` lifetime: the runtime
/// already leaks its `Context`, so leaking the per-type `DatatypeSort`
/// (which borrows from that Context) is consistent. See
/// `EvidentRuntime::new` for why the Context is leaked.
fn get_or_build_datatype(
    type_name: &str,
    ctx: &'static Context,
    schemas: &HashMap<String, SchemaDecl>,
    registry: &DatatypeRegistry,
) -> Option<(&'static DatatypeSort<'static>, Vec<FieldKind>)> {
    // Cache hit: return the previously-built sort + field list.
    if let Some((dt, fields)) = registry.borrow().get(type_name) {
        return Some((*dt, fields.clone()));
    }
    let schema = schemas.get(type_name)?;

    // First pass: walk the type body and resolve each field to either a
    // primitive sort or a recursively-built nested Datatype. We collect
    // both the FieldKind metadata and the parallel `(name, sort)` list
    // for the DatatypeBuilder.
    let mut fields: Vec<FieldKind> = Vec::new();
    let mut field_sorts: Vec<(String, Sort<'static>)> = Vec::new();
    for item in &schema.body {
        if let BodyItem::Membership { name, type_name: ftype } = item {
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
                // Nested: recurse if this name is itself a user type.
                user_type if schemas.contains_key(user_type) => {
                    let Some((nested_dt, sub_fields)) =
                        get_or_build_datatype(user_type, ctx, schemas, registry)
                    else {
                        // Inner build failed (warning already logged); abort the
                        // outer build too — we can't include a partial Datatype.
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
                         only Int/Nat/Pos/Bool/String and other user struct types \
                         are supported in Seq(UserType) elements (v1)",
                        ftype, type_name
                    );
                    return None;
                }
            }
        }
        // Other body items (constraints, passthroughs) don't shape the
        // record. Field invariants from the type body are *not* asserted
        // on Seq elements in v1 — that would require a ∀ i quantifier
        // and is left to a follow-up.
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

/// Walk the accessors of a single Datatype value and assemble a flat
/// `HashMap<String, Value>` of its fields. Recurses for nested
/// composite fields: a `FieldKind::Nested` yields a `Value::Composite`
/// whose own map is built by another call to this helper on the
/// nested `(dt, sub_fields)` pair.
///
/// Caller is responsible for ensuring `dt` and `fields` were built
/// together (same order). The accessor index aligns with `fields[i]`.
fn extract_composite_value<'ctx>(
    elem: &z3::ast::Datatype<'ctx>,
    fields: &[FieldKind],
    dt: &DatatypeSort<'_>,
    model: &z3::Model<'ctx>,
    ctx: &'ctx Context,
) -> Option<HashMap<String, Value>> {
    let mut field_map: HashMap<String, Value> = HashMap::new();
    for (fi, fk) in fields.iter().enumerate() {
        if fi >= dt.variants[0].accessors.len() { break; }
        let accessor = &dt.variants[0].accessors[fi];
        let raw = accessor.apply(&[elem]);
        let value = match fk {
            FieldKind::Primitive { prim_type, .. } => match prim_type.as_str() {
                "Int" | "Nat" | "Pos" => {
                    let z = raw.as_int()?;
                    Value::Int(model.eval(&z, true)?.as_i64()?)
                }
                "Bool" => {
                    let z = raw.as_bool()?;
                    Value::Bool(model.eval(&z, true)?.as_bool()?)
                }
                "String" => {
                    let z = raw.as_string()?;
                    Value::Str(model.eval(&z, true)?.as_string()?)
                }
                _ => return None,
            },
            FieldKind::Nested { dt: nested_dt, sub_fields, .. } => {
                let nested_elem = raw.as_datatype()?;
                let nested_map =
                    extract_composite_value(&nested_elem, sub_fields, *nested_dt, model, ctx)?;
                Value::Composite(nested_map)
            }
        };
        field_map.insert(fk.name().to_string(), value);
    }
    Some(field_map)
}

/// Read a `Seq(UserType)` value out of the model: read the length,
/// then for each `i ∈ 0..length` select the array element (a
/// Datatype value) and call `extract_composite_value` to assemble
/// its field map. Push each element map into a `Vec` and wrap in
/// `Value::SeqComposite`.
fn extract_seq_composite<'ctx>(
    arr: &Array<'ctx>,
    len: &Int<'ctx>,
    fields: &[FieldKind],
    dt: &DatatypeSort<'_>,
    model: &z3::Model<'ctx>,
    ctx: &'ctx Context,
) -> Option<Value> {
    let n = model.eval(len, true)?.as_i64()?;
    if n < 0 { return None; }
    let mut out: Vec<HashMap<String, Value>> = Vec::with_capacity(n as usize);
    for i in 0..n {
        let idx = Int::from_i64(ctx, i);
        let elem_dyn = arr.select(&idx);
        let elem = elem_dyn.as_datatype()?;
        let field_map = extract_composite_value(&elem, fields, dt, model, ctx)?;
        out.push(field_map);
    }
    Some(Value::SeqComposite(out))
}

/// Per-schema cache used by `evaluate_cached`. Holds the shared
/// solver (with the schema's body constraints already asserted at
/// the bottom of the assertion stack) and the env mapping used to
/// resolve given-bindings + extract the model.
pub struct CachedSchema<'ctx> {
    env: HashMap<String, Var<'ctx>>,
    solver: Solver<'ctx>,
}

/// Recursively translate a list of body items into the solver. Used by
/// the constraint-translation pass of both `evaluate` and `build_cache`,
/// and also called recursively when a `Passthrough`, bare-identifier
/// passthrough, or `ClaimCall` references another claim's body.
///
/// Without this, passthroughs only inlined `Constraint` items — any
/// `ClaimCall` (e.g. `PlayerPhysics(state mapsto state.player, …)`)
/// inside a passthrough was silently dropped. Same problem inside a
/// `ClaimCall`: nested claim calls in the called claim's body were
/// dropped. That broke `..DotCollectGameEngine` (no player, no physics,
/// no background — black screen).
///
/// `visited` blocks recursion through cycles (`A` passthroughs `B`,
/// `B` passthroughs `A`). Each entry is the claim name currently being
/// inlined; we add on enter, remove on exit.
fn inline_body_items(
    items: &[BodyItem],
    env: &mut HashMap<String, Var<'static>>,
    solver: &Solver<'static>,
    schemas: &HashMap<String, SchemaDecl>,
    ctx: &'static Context,
    registry: &DatatypeRegistry,
    visited: &mut HashSet<String>,
) {
    for item in items {
        match item {
            BodyItem::Membership { name, type_name } => {
                // Top-level Memberships are pre-declared by pass 1, so this
                // is a no-op there. Useful when the helper recurses into a
                // passthrough's body that introduces variables not yet in
                // env (e.g. a nested claim's locals).
                if !env.contains_key(name) {
                    declare_var(ctx, solver, env, name, type_name, schemas, Some(registry));
                }
            }
            // Bare-identifier-as-passthrough: `Constraint(Identifier(name))`
            // whose name matches a known claim is treated as `..ClaimName`.
            // Falls through to the regular Constraint arm if the name
            // isn't a claim (e.g. a Bool variable named like a claim).
            BodyItem::Constraint(Expr::Identifier(name)) if schemas.contains_key(name) => {
                if visited.contains(name) { continue; }
                let Some(claim) = schemas.get(name) else { continue };
                visited.insert(name.clone());
                inline_body_items(&claim.body, env, solver, schemas, ctx, registry, visited);
                visited.remove(name);
            }
            BodyItem::Constraint(e) => {
                if let Some(b) = translate_bool(e, ctx, env) {
                    solver.assert(&b);
                } else {
                    eprintln!("warning: dropped constraint (couldn't translate to Bool): {:?}", e);
                }
            }
            BodyItem::Passthrough(claim_name) => {
                if visited.contains(claim_name) { continue; }
                let Some(claim) = schemas.get(claim_name) else {
                    eprintln!("warning: ..{} references unknown claim", claim_name);
                    continue;
                };
                visited.insert(claim_name.clone());
                inline_body_items(&claim.body, env, solver, schemas, ctx, registry, visited);
                visited.remove(claim_name);
            }
            BodyItem::ClaimCall { name, mappings } => {
                if visited.contains(name) { continue; }
                let Some(claim) = schemas.get(name) else {
                    eprintln!("warning: ClaimCall to unknown claim {}", name);
                    continue;
                };
                let mut inner = env.clone();
                for m in mappings {
                    let bound = resolve_mapping(&m.slot, &m.value, ctx, env);
                    if bound.is_empty() {
                        eprintln!("warning: mapping value didn't resolve: {:?}", m.value);
                    }
                    for (k, v) in bound {
                        inner.insert(k, v);
                    }
                }
                // Declare any of the claim's own variables that weren't
                // bound by a mapping (the claim's "internal" parameters,
                // like AxisPhysics's `intended` / `target`).
                for sub in &claim.body {
                    if let BodyItem::Membership { name: vname, type_name } = sub {
                        let slot_prefix = format!("{}.", vname);
                        let already_bound = inner.contains_key(vname)
                            || inner.keys().any(|k| k.starts_with(&slot_prefix));
                        if !already_bound {
                            declare_var(ctx, solver, &mut inner, vname, type_name, schemas, Some(registry));
                        }
                    }
                }
                visited.insert(name.clone());
                inline_body_items(&claim.body, &mut inner, solver, schemas, ctx, registry, visited);
                visited.remove(name);
            }
            BodyItem::SubclaimDecl(_) => {}
        }
    }
}

/// Translate the schema's body once into a fresh solver and return a
/// `CachedSchema` that subsequent queries can reuse via push/pop.
pub fn build_cache(
    schema: &SchemaDecl,
    schemas: &HashMap<String, SchemaDecl>,
    ctx: &'static Context,
    registry: &DatatypeRegistry,
) -> CachedSchema<'static> {
    let solver = Solver::new(ctx);
    let mut env: HashMap<String, Var<'static>> = HashMap::new();

    // Same two passes as evaluate(), but writing into the cache's
    // solver instead of a fresh one each time.
    for item in &schema.body {
        match item {
            BodyItem::Membership { name, type_name } => {
                declare_var(ctx, &solver, &mut env, name, type_name, schemas, Some(registry));
            }
            BodyItem::Passthrough(claim_name) => {
                if let Some(claim) = schemas.get(claim_name) {
                    for sub in &claim.body {
                        if let BodyItem::Membership { name, type_name } = sub {
                            if !env.contains_key(name) {
                                declare_var(ctx, &solver, &mut env, name, type_name, schemas, Some(registry));
                            }
                        }
                    }
                }
            }
            // Bare-identifier names-match passthrough: a `BodyItem::Constraint(
            // Identifier(name))` whose `name` is a known claim/type behaves
            // exactly like `..ClaimName`. The parser leaves bare idents as
            // Constraint(Identifier(...)) because it can't disambiguate at
            // parse time (the same shape might be a Bool variable). We
            // resolve here, where `schemas` is in scope.
            BodyItem::Constraint(Expr::Identifier(name)) => {
                if let Some(claim) = schemas.get(name) {
                    for sub in &claim.body {
                        if let BodyItem::Membership { name, type_name } = sub {
                            if !env.contains_key(name) {
                                declare_var(ctx, &solver, &mut env, name, type_name, schemas, Some(registry));
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }

    // Pass 1.5: pin literal-int vars (no givens for build_cache — those
    // come per-query). Lets `∀ i ∈ {0..n - 1}` unroll when n is fixed by
    // a `n = literal` constraint or via #seq length propagation.
    let no_given: HashMap<String, Value> = HashMap::new();
    let seq_lens = collect_seq_lengths(&schema.body, &no_given);
    let pinned   = collect_pinned_ints(&schema.body, &no_given, &seq_lens);
    apply_pinned_ints(&mut env, &pinned);
    apply_seq_lengths(&mut env, &seq_lens, ctx);

    let mut visited: HashSet<String> = HashSet::new();
    inline_body_items(&schema.body, &mut env, &solver, schemas, ctx, registry, &mut visited);

    CachedSchema { env, solver }
}

/// Sample up to `n` distinct models from the cached schema's solver.
///
/// Strategy: one outer push for the per-query givens. Inside the outer
/// frame, loop:
///   1. solver.check(); if UNSAT, break.
///   2. Extract model into bindings; push onto result vec.
///   3. Build a blocking clause: ¬(AND of `binding == value` for every
///      *scalar* binding) — Bool, Int, Str. Sequence/set/composite
///      values are skipped from the clause for v1; schemas whose only
///      bindings are Seq* will return duplicates (documented limitation).
///   4. Assert the blocking clause inside the outer frame, so it
///      persists across iterations but is discarded by the outer pop.
///
/// Final pop restores the cached solver to exactly its build-time state.
pub fn sample_cached_inner<'ctx>(
    cached: &CachedSchema<'ctx>,
    given: &HashMap<String, Value>,
    n: usize,
    ctx: &'ctx Context,
) -> Vec<HashMap<String, Value>> {
    cached.solver.push();

    // Apply per-query givens (mirrors run_cached).
    for (name, value) in given {
        let Some(var) = cached.env.get(name) else { continue };
        match (var, value) {
            (Var::IntVar(v),  Value::Int(n))  => cached.solver.assert(&v._eq(&Int::from_i64(ctx, *n))),
            (Var::BoolVar(v), Value::Bool(b)) => cached.solver.assert(&v._eq(&Bool::from_bool(ctx, *b))),
            (Var::StrVar(v),  Value::Str(s))  => cached.solver.assert(&v._eq(&Z3Str::from_str(ctx, s).expect("nul in str"))),
            (Var::PinnedInt(v), Value::Int(n)) if *v == *n => {}
            (Var::PinnedInt(_), Value::Int(_)) => cached.solver.assert(&Bool::from_bool(ctx, false)),
            _ => {
                if let Some(b) = assert_seq_given(var, value, ctx) {
                    cached.solver.assert(&b);
                } else {
                    eprintln!("warning: type mismatch for given {:?}", name);
                }
            }
        }
    }

    let mut out: Vec<HashMap<String, Value>> = Vec::new();
    for _ in 0..n {
        if !matches!(cached.solver.check(), SatResult::Sat) {
            break;
        }
        let Some(model) = cached.solver.get_model() else { break };

        let mut bindings: HashMap<String, Value> = HashMap::new();
        // Collect scalar `(z3 expr, value)` pairs as we extract; we'll
        // turn them into a blocking clause at the end.
        let mut block_terms: Vec<Bool<'ctx>> = Vec::new();

        for (name, var) in cached.env.iter() {
            match var {
                Var::IntVar(i) => {
                    if let Some(v) = model.eval(i, true).and_then(|x| x.as_i64()) {
                        bindings.insert(name.clone(), Value::Int(v));
                        block_terms.push(i._eq(&Int::from_i64(ctx, v)));
                    }
                }
                Var::BoolVar(b) => {
                    if let Some(v) = model.eval(b, true).and_then(|x| x.as_bool()) {
                        bindings.insert(name.clone(), Value::Bool(v));
                        block_terms.push(b._eq(&Bool::from_bool(ctx, v)));
                    }
                }
                Var::StrVar(s) => {
                    if let Some(v) = model.eval(s, true).and_then(|x| x.as_string()) {
                        bindings.insert(name.clone(), Value::Str(v.clone()));
                        if let Ok(lit) = Z3Str::from_str(ctx, &v) {
                            block_terms.push(s._eq(&lit));
                        }
                    }
                }
                Var::SeqVar { arr, len, elem } => {
                    if let Some(v) = extract_seq(arr, len, *elem, &model, ctx) {
                        bindings.insert(name.clone(), v);
                    }
                    // Seq blocking is non-trivial (would need ¬(arr[0]=v0
                    // ∧ … ∧ len=n)) — skipped for v1. Documented limitation.
                }
                Var::PinnedInt(v) => {
                    bindings.insert(name.clone(), Value::Int(*v));
                    // PinnedInts are constants, not solver vars — no
                    // useful blocking term to add.
                }
                Var::SetVar { .. } => {
                    // Same as run_cached: SetVars aren't enumerable; skip.
                }
                Var::DatatypeSeqVar { arr, len, dt, fields, .. } => {
                    if let Some(v) = extract_seq_composite(
                        arr, len, fields.as_slice(), *dt, &model, ctx)
                    {
                        bindings.insert(name.clone(), v);
                    }
                    // Blocking on composite seq elements is non-trivial
                    // (same shape as primitive seqs); skipped for v1.
                }
            }
        }

        out.push(bindings);

        // If we have no scalar terms to block on at all, we'd loop
        // forever returning the same model. Bail.
        if block_terms.is_empty() {
            break;
        }
        let refs: Vec<&Bool<'ctx>> = block_terms.iter().collect();
        let conj = Bool::and(ctx, &refs);
        cached.solver.assert(&conj.not());
    }

    cached.solver.pop(1);
    out
}

/// Build a `Bool` constraint asserting that the named Seq variable
/// equals the given Value::Seq* (length + per-index element equality).
/// Returns None when the var/value shapes don't match — caller should
/// then warn or fall through.
///
/// Supports:
///   - Var::SeqVar (primitive elements: Int / Bool / String) +
///     Value::SeqInt / SeqBool / SeqStr
///   - Var::DatatypeSeqVar + Value::SeqComposite — builds a Datatype
///     constructor application per element from the field map's
///     primitive values (recursively for nested composites)
fn assert_seq_given<'ctx>(
    var: &Var<'ctx>,
    value: &Value,
    ctx: &'ctx Context,
) -> Option<Bool<'ctx>> {
    match (var, value) {
        (Var::SeqVar { arr, len, elem: SeqElem::Int }, Value::SeqInt(items)) => {
            let mut conjuncts: Vec<Bool> = Vec::with_capacity(items.len() + 1);
            conjuncts.push(len._eq(&Int::from_i64(ctx, items.len() as i64)));
            for (i, n) in items.iter().enumerate() {
                let idx = Int::from_i64(ctx, i as i64);
                let cell = arr.select(&idx).as_int()?;
                conjuncts.push(cell._eq(&Int::from_i64(ctx, *n)));
            }
            let refs: Vec<&Bool> = conjuncts.iter().collect();
            Some(Bool::and(ctx, &refs))
        }
        (Var::SeqVar { arr, len, elem: SeqElem::Bool }, Value::SeqBool(items)) => {
            let mut conjuncts: Vec<Bool> = Vec::with_capacity(items.len() + 1);
            conjuncts.push(len._eq(&Int::from_i64(ctx, items.len() as i64)));
            for (i, b) in items.iter().enumerate() {
                let idx = Int::from_i64(ctx, i as i64);
                let cell = arr.select(&idx).as_bool()?;
                conjuncts.push(cell._eq(&Bool::from_bool(ctx, *b)));
            }
            let refs: Vec<&Bool> = conjuncts.iter().collect();
            Some(Bool::and(ctx, &refs))
        }
        (Var::SeqVar { arr, len, elem: SeqElem::Str }, Value::SeqStr(items)) => {
            let mut conjuncts: Vec<Bool> = Vec::with_capacity(items.len() + 1);
            conjuncts.push(len._eq(&Int::from_i64(ctx, items.len() as i64)));
            for (i, s) in items.iter().enumerate() {
                let idx = Int::from_i64(ctx, i as i64);
                let cell = arr.select(&idx).as_string()?;
                let want = Z3Str::from_str(ctx, s).ok()?;
                conjuncts.push(cell._eq(&want));
            }
            let refs: Vec<&Bool> = conjuncts.iter().collect();
            Some(Bool::and(ctx, &refs))
        }
        (Var::DatatypeSeqVar { arr, len, dt, fields, .. }, Value::SeqComposite(items)) => {
            let mut conjuncts: Vec<Bool> = Vec::with_capacity(items.len() + 1);
            conjuncts.push(len._eq(&Int::from_i64(ctx, items.len() as i64)));
            // The Datatype has a single constructor with fields in
            // declaration order. Build an application per element.
            let ctor = &dt.variants[0].constructor;
            for (i, element) in items.iter().enumerate() {
                let mut field_dyns: Vec<z3::ast::Dynamic> = Vec::with_capacity(fields.len());
                for fk in fields.iter() {
                    let dynamic = match fk {
                        FieldKind::Primitive { name, prim_type } => {
                            let v = element.get(name)?;
                            match (prim_type.as_str(), v) {
                                ("Int" | "Nat" | "Pos", Value::Int(n)) =>
                                    z3::ast::Dynamic::from_ast(&Int::from_i64(ctx, *n)),
                                ("Bool", Value::Bool(b)) =>
                                    z3::ast::Dynamic::from_ast(&Bool::from_bool(ctx, *b)),
                                ("String", Value::Str(s)) => {
                                    let z = Z3Str::from_str(ctx, s).ok()?;
                                    z3::ast::Dynamic::from_ast(&z)
                                }
                                _ => return None,
                            }
                        }
                        FieldKind::Nested { .. } => return None, // skip for v1
                    };
                    field_dyns.push(dynamic);
                }
                let dyn_refs: Vec<&dyn Ast> = field_dyns.iter().map(|d| d as &dyn Ast).collect();
                let elem_ast = ctor.apply(&dyn_refs);
                let idx = Int::from_i64(ctx, i as i64);
                let cell = arr.select(&idx);
                conjuncts.push(cell._eq(&elem_ast));
            }
            let refs: Vec<&Bool> = conjuncts.iter().collect();
            Some(Bool::and(ctx, &refs))
        }
        _ => None,
    }
}

/// Per-query work: push, assert givens against the cached env, check,
/// extract model, pop. Reuses all the constraint translation already
/// in the cache.
pub fn run_cached<'ctx>(
    cached: &CachedSchema<'ctx>,
    given: &HashMap<String, Value>,
    ctx: &'ctx Context,
) -> EvalResult {
    cached.solver.push();
    for (name, value) in given {
        let Some(var) = cached.env.get(name) else { continue };
        match (var, value) {
            (Var::IntVar(v),  Value::Int(n))  => cached.solver.assert(&v._eq(&Int::from_i64(ctx, *n))),
            (Var::BoolVar(v), Value::Bool(b)) => cached.solver.assert(&v._eq(&Bool::from_bool(ctx, *b))),
            (Var::StrVar(v),  Value::Str(s))  => cached.solver.assert(&v._eq(&Z3Str::from_str(ctx, s).expect("nul in str"))),
            // PinnedInt was already folded in via apply_pinned_ints from
            // this same given value, so the assertion is redundant. If
            // the values disagree (caller passes a different int after a
            // body equality pinned the var), force UNSAT.
            (Var::PinnedInt(v), Value::Int(n)) if *v == *n => {}
            (Var::PinnedInt(_), Value::Int(_)) => cached.solver.assert(&Bool::from_bool(ctx, false)),
            _ => {
                if let Some(b) = assert_seq_given(var, value, ctx) {
                    cached.solver.assert(&b);
                } else {
                    eprintln!("warning: type mismatch for given {:?}", name);
                }
            }
        }
    }
    let satisfied = matches!(cached.solver.check(), SatResult::Sat);
    let mut bindings = HashMap::new();
    if satisfied {
        if let Some(model) = cached.solver.get_model() {
            for (name, var) in cached.env.iter() {
                match var {
                    Var::IntVar(i) => {
                        if let Some(v) = model.eval(i, true).and_then(|x| x.as_i64()) {
                            bindings.insert(name.clone(), Value::Int(v));
                        }
                    }
                    Var::BoolVar(b) => {
                        if let Some(v) = model.eval(b, true).and_then(|x| x.as_bool()) {
                            bindings.insert(name.clone(), Value::Bool(v));
                        }
                    }
                    Var::StrVar(s) => {
                        if let Some(v) = model.eval(s, true).and_then(|x| x.as_string()) {
                            bindings.insert(name.clone(), Value::Str(v));
                        }
                    }
                    Var::SeqVar { arr, len, elem } => {
                        if let Some(v) = extract_seq(arr, len, *elem, &model, ctx) {
                            bindings.insert(name.clone(), v);
                        }
                    }
                    Var::PinnedInt(v) => {
                        bindings.insert(name.clone(), Value::Int(*v));
                    }
                    Var::SetVar { .. } => {
                        // Z3 sets are characteristic functions over an
                        // (often infinite) element domain. We don't try
                        // to enumerate; bindings just omit set vars.
                    }
                    Var::DatatypeSeqVar { arr, len, dt, fields, .. } => {
                        if let Some(v) = extract_seq_composite(
                            arr, len, fields.as_slice(), *dt, &model, ctx)
                        {
                            bindings.insert(name.clone(), v);
                        }
                    }
                }
            }
        }
    }
    cached.solver.pop(1);
    EvalResult { satisfied, bindings }
}

/// Evaluate a single schema with optional pre-bound values, using the
/// `schemas` table to resolve user-defined types referenced inside the
/// schema body.
///
/// Sub-schema expansion: `task ∈ Task` doesn't create a Z3 const named
/// `task`. It recursively declares one Z3 const per leaf field of Task,
/// keyed under the dotted prefix `task.field` in the env. Field access
/// (parsed as `Identifier("task.field")` once we hit FieldAccess support)
/// resolves through the env directly. For v0.1 we have a flat
/// `Identifier(String)` so the parser must produce dotted names —
/// currently it only sees bare idents, but the Membership case below
/// expands them in the env regardless.
pub fn evaluate(
    schema: &SchemaDecl,
    given: &HashMap<String, Value>,
    schemas: &HashMap<String, SchemaDecl>,
    ctx: &'static Context,
    registry: &DatatypeRegistry,
) -> EvalResult {
    let solver = Solver::new(ctx);
    let mut env: HashMap<String, Var<'static>> = HashMap::new();

    // Pass 1: declare variables and add per-type constraints. User-defined
    // schema types expand into their leaf fields under a dotted prefix.
    // ..Passthrough imports declarations from the named claim too — any
    // variable name not already in env gets a fresh Z3 const, names that
    // collide with the parent are reused (names-match composition).
    for item in &schema.body {
        match item {
            BodyItem::Membership { name, type_name } => {
                declare_var(ctx, &solver, &mut env, name, type_name, schemas, Some(registry));
            }
            BodyItem::Passthrough(claim_name) => {
                if let Some(claim) = schemas.get(claim_name) {
                    for sub in &claim.body {
                        if let BodyItem::Membership { name, type_name } = sub {
                            if !env.contains_key(name) {
                                declare_var(ctx, &solver, &mut env, name, type_name, schemas, Some(registry));
                            }
                        }
                    }
                } else {
                    eprintln!("warning: ..{} references unknown claim", claim_name);
                }
            }
            BodyItem::ClaimCall { .. } => {
                // Declarations from the claim's body are added in pass 2
                // (where we have the inner env to bind into); no work here.
            }
            BodyItem::SubclaimDecl(_) => {
                // Subclaims contribute no constraints to the parent —
                // they're registered into the runtime's schemas table at
                // load time so other items can reference them.
            }
            // Bare-identifier names-match passthrough (see build_cache for
            // the rationale): a `Constraint(Identifier(name))` whose name
            // is a known claim/type is treated as `..ClaimName`. Adds any
            // of the claim's own variables that aren't already in env.
            BodyItem::Constraint(Expr::Identifier(name)) if schemas.contains_key(name) => {
                if let Some(claim) = schemas.get(name) {
                    for sub in &claim.body {
                        if let BodyItem::Membership { name, type_name } = sub {
                            if !env.contains_key(name) {
                                declare_var(ctx, &solver, &mut env, name, type_name, schemas, Some(registry));
                            }
                        }
                    }
                }
            }
            BodyItem::Constraint(_) => {}
        }
    }

    // Pass 1.5: pin literal-int vars from `given` + body equalities +
    // #seq length propagation. Quantifier ranges over those names then
    // unroll because translate_int yields literal IntVals.
    let seq_lens = collect_seq_lengths(&schema.body, given);
    let pinned   = collect_pinned_ints(&schema.body, given, &seq_lens);
    apply_pinned_ints(&mut env, &pinned);
    apply_seq_lengths(&mut env, &seq_lens, ctx);

    // Pass 2: translate body constraints and assert. Passthrough items
    // also contribute their included claim's constraints under the
    // current env. ClaimCall items translate their claim's body in a
    // fresh env where each mapping slot is pre-bound. Both passthrough
    // and ClaimCall recurse into nested claim composition (one helper
    // unifies all four entry shapes).
    let mut visited: HashSet<String> = HashSet::new();
    inline_body_items(&schema.body, &mut env, &solver, schemas, ctx, registry, &mut visited);

    // Pass 3: assert ground facts for each given binding. Names that
    // aren't declared in the schema are silently ignored (matches the
    // Python runtime's behavior).
    for (name, value) in given {
        let Some(var) = env.get(name) else { continue };
        match (var, value) {
            (Var::IntVar(v),  Value::Int(n))  => solver.assert(&v._eq(&Int::from_i64(ctx, *n))),
            (Var::BoolVar(v), Value::Bool(b)) => solver.assert(&v._eq(&Bool::from_bool(ctx, *b))),
            (Var::StrVar(v),  Value::Str(s))  => solver.assert(&v._eq(&Z3Str::from_str(ctx, s).expect("nul in str"))),
            // PinnedInt was already folded in via apply_pinned_ints from
            // this same given value — assertion is redundant. If values
            // disagree, force UNSAT.
            (Var::PinnedInt(v), Value::Int(n)) if *v == *n => {}
            (Var::PinnedInt(_), Value::Int(_)) => solver.assert(&Bool::from_bool(ctx, false)),
            _ => {
                if let Some(b) = assert_seq_given(var, value, ctx) {
                    solver.assert(&b);
                } else {
                    eprintln!("warning: type mismatch for given {:?}", name);
                }
            }
        }
    }

    let satisfied = matches!(solver.check(), SatResult::Sat);
    let mut bindings = HashMap::new();
    if satisfied {
        if let Some(model) = solver.get_model() {
            for (name, var) in env.iter() {
                match var {
                    Var::IntVar(i) => {
                        if let Some(val) = model.eval(i, true) {
                            if let Some(n) = val.as_i64() {
                                bindings.insert(name.clone(), Value::Int(n));
                            }
                        }
                    }
                    Var::BoolVar(b) => {
                        if let Some(val) = model.eval(b, true) {
                            if let Some(bv) = val.as_bool() {
                                bindings.insert(name.clone(), Value::Bool(bv));
                            }
                        }
                    }
                    Var::StrVar(s) => {
                        if let Some(val) = model.eval(s, true) {
                            if let Some(sv) = val.as_string() {
                                bindings.insert(name.clone(), Value::Str(sv));
                            }
                        }
                    }
                    Var::SeqVar { arr, len, elem } => {
                        if let Some(v) = extract_seq(arr, len, *elem, &model, ctx) {
                            bindings.insert(name.clone(), v);
                        }
                    }
                    Var::PinnedInt(v) => {
                        bindings.insert(name.clone(), Value::Int(*v));
                    }
                    Var::SetVar { .. } => {
                        // Z3 sets are characteristic functions over an
                        // (often infinite) element domain. We don't try
                        // to enumerate; bindings just omit set vars.
                    }
                    Var::DatatypeSeqVar { arr, len, dt, fields, .. } => {
                        if let Some(v) = extract_seq_composite(
                            arr, len, fields.as_slice(), *dt, &model, ctx)
                        {
                            bindings.insert(name.clone(), v);
                        }
                    }
                }
            }
        }
    }
    EvalResult { satisfied, bindings }
}

/// Pre-scan the schema body and `given` for variables that can be
/// pinned to a literal int *before* the solver runs:
///
///   - any `given` entry of value `Value::Int(n)` → `name → n`
///   - any body constraint of shape `name = literal_int_expr` (or
///     reverse) where the literal side resolves to a constant under
///     the names already pinned → `name → value`
///   - any body constraint of shape `name = #seq` where `#seq`'s
///     length itself reduces (e.g. via a sibling `#seq = N` constraint)
///     → `name → length` (length-propagation, mirrors Python's Pass 3)
///
/// Iterates to a fixed point so chains like `n = #s ∧ #s = 4 ∧ k = n - 1`
/// all resolve. The result is fed into `apply_pinned_ints` to upgrade
/// env entries to `Var::PinnedInt`, which lets `literal_range` unroll
/// quantifiers like `∀ i ∈ {0..n - 1}` even when `n` is symbolic.
fn collect_pinned_ints(
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
                            // Try as a pure-int expression over already-pinned
                            // names + literal Ints + #seq lengths.
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

/// Pure constant-folding evaluator over Int expressions. Honors PinnedInt
/// names, literal Ints, arithmetic, and `#seq` references whose lengths
/// are concrete in `seq_lengths`.
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
                BinOp::Div => if r == 0 { return None } else { l / r },
                _ => return None,
            })
        }
        _ => None,
    }
}

/// Pre-scan body for `#seq = literal_int` constraints. Mirrors Python's
/// "Pass 3" length propagation. The returned map is consumed by
/// `collect_pinned_ints` so e.g. `n = #s` resolves through it.
fn collect_seq_lengths(
    body: &[BodyItem],
    given: &HashMap<String, Value>,
) -> HashMap<String, i64> {
    let mut out = HashMap::new();
    // Seq lengths from `given` Seq values are exact.
    for (k, v) in given {
        let len = match v {
            Value::SeqInt(v)  => v.len() as i64,
            Value::SeqBool(v) => v.len() as i64,
            Value::SeqStr(v)  => v.len() as i64,
            _ => continue,
        };
        out.insert(k.clone(), len);
    }
    // From body: `#seq = N` (or `N = #seq`) where N is a literal Int,
    // or `seq = ⟨…⟩` (sequence literal pins length to its arity).
    for item in body {
        if let BodyItem::Constraint(Expr::Binary(BinOp::Eq, lhs, rhs)) = item {
            for (a, b) in [(lhs, rhs), (rhs, lhs)] {
                if let Expr::Cardinality(inner) = a.as_ref() {
                    if let Expr::Identifier(name) = inner.as_ref() {
                        if let Expr::Int(n) = b.as_ref() {
                            out.insert(name.clone(), *n);
                        }
                    }
                }
                // `seq_var = ⟨e1, e2, …⟩` pins #seq_var to items.len().
                if let (Expr::Identifier(name), Expr::SeqLit(items)) =
                    (a.as_ref(), b.as_ref())
                {
                    out.insert(name.clone(), items.len() as i64);
                }
            }
        }
    }
    out
}

/// Replace env entries for pinned names with `Var::PinnedInt(value)`.
/// The replacement is a no-op for names not in env (e.g. a `n = 5`
/// constraint where `n` was never declared with `n ∈ ...`).
fn apply_pinned_ints<'ctx>(
    env: &mut HashMap<String, Var<'ctx>>,
    pinned: &HashMap<String, i64>,
) {
    for (name, value) in pinned {
        if env.contains_key(name) {
            env.insert(name.clone(), Var::PinnedInt(*value));
        }
    }
}

/// Replace each `Var::SeqVar` / `Var::DatatypeSeqVar`'s symbolic `len`
/// with an `Int::from_i64` literal when `seq_lengths` knows the value.
/// Without this, `translate_int(Cardinality(seq))` returns the
/// solver-side free `len` symbol, so `literal_range` can't fold
/// `Range(0, #seq - 1)` to a concrete pair and the quantifier is
/// silently dropped.
///
/// Idempotent and safe to run after `apply_pinned_ints` (different
/// var kinds, no overlap).
fn apply_seq_lengths<'ctx>(
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
fn resolve_mapping<'ctx>(
    slot: &str,
    value: &Expr,
    ctx: &'ctx Context,
    env: &HashMap<String, Var<'ctx>>,
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
        Expr::Str(s)  => Z3Str::from_str(ctx, s).ok().map(Var::StrVar),
        _ => None,
    }
}

/// Resolve `Range(lo, hi)` to a `(lo, hi)` literal pair.
///
/// Both bounds are evaluated through `translate_int` (so identifiers
/// bound to `Var::PinnedInt` resolve to literal `IntVal`s and arithmetic
/// over them folds), then Z3 `simplify` reduces to a literal that
/// `as_i64` can extract. Returns None if either bound stays symbolic
/// (no PinnedInt for it) or the simplified form isn't a literal.
///
/// This is what enables `∀ i ∈ {0..n - 1}` when n is bound to a
/// concrete value via `n = #seq` length propagation, `n = 4`
/// pinning, or a `given` value.
fn literal_range<'ctx>(
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

/// Clone an env. Var derives Clone (Z3 ast types are reference-counted)
/// so we can shadow the bound variable in quantifier unrolling.
fn env_clone<'ctx>(env: &HashMap<String, Var<'ctx>>) -> HashMap<String, Var<'ctx>> {
    env.clone()
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
fn declare_var(
    ctx: &'static Context,
    solver: &Solver<'static>,
    env: &mut HashMap<String, Var<'static>>,
    prefix: &str,
    type_name: &str,
    schemas: &HashMap<String, SchemaDecl>,
    registry: Option<&DatatypeRegistry>,
) {
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
    if env.contains_key(prefix) { return; }
    match type_name {
        "Int" => {
            env.insert(prefix.to_string(), Var::IntVar(Int::new_const(ctx, prefix)));
        }
        "Nat" => {
            let v = Int::new_const(ctx, prefix);
            solver.assert(&v.ge(&Int::from_i64(ctx, 0)));
            env.insert(prefix.to_string(), Var::IntVar(v));
        }
        "Pos" => {
            let v = Int::new_const(ctx, prefix);
            solver.assert(&v.gt(&Int::from_i64(ctx, 0)));
            env.insert(prefix.to_string(), Var::IntVar(v));
        }
        "Bool" => {
            env.insert(prefix.to_string(), Var::BoolVar(Bool::new_const(ctx, prefix)));
        }
        "String" => {
            env.insert(prefix.to_string(), Var::StrVar(Z3Str::new_const(ctx, prefix)));
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
                    solver.assert(&len.ge(&Int::from_i64(ctx, 0)));
                    env.insert(prefix.to_string(), Var::SeqVar { arr, len, elem });
                }
                user_type if schemas.contains_key(user_type) => {
                    let Some(reg) = registry else {
                        eprintln!(
                            "warning: Seq({}) requires a DatatypeRegistry; \
                             skipping declaration of {}",
                            user_type, prefix
                        );
                        return;
                    };
                    let Some((dt, fields)) = get_or_build_datatype(user_type, ctx, schemas, reg) else {
                        return; // warning already emitted by get_or_build_datatype
                    };
                    let arr = Array::new_const(ctx, prefix, &Sort::int(ctx), &dt.sort);
                    let len = Int::new_const(ctx, format!("{}__len", prefix).as_str());
                    solver.assert(&len.ge(&Int::from_i64(ctx, 0)));
                    env.insert(prefix.to_string(), Var::DatatypeSeqVar {
                        arr, len,
                        type_name: user_type.to_string(),
                        dt,
                        fields,
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
                    return;
                }
            };
            let set = Set::new_const(ctx, prefix, &eltype);
            env.insert(prefix.to_string(), Var::SetVar { set, elem });
        }
        _ => {
            if let Some(schema) = schemas.get(type_name) {
                // Expand each membership in the sub-schema's body.
                for item in &schema.body {
                    if let BodyItem::Membership { name: field, type_name: ftype } = item {
                        let dotted = format!("{}.{}", prefix, field);
                        declare_var(ctx, solver, env, &dotted, ftype, schemas, registry);
                    }
                }
            } else {
                eprintln!("warning: unknown type {} for {}", type_name, prefix);
            }
        }
    }
}

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

fn translate_str<'ctx>(e: &Expr, ctx: &'ctx Context, env: &HashMap<String, Var<'ctx>>) -> Option<Z3Str<'ctx>> {
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
        _ => None,
    }
}

fn translate_int<'ctx>(e: &Expr, ctx: &'ctx Context, env: &HashMap<String, Var<'ctx>>) -> Option<Int<'ctx>> {
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
        Expr::Cardinality(inner) => {
            if let Expr::Identifier(name) = inner.as_ref() {
                if let Some(var) = env.get(name) {
                    if let Some((_, len, _)) = var.as_seq() {
                        return Some(len.clone());
                    }
                    if let Some((_, len, _, _, _)) = var.as_datatype_seq() {
                        return Some(len.clone());
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
        _ => None,
    }
}

/// Handle `seq_var = ⟨e1, e2, …⟩` (sequence-literal assignment).
///
/// Returns the conjunction `len == items.len() ∧ ∀i: arr[i] == translated(e_i)`
/// when `lhs` is an `Identifier(name)` resolving to a `Var::SeqVar` (primitive
/// element) or `Var::DatatypeSeqVar` (composite element), and `rhs` is an
/// `Expr::SeqLit(items)`. Returns `None` otherwise — caller then falls back
/// through the Bool/Int/Str equality paths.
///
/// **v1 limitation**: composite-element seq literals (`Seq(UserType)` on the
/// LHS) are not supported. Each item would need to be assembled into a
/// Datatype constructor application from the corresponding sub-schema fields
/// in env, which is fiddly enough to defer. We log a warning and return None
/// in that case so the equality is dropped (rather than mis-translated), and
/// callers know the constraint silently failed to apply.
fn translate_seq_lit_eq<'ctx>(
    lhs: &Expr,
    rhs: &Expr,
    ctx: &'ctx Context,
    env: &HashMap<String, Var<'ctx>>,
) -> Option<Bool<'ctx>> {
    let items = match rhs {
        Expr::SeqLit(items) => items,
        _ => return None,
    };
    let name = match lhs {
        Expr::Identifier(n) => n,
        _ => return None,
    };
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
                    let v = translate_bool(item, ctx, env)?;
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

    // Composite-element Seq: each item must be a bare Identifier referring to
    // flat sub-schema fields (e.g. `ball_rect`). Walk the Datatype's FieldKind
    // list and assemble a constructor application from `env["ident.field"]`
    // lookups, recursing for nested composites (e.g. `ball_rect.color.r`).
    if let Some((arr, len, _, dt, fields)) = var.as_datatype_seq() {
        let n = items.len() as i64;
        let mut clauses: Vec<Bool<'ctx>> = Vec::with_capacity(items.len() + 1);
        clauses.push(len._eq(&Int::from_i64(ctx, n)));
        for (i, item) in items.iter().enumerate() {
            // Each composite item must be an Identifier whose flat-expanded
            // sub-schema fields live in env under `ident.field` keys.
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

fn translate_bool<'ctx>(e: &Expr, ctx: &'ctx Context, env: &HashMap<String, Var<'ctx>>) -> Option<Bool<'ctx>> {
    match e {
        Expr::Bool(b) => Some(Bool::from_bool(ctx, *b)),
        Expr::Identifier(name) => env.get(name).and_then(|v| v.as_bool().cloned()),
        Expr::Not(inner) => Some(translate_bool(inner, ctx, env)?.not()),

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
                            let x = translate_bool(lhs, ctx, env)?;
                            Some(set.member(&x))
                        }
                        SeqElem::Str => {
                            let x = translate_str(lhs, ctx, env)?;
                            Some(set.member(&x))
                        }
                    };
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
                if let Some(b) = translate_bool(&eq, ctx, env) {
                    clauses.push(b);
                }
            }
            if clauses.is_empty() { return Some(Bool::from_bool(ctx, false)); }
            let refs: Vec<&Bool> = clauses.iter().collect();
            Some(Bool::or(ctx, &refs))
        }

        // `∀ i ∈ {lo..hi} : body` / `∃ …`: unroll when the range
        // resolves to a literal pair (after PinnedInt substitution).
        Expr::Forall(var, range, body) | Expr::Exists(var, range, body) => {
            let (lo, hi) = literal_range(range, ctx, env)?;
            let mut clauses: Vec<Bool> = Vec::new();
            for i in lo..=hi {
                let mut env2 = env_clone(env);
                env2.insert(var.clone(), Var::IntVar(Int::from_i64(ctx, i)));
                if let Some(b) = translate_bool(body, ctx, &env2) {
                    clauses.push(b);
                }
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
                let l = translate_bool(lhs, ctx, env)?;
                let r = translate_bool(rhs, ctx, env)?;
                Some(Bool::and(ctx, &[&l, &r]))
            }
            BinOp::Or => {
                let l = translate_bool(lhs, ctx, env)?;
                let r = translate_bool(rhs, ctx, env)?;
                Some(Bool::or(ctx, &[&l, &r]))
            }
            BinOp::Implies => {
                let l = translate_bool(lhs, ctx, env)?;
                let r = translate_bool(rhs, ctx, env)?;
                Some(l.implies(&r))
            }
            // Eq/Neq work over Bool, Int, or String. Try in that order.
            BinOp::Eq | BinOp::Neq => {
                // First: handle `seq_var = ⟨e1, e2, …⟩` (sequence literal
                // assignment). This pins both length and per-element values
                // and lives outside the Bool/Int/Str scalar paths because
                // it produces a conjunction over the elements rather than
                // a single _eq.
                if let Some(b) = translate_seq_lit_eq(lhs, rhs, ctx, env) {
                    return Some(match op {
                        BinOp::Eq  => b,
                        BinOp::Neq => b.not(),
                        _ => unreachable!(),
                    });
                }
                if let Some(b) = translate_seq_lit_eq(rhs, lhs, ctx, env) {
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
                    (translate_bool(lhs, ctx, env), translate_bool(rhs, ctx, env))
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
                let l = translate_str(lhs, ctx, env)?;
                let r = translate_str(rhs, ctx, env)?;
                Some(match op {
                    BinOp::Eq  => l._eq(&r),
                    BinOp::Neq => l._eq(&r).not(),
                    _ => unreachable!(),
                })
            }
            // Numeric-only comparisons.
            BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
                let l = translate_int(lhs, ctx, env)?;
                let r = translate_int(rhs, ctx, env)?;
                Some(match op {
                    BinOp::Lt => l.lt(&r),
                    BinOp::Le => l.le(&r),
                    BinOp::Gt => l.gt(&r),
                    BinOp::Ge => l.ge(&r),
                    _ => unreachable!(),
                })
            }
            _ => None,
        }
        _ => None,
    }
}
