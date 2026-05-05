//! AST → Z3 expressions. v0.1: Int/Bool only, flat declarations,
//! arithmetic + boolean + comparisons.

use crate::ast::*;
use std::cell::RefCell;
use std::collections::HashMap;
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
pub type DatatypeRegistry = RefCell<HashMap<String, &'static DatatypeSort<'static>>>;

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
        /// `dt.variants[0].accessors`. Each entry is `(field_name,
        /// primitive_type_name)` where the type name routes the
        /// extracted `Dynamic` through the right `as_int` / `as_bool`
        /// / `as_string` extractor.
        fields: Vec<(String, String)>,
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
                                         &[(String, String)])> {
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
/// items, mapping each to a primitive `DatatypeAccessor::Sort(...)`.
///
/// v1 limitation: only flat user structs whose fields are Int/Nat/Pos/
/// Bool/String. Nested user types (recursive Datatype building) and
/// `Seq`/`Set` fields are out of scope — returns None and logs a
/// warning if any field doesn't fit.
///
/// The returned reference has a `'static` lifetime: the runtime
/// already leaks its `Context`, so leaking the per-type `DatatypeSort`
/// (which borrows from that Context) is consistent. See
/// `EvidentRuntime::new` for why the Context is leaked.
///
/// Also returns the parallel `(field_name, type_name)` list so the
/// caller can build a `DatatypeSeqVar` without re-walking the body.
fn get_or_build_datatype(
    type_name: &str,
    ctx: &'static Context,
    schemas: &HashMap<String, SchemaDecl>,
    registry: &DatatypeRegistry,
) -> Option<(&'static DatatypeSort<'static>, Vec<(String, String)>)> {
    let schema = schemas.get(type_name)?;
    let mut fields: Vec<(String, String)> = Vec::new();
    for item in &schema.body {
        if let BodyItem::Membership { name, type_name: ftype } = item {
            match ftype.as_str() {
                "Int" | "Nat" | "Pos" | "Bool" | "String" => {
                    fields.push((name.clone(), ftype.clone()));
                }
                _ => {
                    eprintln!(
                        "warning: unsupported field type {} in Datatype for {}; \
                         only Int/Nat/Pos/Bool/String fields are supported \
                         in Seq(UserType) elements (v1)",
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
    if let Some(dt) = registry.borrow().get(type_name) {
        return Some((*dt, fields));
    }
    let mut field_sorts: Vec<(String, Sort<'static>)> = Vec::with_capacity(fields.len());
    for (fname, ftype) in &fields {
        let sort = match ftype.as_str() {
            "Int" | "Nat" | "Pos" => Sort::int(ctx),
            "Bool" => Sort::bool(ctx),
            "String" => Sort::string(ctx),
            _ => unreachable!(),
        };
        field_sorts.push((fname.clone(), sort));
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
    registry.borrow_mut().insert(type_name.to_string(), leaked);
    Some((leaked, fields))
}

/// Read a `Seq(UserType)` value out of the model: read the length,
/// then for each `i ∈ 0..length` select the array element (a
/// Datatype value), walk the Datatype's accessors to extract each
/// field's primitive value, and assemble into a `HashMap`. Push each
/// element map into a `Vec` and wrap in `Value::SeqComposite`.
fn extract_seq_composite<'ctx>(
    arr: &Array<'ctx>,
    len: &Int<'ctx>,
    fields: &[(String, String)],
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
        let mut field_map: HashMap<String, Value> = HashMap::new();
        for (fi, (fname, ftype)) in fields.iter().enumerate() {
            if fi >= dt.variants[0].accessors.len() { break; }
            let accessor = &dt.variants[0].accessors[fi];
            let raw = accessor.apply(&[&elem]);
            let value = match ftype.as_str() {
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
            };
            field_map.insert(fname.clone(), value);
        }
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

    for item in &schema.body {
        match item {
            BodyItem::Constraint(e) => {
                if let Some(b) = translate_bool(e, ctx, &env) { solver.assert(&b); }
            }
            BodyItem::Passthrough(claim_name) => {
                if let Some(claim) = schemas.get(claim_name) {
                    for sub in &claim.body {
                        if let BodyItem::Constraint(e) = sub {
                            if let Some(b) = translate_bool(e, ctx, &env) { solver.assert(&b); }
                        }
                    }
                }
            }
            BodyItem::ClaimCall { name, mappings } => {
                let Some(claim) = schemas.get(name) else { continue };
                let mut inner = env.clone();
                for m in mappings {
                    for (k, v) in resolve_mapping(&m.slot, &m.value, ctx, &env) {
                        inner.insert(k, v);
                    }
                }
                for sub in &claim.body {
                    if let BodyItem::Membership { name: vname, type_name } = sub {
                        let prefix = format!("{}.", vname);
                        let bound = inner.contains_key(vname)
                            || inner.keys().any(|k| k.starts_with(&prefix));
                        if !bound {
                            declare_var(ctx, &solver, &mut inner, vname, type_name, schemas, Some(registry));
                        }
                    }
                }
                for sub in &claim.body {
                    if let BodyItem::Constraint(e) = sub {
                        if let Some(b) = translate_bool(e, ctx, &inner) { solver.assert(&b); }
                    }
                }
            }
            _ => {}
        }
    }

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
            _ => eprintln!("warning: type mismatch for given {:?}", name),
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
            _ => eprintln!("warning: type mismatch for given {:?}", name),
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
            BodyItem::Constraint(_) => {}
        }
    }

    // Pass 1.5: pin literal-int vars from `given` + body equalities +
    // #seq length propagation. Quantifier ranges over those names then
    // unroll because translate_int yields literal IntVals.
    let seq_lens = collect_seq_lengths(&schema.body, given);
    let pinned   = collect_pinned_ints(&schema.body, given, &seq_lens);
    apply_pinned_ints(&mut env, &pinned);

    // Pass 2: translate body constraints and assert. Passthrough items
    // also contribute their included claim's constraints under the
    // current env. ClaimCall items translate their claim's body in a
    // fresh env where each mapping slot is pre-bound.
    for item in &schema.body {
        match item {
            BodyItem::Constraint(e) => {
                if let Some(b) = translate_bool(e, ctx, &env) {
                    solver.assert(&b);
                } else {
                    eprintln!("warning: dropped constraint that didn't translate to Bool");
                }
            }
            BodyItem::Passthrough(claim_name) => {
                if let Some(claim) = schemas.get(claim_name) {
                    for sub in &claim.body {
                        if let BodyItem::Constraint(e) = sub {
                            if let Some(b) = translate_bool(e, ctx, &env) {
                                solver.assert(&b);
                            }
                        }
                    }
                }
            }
            BodyItem::ClaimCall { name, mappings } => {
                let Some(claim) = schemas.get(name) else {
                    eprintln!("warning: ClaimCall to unknown claim {}", name);
                    continue;
                };
                // Build the inner env: start from the parent env, then
                // bind each mapping slot. For now we only handle leaf
                // mappings — sub-schema mapping (`state mapsto state.player`)
                // is deferred (would need to recursively expand).
                let mut inner = env.clone();
                for m in mappings {
                    let bound = resolve_mapping(&m.slot, &m.value, ctx, &env);
                    if bound.is_empty() {
                        eprintln!("warning: mapping value didn't resolve: {:?}", m.value);
                    }
                    for (k, v) in bound {
                        inner.insert(k, v);
                    }
                }
                // Declare any of the claim's own variables that weren't
                // mapped (fresh consts — these are the claim's "internal"
                // parameters, like AxisPhysics's `intended`/`target`).
                // A slot counts as "already bound" if either the bare
                // name is in env (leaf mapping) or any `slot.*` key is
                // (sub-schema mapping like `state mapsto state.player`).
                for sub in &claim.body {
                    if let BodyItem::Membership { name: vname, type_name } = sub {
                        let slot_prefix = format!("{}.", vname);
                        let already_bound = inner.contains_key(vname)
                            || inner.keys().any(|k| k.starts_with(&slot_prefix));
                        if !already_bound {
                            declare_var(ctx, &solver, &mut inner, vname, type_name, schemas, Some(registry));
                        }
                    }
                }
                // Translate the claim's constraints in the inner env.
                for sub in &claim.body {
                    if let BodyItem::Constraint(e) = sub {
                        if let Some(b) = translate_bool(e, ctx, &inner) {
                            solver.assert(&b);
                        }
                    }
                }
            }
            BodyItem::SubclaimDecl(_) => {}
            BodyItem::Membership { .. } => {}
        }
    }

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
            _ => eprintln!("warning: type mismatch for given {:?}", name),
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
    // From body: `#seq = N` (or `N = #seq`) where N is a literal Int.
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

/// Resolve `Field(Index(seq_var_ident, idx_expr), field_name)` against
/// a `DatatypeSeqVar` in the env. Returns the raw `Dynamic` produced by
/// applying the matching field accessor; the caller routes it through
/// `as_int` / `as_bool` / `as_string` based on the field's primitive
/// type. Returns `(dynamic, primitive_type_name)` so callers can do
/// the right cast.
fn resolve_seq_field<'ctx>(
    field_expr: &Expr,
    ctx: &'ctx Context,
    env: &HashMap<String, Var<'ctx>>,
) -> Option<(z3::ast::Dynamic<'ctx>, String)> {
    // Expected shape: Field(Index(Identifier(seq_name), idx_expr), field_name).
    let Expr::Field(receiver, field_name) = field_expr else { return None };
    let Expr::Index(seq_expr, idx_expr) = receiver.as_ref() else { return None };
    let Expr::Identifier(seq_name) = seq_expr.as_ref() else { return None };
    let var = env.get(seq_name)?;
    let (arr, _, _, dt, fields) = var.as_datatype_seq()?;
    let field_idx = fields.iter().position(|(n, _)| n == field_name)?;
    if field_idx >= dt.variants[0].accessors.len() { return None; }
    let i = translate_int(idx_expr, ctx, env)?;
    let elem_dyn = arr.select(&i);
    let elem = elem_dyn.as_datatype()?;
    let raw = dt.variants[0].accessors[field_idx].apply(&[&elem]);
    let ftype = fields[field_idx].1.clone();
    Some((raw, ftype))
}

fn translate_str<'ctx>(e: &Expr, ctx: &'ctx Context, env: &HashMap<String, Var<'ctx>>) -> Option<Z3Str<'ctx>> {
    match e {
        Expr::Str(s) => Z3Str::from_str(ctx, s).ok(),
        Expr::Identifier(name) => env.get(name).and_then(|v| v.as_str().cloned()),
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
        // `#seq` → the seq's length variable.
        Expr::Cardinality(inner) => {
            if let Expr::Identifier(name) = inner.as_ref() {
                if let Some((_, len, _)) = env.get(name).and_then(|v| v.as_seq()) {
                    return Some(len.clone());
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
