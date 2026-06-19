use crate::core::ast::Effect;
use std::collections::HashMap;
use std::sync::Mutex;

pub(super) static DISPATCH_ORDER_CACHE: Mutex<Option<HashMap<DispatchKey, Vec<String>>>>
    = Mutex::new(None);

pub(super) type DispatchKey = (Vec<String>, Vec<(String, String)>);

pub(super) fn resolve_synthetic_names_to_effects(
    names: &[String],
    node_values: &HashMap<String, Effect>,
) -> Vec<Effect> {
    names.iter()
        .filter_map(|n| node_values.get(n).cloned())
        .collect()
}

pub(super) fn topo_sort_with_random_tiebreak(
    nodes: &[String],
    edges: &[(String, String)],
    rng: &mut rand::rngs::StdRng,
) -> Vec<String> {
    use rand::seq::SliceRandom;
    use std::collections::HashSet;

    let mut in_degree: HashMap<&str, usize> = nodes.iter()
        .map(|n| (n.as_str(), 0))
        .collect();
    let mut adj: HashMap<&str, Vec<&str>> = HashMap::new();
    for (from, to) in edges {

        if !in_degree.contains_key(to.as_str()) { continue; }
        if !in_degree.contains_key(from.as_str()) { continue; }
        adj.entry(from.as_str()).or_default().push(to.as_str());
        *in_degree.get_mut(to.as_str()).unwrap() += 1;
    }

    let mut ready: Vec<&str> = in_degree.iter()
        .filter(|(_, &d)| d == 0)
        .map(|(&n, _)| n)
        .collect();
    ready.shuffle(rng);

    let mut out: Vec<String> = Vec::with_capacity(nodes.len());
    while let Some(_) = ready.first() {

        ready.shuffle(rng);
        let n = ready.pop().unwrap();
        out.push(n.to_string());
        if let Some(succs) = adj.get(n) {
            for &m in succs {
                let d = in_degree.get_mut(m).unwrap();
                *d -= 1;
                if *d == 0 { ready.push(m); }
            }
        }
    }

    if out.len() < nodes.len() {

        eprintln!("warning: cycle in declared Effect ordering edges — \
                   {} of {} nodes emitted before stall; remaining nodes \
                   appended in input order",
                  out.len(), nodes.len());
        let emitted: HashSet<String> = out.iter().cloned().collect();
        for n in nodes {
            if !emitted.contains(n) {
                out.push(n.clone());
            }
        }
    }

    out
}
