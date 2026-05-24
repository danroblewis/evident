//! Pure AST→AST identifier rewriters used by the inline walker.
//!
//!   * `rewrite_idents_with_prefix` — inherit a type body's Constraint
//!     items onto a sub-schema instance by prefixing field references.
//!   * `substitute_bound_var` — replace a `∀`-bound variable (and its
//!     dotted suffixes) with a per-iteration element expression.

use std::collections::HashSet;

use crate::core::ast::*;

/// Rewrite identifiers in `e` so any leading-segment match against the
/// type's `field_set` becomes `<prefix>.<original>`. Used to inherit a
/// type body's Constraint items onto a sub-schema instance:
///
/// ```text
/// type Foo(p ∈ Int)
///     d ∈ Int = p + 1   -- inside Foo's body, `p` and `d` are bare
///
/// claim caller
///     f ∈ Foo (p ↦ 5)
///     -- The body constraint `d = p + 1`, when inherited onto `f`,
///     -- becomes `f.d = f.p + 1`. This function does that rewrite.
/// ```
///
/// Identifiers whose leading segment is NOT a field of the type are
/// left untouched — they're external references (other schemas,
/// quantifier-bound names, constants like `is_first_tick`).
///
/// Both `Identifier("foo")` and `Identifier("foo.bar")` are recognized
/// — the parser folds source-level `foo.bar` into a single dotted
/// `Identifier` (see ast.rs::Field comment). Receiver-side recursion
/// also covers the explicit `Field(receiver, …)` shape used when the
/// receiver is itself an expression (e.g. `seq[i].x`).
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
            // Quantifier bound names shadow field names within the body.
            // If a quantifier introduces `pos` (say) and the type also
            // has a field `pos`, body uses of `pos` inside this forall
            // should NOT get prefixed — they're the bound var, not the
            // field. Build a temporary field_set that excludes the
            // shadowed names.
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
        // Call's first arg is the function/type/constructor NAME — don't
        // touch. Only its args might contain field refs.
        Expr::Call(name, args) => Expr::Call(name.clone(), rv(args)),
        Expr::Cardinality(x) => Expr::Cardinality(r(x)),
        Expr::Index(a, b)    => Expr::Index(r(a), r(b)),
        Expr::Field(recv, f) => Expr::Field(r(recv), f.clone()),
        Expr::Binary(op, a, b) => Expr::Binary(op.clone(), r(a), r(b)),
        Expr::Not(x)           => Expr::Not(r(x)),
        Expr::Ternary(c, a, b) => Expr::Ternary(r(c), r(a), r(b)),
        Expr::Match(scr, arms) => {
            let new_arms: Vec<MatchArm> = arms.iter().map(|arm| {
                // Pattern-bound names shadow field names within this arm.
                let shadowed: HashSet<String> = match &arm.pattern {
                    MatchPattern::Ctor { binds, .. } => binds.iter()
                        .filter_map(|b| b.clone())
                        .collect(),
                    MatchPattern::Wildcard => HashSet::new(),
                };
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

/// Recursively replace identifier paths that start with `bound_var`
/// with the per-iteration element expression. Handles bare matches
/// (`p`) and dotted suffixes (`p.color`, `p.aabb.pos.x`).
///
/// `elem_expr` is the expression that the bound variable refers to
/// at this iteration (e.g. `Index(Identifier("platforms"), Int(i))`).
/// A dotted suffix like `p.color` becomes
/// `Field(elem_expr, "color")`; deeper paths chain `Field`s.
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
            // Inner quantifier shadows the substitution if it rebinds
            // the same name.
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
