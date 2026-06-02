//! Pure AST→AST identifier rewriters: prefix-injection for type-body inheritance and bound-var substitution.

use std::collections::HashSet;

use crate::core::ast::*;

/// Prefix any identifier whose leading segment is in `field_set` with `prefix.`.
/// Used to inherit a type body's constraints onto an instance: bare `p` → `f.p`.
pub(super) fn rewrite_idents_with_prefix(
    e: &Expr,
    prefix: &str,
    field_set: &HashSet<String>,
) -> Expr {
    let r = |x: &Expr| Box::new(rewrite_idents_with_prefix(x, prefix, field_set));
    let rv = |xs: &Vec<Expr>| xs.iter()
        .map(|x| rewrite_idents_with_prefix(x, prefix, field_set))
        .collect();
    match e {
        Expr::Identifier(name) => {
            let first_seg = name.split('.').next().unwrap_or("");
            if field_set.contains(first_seg) {
                Expr::Identifier(format!("{}.{}", prefix, name))
            } else {
                e.clone()
            }
        }
        Expr::Int(_) | Expr::Real(_) | Expr::Bool(_) | Expr::Str(_) => e.clone(),
        Expr::SetLit(xs)  => Expr::SetLit(rv(xs)),
        Expr::SeqLit(xs)  => Expr::SeqLit(rv(xs)),
        Expr::Tuple(xs)   => Expr::Tuple(rv(xs)),
        Expr::Range(a, b) => Expr::Range(r(a), r(b)),
        Expr::InExpr(a, b) => Expr::InExpr(r(a), r(b)),
        Expr::Forall(vars, range, body) => {
            // Bound vars shadow fields inside the body — exclude them from the field_set.
            let inner_set: HashSet<String> = field_set.iter()
                .filter(|f| !vars.contains(f))
                .cloned()
                .collect();
            Expr::Forall(
                vars.clone(),
                Box::new(rewrite_idents_with_prefix(range, prefix, field_set)),
                Box::new(rewrite_idents_with_prefix(body,  prefix, &inner_set)),
            )
        }
        Expr::Exists(vars, range, body) => {
            let inner_set: HashSet<String> = field_set.iter()
                .filter(|f| !vars.contains(f))
                .cloned()
                .collect();
            Expr::Exists(
                vars.clone(),
                Box::new(rewrite_idents_with_prefix(range, prefix, field_set)),
                Box::new(rewrite_idents_with_prefix(body,  prefix, &inner_set)),
            )
        }
        // Call name is a schema/ctor reference, not a prefixable field.
        Expr::Call(name, args) => Expr::Call(name.clone(), rv(args)),
        Expr::Cardinality(x) => Expr::Cardinality(r(x)),
        Expr::Index(a, b)    => Expr::Index(r(a), r(b)),
        Expr::Field(recv, f) => Expr::Field(r(recv), f.clone()),
        Expr::Binary(op, a, b) => Expr::Binary(op.clone(), r(a), r(b)),
        Expr::Not(x)           => Expr::Not(r(x)),
        Expr::Ternary(c, a, b) => Expr::Ternary(r(c), r(a), r(b)),
        Expr::Match(scr, arms) => {
            let new_arms: Vec<MatchArm> = arms.iter().map(|arm| {
                // Pattern-bound names shadow fields within this arm.
                let mut shadowed: HashSet<String> = HashSet::new();
                collect_pattern_binds(&arm.pattern, &mut shadowed);
                let inner: HashSet<String> = field_set.iter()
                    .filter(|n| !shadowed.contains(*n))
                    .cloned()
                    .collect();
                MatchArm {
                    pattern: arm.pattern.clone(),
                    body: Box::new(rewrite_idents_with_prefix(&arm.body, prefix, &inner)),
                }
            }).collect();
            Expr::Match(r(scr), new_arms)
        }
        Expr::Matches(x, p) => Expr::Matches(r(x), p.clone()),
    }
}

/// Collect all names bound by a match pattern (recursing into constructor sub-patterns).
fn collect_pattern_binds(pat: &MatchPattern, out: &mut HashSet<String>) {
    match pat {
        MatchPattern::Wildcard => {}
        MatchPattern::Bind(name) => { out.insert(name.clone()); }
        MatchPattern::Ctor { binds, .. } =>
            for sub in binds { collect_pattern_binds(sub, out); }
    }
}

/// Replace `bound_var` (and dotted suffixes like `bound_var.color`) with `elem` throughout `e`.
/// Deeper paths (`p.aabb.pos.x`) chain `Field` nodes.
pub(super) fn substitute_bound_var(e: &Expr, bound: &str, elem: &Expr) -> Expr {
    let r = |x: &Expr| Box::new(substitute_bound_var(x, bound, elem));
    let rv = |xs: &Vec<Expr>| xs.iter()
        .map(|x| substitute_bound_var(x, bound, elem))
        .collect();
    match e {
        Expr::Identifier(name) => {
            if name == bound { return elem.clone(); }
            let prefix = format!("{}.", bound);
            if let Some(suffix) = name.strip_prefix(&prefix) {
                // Build Field(Field(... Field(elem, seg1), seg2), …, segN).
                let mut out = elem.clone();
                for seg in suffix.split('.') {
                    out = Expr::Field(Box::new(out), seg.to_string());
                }
                return out;
            }
            e.clone()
        }
        Expr::Int(_) | Expr::Real(_) | Expr::Bool(_) | Expr::Str(_) => e.clone(),
        Expr::SetLit(xs)  => Expr::SetLit(rv(xs)),
        Expr::SeqLit(xs)  => Expr::SeqLit(rv(xs)),
        Expr::Tuple(xs)   => Expr::Tuple(rv(xs)),
        Expr::Range(a, b) => Expr::Range(r(a), r(b)),
        Expr::InExpr(a, b) => Expr::InExpr(r(a), r(b)),
        Expr::Forall(vars, range, body) => {
            // Inner quantifier rebinding same name shadows the substitution.
            if vars.iter().any(|v| v == bound) {
                Expr::Forall(vars.clone(), r(range), body.clone())
            } else {
                Expr::Forall(vars.clone(), r(range), r(body))
            }
        }
        Expr::Exists(vars, range, body) => {
            if vars.iter().any(|v| v == bound) {
                Expr::Exists(vars.clone(), r(range), body.clone())
            } else {
                Expr::Exists(vars.clone(), r(range), r(body))
            }
        }
        Expr::Call(n, args)    => Expr::Call(n.clone(), rv(args)),
        Expr::Cardinality(x)   => Expr::Cardinality(r(x)),
        Expr::Index(a, b)      => Expr::Index(r(a), r(b)),
        Expr::Field(recv, f)   => Expr::Field(r(recv), f.clone()),
        Expr::Binary(op, a, b) => Expr::Binary(op.clone(), r(a), r(b)),
        Expr::Not(x)           => Expr::Not(r(x)),
        Expr::Ternary(c, a, b) => Expr::Ternary(r(c), r(a), r(b)),
        Expr::Match(scr, arms) => {
            let new_arms: Vec<MatchArm> = arms.iter().map(|arm| MatchArm {
                pattern: arm.pattern.clone(),
                body: Box::new(substitute_bound_var(&arm.body, bound, elem)),
            }).collect();
            Expr::Match(r(scr), new_arms)
        }
        Expr::Matches(x, p) => Expr::Matches(r(x), p.clone()),
    }
}
