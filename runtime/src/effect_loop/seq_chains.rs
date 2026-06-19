//! Body `Seq(Effect)` edge extraction.
//!
//! Pulls pairwise ordering edges from a claim's body — each
//! `name = ⟨a, b, c⟩` literal contributes edges (a→b, b→c) between
//! its resolved node names. The toposort downstream consumes these
//! to derive a dispatch ordering.

use crate::core::ast::{BodyItem, Expr, BinOp, Effect};
use crate::translate::{Value, effect_decoder};
use std::collections::{HashMap, HashSet};

/// Walk a body for `Seq(Effect)` literal assignments and pull out
/// pairwise ordering edges. Recognized shape: `Constraint(Eq(Identifier(_),
/// SeqLit(elements)))` where every element is `Identifier(name)` and
/// `name` is in `effect_node_set`. Each such literal contributes edges
/// `(elements[i], elements[i+1])` for `i in 0..len-1`.
///
/// Membership-with-equality forms like `xs ∈ Seq(Effect) = ⟨a, b, c⟩`
/// desugar (during parse) into a `Membership` plus a separate
/// `Constraint(Eq(Identifier("xs"), SeqLit(...)))` — only the
/// Constraint half is what we look for here.
pub(super) fn extract_seq_effect_chains(
    body: &[BodyItem],
    effect_node_set: &HashSet<&String>,
) -> Vec<Vec<String>> {
    // Resolve a SeqLit element to its node name. Recognizes:
    //   * `Identifier(name)` where `name` is a bare Effect binding.
    //   * `Index(Identifier(name), Int(i))` where `name[i]` names a
    //     synthetic Seq(Effect) element (e.g. `hat_effs[0]`).
    //   * `Index(Field(Index(Identifier(outer), Int(i)), field), Int(j))`
    //     where `outer[i].field[j]` names a synthetic
    //     Seq(Composite-with-Seq-Effect-field) element (e.g.
    //     `plat_effs[0].effs[0]`).
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
            // Only emit a chain when every element resolves to a known
            // node. A Seq with even one unresolved element isn't a clean
            // ordering chain — bail.
            if names.len() != seq_items.len() { continue; }
            chains.push(names);
        }
    }
    chains
}

/// Map dispatch-order binding names back to their Effect values from
/// the model. Names not resolving to a decodable Effect (e.g. `NoEffect`
/// the runtime drops) are filtered.
#[allow(dead_code)]
pub(super) fn resolve_nodes_to_effects(
    names: &[String],
    bindings: &HashMap<String, Value>,
) -> Vec<Effect> {
    names.iter()
        .filter_map(|name| bindings.get(name))
        .filter_map(|v| effect_decoder::decode_effect(v).ok())
        .collect()
}
