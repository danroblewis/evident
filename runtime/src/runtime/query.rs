use crate::core::{CompiledModel, QueryResult, RuntimeError, Var, Z3Step};
use crate::functionize::cranelift::JitProgram;
use super::{EvidentRuntime, Value};
use crate::translate::run_cached;
use crate::z3_eval::{collect_touched_names, extract_program_partial,
                     recompose_record_seqs, simplify_assertions};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use z3::ast::{Ast, Bool};
use z3::{Context, Params, SatResult, Solver, Tactic};
use z3_sys::DeclKind;

fn component_has_defining_assertion(assertions: &[Bool<'static>]) -> bool {
    !assertions.iter().all(|a| {
        a.safe_decl().ok()
            .map(|d| matches!(d.kind(),
                DeclKind::GE | DeclKind::GT | DeclKind::LE | DeclKind::LT))
            .unwrap_or(false)
    })
}

pub(crate) struct ClaimPlan {

    pub(super) compiled: Vec<Rc<JitProgram>>,

    pub(super) slow: Option<SlowPart>,

    pub(super) pinned_ints: Vec<(String, Value)>,
}

pub(crate) struct SlowPart {

    cached: CompiledModel<'static>,

    outputs: Vec<String>,
}

enum ComponentOutcome {

    Compiled(Rc<JitProgram>),

    Slow,

    Bail,
}

struct UnionFind {
    parent: Vec<usize>,
    rank: Vec<u8>,
}

impl UnionFind {
    fn new(n: usize) -> Self {
        UnionFind { parent: (0..n).collect(), rank: vec![0; n] }
    }
    fn find(&mut self, x: usize) -> usize {
        let mut r = x;
        while self.parent[r] != r { r = self.parent[r]; }
        let mut y = x;
        while self.parent[y] != r {
            let next = self.parent[y];
            self.parent[y] = r;
            y = next;
        }
        r
    }
    fn union(&mut self, a: usize, b: usize) {
        let (ra, rb) = (self.find(a), self.find(b));
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

fn decompose_simplified(
    simplified: &[Bool<'static>],
    outputs: &[String],
) -> (Vec<Vec<String>>, Vec<Vec<usize>>, Vec<usize>) {
    let index_of: HashMap<&str, usize> = outputs.iter().enumerate()
        .map(|(i, n)| (n.as_str(), i)).collect();
    let mut uf = UnionFind::new(outputs.len());

    let mut per_assert: Vec<Vec<usize>> = Vec::with_capacity(simplified.len());
    for a in simplified {
        let mut touched: HashSet<String> = HashSet::new();
        collect_touched_names(a, &mut touched);

        let mut idxs: Vec<usize> = touched.iter()
            .filter_map(|n| {
                let base = n.strip_suffix("__len")
                    .or_else(|| n.strip_suffix("__arr"))
                    .unwrap_or(n.as_str());
                index_of.get(base).copied()
            })
            .collect();
        idxs.sort_unstable();
        idxs.dedup();
        for w in idxs.windows(2) { uf.union(w[0], w[1]); }
        per_assert.push(idxs);
    }

    let mut root_to_comp: HashMap<usize, usize> = HashMap::new();
    let mut comp_vars: Vec<Vec<String>> = Vec::new();
    for i in 0..outputs.len() {
        let r = uf.find(i);
        let comp = *root_to_comp.entry(r).or_insert_with(|| {
            comp_vars.push(Vec::new());
            comp_vars.len() - 1
        });
        comp_vars[comp].push(outputs[i].clone());
    }
    let mut comp_assertions: Vec<Vec<usize>> = vec![Vec::new(); comp_vars.len()];
    let mut global: Vec<usize> = Vec::new();
    for (ai, idxs) in per_assert.iter().enumerate() {
        match idxs.first() {
            Some(&first_out) => {
                let r = uf.find(first_out);
                let comp = root_to_comp[&r];
                comp_assertions[comp].push(ai);
            }
            None => global.push(ai),
        }
    }
    (comp_vars, comp_assertions, global)
}

fn build_tuned_solver(ctx: &'static Context, arith_solver: u32) -> Solver<'static> {
    let solver = Tactic::new(ctx, "solve-eqs")
        .and_then(&Tactic::new(ctx, "smt"))
        .solver();
    if arith_solver != 0 {
        let mut params = Params::new(ctx);
        params.set_u32("smt.arith.solver", arith_solver);
        solver.set_params(&params);
    }
    solver
}

impl EvidentRuntime {

    pub(super) fn try_functionize_z3(&self, name: &str, schema: &crate::core::ast::SchemaDecl,
                          given: &HashMap<String, Value>) -> Option<QueryResult>
    {

        let mut given_keys: Vec<String> = given.keys().cloned().collect();
        given_keys.sort();
        let cache_key = (name.to_string(), given_keys.clone());

        if let Some(entry) = self.fn_cache.borrow().get(&cache_key).cloned() {
            let Some(plan) = entry else { return None };
            self.functionize_stats.borrow_mut()
                .claims.entry(name.to_string()).or_default().cache_hits += 1;
            return self.execute_plan(&plan, given);
        }

        let arith: u32 = 2;

        if crate::z3_eval::has_known_translator_gap(&schema.body) {
            self.fn_cache.borrow_mut().insert(cache_key, None);
            return None;
        }

        let empty_given: HashMap<String, Value> = HashMap::new();
        let cached = crate::translate::build_cache(
            schema, &self.schemas, self.z3_ctx, &self.datatypes,
            Some(&self.enums), &empty_given, arith);

        let assertions_local = cached.solver.get_assertions();
        let assertions: Vec<z3::ast::Bool<'static>> = unsafe {
            std::mem::transmute::<Vec<z3::ast::Bool<'_>>, Vec<z3::ast::Bool<'static>>>(
                assertions_local)
        };
        let simplify_result = simplify_assertions(self.z3_ctx, &assertions);
        {
            let mut stats = self.functionize_stats.borrow_mut();
            let per = stats.claims.entry(name.to_string()).or_default();
            per.analyses += 1;
            per.simplified_total += simplify_result.formulas.len() as u32;
        }
        if simplify_result.unsat {
            self.functionize_stats.borrow_mut()
                .claims.entry(name.to_string()).or_default().decided_unsat += 1;
            return Some(QueryResult { satisfied: false, bindings: HashMap::new() });
        }
        let simplified = &simplify_result.formulas;

        let mut touched: std::collections::HashSet<String> = std::collections::HashSet::new();
        for a in simplified {
            crate::z3_eval::collect_touched_names(a, &mut touched);
        }
        let outputs: Vec<String> = cached.env.iter()
            .filter(|(name, _)| !given.contains_key(name.as_str()))
            .filter(|(_, v)| !matches!(v,
                crate::translate::Var::EnumValue { .. }
                | crate::translate::Var::EnumCtor { .. }
                | crate::translate::Var::PinnedInt(_)))
            .filter(|(name, _)| touched.contains(name.as_str()))
            .map(|(n, _)| n.clone())
            .collect();

        let pinned_steps: Vec<crate::core::Z3Step<'static>> = cached.env.iter()
            .filter(|(name, _)| !given.contains_key(name.as_str()))
            .filter_map(|(n, v)| match v {
                crate::translate::Var::PinnedInt(i) => Some(crate::core::Z3Step::Scalar {
                    var:  n.clone(),
                    expr: z3::ast::Dynamic::from_ast(&z3::ast::Int::from_i64(self.z3_ctx, *i)),
                }),
                _ => None,
            })
            .collect();

        let pinned_ints: Vec<(String, Value)> = cached.env.iter()
            .filter(|(name, _)| !given.contains_key(name.as_str()))
            .filter_map(|(n, v)| match v {
                crate::translate::Var::PinnedInt(i) => Some((n.clone(), Value::Int(*i))),
                _ => None,
            })
            .collect();

        if outputs.is_empty() {

            self.fn_cache.borrow_mut().insert(cache_key, None);
            return None;
        }

        let (comp_vars, comp_assert_idx, global_idx) =
            decompose_simplified(simplified, &outputs);
        let n_components = comp_vars.len();

        let mut compiled: Vec<Rc<JitProgram>> = Vec::new();
        let mut uncompiled_outputs: Vec<String> = Vec::new();
        let mut uncompiled_assert_idx: Vec<usize> = Vec::new();
        let mut n_compiled = 0u32;
        let mut bail = false;
        for (ci, cvars) in comp_vars.iter().enumerate() {
            let casserts: Vec<Bool<'static>> =
                comp_assert_idx[ci].iter().map(|&i| simplified[i].clone()).collect();
            match self.compile_one_component(name, cvars, &casserts, &cached, given, &pinned_steps) {
                ComponentOutcome::Compiled(c) => { compiled.push(c); n_compiled += 1; }
                ComponentOutcome::Slow => {
                    uncompiled_outputs.extend(cvars.iter().cloned());
                    uncompiled_assert_idx.extend(comp_assert_idx[ci].iter().copied());
                }
                ComponentOutcome::Bail => { bail = true; break; }
            }
        }

        if bail {
            self.functionize_stats.borrow_mut()
                .claims.entry(name.to_string()).or_default().last_extract_ok = Some(false);
            let cached_static: CompiledModel<'static> = cached;
            self.slow_path_cache.borrow_mut()
                .insert(cache_key.clone(), Rc::new(cached_static));
            self.fn_cache.borrow_mut().insert(cache_key, None);
            return None;
        }

        {
            let mut stats = self.functionize_stats.borrow_mut();
            let per = stats.claims.entry(name.to_string()).or_default();
            per.last_extract_ok = Some(true);
            per.components += n_components as u32;
            per.components_compiled += n_compiled;
            if n_compiled > 0 { per.compiled += 1; }
        }

        let slow = if uncompiled_outputs.is_empty() {
            None
        } else {
            let mut slow_assertions: Vec<Bool<'static>> = uncompiled_assert_idx.iter()
                .map(|&i| simplified[i].clone()).collect();
            for &i in &global_idx { slow_assertions.push(simplified[i].clone()); }
            let slow_solver = build_tuned_solver(self.z3_ctx, arith);
            for a in &slow_assertions { slow_solver.assert(a); }

            let CompiledModel { env, .. } = cached;
            Some(SlowPart {
                cached: CompiledModel { env, solver: slow_solver, arith_solver: arith },
                outputs: uncompiled_outputs,
            })
        };

        let plan = Rc::new(ClaimPlan { compiled, slow, pinned_ints });
        self.fn_cache.borrow_mut().insert(cache_key, Some(plan.clone()));
        self.execute_plan(&plan, given)
    }

    fn compile_one_component(
        &self,
        name: &str,
        comp_outputs: &[String],
        comp_assertions: &[Bool<'static>],
        cached: &CompiledModel<'static>,
        given: &HashMap<String, Value>,
        pinned_steps: &[Z3Step<'static>],
    ) -> ComponentOutcome {
        let _ = name;
        if comp_outputs.is_empty() { return ComponentOutcome::Slow; }

        for v in comp_outputs {
            if matches!(cached.env.get(v),
                Some(Var::SetVar { .. }) | Some(Var::DatatypeSetVar { .. }))
            {
                return ComponentOutcome::Slow;
            }
        }
        let comp_out_vec: Vec<String> = comp_outputs.to_vec();
        let Some((mut program, mut missing)) =
            extract_program_partial(comp_assertions, &comp_out_vec)
        else {

            return ComponentOutcome::Slow;
        };

        if !missing.is_empty() {
            recompose_record_seqs(
                comp_assertions, &mut missing, &mut program, &self.datatypes, self.z3_ctx);
        }
        if !missing.is_empty() {

            let mut touched: HashSet<String> = HashSet::new();
            for a in comp_assertions { collect_touched_names(a, &mut touched); }
            let output_set: HashSet<&str> = comp_out_vec.iter().map(|s| s.as_str()).collect();
            let missing_set: HashSet<&str> = missing.iter().map(|s| s.as_str()).collect();
            let mut unsafe_free = false;
            for n in &touched {
                let in_given = given.contains_key(n);
                let is_covered = output_set.contains(n.as_str())
                    && !missing_set.contains(n.as_str());
                let in_env = cached.env.contains_key(n);
                let is_const = cached.env.get(n).map(|v| matches!(v,
                    Var::PinnedInt(_) | Var::EnumValue { .. } | Var::EnumCtor { .. }))
                    .unwrap_or(false);
                if in_env && !in_given && !is_covered && !is_const {
                    unsafe_free = true;
                    break;
                }
            }
            if unsafe_free {

                if component_has_defining_assertion(comp_assertions) {
                    return ComponentOutcome::Slow;
                }
                return ComponentOutcome::Bail;
            }

            if !matches!(cached.solver.check(), SatResult::Sat) {
                return ComponentOutcome::Bail;
            }
            let Some(model) = cached.solver.get_model() else {
                return ComponentOutcome::Bail;
            };
            let mut prebaked: Vec<Z3Step<'static>> = Vec::with_capacity(missing.len());
            for var_name in &missing {
                let Some(var) = cached.env.get(var_name) else {
                    return ComponentOutcome::Bail;
                };
                let mut tmp: HashMap<String, Value> = HashMap::new();
                crate::translate::extract_binding(
                    var_name, var, &model, self.z3_ctx, &mut tmp, Some(&self.enums));
                let Some(value) = tmp.remove(var_name) else {
                    return ComponentOutcome::Bail;
                };
                prebaked.push(Z3Step::PreBaked { var: var_name.clone(), value });
            }
            let mut all = prebaked;
            all.append(&mut program.steps);
            program.steps = all;
        }

        {
            let mut stats = self.functionize_stats.borrow_mut();
            let per = stats.claims.entry(name.to_string()).or_default();
            per.steps_total      += program.steps.len() as u32;
            per.checks_total     += program.checks.len() as u32;
            per.predicates_total += program.predicates.len() as u32;
        }

        let mut all = pinned_steps.to_vec();
        all.append(&mut program.steps);
        program.steps = all;
        match self.functionizer.compile(&program, &self.enums, &self.datatypes) {

            Some(c) => ComponentOutcome::Compiled(c),
            None => ComponentOutcome::Slow,
        }
    }

    fn execute_plan(&self, plan: &ClaimPlan, given: &HashMap<String, Value>)
        -> Option<QueryResult>
    {
        let mut out: HashMap<String, Value> = HashMap::new();

        for (k, v) in &plan.pinned_ints {
            if !given.contains_key(k) { out.insert(k.clone(), v.clone()); }
        }
        for c in &plan.compiled {
            let bindings = c.call(given)?;
            for (k, v) in bindings {
                if !given.contains_key(&k) { out.insert(k, v); }
            }
        }
        if let Some(slow) = &plan.slow {

            slow.cached.solver.push();
            let mut scalar_given: HashMap<String, Value> =
                HashMap::with_capacity(given.len());
            for (n, v) in given {
                match (slow.cached.env.get(n), v) {
                    (Some(Var::EnumVar { ast, .. }), Value::Enum { .. }) => {
                        if let Some(dt) = crate::translate::value_enum_to_datatype(
                            v, self.z3_ctx, &self.enums)
                        {
                            slow.cached.solver.assert(&ast._eq(&dt));
                        }
                    }
                    _ => { scalar_given.insert(n.clone(), v.clone()); }
                }
            }
            let r = run_cached(&slow.cached, &scalar_given, self.z3_ctx, Some(&self.enums));
            slow.cached.solver.pop(1);
            if !r.satisfied { return None; }
            for vn in &slow.outputs {
                if let Some(v) = r.bindings.get(vn) {
                    out.insert(vn.clone(), v.clone());
                }
            }
        }
        for (k, v) in given { out.insert(k.clone(), v.clone()); }
        Some(QueryResult { satisfied: true, bindings: out })
    }

    pub fn query(&self, name: &str, given: &HashMap<String, Value>) -> Result<QueryResult, RuntimeError> {
        let schema = self.schemas.get(name)
            .ok_or_else(|| RuntimeError::UnknownSchema(name.to_string()))?;

        if let Some(result) = self.try_functionize_z3(name, schema, given) {
            return Ok(result);
        }

        let arith: u32 = 2;
        let r = crate::translate::evaluate(schema, given, &self.schemas, self.z3_ctx, &self.datatypes, Some(&self.enums), arith);
        Ok(QueryResult { satisfied: r.satisfied, bindings: r.bindings })
    }

}
