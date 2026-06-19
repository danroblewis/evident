use crate::core::RuntimeError;
use crate::core::ast::SchemaDecl;
use std::collections::HashMap;

pub(super) fn desugar_seq_concat(s: &mut SchemaDecl) {
    use crate::core::ast::{BinOp, BodyItem, Expr};
    if s.external { return; }

    let mut seq_lits: HashMap<String, Vec<Expr>> = HashMap::new();
    for item in &s.body {
        let BodyItem::Constraint(Expr::Binary(BinOp::Eq, lhs, rhs)) = item else { continue };
        if let (Expr::Identifier(name), Expr::SeqLit(items)) =
            (lhs.as_ref(), rhs.as_ref())
        {
            seq_lits.insert(name.clone(), items.clone());
        }
    }

    fn flatten(
        e: &Expr,
        seq_lits: &HashMap<String, Vec<Expr>>,
    ) -> Option<Vec<Expr>> {
        match e {
            Expr::Binary(BinOp::Concat, l, r) => {
                let mut left = flatten(l, seq_lits)?;
                let right = flatten(r, seq_lits)?;
                left.extend(right);
                Some(left)
            }
            Expr::SeqLit(items) => Some(items.clone()),
            Expr::Identifier(name) => seq_lits.get(name).cloned(),
            _ => None,
        }
    }

    fn rewrite(
        e: &mut Expr,
        seq_lits: &HashMap<String, Vec<Expr>>,
    ) {
        if let Expr::Binary(BinOp::Concat, ..) = e {
            if let Some(items) = flatten(e, seq_lits) {
                *e = Expr::SeqLit(items);
                return;
            }
        }
        match e {
            Expr::Binary(_, l, r)
            | Expr::Range(l, r)
            | Expr::InExpr(l, r)
            | Expr::Index(l, r) => { rewrite(l, seq_lits); rewrite(r, seq_lits); }
            Expr::Ternary(c, a, b) => {
                rewrite(c, seq_lits); rewrite(a, seq_lits); rewrite(b, seq_lits);
            }
            Expr::SetLit(es) | Expr::SeqLit(es) | Expr::Tuple(es)
            | Expr::Call(_, es) => {
                for x in es { rewrite(x, seq_lits); }
            }
            Expr::Forall(_, r, b) | Expr::Exists(_, r, b) => {
                rewrite(r, seq_lits); rewrite(b, seq_lits);
            }
            Expr::Cardinality(i) | Expr::Not(i) | Expr::Matches(i, _) => {
                rewrite(i, seq_lits);
            }
            Expr::Field(recv, _) => rewrite(recv, seq_lits),
            Expr::Match(scr, arms) => {
                rewrite(scr, seq_lits);
                for a in arms { rewrite(&mut a.body, seq_lits); }
            }
            _ => {}
        }
    }

    for item in s.body.iter_mut() {
        match item {
            BodyItem::Constraint(e) => rewrite(e, &seq_lits),
            BodyItem::ClaimCall { mappings, .. } => {
                for m in mappings.iter_mut() {
                    rewrite(&mut m.value, &seq_lits);
                }
            }
            _ => {}
        }
    }

    for item in s.body.iter_mut() {
        if let BodyItem::SubclaimDecl(sub) = item {
            desugar_seq_concat(sub);
        }
    }
}

pub(super) fn unify_world_syntax(s: &mut SchemaDecl) -> Result<(), RuntimeError> {
    use crate::core::ast::{BodyItem, Expr, Keyword, Pins};
    if !matches!(s.keyword, Keyword::Fsm) { return Ok(()); }
    if s.external { return Ok(()); }

    let mut world_type: Option<String> = None;
    let mut has_world_next = false;
    for item in &s.body {
        if let BodyItem::Membership { name, type_name, .. } = item {
            if name == "world" { world_type = Some(type_name.clone()); }
            if name == "world_next" { has_world_next = true; }
        }
    }
    let Some(world_ty) = world_type else { return Ok(()); };
    if has_world_next { return Ok(()); }

    fn uses_underscore_world(e: &Expr) -> bool {
        let mut found = false;
        crate::core::ast::walk_expr(e, &mut |n| {
            if let Expr::Identifier(n) = n {
                if n.starts_with("_world.") { found = true; }
            }
        });
        found
    }
    let uses_new_syntax = s.body.iter().any(|item| match item {
        BodyItem::Constraint(e) => uses_underscore_world(e),
        BodyItem::ClaimCall { mappings, .. } =>
            mappings.iter().any(|m| uses_underscore_world(&m.value)),
        _ => false,
    });
    if !uses_new_syntax { return Ok(()); }

    fn rewrite_ident(name: &str) -> Option<String> {
        if let Some(rest) = name.strip_prefix("_world.") {
            return Some(format!("world.{rest}"));
        }
        if let Some(rest) = name.strip_prefix("world.") {
            return Some(format!("world_next.{rest}"));
        }
        None
    }
    fn walk(e: &mut Expr) {
        crate::core::ast::walk_expr_mut(e, &mut |n| {
            if let Expr::Identifier(n) = n {
                if let Some(new_n) = rewrite_ident(n) { *n = new_n; }
            }
        });
    }
    for item in s.body.iter_mut() {
        match item {
            BodyItem::Constraint(e) => walk(e),
            BodyItem::ClaimCall { mappings, .. } =>
                for m in mappings { walk(&mut m.value); },

            BodyItem::Membership { pins, .. } => match pins {
                Pins::Named(named) => for m in named { walk(&mut m.value); },
                Pins::Positional(vals) => for v in vals { walk(v); },
                Pins::None => {}
            },
            _ => {}
        }
    }

    let insert_pos = s.param_count;
    s.body.insert(insert_pos, BodyItem::Membership {
        name: "world_next".to_string(),
        type_name: world_ty,
        pins: Pins::None,
    });
    Ok(())
}
