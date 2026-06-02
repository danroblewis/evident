//! Source-level desugaring: `++` Seq concat flattening.
//!
//! Walks the schema body, collects `name = ⟨items⟩` bindings, and rewrites
//! any `Binary(Concat, …)` subtree whose leaves are all SeqLit (literal or
//! via the gathered binding map) into a single flat `SeqLit`. Opaque-leaf
//! subtrees are left alone for the translator to surface as a normal type
//! error.

use crate::core::ast::{BinOp, BodyItem, Expr, Mapping, Pins, SchemaDecl};
use std::collections::HashMap;

pub(crate) fn desugar_seq_concat(s: &mut SchemaDecl) {
    // Gather `name = ⟨…⟩` bindings appearing as constraints.
    let mut bindings: HashMap<String, Vec<Expr>> = HashMap::new();
    for item in &s.body {
        if let BodyItem::Constraint(Expr::Binary(BinOp::Eq, lhs, rhs)) = item {
            if let (Expr::Identifier(n), Expr::SeqLit(items)) = (lhs.as_ref(), rhs.as_ref()) {
                bindings.insert(n.clone(), items.clone());
            }
        }
    }

    fn try_flatten(e: &Expr, bindings: &HashMap<String, Vec<Expr>>) -> Option<Vec<Expr>> {
        match e {
            Expr::SeqLit(items) => Some(items.clone()),
            Expr::Identifier(n) => bindings.get(n).cloned(),
            Expr::Binary(BinOp::Concat, a, b) => {
                let mut out = try_flatten(a, bindings)?;
                out.extend(try_flatten(b, bindings)?);
                Some(out)
            }
            _ => None,
        }
    }

    fn walk(e: &mut Expr, bindings: &HashMap<String, Vec<Expr>>) {
        if let Expr::Binary(BinOp::Concat, _, _) = e {
            if let Some(flat) = try_flatten(e, bindings) {
                *e = Expr::SeqLit(flat);
                for child in match e { Expr::SeqLit(xs) => xs, _ => unreachable!() } {
                    walk(child, bindings);
                }
                return;
            }
        }
        match e {
            Expr::Identifier(_) | Expr::Int(_) | Expr::Real(_) | Expr::Bool(_) | Expr::Str(_) => {}
            Expr::SetLit(es) | Expr::SeqLit(es) | Expr::Tuple(es) => {
                for x in es { walk(x, bindings); }
            }
            Expr::Range(a, b) | Expr::InExpr(a, b) | Expr::Index(a, b) => {
                walk(a, bindings); walk(b, bindings);
            }
            Expr::Forall(_, r, b) | Expr::Exists(_, r, b) => {
                walk(r, bindings); walk(b, bindings);
            }
            Expr::Call(_, args) => for a in args { walk(a, bindings); },
            Expr::Cardinality(i) | Expr::Not(i) => walk(i, bindings),
            Expr::Field(recv, _) => walk(recv, bindings),
            Expr::Binary(_, l, r) => { walk(l, bindings); walk(r, bindings); }
            Expr::Ternary(c, a, b) => { walk(c, bindings); walk(a, bindings); walk(b, bindings); }
            Expr::Match(scr, arms) => {
                walk(scr, bindings);
                for arm in arms { walk(arm.body.as_mut(), bindings); }
            }
            Expr::Matches(e, _) => walk(e, bindings),
        }
    }

    fn walk_mappings(ms: &mut [Mapping], b: &HashMap<String, Vec<Expr>>) {
        for m in ms { walk(&mut m.value, b); }
    }

    for item in s.body.iter_mut() {
        match item {
            BodyItem::Constraint(e) => walk(e, &bindings),
            BodyItem::ClaimCall { mappings, .. } => walk_mappings(mappings, &bindings),
            BodyItem::Membership { pins, .. } => match pins {
                Pins::Named(named) => walk_mappings(named, &bindings),
                Pins::Positional(vals) => for v in vals { walk(v, &bindings); },
                Pins::None => {}
            },
            _ => {}
        }
    }
}
