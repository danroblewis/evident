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
#[derive(Debug, Clone)]
pub struct SubstitutionChain {
    pub steps: Vec<Substitution>,
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
    if !is_pure_assignment_body_full(schema, is_enum, is_simple_record) { return None; }
    let target: HashSet<&str> = component.vars.iter().map(|s| s.as_str()).collect();

    // Collect candidate substitutions: every `var = expr` or `expr = var`
    // where `var` is in our component and the other side doesn't
    // reference `var` itself.
    let mut candidates: HashMap<String, Expr> = HashMap::new();
    for item in body_constraints(&schema.body) {
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
            }
        }
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
    Some(SubstitutionChain { steps })
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

/// Most permissive form: accepts enum types AND user-record types
/// (per the `is_simple_record` predicate). User records are accepted
/// when all their fields are primitive types — recursive record
/// composition (record of records, record of Seq) is not v1.
///
/// Memberships of user records expand to per-field Z3 consts in
/// `declare_and_assert`. The native evaluator handles those because
/// the AST sees them as dotted identifiers (`pos.x`, `pos.y`) — env
/// lookup resolves them like any other named variable.
pub fn is_pure_assignment_body_full(
    schema: &SchemaDecl,
    is_enum: &dyn Fn(&str) -> bool,
    is_simple_record: &dyn Fn(&str) -> bool,
) -> bool {
    if !matches!(schema.keyword,
        crate::ast::Keyword::Claim | crate::ast::Keyword::Schema
        | crate::ast::Keyword::Type | crate::ast::Keyword::Fsm) {
        return false;
    }
    for item in &schema.body {
        match item {
            BodyItem::Constraint(Expr::Binary(BinOp::Eq, _, _)) => {}  // OK
            BodyItem::Constraint(_) => return false,  // filter — bail
            BodyItem::Membership { type_name, .. } => {
                let primitive = matches!(type_name.as_str(),
                    "Int" | "Real" | "Bool" | "String");
                if primitive { continue; }
                if is_enum(type_name) { continue; }
                if is_simple_record(type_name) { continue; }
                return false;
            }
            BodyItem::Passthrough(_) => return false,  // body lives elsewhere
            BodyItem::ClaimCall { .. } => return false,  // ditto
            BodyItem::SubclaimDecl(_) => {}  // no runtime effect on parent
        }
    }
    true
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
    let mut env: HashMap<String, Value> = given.clone();
    for step in &chain.steps {
        let value = eval_expr(&step.expr, &env, resolver)?;
        env.insert(step.var.clone(), value);
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
            let lv = eval_expr(l, env, resolver)?;
            let rv = eval_expr(r, env, resolver)?;
            eval_binop(op.clone(), &lv, &rv)
        }
        Expr::Not(x) => {
            let v = eval_expr(x, env, resolver)?;
            match v { Value::Bool(b) => Some(Value::Bool(!b)), _ => None }
        }
        Expr::Ternary(c, a, b) => {
            let cv = eval_expr(c, env, resolver)?;
            let Value::Bool(cb) = cv else { return None };
            if cb { eval_expr(a, env, resolver) } else { eval_expr(b, env, resolver) }
        }
        Expr::Match(scrut, arms) => {
            let scrut_val = eval_expr(scrut, env, resolver)?;
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
                        return eval_expr(&arm.body, &sub_env, resolver);
                    }
                    crate::ast::MatchPattern::Wildcard => {
                        return eval_expr(&arm.body, env, resolver);
                    }
                }
            }
            None  // no arm matched
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
