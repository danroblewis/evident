//! AST → Z3 expressions. v0.1: Int/Bool only, flat declarations,
//! arithmetic + boolean + comparisons.

use crate::ast::*;
use std::collections::HashMap;
use z3::ast::{Ast, Bool, Int, String as Z3Str};
use z3::{Context, SatResult, Solver};

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
}

/// Z3 binding for a declared variable. Keep a typed handle so we know
/// which AST kind to translate against.
#[derive(Clone)]
enum Var<'ctx> {
    IntVar(Int<'ctx>),
    BoolVar(Bool<'ctx>),
    StrVar(Z3Str<'ctx>),
}

impl<'ctx> Var<'ctx> {
    fn as_int(&self) -> Option<&Int<'ctx>> {
        match self { Var::IntVar(i) => Some(i), _ => None }
    }
    fn as_bool(&self) -> Option<&Bool<'ctx>> {
        match self { Var::BoolVar(b) => Some(b), _ => None }
    }
    fn as_str(&self) -> Option<&Z3Str<'ctx>> {
        match self { Var::StrVar(s) => Some(s), _ => None }
    }
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
) -> EvalResult {
    let cfg = z3::Config::new();
    let ctx = Context::new(&cfg);
    let solver = Solver::new(&ctx);
    let mut env: HashMap<String, Var> = HashMap::new();

    // Pass 1: declare variables and add per-type constraints. User-defined
    // schema types expand into their leaf fields under a dotted prefix.
    // ..Passthrough imports declarations from the named claim too — any
    // variable name not already in env gets a fresh Z3 const, names that
    // collide with the parent are reused (names-match composition).
    for item in &schema.body {
        match item {
            BodyItem::Membership { name, type_name } => {
                declare_var(&ctx, &solver, &mut env, name, type_name, schemas);
            }
            BodyItem::Passthrough(claim_name) => {
                if let Some(claim) = schemas.get(claim_name) {
                    for sub in &claim.body {
                        if let BodyItem::Membership { name, type_name } = sub {
                            if !env.contains_key(name) {
                                declare_var(&ctx, &solver, &mut env, name, type_name, schemas);
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

    // Pass 2: translate body constraints and assert. Passthrough items
    // also contribute their included claim's constraints under the
    // current env. ClaimCall items translate their claim's body in a
    // fresh env where each mapping slot is pre-bound.
    for item in &schema.body {
        match item {
            BodyItem::Constraint(e) => {
                if let Some(b) = translate_bool(e, &ctx, &env) {
                    solver.assert(&b);
                } else {
                    eprintln!("warning: dropped constraint that didn't translate to Bool");
                }
            }
            BodyItem::Passthrough(claim_name) => {
                if let Some(claim) = schemas.get(claim_name) {
                    for sub in &claim.body {
                        if let BodyItem::Constraint(e) = sub {
                            if let Some(b) = translate_bool(e, &ctx, &env) {
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
                    let bound = resolve_mapping(&m.slot, &m.value, &ctx, &env);
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
                            declare_var(&ctx, &solver, &mut inner, vname, type_name, schemas);
                        }
                    }
                }
                // Translate the claim's constraints in the inner env.
                for sub in &claim.body {
                    if let BodyItem::Constraint(e) = sub {
                        if let Some(b) = translate_bool(e, &ctx, &inner) {
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
            (Var::IntVar(v),  Value::Int(n))  => solver.assert(&v._eq(&Int::from_i64(&ctx, *n))),
            (Var::BoolVar(v), Value::Bool(b)) => solver.assert(&v._eq(&Bool::from_bool(&ctx, *b))),
            (Var::StrVar(v),  Value::Str(s))  => solver.assert(&v._eq(&Z3Str::from_str(&ctx, s).expect("nul in str"))),
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
                }
            }
        }
    }
    EvalResult { satisfied, bindings }
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

/// Resolve `Range(Int, Int)` to a `(lo, hi)` pair. Returns None if
/// either bound isn't a literal Int (we don't support symbolic ∀ bounds
/// yet — would need the Python length-propagation shim).
fn literal_range(e: &Expr) -> Option<(i64, i64)> {
    if let Expr::Range(lo, hi) = e {
        if let (Expr::Int(l), Expr::Int(h)) = (lo.as_ref(), hi.as_ref()) {
            return Some((*l, *h));
        }
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
fn declare_var<'ctx>(
    ctx: &'ctx Context,
    solver: &Solver<'ctx>,
    env: &mut HashMap<String, Var<'ctx>>,
    prefix: &str,
    type_name: &str,
    schemas: &HashMap<String, SchemaDecl>,
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
        _ => {
            if let Some(schema) = schemas.get(type_name) {
                // Expand each membership in the sub-schema's body.
                for item in &schema.body {
                    if let BodyItem::Membership { name: field, type_name: ftype } = item {
                        let dotted = format!("{}.{}", prefix, field);
                        declare_var(ctx, solver, env, &dotted, ftype, schemas);
                    }
                }
            } else {
                eprintln!("warning: unknown type {} for {}", type_name, prefix);
            }
        }
    }
}

fn translate_str<'ctx>(e: &Expr, ctx: &'ctx Context, env: &HashMap<String, Var<'ctx>>) -> Option<Z3Str<'ctx>> {
    match e {
        Expr::Str(s) => Z3Str::from_str(ctx, s).ok(),
        Expr::Identifier(name) => env.get(name).and_then(|v| v.as_str().cloned()),
        _ => None,
    }
}

fn translate_int<'ctx>(e: &Expr, ctx: &'ctx Context, env: &HashMap<String, Var<'ctx>>) -> Option<Int<'ctx>> {
    match e {
        Expr::Int(n) => Some(Int::from_i64(ctx, *n)),
        Expr::Identifier(name) => env.get(name).and_then(|v| v.as_int().cloned()),
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
        _ => None,
    }
}

fn translate_bool<'ctx>(e: &Expr, ctx: &'ctx Context, env: &HashMap<String, Var<'ctx>>) -> Option<Bool<'ctx>> {
    match e {
        Expr::Bool(b) => Some(Bool::from_bool(ctx, *b)),
        Expr::Identifier(name) => env.get(name).and_then(|v| v.as_bool().cloned()),
        Expr::Not(inner) => Some(translate_bool(inner, ctx, env)?.not()),

        // `x ∈ {a, b, c}` → x = a ∨ x = b ∨ x = c.
        Expr::InExpr(lhs, rhs) => {
            let items = match rhs.as_ref() {
                Expr::SetLit(items) => items.clone(),
                _ => return None,    // only SetLit RHS for v0.1
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

        // `∀ i ∈ {lo..hi} : body` / `∃ …`: unroll when the range is a
        // pair of integer literals.
        Expr::Forall(var, range, body) | Expr::Exists(var, range, body) => {
            let (lo, hi) = literal_range(range)?;
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
