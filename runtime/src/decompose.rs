//! Union-find decomposition: split a claim's assertions into independent sub-models.
//! Purely structural — no `Z3.check()` calls. Linear in formula size after mild normalization.

use std::collections::HashMap;
use z3::ast::{Ast, Bool};
use z3::{AstKind, Context, Goal, Tactic};

/// One separable sub-model: variables (sorted) + constraint indices from the original list.
#[derive(Debug, Clone)]
pub struct Component {
    pub vars: Vec<String>,
    pub constraint_indices: Vec<usize>,
}

/// Decompose assertions into connected components over `var_names`.
/// Names absent from `var_names` are treated as pinned and don't link components.
pub fn decompose<'ctx>(
    ctx: &'ctx Context,
    assertions: &[Bool<'ctx>],
    var_names: &[String],
) -> Vec<Component> {
    // `simplify` only — NOT `solve-eqs` (it eliminates variables by substitution,
    // destroying dependency structure; belongs inside per-component analysis).
    let goal = Goal::new(ctx, false, false, false);
    for c in assertions {
        goal.assert(c);
    }
    let normalized = mild_normalize(ctx, &goal);

    let name_index: HashMap<&str, usize> = var_names.iter().enumerate()
        .map(|(i, n)| (n.as_str(), i)).collect();
    let mut uf = UnionFind::new(var_names.len());
    let mut per_constraint_vars: Vec<Vec<usize>> = Vec::new();

    for subgoal in normalized.into_iter() {
        for formula in subgoal.iter_formulas::<Bool>() {
            let vars = collect_free_vars(&formula, &name_index);
            for w in vars.windows(2) {
                uf.union(w[0], w[1]);
            }
            per_constraint_vars.push(vars);
        }
    }

    components_from_uf(&uf, var_names, &per_constraint_vars)
}

fn mild_normalize<'ctx>(ctx: &'ctx Context, goal: &Goal<'ctx>) -> Vec<Goal<'ctx>> {
    let simplify = Tactic::new(ctx, "simplify");
    let result = simplify.apply(goal, None).expect("tactic apply");
    result.list_subgoals().collect()
}

/// Collect `name_index` indices for every 0-arg App matching a known variable.
/// Bound variables inside quantifiers use `AstKind::Var`, so they're skipped naturally.
fn collect_free_vars<'ctx>(
    ast: &impl Ast<'ctx>,
    name_index: &HashMap<&str, usize>,
) -> Vec<usize> {
    let mut out = Vec::new();
    walk(ast, &mut |a| {
        if a.kind() == AstKind::App && a.num_children() == 0 {
            if let Ok(decl) = a.safe_decl() {
                let nm = decl.name().to_string();
                if let Some(&i) = name_index.get(nm.as_str()) {
                    out.push(i);
                }
            }
        }
    });
    out.sort_unstable();
    out.dedup();
    out
}

/// Recursive walker. Descends into App children; bound vars appear as `AstKind::Var` and are
/// skipped by `collect_free_vars`.
fn walk<'ctx, A: Ast<'ctx>>(ast: &A, f: &mut impl FnMut(&dyn Ast<'ctx>)) {
    f(ast);
    if ast.kind() == AstKind::App {
        for child in ast.children() { walk(&child, f); }
    }
}

struct UnionFind {
    parent: Vec<usize>,
    rank:   Vec<u8>,
}

impl UnionFind {
    fn new(n: usize) -> Self {
        UnionFind {
            parent: (0..n).collect(),
            rank:   vec![0; n],
        }
    }
    fn find(&mut self, x: usize) -> usize {
        let mut r = x;
        while self.parent[r] != r { r = self.parent[r]; }
        let mut y = x; // path compression
        while self.parent[y] != r {
            let next = self.parent[y];
            self.parent[y] = r;
            y = next;
        }
        r
    }
    fn union(&mut self, a: usize, b: usize) {
        let ra = self.find(a);
        let rb = self.find(b);
        if ra == rb { return; }
        if self.rank[ra] < self.rank[rb] {
            self.parent[ra] = rb;
        } else if self.rank[ra] > self.rank[rb] {
            self.parent[rb] = ra;
        } else {
            self.parent[rb] = ra;
            self.rank[ra] += 1;
        }
    }
}

fn components_from_uf(
    uf: &UnionFind,
    var_names: &[String],
    per_constraint_vars: &[Vec<usize>],
) -> Vec<Component> {
    let mut buckets: HashMap<usize, Vec<usize>> = HashMap::new();
    let mut uf_q = UnionFind { parent: uf.parent.clone(), rank: uf.rank.clone() };
    for i in 0..var_names.len() {
        let r = uf_q.find(i);
        buckets.entry(r).or_default().push(i);
    }
    let mut constraint_buckets: HashMap<usize, Vec<usize>> = HashMap::new();
    for (idx, vars) in per_constraint_vars.iter().enumerate() {
        if let Some(&first) = vars.first() {
            let r = uf_q.find(first);
            constraint_buckets.entry(r).or_default().push(idx);
        }
    }
    let mut comps: Vec<Component> = buckets.into_iter().map(|(r, vs)| {
        let names: Vec<String> = vs.into_iter().map(|i| var_names[i].clone()).collect();
        let cs = constraint_buckets.remove(&r).unwrap_or_default();
        Component { vars: names, constraint_indices: cs }
    }).collect();
    comps.sort_by(|a, b| b.vars.len().cmp(&a.vars.len())
        .then_with(|| a.vars.first().cmp(&b.vars.first())));
    for c in comps.iter_mut() {
        c.vars.sort();
    }
    comps
}

#[cfg(test)]
mod tests {
    use super::*;
    use z3::ast::Int;
    use z3::Config;

    fn ctx() -> &'static Context {
        let cfg = Config::new();
        Box::leak(Box::new(Context::new(&cfg)))
    }

    #[test]
    fn three_disjoint_clusters_become_three_components() {
        let ctx = ctx();
        let a = Int::new_const(ctx, "a");
        let b = Int::new_const(ctx, "b");
        let c = Int::new_const(ctx, "c");
        let d = Int::new_const(ctx, "d");
        let e = Int::new_const(ctx, "e");
        let f = Int::new_const(ctx, "f");

        let one = Int::from_i64(ctx, 1);
        let assertions = vec![
            a._eq(&(b.clone() + one.clone())),
            c.gt(&d),
            e._eq(&(f.clone() * Int::from_i64(ctx, 2))),
        ];
        let names = vec!["a", "b", "c", "d", "e", "f"]
            .into_iter().map(String::from).collect::<Vec<_>>();
        let comps = decompose(ctx, &assertions, &names);
        assert_eq!(comps.len(), 3, "got {} components: {:?}", comps.len(), comps);
        for c in &comps { assert_eq!(c.vars.len(), 2, "{:?}", c); }
    }

    #[test]
    fn shared_variable_merges_components() {
        let ctx = ctx();
        let a = Int::new_const(ctx, "a");
        let b = Int::new_const(ctx, "b");
        let x = Int::new_const(ctx, "x");
        let c = Int::new_const(ctx, "c");
        let d = Int::new_const(ctx, "d");

        let assertions = vec![
            a._eq(&(x.clone() + Int::from_i64(ctx, 1))),
            b._eq(&(x.clone() + Int::from_i64(ctx, 2))),
            c._eq(&d),
        ];
        let names = vec!["a", "b", "x", "c", "d"]
            .into_iter().map(String::from).collect::<Vec<_>>();
        let comps = decompose(ctx, &assertions, &names);
        assert_eq!(comps.len(), 2, "got {} components: {:?}", comps.len(), comps);
        // Largest component should have a, b, x.
        assert_eq!(comps[0].vars.len(), 3);
        assert_eq!(comps[1].vars.len(), 2);
    }

    #[test]
    fn variable_with_no_constraints_is_its_own_component() {
        let ctx = ctx();
        let a = Int::new_const(ctx, "a");
        let b = Int::new_const(ctx, "b");
        let _orphan = Int::new_const(ctx, "orphan");

        let assertions = vec![a._eq(&b)];
        let names = vec!["a", "b", "orphan"]
            .into_iter().map(String::from).collect::<Vec<_>>();
        let comps = decompose(ctx, &assertions, &names);
        assert_eq!(comps.len(), 2);
        assert_eq!(comps[0].vars, vec!["a".to_string(), "b".to_string()]);
        assert_eq!(comps[1].vars, vec!["orphan".to_string()]);
    }

    #[test]
    fn empty_constraints_give_singleton_components() {
        let ctx = ctx();
        let _a = Int::new_const(ctx, "a");
        let _b = Int::new_const(ctx, "b");

        let assertions: Vec<Bool> = vec![];
        let names = vec!["a", "b"]
            .into_iter().map(String::from).collect::<Vec<_>>();
        let comps = decompose(ctx, &assertions, &names);
        assert_eq!(comps.len(), 2);
        for c in &comps {
            assert_eq!(c.vars.len(), 1);
        }
    }

    #[test]
    fn given_like_vars_omitted_dont_link_components() {
        // `pinned` absent from var_names → treated as broadcast constant; a and b stay separate.
        let ctx = ctx();
        let a = Int::new_const(ctx, "a");
        let b = Int::new_const(ctx, "b");
        let pinned = Int::new_const(ctx, "pinned");
        let assertions = vec![a._eq(&pinned), b._eq(&pinned)];
        let names = vec!["a", "b"]
            .into_iter().map(String::from).collect::<Vec<_>>();
        let comps = decompose(ctx, &assertions, &names);
        // a and b should be SEPARATE — pinned is broadcast.
        assert_eq!(comps.len(), 2, "given-like vars must not link: {:?}", comps);
    }
}
