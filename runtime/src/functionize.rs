//! Function-izer: extract substitution chains for function-shaped
//! components and evaluate them natively (skip Z3).
//!
//! This is the second half of the compile-claims-to-functions effort.
//! `decompose` + `classify_components` upstream identify which components
//! are functional; this module extracts the actual computation and
//! makes it usable without going through Z3.
//!
//! v1 scope: pure Evident-AST walk of the schema body, looking for
//! explicit `var = expr` equalities defining the component's variables.
//! No Z3 tactic interaction (no `solve-eqs` diff), no native code
//! generation. The output is a `SubstitutionChain` data structure plus
//! an interpreter that evaluates it against a given binding map.
//!
//! For more complex cases (substitutions that emerge from constraint
//! algebra rather than direct equalities), v2 would add a `solve-eqs`
//! pass and diff its output against the original — see
//! `docs/design/compile-claims-to-functions.md` ("The pipeline").

use crate::ast::{BinOp, BodyItem, Expr, SchemaDecl};
use crate::decompose::Component;
use crate::translate::Value;
use std::collections::{HashMap, HashSet};

/// One step in a substitution chain: `var = expr`. The expression
/// references variables that are either inputs (in `given`) or
/// earlier substitutions in the chain.
#[derive(Debug, Clone)]
pub struct Substitution {
    pub var:  String,
    pub expr: Expr,
}

/// A chain of substitutions ordered so each `expr` only references
/// variables defined earlier in the chain (or inputs).
///
/// `checks` are equalities that must hold but don't *define* a new
/// variable — typically because both sides reference variables
/// already in scope (e.g. given-pinned vars). The evaluator
/// computes each check and confirms equality; if any fails the
/// claim is UNSAT under these inputs.
#[derive(Debug, Clone, Default)]
pub struct SubstitutionChain {
    pub steps:  Vec<Substitution>,
    pub checks: Vec<(Expr, Expr)>,
}

/// Try to extract a substitution chain for the given component from
/// the schema body. Returns `Some` if every variable in the component
/// has a directly-stated defining equation in the schema body
/// (`var = expr` or `expr = var` where the other side doesn't
/// reference `var`); returns `None` if some variable doesn't.
///
/// For component variables defined via more complex constraints (not
/// a single equality), we can't extract them this way — those need
/// the `solve-eqs` diff approach.
pub fn extract_chain(schema: &SchemaDecl, component: &Component) -> Option<SubstitutionChain> {
    extract_chain_with_enums(schema, component, &|_| false)
}

/// `extract_chain` variant that takes an enum-type predicate, used
/// when the caller knows about enum types and wants to allow
/// enum-typed Memberships through the gate.
pub fn extract_chain_with_enums(
    schema: &SchemaDecl,
    component: &Component,
    is_enum: &dyn Fn(&str) -> bool,
) -> Option<SubstitutionChain> {
    extract_chain_full(schema, component, is_enum, &|_| false)
}

/// `extract_chain` with full predicate support — enums + user-record
/// types. Matches the gate-side `is_pure_assignment_body_full`.
pub fn extract_chain_full(
    schema: &SchemaDecl,
    component: &Component,
    is_enum: &dyn Fn(&str) -> bool,
    is_simple_record: &dyn Fn(&str) -> bool,
) -> Option<SubstitutionChain> {
    extract_chain_xl(schema, component, is_enum, is_simple_record,
                     &|_| false, &|_| None)
}

/// `extract_chain_xl` — full predicate set including Passthrough.
///
/// `passthrough_body(claim_name)` returns the body items of the
/// referenced claim if it should be inlined. The runtime resolver
/// consults `self.schemas`. None means "don't inline this name."
pub fn extract_chain_xl(
    schema: &SchemaDecl,
    component: &Component,
    is_enum: &dyn Fn(&str) -> bool,
    is_simple_record: &dyn Fn(&str) -> bool,
    is_pure_passthrough: &dyn Fn(&str) -> bool,
    passthrough_body: &dyn Fn(&str) -> Option<Vec<BodyItem>>,
) -> Option<SubstitutionChain> {
    if !is_pure_assignment_body_xl(schema, is_enum, is_simple_record, is_pure_passthrough) {
        return None;
    }
    // Collect this schema's body + each Passthrough'd body. The
    // referenced body's Memberships also count as declarations in
    // the current frame; its equality Constraints are additional
    // substitution candidates for our component.
    let mut all_body: Vec<BodyItem> = schema.body.clone();
    let mut to_walk: Vec<String> = schema.body.iter().filter_map(|i| match i {
        BodyItem::Passthrough(n) => Some(n.clone()), _ => None,
    }).collect();
    let mut walked: HashSet<String> = HashSet::new();
    while let Some(name) = to_walk.pop() {
        if !walked.insert(name.clone()) { continue; }
        let Some(body) = passthrough_body(&name) else { continue };
        for item in &body {
            if let BodyItem::Passthrough(n) = item { to_walk.push(n.clone()); }
        }
        all_body.extend(body);
    }
    // Unroll ∀-over-Range constraints into flat copies. This must
    // happen AFTER Passthrough flattening (so we catch ∀s in
    // inlined bodies too) but BEFORE substitution extraction.
    all_body = expand_foralls(all_body);
    let target: HashSet<&str> = component.vars.iter().map(|s| s.as_str()).collect();

    // Collect candidate substitutions: every `var = expr` or `expr = var`
    // where `var` is in our component and the other side doesn't
    // reference `var` itself.
    //
    // Equalities that DON'T define a target var (e.g., both sides
    // reference given-pinned vars) become consistency checks — the
    // body says they must be equal, so the native evaluator must
    // verify that at runtime against the given values.
    let mut candidates: HashMap<String, Expr> = HashMap::new();
    let mut checks: Vec<(Expr, Expr)> = Vec::new();
    for item in body_constraints(&all_body) {
        let Expr::Binary(BinOp::Eq, lhs, rhs) = item else { continue };
        // Try LHS as the defined var.
        if let Expr::Identifier(name) = lhs.as_ref() {
            if target.contains(name.as_str())
                && !candidates.contains_key(name)
                && !mentions(rhs.as_ref(), name)
            {
                candidates.insert(name.clone(), (**rhs).clone());
                continue;
            }
        }
        // Try RHS as the defined var.
        if let Expr::Identifier(name) = rhs.as_ref() {
            if target.contains(name.as_str())
                && !candidates.contains_key(name)
                && !mentions(lhs.as_ref(), name)
            {
                candidates.insert(name.clone(), (**lhs).clone());
                continue;
            }
        }
        // Neither side was a fresh substitution target. Record as a
        // consistency check — evaluator verifies lhs == rhs at runtime.
        checks.push(((**lhs).clone(), (**rhs).clone()));
    }
    // Every variable in the component must have a substitution.
    if component.vars.iter().any(|v| !candidates.contains_key(v)) {
        return None;
    }
    // Topo-sort: each step's expr may reference earlier-defined vars
    // plus inputs. A var depends on another iff its expr mentions it.
    let mut in_deg: HashMap<&str, usize> = component.vars.iter()
        .map(|v| (v.as_str(), 0)).collect();
    let mut reverse: HashMap<&str, Vec<&str>> = HashMap::new();
    for v in &component.vars {
        let Some(expr) = candidates.get(v) else { continue };
        for other in &component.vars {
            if other == v { continue; }
            if mentions(expr, other) {
                *in_deg.get_mut(v.as_str()).unwrap() += 1;
                reverse.entry(other.as_str()).or_default().push(v.as_str());
            }
        }
    }
    let mut ready: Vec<&str> = in_deg.iter()
        .filter(|(_, &d)| d == 0).map(|(&n, _)| n).collect();
    ready.sort_unstable();    // stable order
    let mut order: Vec<&str> = Vec::with_capacity(component.vars.len());
    while let Some(n) = ready.pop() {
        order.push(n);
        if let Some(succs) = reverse.get(n) {
            for &m in succs {
                let d = in_deg.get_mut(m).unwrap();
                *d -= 1;
                if *d == 0 { ready.push(m); }
            }
        }
        ready.sort_unstable();
    }
    if order.len() != component.vars.len() {
        return None;  // cycle — shouldn't happen, but guard.
    }
    let steps = order.into_iter().map(|v| Substitution {
        var:  v.to_string(),
        expr: candidates.remove(v).unwrap(),
    }).collect();
    Some(SubstitutionChain { steps, checks })
}

/// Walk all `BodyItem::Constraint` Exprs at the top level of the
/// schema body. v1 doesn't recurse into Passthrough / ClaimCall;
/// those would need additional substitution flow.
fn body_constraints(body: &[BodyItem]) -> impl Iterator<Item = &Expr> {
    body.iter().filter_map(|item| match item {
        BodyItem::Constraint(e) => Some(e),
        _ => None,
    })
}

/// Classify a non-Eq Constraint expression. Returns None when the
/// constraint is acceptable (after unrolling), or Some(reason) when
/// it's a hard refusal.
fn constraint_kind(e: &Expr) -> Option<String> {
    match e {
        Expr::Binary(BinOp::Eq, _, _) => None,  // pure equality — fine
        Expr::Forall(_, _, _)  => Some("Forall (non-static bounds)".into()),
        Expr::Exists(_, _, _)  => Some("Exists".into()),
        Expr::Binary(op, _, _) => Some(format!("non-Eq Binary op {:?}", op)),
        Expr::Ternary(_, _, _) => Some("top-level Ternary".into()),
        Expr::Match(_, _)      => Some("top-level Match".into()),
        Expr::Identifier(_)    => Some("bare-identifier constraint".into()),
        Expr::Call(name, _)    => Some(format!("body Call {}", name)),
        _                      => Some("non-Eq Constraint".into()),
    }
}

/// Substitute every `Expr::Identifier(name)` in `e` with `value`.
/// Returns a new Expr. Used to unroll ∀-bound variables.
pub fn substitute(e: &Expr, name: &str, value: &Expr) -> Expr {
    match e {
        Expr::Identifier(s) if s == name => value.clone(),
        Expr::Identifier(_) | Expr::Int(_) | Expr::Real(_)
            | Expr::Bool(_) | Expr::Str(_) => e.clone(),
        Expr::Binary(op, l, r) => Expr::Binary(op.clone(),
            Box::new(substitute(l, name, value)),
            Box::new(substitute(r, name, value))),
        Expr::Not(x) => Expr::Not(Box::new(substitute(x, name, value))),
        Expr::Ternary(c, a, b) => Expr::Ternary(
            Box::new(substitute(c, name, value)),
            Box::new(substitute(a, name, value)),
            Box::new(substitute(b, name, value))),
        Expr::Call(n, args) => Expr::Call(n.clone(),
            args.iter().map(|a| substitute(a, name, value)).collect()),
        Expr::Field(x, f) => Expr::Field(Box::new(substitute(x, name, value)), f.clone()),
        Expr::Index(s, i) => Expr::Index(
            Box::new(substitute(s, name, value)),
            Box::new(substitute(i, name, value))),
        Expr::Cardinality(x) => Expr::Cardinality(Box::new(substitute(x, name, value))),
        Expr::SeqLit(items) => Expr::SeqLit(items.iter()
            .map(|a| substitute(a, name, value)).collect()),
        Expr::SetLit(items) => Expr::SetLit(items.iter()
            .map(|a| substitute(a, name, value)).collect()),
        Expr::Tuple(items) => Expr::Tuple(items.iter()
            .map(|a| substitute(a, name, value)).collect()),
        Expr::InExpr(a, b) => Expr::InExpr(
            Box::new(substitute(a, name, value)),
            Box::new(substitute(b, name, value))),
        Expr::Range(a, b) => Expr::Range(
            Box::new(substitute(a, name, value)),
            Box::new(substitute(b, name, value))),
        Expr::Forall(vars, range, body) => {
            // Don't substitute through a binder that shadows the name.
            if vars.iter().any(|v| v == name) { e.clone() }
            else {
                Expr::Forall(vars.clone(),
                    Box::new(substitute(range, name, value)),
                    Box::new(substitute(body, name, value)))
            }
        }
        Expr::Exists(vars, range, body) => {
            if vars.iter().any(|v| v == name) { e.clone() }
            else {
                Expr::Exists(vars.clone(),
                    Box::new(substitute(range, name, value)),
                    Box::new(substitute(body, name, value)))
            }
        }
        Expr::Match(scrut, arms) => Expr::Match(
            Box::new(substitute(scrut, name, value)),
            arms.iter().map(|arm| crate::ast::MatchArm {
                pattern: arm.pattern.clone(),
                body: Box::new(substitute(&arm.body, name, value)),
            }).collect()),
        Expr::Matches(scrut, pat) => Expr::Matches(
            Box::new(substitute(scrut, name, value)),
            pat.clone()),
    }
}

/// Try to evaluate an `Expr::Int` literal or simple arithmetic on
/// literals to a concrete i64. Used to resolve `∀ i ∈ {lo..hi}` bounds.
fn try_eval_const_int(e: &Expr) -> Option<i64> {
    match e {
        Expr::Int(n) => Some(*n),
        Expr::Binary(BinOp::Add, l, r) =>
            Some(try_eval_const_int(l)? + try_eval_const_int(r)?),
        Expr::Binary(BinOp::Sub, l, r) =>
            Some(try_eval_const_int(l)? - try_eval_const_int(r)?),
        Expr::Binary(BinOp::Mul, l, r) =>
            Some(try_eval_const_int(l)? * try_eval_const_int(r)?),
        _ => None,
    }
}

/// Unroll a body item that is `∀ var ∈ {lo..hi} : inner_body` into
/// N copies of `inner_body[var/i]` as fresh `BodyItem::Constraint`s.
/// Returns None if the Forall isn't of that shape or bounds aren't
/// statically resolvable.
fn try_unroll_forall_range(e: &Expr) -> Option<Vec<BodyItem>> {
    let Expr::Forall(vars, range, body) = e else { return None; };
    if vars.len() != 1 { return None; }
    let Expr::Range(lo, hi) = range.as_ref() else { return None; };
    let lo_v = try_eval_const_int(lo)?;
    let hi_v = try_eval_const_int(hi)?;
    let var = &vars[0];
    let mut out = Vec::with_capacity((hi_v - lo_v + 1).max(0) as usize);
    for i in lo_v..=hi_v {
        let inst = substitute(body, var, &Expr::Int(i));
        out.push(BodyItem::Constraint(inst));
    }
    Some(out)
}

/// Expand a body by unrolling any `∀ i ∈ {lo..hi}` constraints into
/// N copies of their inner body. Returns a new vector with the
/// Forall items replaced by their unrolled instances. Other items
/// pass through.
fn expand_foralls(body: Vec<BodyItem>) -> Vec<BodyItem> {
    let mut out = Vec::with_capacity(body.len());
    for item in body {
        match &item {
            BodyItem::Constraint(e) => {
                if let Some(unrolled) = try_unroll_forall_range(e) {
                    out.extend(expand_foralls(unrolled));
                    continue;
                }
                out.push(item);
            }
            _ => out.push(item),
        }
    }
    out
}

/// Schema-wide checks: every equality in the schema body (plus
/// transitively passthrough'd bodies) whose LHS or RHS is a known
/// given variable (i.e., not a component-substitution target). The
/// runtime emits these as consistency assertions for the native
/// evaluator to verify against the pinned given values.
///
/// `is_in_given` answers "does the caller pin this name in given?"
/// — typically `|n| given.contains_key(n)` from rt.query.
pub fn extract_schema_wide_checks(
    schema: &SchemaDecl,
    is_in_given: &dyn Fn(&str) -> bool,
    passthrough_body: &dyn Fn(&str) -> Option<Vec<BodyItem>>,
) -> Vec<(Expr, Expr)> {
    let mut all_body: Vec<BodyItem> = schema.body.clone();
    let mut to_walk: Vec<String> = schema.body.iter().filter_map(|i| match i {
        BodyItem::Passthrough(n) => Some(n.clone()), _ => None,
    }).collect();
    let mut walked: HashSet<String> = HashSet::new();
    while let Some(name) = to_walk.pop() {
        if !walked.insert(name.clone()) { continue; }
        let Some(body) = passthrough_body(&name) else { continue };
        for item in &body {
            if let BodyItem::Passthrough(n) = item { to_walk.push(n.clone()); }
        }
        all_body.extend(body);
    }
    let mut out = Vec::new();
    for item in body_constraints(&all_body) {
        let Expr::Binary(BinOp::Eq, lhs, rhs) = item else { continue };
        let lhs_given = matches!(lhs.as_ref(),
            Expr::Identifier(n) if is_in_given(n));
        let rhs_given = matches!(rhs.as_ref(),
            Expr::Identifier(n) if is_in_given(n));
        if lhs_given || rhs_given {
            out.push(((**lhs).clone(), (**rhs).clone()));
        }
    }
    out
}

/// Soundness gate: the v1 native evaluator only enforces equality
/// definitions. If the body has ANY non-equality Constraint, the
/// native path can return SAT for inputs that Z3 would reject (e.g.
/// `n ∈ Nat ∧ n < 5` with given n=10 — `n < 5` is the filter that
/// Z3 enforces but the native chain doesn't). Returns false in that
/// case; callers should fall through to Z3.
///
/// Body Memberships (`x ∈ Type`) and Passthrough / ClaimCall items
/// aren't constraints in the AST sense — they're declarations. Their
/// type-bound effects (Nat → x ≥ 0) live in declare_and_assert at
/// translation time, which the function-izer-cached path bypasses;
/// for that reason the gate is conservative and prefers refusing.
pub fn is_pure_assignment_body(schema: &SchemaDecl) -> bool {
    is_pure_assignment_body_with_enums(schema, &|_| false)
}

/// `is_pure_assignment_body` variant that also accepts a "is this type
/// name an enum?" predicate. When called from the runtime, callers
/// pass an enum-registry-backed predicate; this lets the gate accept
/// claims with enum-typed memberships (state machines, etc.) without
/// hard-coding type names.
pub fn is_pure_assignment_body_with_enums(
    schema: &SchemaDecl,
    is_enum: &dyn Fn(&str) -> bool,
) -> bool {
    is_pure_assignment_body_full(schema, is_enum, &|_| false)
}

/// Most permissive form: accepts enum types, user-record types,
/// and Passthroughs to claims whose body also passes the gate.
///
/// `is_pure_passthrough(claim_name)` — for a `BodyItem::Passthrough`
/// item, the runtime can recursively check the referenced claim
/// passes the gate too. Cycle detection lives in the predicate
/// implementation. Setting it to always-false keeps the v3 behavior
/// (refuse Passthroughs).
pub fn is_pure_assignment_body_full(
    schema: &SchemaDecl,
    is_enum: &dyn Fn(&str) -> bool,
    is_simple_record: &dyn Fn(&str) -> bool,
) -> bool {
    is_pure_assignment_body_xl(schema, is_enum, is_simple_record, &|_| false)
}

/// Extra-large gate: also accepts Passthrough(claim_name) when
/// `is_pure_passthrough(claim_name)` is true.
pub fn is_pure_assignment_body_xl(
    schema: &SchemaDecl,
    is_enum: &dyn Fn(&str) -> bool,
    is_simple_record: &dyn Fn(&str) -> bool,
    is_pure_passthrough: &dyn Fn(&str) -> bool,
) -> bool {
    gate_diagnostics(schema, is_enum, is_simple_record, is_pure_passthrough).is_none()
}

/// Diagnostic variant: returns `None` if the gate accepts the schema,
/// otherwise returns a short string explaining WHY it refused. Used
/// by EVIDENT_FUNCTIONIZE_TRACE=1 to make gate-rejection actionable.
pub fn gate_diagnostics(
    schema: &SchemaDecl,
    is_enum: &dyn Fn(&str) -> bool,
    is_simple_record: &dyn Fn(&str) -> bool,
    is_pure_passthrough: &dyn Fn(&str) -> bool,
) -> Option<String> {
    if !matches!(schema.keyword,
        crate::ast::Keyword::Claim | crate::ast::Keyword::Schema
        | crate::ast::Keyword::Type | crate::ast::Keyword::Fsm) {
        return Some(format!("keyword {:?}", schema.keyword));
    }
    for item in &schema.body {
        match item {
            BodyItem::Constraint(Expr::Binary(BinOp::Eq, _, _)) => {}  // OK
            BodyItem::Constraint(other) => {
                // Forall over a static Range with literal bounds is
                // OK — it unrolls at extract time. Check that the
                // unrolled body is itself only equalities.
                if let Some(unrolled) = try_unroll_forall_range(other) {
                    for sub in &unrolled {
                        if let BodyItem::Constraint(e) = sub {
                            if let Some(why) = constraint_kind(e) {
                                return Some(format!("Forall body: {}", why));
                            }
                        }
                    }
                    continue;
                }
                if let Some(why) = constraint_kind(other) {
                    return Some(why);
                }
                continue;
            }
            BodyItem::Membership { name, type_name, .. } => {
                let primitive = matches!(type_name.as_str(),
                    "Int" | "Real" | "Bool" | "String");
                if primitive { continue; }
                if is_enum(type_name) { continue; }
                if is_simple_record(type_name) { continue; }
                return Some(format!("Membership {}∈{}", name, type_name));
            }
            BodyItem::Passthrough(claim_name) => {
                if !is_pure_passthrough(claim_name) {
                    return Some(format!("..{} not pure", claim_name));
                }
            }
            BodyItem::ClaimCall { name, .. } =>
                return Some(format!("ClaimCall {}", name)),
            BodyItem::SubclaimDecl(_) => {}
        }
    }
    None
}

/// Does `e` reference an identifier named `name`?
fn mentions(e: &Expr, name: &str) -> bool {
    match e {
        Expr::Identifier(s) => s == name,
        Expr::Int(_) | Expr::Real(_) | Expr::Bool(_) | Expr::Str(_) => false,
        Expr::Binary(_, l, r) => mentions(l, name) || mentions(r, name),
        Expr::Not(x) => mentions(x, name),
        Expr::Ternary(c, a, b) => mentions(c, name) || mentions(a, name) || mentions(b, name),
        Expr::Call(_, args) => args.iter().any(|a| mentions(a, name)),
        Expr::Field(x, _) => mentions(x, name),
        Expr::Index(s, i) => mentions(s, name) || mentions(i, name),
        Expr::Cardinality(x) => mentions(x, name),
        Expr::SeqLit(items) | Expr::SetLit(items) | Expr::Tuple(items) =>
            items.iter().any(|a| mentions(a, name)),
        Expr::InExpr(a, b) => mentions(a, name) || mentions(b, name),
        Expr::Range(a, b) => mentions(a, name) || mentions(b, name),
        Expr::Forall(_, range, body) | Expr::Exists(_, range, body) =>
            mentions(range, name) || mentions(body, name),
        Expr::Match(scrut, arms) => {
            if mentions(scrut, name) { return true; }
            arms.iter().any(|arm| mentions(&arm.body, name))
        }
        Expr::Matches(scrut, _) => mentions(scrut, name),
    }
}

/// Resolves identifiers to values. Used during native evaluation when
/// the environment doesn't have a binding — typically to resolve bare
/// enum-variant names (`Init`, `Done`, `North`) to `Value::Enum`.
///
/// Callers from `rt.query`'s function-izer hook construct a resolver
/// that consults the runtime's `EnumRegistry`. Tests can pass a
/// no-op resolver (which behaves like the env-only lookup).
pub type IdentResolver<'a> = dyn Fn(&str) -> Option<Value> + 'a;

/// Resolves constructor calls — names with positional arg values —
/// to `Value::Enum`. Used during native evaluation when the body
/// contains a `Ctor(arg, ...)` call like `Println("hello")`. The
/// resolver looks up the variant in the enum registry and builds a
/// `Value::Enum` with the given fields.
pub type CtorResolver<'a> = dyn Fn(&str, &[Value]) -> Option<Value> + 'a;

fn no_ctor(_: &str, _: &[Value]) -> Option<Value> { None }

/// Evaluate a substitution chain against a given binding map. Returns
/// the bindings the chain produces (input bindings echoed + each
/// substitution's computed value).
///
/// Returns `None` if any step can't be evaluated (e.g., the expression
/// references a variable not in `given` and not yet substituted, or
/// uses an Expr variant the v1 evaluator doesn't yet support).
pub fn evaluate_chain(
    chain: &SubstitutionChain,
    given: &HashMap<String, Value>,
) -> Option<HashMap<String, Value>> {
    evaluate_chain_with_resolver(chain, given, &|_| None)
}

/// `evaluate_chain` variant that also accepts a fallback identifier
/// resolver (used for enum-variant names not in env). When the env
/// lookup fails, we consult this resolver before giving up.
pub fn evaluate_chain_with_resolver(
    chain: &SubstitutionChain,
    given: &HashMap<String, Value>,
    resolver: &IdentResolver<'_>,
) -> Option<HashMap<String, Value>> {
    evaluate_chain_with_resolvers(chain, given, resolver, &no_ctor)
}

/// Full variant: accepts both an identifier resolver (for bare
/// nullary enum variants) and a constructor resolver (for variant
/// constructor calls with payload values).
pub fn evaluate_chain_with_resolvers(
    chain: &SubstitutionChain,
    given: &HashMap<String, Value>,
    resolver: &IdentResolver<'_>,
    ctor: &CtorResolver<'_>,
) -> Option<HashMap<String, Value>> {
    let mut env: HashMap<String, Value> = given.clone();
    for step in &chain.steps {
        let value = eval_expr(&step.expr, &env, resolver, ctor)?;
        if let Some(pinned) = given.get(&step.var) {
            if pinned != &value { return None; }   // UNSAT — body conflicts with pin.
        }
        env.insert(step.var.clone(), value);
    }
    // Verify consistency constraints (equalities between two non-
    // component vars — typically given vars that the body further
    // constrains). Failure here means the body's constraint conflicts
    // with the given values → UNSAT.
    for (lhs, rhs) in &chain.checks {
        let lv = eval_expr(lhs, &env, resolver, ctor)?;
        let rv = eval_expr(rhs, &env, resolver, ctor)?;
        if lv != rv { return None; }
    }
    Some(env)
}

/// Pure Rust interpreter for Evident expressions. v1: arithmetic,
/// comparisons, logical ops, literals, identifiers, ternary, match.
/// More exotic constructs (∀, sequences, sets, claim calls) are TODOs.
fn eval_expr(
    e: &Expr,
    env: &HashMap<String, Value>,
    resolver: &IdentResolver<'_>,
    ctor: &CtorResolver<'_>,
) -> Option<Value> {
    match e {
        Expr::Int(n)  => Some(Value::Int(*n)),
        Expr::Real(r) => Some(Value::Real(*r)),
        Expr::Bool(b) => Some(Value::Bool(*b)),
        Expr::Str(s)  => Some(Value::Str(s.clone())),
        Expr::Identifier(name) => {
            env.get(name).cloned().or_else(|| resolver(name))
        }
        Expr::Binary(op, l, r) => {
            let lv = eval_expr(l, env, resolver, ctor)?;
            let rv = eval_expr(r, env, resolver, ctor)?;
            eval_binop(op.clone(), &lv, &rv)
        }
        Expr::Not(x) => {
            let v = eval_expr(x, env, resolver, ctor)?;
            match v { Value::Bool(b) => Some(Value::Bool(!b)), _ => None }
        }
        Expr::Ternary(c, a, b) => {
            let cv = eval_expr(c, env, resolver, ctor)?;
            let Value::Bool(cb) = cv else { return None };
            if cb { eval_expr(a, env, resolver, ctor) } else { eval_expr(b, env, resolver, ctor) }
        }
        Expr::Match(scrut, arms) => {
            let scrut_val = eval_expr(scrut, env, resolver, ctor)?;
            let Value::Enum { variant: scr_variant, fields: scr_fields, .. } = &scrut_val
                else { return None };
            for arm in arms {
                match &arm.pattern {
                    crate::ast::MatchPattern::Ctor { name, binds } => {
                        if name != scr_variant { continue; }
                        if binds.len() != scr_fields.len() { continue; }
                        // Bind named payload fields (None = wildcard, skip).
                        let mut sub_env = env.clone();
                        for (bind, field) in binds.iter().zip(scr_fields.iter()) {
                            if let Some(bind_name) = bind {
                                sub_env.insert(bind_name.clone(), field.clone());
                            }
                        }
                        return eval_expr(&arm.body, &sub_env, resolver, ctor);
                    }
                    crate::ast::MatchPattern::Wildcard => {
                        return eval_expr(&arm.body, env, resolver, ctor);
                    }
                }
            }
            None  // no arm matched
        }
        Expr::SeqLit(items) => {
            // Evaluate each item; classify the resulting Vec into the
            // appropriate Value::Seq* variant by first-element type.
            // Empty → SeqInt([]) (caller can coerce; SeqEnum vs SeqInt
            // for an empty sequence is opaque at this layer — Z3
            // equality only inspects len for empty seqs).
            let mut vals = Vec::with_capacity(items.len());
            for item in items {
                vals.push(eval_expr(item, env, resolver, ctor)?);
            }
            match vals.first() {
                None                  => {
                    // Empty SeqLit: element type is determined by the
                    // declared sort of the receiving variable, which
                    // the value-level evaluator doesn't track. Fall
                    // through to Z3 — only one extra call's worth of
                    // overhead, and only for `s = ⟨⟩` shapes which
                    // are rare in real programs.
                    return None;
                }
                Some(Value::Int(_))   => {
                    let mut out = Vec::with_capacity(vals.len());
                    for v in vals {
                        if let Value::Int(n) = v { out.push(n) } else { return None }
                    }
                    Some(Value::SeqInt(out))
                }
                Some(Value::Bool(_))  => {
                    let mut out = Vec::with_capacity(vals.len());
                    for v in vals {
                        if let Value::Bool(b) = v { out.push(b) } else { return None }
                    }
                    Some(Value::SeqBool(out))
                }
                Some(Value::Str(_))   => {
                    let mut out = Vec::with_capacity(vals.len());
                    for v in vals {
                        if let Value::Str(s) = v { out.push(s) } else { return None }
                    }
                    Some(Value::SeqStr(out))
                }
                Some(Value::Enum { .. }) => Some(Value::SeqEnum(vals)),
                _ => None,
            }
        }
        Expr::Call(name, args) => {
            // Constructor call. Evaluate args, then ask the ctor
            // resolver to build a `Value::Enum`. (`coindexed`/`edges`
            // builtins don't appear in chain expression position.)
            let mut arg_vals = Vec::with_capacity(args.len());
            for a in args {
                arg_vals.push(eval_expr(a, env, resolver, ctor)?);
            }
            ctor(name, &arg_vals)
        }
        Expr::Index(target, idx) => {
            // Sequence indexing: `seq[i]`. Evaluate both, select
            // out the element. Out-of-bounds returns None
            // (eval falls through to Z3).
            let target_val = eval_expr(target, env, resolver, ctor)?;
            let Value::Int(i) = eval_expr(idx, env, resolver, ctor)? else { return None };
            if i < 0 { return None; }
            let i = i as usize;
            match target_val {
                Value::SeqInt(v)   => v.get(i).map(|n| Value::Int(*n)),
                Value::SeqBool(v)  => v.get(i).map(|b| Value::Bool(*b)),
                Value::SeqStr(v)   => v.get(i).map(|s| Value::Str(s.clone())),
                Value::SeqEnum(v)  => v.get(i).cloned(),
                _ => None,
            }
        }
        Expr::Cardinality(target) => {
            let v = eval_expr(target, env, resolver, ctor)?;
            match v {
                Value::SeqInt(v)   => Some(Value::Int(v.len() as i64)),
                Value::SeqBool(v)  => Some(Value::Int(v.len() as i64)),
                Value::SeqStr(v)   => Some(Value::Int(v.len() as i64)),
                Value::SeqEnum(v)  => Some(Value::Int(v.len() as i64)),
                Value::SeqComposite(v) => Some(Value::Int(v.len() as i64)),
                Value::SetInt(v)   => Some(Value::Int(v.len() as i64)),
                Value::SetBool(v)  => Some(Value::Int(v.len() as i64)),
                Value::SetStr(v)   => Some(Value::Int(v.len() as i64)),
                _ => None,
            }
        }
        Expr::Field(target, name) => {
            // Field access on an Index'd composite-element value, etc.
            // Bare-identifier-prefixed fields fold into a dotted
            // Identifier at parse time and resolve via env. This
            // path handles `seq[i].field` style.
            let target_val = eval_expr(target, env, resolver, ctor)?;
            match target_val {
                Value::Composite(map) => map.get(name).cloned(),
                Value::Enum { variant: _, fields, enum_name: _ } => {
                    // Indexing fields by name on an enum value isn't
                    // directly supported — enum payload fields are
                    // positional. Refuse.
                    let _ = fields;
                    None
                }
                _ => None,
            }
        }
        Expr::Matches(scrut, pattern) => {
            // Constructor-recognizer: does `e`'s variant equal Ctor?
            // Payload bindings are ignored — pattern is just for the
            // tag. We accept any binds shape and only check the name.
            let scrut_val = eval_expr(scrut, env, resolver, ctor)?;
            let Value::Enum { variant, .. } = scrut_val else { return None };
            match pattern {
                crate::ast::MatchPattern::Ctor { name, .. } => {
                    Some(Value::Bool(&variant == name))
                }
                crate::ast::MatchPattern::Wildcard => Some(Value::Bool(true)),
            }
        }
        _ => None,  // unsupported variant in v1
    }
}

fn eval_binop(op: BinOp, l: &Value, r: &Value) -> Option<Value> {
    use Value::*;
    match (op, l, r) {
        (BinOp::Add, Int(a), Int(b)) => Some(Int(a + b)),
        (BinOp::Sub, Int(a), Int(b)) => Some(Int(a - b)),
        (BinOp::Mul, Int(a), Int(b)) => Some(Int(a * b)),
        (BinOp::Div, Int(a), Int(b)) if *b != 0 => Some(Int(a / b)),
        (BinOp::Add, Real(a), Real(b)) => Some(Real(a + b)),
        (BinOp::Sub, Real(a), Real(b)) => Some(Real(a - b)),
        (BinOp::Mul, Real(a), Real(b)) => Some(Real(a * b)),
        (BinOp::Div, Real(a), Real(b)) if *b != 0.0 => Some(Real(a / b)),
        (BinOp::Eq,  Int(a), Int(b)) => Some(Bool(a == b)),
        (BinOp::Neq, Int(a), Int(b)) => Some(Bool(a != b)),
        (BinOp::Lt,  Int(a), Int(b)) => Some(Bool(a <  b)),
        (BinOp::Le,  Int(a), Int(b)) => Some(Bool(a <= b)),
        (BinOp::Gt,  Int(a), Int(b)) => Some(Bool(a >  b)),
        (BinOp::Ge,  Int(a), Int(b)) => Some(Bool(a >= b)),
        (BinOp::Eq,  Bool(a), Bool(b)) => Some(Bool(a == b)),
        (BinOp::And, Bool(a), Bool(b)) => Some(Bool(*a && *b)),
        (BinOp::Or,  Bool(a), Bool(b)) => Some(Bool(*a || *b)),
        (BinOp::Concat, Str(a), Str(b)) => Some(Str(format!("{a}{b}"))),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decompose::Component;

    fn ident(s: &str) -> Expr { Expr::Identifier(s.to_string()) }
    fn int(n: i64) -> Expr { Expr::Int(n) }

    #[test]
    fn pair_substitutions_topo_sort_correctly() {
        // Synthesize a Pair-like schema: sum = a + b, prod = a * b.
        let schema = SchemaDecl {
            keyword: crate::ast::Keyword::Claim,
            name: "Pair".into(),
            type_params: vec![],
            param_count: 0,
            external: false,
            body: vec![
                BodyItem::Membership { name: "a".into(),    type_name: "Int".into(), pins: crate::ast::Pins::None },
                BodyItem::Membership { name: "b".into(),    type_name: "Int".into(), pins: crate::ast::Pins::None },
                BodyItem::Membership { name: "sum".into(),  type_name: "Int".into(), pins: crate::ast::Pins::None },
                BodyItem::Membership { name: "prod".into(), type_name: "Int".into(), pins: crate::ast::Pins::None },
                BodyItem::Constraint(Expr::Binary(BinOp::Eq, Box::new(ident("sum")),
                    Box::new(Expr::Binary(BinOp::Add, Box::new(ident("a")), Box::new(ident("b")))))),
                BodyItem::Constraint(Expr::Binary(BinOp::Eq, Box::new(ident("prod")),
                    Box::new(Expr::Binary(BinOp::Mul, Box::new(ident("a")), Box::new(ident("b")))))),
            ],
        };
        let comp = Component {
            vars: vec!["sum".into(), "prod".into()],
            constraint_indices: vec![],
        };
        let chain = extract_chain(&schema, &comp).expect("should extract");
        assert_eq!(chain.steps.len(), 2);
        // Evaluate with a=5, b=3.
        let mut given = HashMap::new();
        given.insert("a".into(), Value::Int(5));
        given.insert("b".into(), Value::Int(3));
        let env = evaluate_chain(&chain, &given).expect("eval ok");
        assert_eq!(env.get("sum"),  Some(&Value::Int(8)));
        assert_eq!(env.get("prod"), Some(&Value::Int(15)));
    }

    #[test]
    fn missing_definition_returns_none() {
        // Component has a var with no defining equality.
        let schema = SchemaDecl {
            keyword: crate::ast::Keyword::Claim,
            name: "Incomplete".into(),
            type_params: vec![],
            param_count: 0,
            external: false,
            body: vec![
                BodyItem::Membership { name: "a".into(), type_name: "Int".into(), pins: crate::ast::Pins::None },
                BodyItem::Membership { name: "b".into(), type_name: "Int".into(), pins: crate::ast::Pins::None },
                // No definition for `a` or `b`.
            ],
        };
        let comp = Component {
            vars: vec!["a".into(), "b".into()],
            constraint_indices: vec![],
        };
        let chain = extract_chain(&schema, &comp);
        assert!(chain.is_none());
    }

    #[test]
    fn dependent_substitution_orders_correctly() {
        // a = 1, b = a + 1, c = b * 2.  Topo order: a, b, c.
        let schema = SchemaDecl {
            keyword: crate::ast::Keyword::Claim,
            name: "Chain".into(),
            type_params: vec![],
            param_count: 0,
            external: false,
            body: vec![
                BodyItem::Membership { name: "a".into(), type_name: "Int".into(), pins: crate::ast::Pins::None },
                BodyItem::Membership { name: "b".into(), type_name: "Int".into(), pins: crate::ast::Pins::None },
                BodyItem::Membership { name: "c".into(), type_name: "Int".into(), pins: crate::ast::Pins::None },
                BodyItem::Constraint(Expr::Binary(BinOp::Eq, Box::new(ident("a")), Box::new(int(1)))),
                BodyItem::Constraint(Expr::Binary(BinOp::Eq, Box::new(ident("b")),
                    Box::new(Expr::Binary(BinOp::Add, Box::new(ident("a")), Box::new(int(1)))))),
                BodyItem::Constraint(Expr::Binary(BinOp::Eq, Box::new(ident("c")),
                    Box::new(Expr::Binary(BinOp::Mul, Box::new(ident("b")), Box::new(int(2)))))),
            ],
        };
        let comp = Component {
            vars: vec!["a".into(), "b".into(), "c".into()],
            constraint_indices: vec![],
        };
        let chain = extract_chain(&schema, &comp).expect("should extract");
        let order: Vec<&str> = chain.steps.iter().map(|s| s.var.as_str()).collect();
        // a comes first (no deps), then b (depends on a), then c (depends on b).
        let pos_a = order.iter().position(|v| *v == "a").unwrap();
        let pos_b = order.iter().position(|v| *v == "b").unwrap();
        let pos_c = order.iter().position(|v| *v == "c").unwrap();
        assert!(pos_a < pos_b, "a before b in {:?}", order);
        assert!(pos_b < pos_c, "b before c in {:?}", order);
        // Evaluate.
        let env = evaluate_chain(&chain, &HashMap::new()).expect("eval ok");
        assert_eq!(env.get("a"), Some(&Value::Int(1)));
        assert_eq!(env.get("b"), Some(&Value::Int(2)));
        assert_eq!(env.get("c"), Some(&Value::Int(4)));
    }
}
