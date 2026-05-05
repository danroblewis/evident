//! AST → readable-infix string. Used for diagnostics on UNSAT (so the
//! user sees `state.dots[i].pos_x = state.dots[i].pos_x` instead of a
//! deeply-nested `Binary(Eq, Field(Index(Identifier("state.dots"),
//! Identifier("i")), "pos_x"), …)` tree).
//!
//! Not a precise round-trip pretty-printer — operator spacing matches
//! source style and Unicode operators (∈, ∀, ⇒, …) are restored, but
//! nothing here is parsed back. If a future feature needs accurate
//! re-parse, write a separate one.

use crate::ast::{BinOp, BodyItem, Expr, Mapping};

pub fn expr(e: &Expr) -> String {
    match e {
        Expr::Identifier(n) => n.clone(),
        Expr::Int(n)        => n.to_string(),
        Expr::Bool(b)       => b.to_string(),
        Expr::Str(s)        => format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\"")),
        Expr::SetLit(items) => format!("{{{}}}", items.iter().map(expr).collect::<Vec<_>>().join(", ")),
        Expr::SeqLit(items) => format!("⟨{}⟩",   items.iter().map(expr).collect::<Vec<_>>().join(", ")),
        Expr::Range(lo, hi) => format!("{{{}..{}}}", expr(lo), expr(hi)),
        Expr::InExpr(lhs, rhs) => format!("{} ∈ {}", expr(lhs), expr(rhs)),
        Expr::Forall(v, range, body) =>
            format!("∀ {} ∈ {} : {}", v, expr(range), expr(body)),
        Expr::Exists(v, range, body) =>
            format!("∃ {} ∈ {} : {}", v, expr(range), expr(body)),
        Expr::Cardinality(inner) => format!("#{}", expr(inner)),
        Expr::Index(seq, idx)    => format!("{}[{}]", expr(seq), expr(idx)),
        Expr::Field(receiver, f) => format!("{}.{}", expr(receiver), f),
        Expr::Not(inner)         => format!("¬({})", expr(inner)),
        Expr::Binary(op, lhs, rhs) => {
            let l = expr(lhs);
            let r = expr(rhs);
            // Wrap any Binary operand in parens — cheap, slightly noisy,
            // never wrong. A precedence-aware printer is overkill for
            // diagnostics.
            let l = if matches!(lhs.as_ref(), Expr::Binary(..)) { format!("({})", l) } else { l };
            let r = if matches!(rhs.as_ref(), Expr::Binary(..)) { format!("({})", r) } else { r };
            format!("{} {} {}", l, binop_sym(op), r)
        }
    }
}

fn binop_sym(op: &BinOp) -> &'static str {
    match op {
        BinOp::Eq      => "=",
        BinOp::Neq     => "≠",
        BinOp::Lt      => "<",
        BinOp::Le      => "≤",
        BinOp::Gt      => ">",
        BinOp::Ge      => "≥",
        BinOp::And     => "∧",
        BinOp::Or      => "∨",
        BinOp::Implies => "⇒",
        BinOp::Add     => "+",
        BinOp::Sub     => "-",
        BinOp::Mul     => "*",
        BinOp::Div     => "/",
        BinOp::Concat  => "++",
    }
}

pub fn body_item(item: &BodyItem) -> String {
    match item {
        BodyItem::Membership { name, type_name } => format!("{} ∈ {}", name, type_name),
        BodyItem::Passthrough(c) => format!("..{}", c),
        BodyItem::SubclaimDecl(s) => format!("subclaim {} (…)", s.name),
        BodyItem::ClaimCall { name, mappings } => {
            if mappings.is_empty() {
                name.clone()
            } else {
                format!("{} ({})", name, mappings.iter().map(mapping).collect::<Vec<_>>().join(", "))
            }
        }
        BodyItem::Constraint(e) => expr(e),
    }
}

fn mapping(m: &Mapping) -> String {
    format!("{} ↦ {}", m.slot, expr(&m.value))
}
