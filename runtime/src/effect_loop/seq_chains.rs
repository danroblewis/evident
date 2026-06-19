use crate::core::ast::{BodyItem, Expr, BinOp};
use std::collections::HashSet;

pub(super) fn extract_seq_effect_chains(
    body: &[BodyItem],
    effect_node_set: &HashSet<&String>,
) -> Vec<Vec<String>> {

    fn node_name(e: &Expr, set: &HashSet<&String>) -> Option<String> {
        match e {
            Expr::Identifier(n) if set.contains(n) => Some(n.clone()),
            Expr::Index(seq, idx) => match seq.as_ref() {
                Expr::Identifier(name) => {
                    if let Expr::Int(i) = idx.as_ref() {
                        let syn = format!("{}[{}]", name, i);
                        if set.contains(&syn) { return Some(syn); }
                    }
                    None
                }
                Expr::Field(inner_seq, field) => {
                    let Expr::Index(outer_seq, outer_idx) = inner_seq.as_ref() else {
                        return None;
                    };
                    let Expr::Identifier(outer_name) = outer_seq.as_ref() else {
                        return None;
                    };
                    let (Expr::Int(i), Expr::Int(j)) = (outer_idx.as_ref(), idx.as_ref())
                        else { return None };
                    let syn = format!("{}[{}].{}[{}]", outer_name, i, field, j);
                    if set.contains(&syn) { Some(syn) } else { None }
                }
                _ => None,
            },
            _ => None,
        }
    }
    let mut chains: Vec<Vec<String>> = Vec::new();
    for item in body {
        if let BodyItem::Constraint(Expr::Binary(BinOp::Eq, lhs, rhs)) = item {
            let seq_items = match (lhs.as_ref(), rhs.as_ref()) {
                (_, Expr::SeqLit(items)) => items,
                (Expr::SeqLit(items), _) => items,
                _ => continue,
            };
            let names: Vec<String> = seq_items.iter()
                .filter_map(|e| node_name(e, effect_node_set))
                .collect();

            if names.len() != seq_items.len() { continue; }
            chains.push(names);
        }
    }
    chains
}
