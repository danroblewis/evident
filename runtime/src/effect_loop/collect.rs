use crate::core::ast::{Effect, BodyItem, Expr, BinOp};
use crate::runtime::EvidentRuntime;
use crate::translate::{Value, effect_decoder};
use std::collections::{HashMap, HashSet};

use super::seq_chains::extract_seq_effect_chains;
use super::toposort::{
    DISPATCH_ORDER_CACHE, DispatchKey,
    resolve_synthetic_names_to_effects,
    topo_sort_with_random_tiebreak,
};

pub(crate) fn collect_dispatchable_effects(
    rt: &EvidentRuntime,
    claim_name: &str,
    bindings: &HashMap<String, Value>,
    primary_var: Option<&str>,
) -> Vec<Effect> {

    if let Some(pv) = primary_var {
        if let Some(Value::SeqEnum(items)) = bindings.get(pv) {
            return items.iter()
                .filter_map(|v| effect_decoder::decode_effect(v).ok())
                .collect();
        }

    }

    let has_body_seqlit: HashSet<&str> = match rt.get_schema(claim_name) {
        Some(schema) => schema.body.iter().filter_map(|item| match item {
            BodyItem::Constraint(Expr::Binary(BinOp::Eq, lhs, rhs)) => {
                let lhs_name = match (lhs.as_ref(), rhs.as_ref()) {
                    (Expr::Identifier(n), Expr::SeqLit(_)) => Some(n.as_str()),
                    (Expr::SeqLit(_), Expr::Identifier(n)) => Some(n.as_str()),
                    _ => None,
                };
                lhs_name
            }
            _ => None,
        }).collect(),
        None => HashSet::new(),
    };

    let mut node_values: HashMap<String, Effect> = HashMap::new();
    let mut all_names: Vec<String> = Vec::new();
    let mut all_auto_edges: Vec<(String, String)> = Vec::new();
    for (name, v) in bindings {
        match v {
            Value::Enum { enum_name, .. } if enum_name == "Effect" => {
                if let Ok(e) = effect_decoder::decode_effect(v) {
                    node_values.insert(name.clone(), e);
                    all_names.push(name.clone());
                }
            }
            Value::SeqEnum(items) => {
                let is_effect_seq = !items.is_empty() && items.iter().all(|it|
                    matches!(it, Value::Enum { enum_name, .. } if enum_name == "Effect")
                );

                if is_effect_seq && !has_body_seqlit.contains(name.as_str()) {
                    let mut prev: Option<String> = None;
                    for (i, item) in items.iter().enumerate() {
                        if let Ok(e) = effect_decoder::decode_effect(item) {
                            let syn = format!("{}[{}]", name, i);
                            node_values.insert(syn.clone(), e);
                            all_names.push(syn.clone());
                            if let Some(p) = prev.take() {
                                all_auto_edges.push((p, syn.clone()));
                            }
                            prev = Some(syn);
                        }
                    }
                }
            }

            Value::SeqComposite(items) => {
                for (i, fields_map) in items.iter().enumerate() {
                    for (fname, fval) in fields_map {
                        let Value::SeqEnum(inner) = fval else { continue };
                        let is_effect_inner = !inner.is_empty() && inner.iter().all(|it|
                            matches!(it, Value::Enum { enum_name, .. }
                                if enum_name == "Effect")
                        );
                        if !is_effect_inner { continue; }
                        let mut prev: Option<String> = None;
                        for (j, item) in inner.iter().enumerate() {
                            if let Ok(e) = effect_decoder::decode_effect(item) {
                                let syn = format!("{}[{}].{}[{}]", name, i, fname, j);
                                node_values.insert(syn.clone(), e);
                                all_names.push(syn.clone());
                                if let Some(p) = prev.take() {
                                    all_auto_edges.push((p, syn.clone()));
                                }
                                prev = Some(syn);
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }
    if all_names.is_empty() { return Vec::new(); }

    let mut nodes: Vec<String> = all_names.clone();
    let auto_edges: Vec<(String, String)> = all_auto_edges.clone();

    let alias_to_canonical: HashMap<String, String> =
        all_names.iter().map(|n| (n.clone(), n.clone())).collect();

    let alias_set: HashSet<&String> = all_names.iter().collect();
    let raw_chains = match rt.get_schema(claim_name) {
        Some(schema) => extract_seq_effect_chains(&schema.body, &alias_set),
        None => Vec::new(),
    };
    let mut edges: Vec<(String, String)> = Vec::new();
    for chain in raw_chains {
        let mut deduped: Vec<String> = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();
        for name in &chain {
            let canon = alias_to_canonical.get(name).cloned().unwrap_or_else(|| name.clone());
            if seen.insert(canon.clone()) {
                deduped.push(canon);
            }
        }
        for w in deduped.windows(2) {
            edges.push((w[0].clone(), w[1].clone()));
        }
    }
    edges.extend(auto_edges);

    use rand::seq::SliceRandom;
    use rand::SeedableRng;
    let seed: u64 = {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now().duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64).unwrap_or(0)
    };
    let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
    nodes.shuffle(&mut rng);

    if edges.is_empty() {
        return resolve_synthetic_names_to_effects(&nodes, &node_values);
    }

    let mut canon_nodes = nodes.clone();
    canon_nodes.sort();
    let mut canon_edges = edges.clone();
    canon_edges.sort();
    let cache_key: DispatchKey = (canon_nodes, canon_edges);
    {
        let mut guard = DISPATCH_ORDER_CACHE.lock().unwrap();
        if let Some(map) = guard.as_ref() {
            if let Some(cached) = map.get(&cache_key) {
                return resolve_synthetic_names_to_effects(cached, &node_values);
            }
        } else {
            *guard = Some(HashMap::new());
        }
    }

    let sorted_names = topo_sort_with_random_tiebreak(&nodes, &edges, &mut rng);

    if let Ok(mut guard) = DISPATCH_ORDER_CACHE.lock() {
        if let Some(map) = guard.as_mut() {
            map.insert(cache_key, sorted_names.clone());
        }
    }

    resolve_synthetic_names_to_effects(&sorted_names, &node_values)
}
