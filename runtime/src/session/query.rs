use crate::core::{CompiledModel, QueryResult, RuntimeError, Var, Z3Step};
use crate::functionize::cranelift::JitProgram;
use super::{EvidentRuntime, Value};
use crate::encode::run_cached;
use crate::functionize::extract_program::{collect_touched_names, extract_program_partial,
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

use super::UnionFind;

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
        // The functionizer is an optimization and is lossy — when in doubt it
        // must defer to the slow Z3 path, which is the correctness oracle.
        // `EVIDENT_NO_JIT=1` (env) or `set_functionize_enabled(false)`
        // (programmatic, for differential testing) forces the slow path so its
        // result can be trusted and diffed against the JIT. On by default.
        if !self.functionize_enabled.get() || std::env::var_os("EVIDENT_NO_JIT").is_some() {
            return None;
        }

        let mut given_keys: Vec<String> = given.keys().cloned().collect();
        given_keys.sort();
        let cache_key = (name.to_string(), given_keys.clone());

        if let Some(entry) = self.fn_cache.borrow().get(&cache_key).cloned() {
            let Some(plan) = entry else { return None };
            return self.execute_plan(&plan, given);
        }

        let arith: u32 = 2;

        if crate::functionize::extract_program::has_known_translator_gap(&schema.body) {
            self.fn_cache.borrow_mut().insert(cache_key, None);
            return None;
        }

        let empty_given: HashMap<String, Value> = HashMap::new();
        let cached = crate::encode::build_cache(
            schema, &self.schemas, self.z3_ctx, &self.datatypes,
            Some(&self.enums), &empty_given, arith);

        let assertions_local = cached.solver.get_assertions();
        let assertions: Vec<z3::ast::Bool<'static>> = unsafe {
            std::mem::transmute::<Vec<z3::ast::Bool<'_>>, Vec<z3::ast::Bool<'static>>>(
                assertions_local)
        };
        let simplify_result = simplify_assertions(self.z3_ctx, &assertions);
        if simplify_result.unsat {
            return Some(QueryResult { satisfied: false, bindings: HashMap::new() });
        }
        let simplified = &simplify_result.formulas;

        let mut touched: std::collections::HashSet<String> = std::collections::HashSet::new();
        for a in simplified {
            crate::functionize::extract_program::collect_touched_names(a, &mut touched);
        }
        let outputs: Vec<String> = cached.env.iter()
            .filter(|(name, _)| !given.contains_key(name.as_str()))
            .filter(|(_, v)| !matches!(v,
                crate::encode::Var::EnumValue { .. }
                | crate::encode::Var::EnumCtor { .. }
                | crate::encode::Var::PinnedInt(_)))
            .filter(|(name, _)| touched.contains(name.as_str()))
            .map(|(n, _)| n.clone())
            .collect();

        let pinned_steps: Vec<crate::core::Z3Step<'static>> = cached.env.iter()
            .filter(|(name, _)| !given.contains_key(name.as_str()))
            .filter_map(|(n, v)| match v {
                crate::encode::Var::PinnedInt(i) => Some(crate::core::Z3Step::Scalar {
                    var:  n.clone(),
                    expr: z3::ast::Dynamic::from_ast(&z3::ast::Int::from_i64(self.z3_ctx, *i)),
                }),
                _ => None,
            })
            .collect();

        let pinned_ints: Vec<(String, Value)> = cached.env.iter()
            .filter(|(name, _)| !given.contains_key(name.as_str()))
            .filter_map(|(n, v)| match v {
                crate::encode::Var::PinnedInt(i) => Some((n.clone(), Value::Int(*i))),
                _ => None,
            })
            .collect();

        if outputs.is_empty() {

            self.fn_cache.borrow_mut().insert(cache_key, None);
            return None;
        }

        let (comp_vars, comp_assert_idx, global_idx) =
            decompose_simplified(simplified, &outputs);

        let mut compiled: Vec<Rc<JitProgram>> = Vec::new();
        let mut uncompiled_outputs: Vec<String> = Vec::new();
        let mut uncompiled_assert_idx: Vec<usize> = Vec::new();
        let mut bail = false;
        for (ci, cvars) in comp_vars.iter().enumerate() {
            let casserts: Vec<Bool<'static>> =
                comp_assert_idx[ci].iter().map(|&i| simplified[i].clone()).collect();
            match self.compile_one_component(name, cvars, &casserts, &cached, given, &pinned_steps) {
                ComponentOutcome::Compiled(c) => { compiled.push(c); }
                ComponentOutcome::Slow => {
                    uncompiled_outputs.extend(cvars.iter().cloned());
                    uncompiled_assert_idx.extend(comp_assert_idx[ci].iter().copied());
                }
                ComponentOutcome::Bail => { bail = true; break; }
            }
        }

        if bail {
            let cached_static: CompiledModel<'static> = cached;
            self.slow_path_cache.borrow_mut()
                .insert(cache_key.clone(), Rc::new(cached_static));
            self.fn_cache.borrow_mut().insert(cache_key, None);
            return None;
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
                crate::encode::extract_binding(
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
                        if let Some(dt) = crate::encode::value_enum_to_datatype(
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

    pub fn query_free(&self, name: &str) -> Result<QueryResult, RuntimeError> {
        self.query(name, &HashMap::new())
    }

    pub fn query(&self, name: &str, given: &HashMap<String, Value>) -> Result<QueryResult, RuntimeError> {
        let schema = self.schemas.get(name)
            .ok_or_else(|| RuntimeError::UnknownSchema(name.to_string()))?;

        if let Some(result) = self.try_functionize_z3(name, schema, given) {
            return Ok(result);
        }

        let arith: u32 = 2;
        let r = crate::encode::evaluate(schema, given, &self.schemas, self.z3_ctx, &self.datatypes, Some(&self.enums), arith);
        Ok(QueryResult { satisfied: r.satisfied, bindings: r.bindings })
    }

    /// Export the transition relation of `name` as SMT-LIB plus a JSON schema of
    /// the carried state leaves (a leaf `X` is carried iff `_X` also exists). The
    /// Python visualization tools load these and query the transition by pinning
    /// the `_X` constants and solving for the `X` constants. Returns (smt2, json).
    pub fn export_transition(&self, name: &str) -> Result<(String, String), RuntimeError> {
        let schema = self.schemas.get(name)
            .ok_or_else(|| RuntimeError::UnknownSchema(name.to_string()))?;
        let empty: HashMap<String, Value> = HashMap::new();
        let cached = crate::encode::build_cache(
            schema, &self.schemas, self.z3_ctx, &self.datatypes, Some(&self.enums), &empty, 2);
        let smt2 = format!("{}", cached.solver);

        // The interface = the first `param_count` body items (the claim's first-line
        // params). A carried leaf is `interface` iff its prefix is a param name,
        // else `internal` (body-declared). Renderers default their axes to the
        // interface — the model's observable contract — and treat internals as
        // existentially-projected witnesses (see docs/design/portrait-axes.md).
        let interface: std::collections::HashSet<&str> = schema.body.iter()
            .take(schema.param_count)
            .filter_map(|it| match it {
                crate::core::ast::BodyItem::Membership { name, .. } => Some(name.as_str()),
                _ => None,
            })
            .collect();

        let mut names: Vec<&String> = cached.env.keys().collect();
        names.sort();
        let mut rows: Vec<String> = Vec::new();
        let mut any_two_tick = false;
        for n in names {
            if n.starts_with('_') { continue; }
            let prev = format!("_{n}");
            if !cached.env.contains_key(&prev) { continue; }
            let kind = match cached.env.get(n) {
                Some(crate::encode::Var::IntVar(_))    => "int",
                Some(crate::encode::Var::RealVar(_))   => "real",
                Some(crate::encode::Var::BoolVar(_))   => "bool",
                Some(crate::encode::Var::StrVar(_))    => "string",
                Some(crate::encode::Var::EnumVar { .. }) => "enum",
                _ => continue,
            };
            let role = if interface.contains(n.split('.').next().unwrap_or(n))
                { "interface" } else { "internal" };
            // History depth: how many ticks back the transition reads this var.
            // 1 = only `_x` referenced (the normal case); 2 = `__x` (two ticks
            // back) is also bound in env — a ΔΔ / second-difference model, which
            // needs a two-snapshot (cur, prev) reachability node in the viz.
            let prev2 = format!("__{n}");
            let hist = if cached.env.contains_key(&prev2) { 2 } else { 1 };
            if hist == 2 { any_two_tick = true; }
            rows.push(format!(
                "    {{\"name\": \"{n}\", \"prev\": \"{prev}\", \"kind\": \"{kind}\", \"role\": \"{role}\", \"hist\": {hist}}}"));
        }
        // DERIVED vars: scalars the transition CONSTRAINS but does NOT carry (no `_x`
        // prev-twin) and that are a pure function of the CURRENT carried state — e.g.
        // `done ∈ Bool = (count ≥ 5)`. They're determined in every solved state but
        // never appear in `state` (not carried), so the viz can't plot them. Surface
        // them in a SEPARATE `"derived"` array (role "derived") so the renderer can
        // read+plot their value WITHOUT them entering the reachable-graph IDENTITY —
        // a derived var is a function of the carried state, so it must never widen the
        // dedup key.
        //
        // Two filters keep this to *genuine current-state observables*:
        //   1. VARIES — it can take ≥2 values over the transition. A global config
        //      constant (`red.red = 255`) is determined but invariant; skip it (no
        //      flat track per config leaf).
        //   2. DETERMINED BY THE CURRENT CARRIED STATE — fixing every CURRENT carried
        //      leaf (`count`) + is_first_tick fixes this var's value. `done = (count≥5)`
        //      is an observation OF the post-state, so it passes. A prev-lag alias
        //      (`pm = _state.mode`, `pb = _state.balance`) is determined by the PREVIOUS
        //      state, not the current one, so fixing current carried leaves it free → it
        //      fails. Likewise prev-only arithmetic intermediates (`term = f(_state.x)`).
        //      This is exactly "a pure function of the carried (current) state".
        let current_carried: Vec<(String, z3::ast::Dynamic<'static>)> = cached.env.iter()
            .filter(|(n, _)| !n.starts_with('_') && cached.env.contains_key(&format!("_{n}")))
            .filter_map(|(n, v)| carried_ast(v).map(|a| (n.clone(), a)))
            .collect();

        // The varies/determinism probes solve the transition with extra pins. On a
        // nonlinear/division-heavy transition (vanderpol) that can be hard, so cap each
        // probe with a per-check timeout — z3 returns Unknown, which both helpers treat
        // CONSERVATIVELY (exclude the var). A derived track we can't prove is just
        // omitted; we never hang the export.
        let mut probe_params = Params::new(self.z3_ctx);
        probe_params.set_u32("timeout", 1500);
        cached.solver.set_params(&probe_params);

        let mut names2: Vec<&String> = cached.env.keys().collect();
        names2.sort();
        let mut derived_rows: Vec<String> = Vec::new();
        for n in names2 {
            if n.starts_with('_') { continue; }                 // prev-twins / __x
            if n == "is_first_tick" || n == "is_second_tick" { continue; }  // selectors
            if n.ends_with("__len") || n.ends_with("__arr") { continue; }   // seq plumbing
            if cached.env.contains_key(&format!("_{n}")) { continue; }      // carried (already in state)
            let (kind, ast): (&str, z3::ast::Dynamic<'static>) = match cached.env.get(n) {
                Some(crate::encode::Var::IntVar(i))  => ("int",  z3::ast::Dynamic::from_ast(i)),
                Some(crate::encode::Var::RealVar(r)) => ("real", z3::ast::Dynamic::from_ast(r)),
                Some(crate::encode::Var::BoolVar(b)) => ("bool", z3::ast::Dynamic::from_ast(b)),
                Some(crate::encode::Var::EnumVar { ast, .. }) => ("enum", z3::ast::Dynamic::from_ast(ast)),
                _ => continue,   // string/seq/set/array — not a plottable derived scalar
            };
            if !varies_in_transition(&cached.solver, &ast) { continue; }
            if !determined_by_current_state(&cached.solver, &ast, &current_carried) { continue; }
            let extra = if kind == "enum" {
                if let Some(crate::encode::Var::EnumVar { enum_name, .. }) = cached.env.get(n) {
                    let variants = self.enums.by_name.borrow().get(enum_name)
                        .map(|(_, vs)| vs.iter()
                            .map(|v| format!("\"{}\"", v.name))
                            .collect::<Vec<_>>().join(", "))
                        .unwrap_or_default();
                    format!(", \"variants\": [{variants}]")
                } else { String::new() }
            } else { String::new() };
            derived_rows.push(format!(
                "    {{\"name\": \"{n}\", \"kind\": \"{kind}\", \"role\": \"derived\"{extra}}}"));
        }

        let second_tick_field = if any_two_tick {
            ",\n  \"is_second_tick\": \"is_second_tick\""
        } else { "" };
        let derived_field = format!(",\n  \"derived\": [\n{}\n  ]", derived_rows.join(",\n"));
        let json = format!(
            "{{\n  \"fsm\": \"{name}\",\n  \"is_first_tick\": \"is_first_tick\"{second_tick_field},\n  \"state\": [\n{}\n  ]{derived_field}\n}}\n",
            rows.join(",\n"));
        Ok((smt2, json))
    }

    /// Export a (non-FSM) claim's constraint as SMT-LIB plus a JSON schema of its
    /// variables. A claim is a *static* constraint — no transition — so we build the
    /// cache at snapshot count 1 (no `_prev` twin). The Python visualization tools
    /// load these to explore the claim's solution space (sample/enumerate witnesses).
    ///
    /// Schema shape:
    ///   {"claim": "<name>", "vars": [{"name": "x", "kind": "int|real|bool|string|enum|seq",
    ///                                 "role": "interface|internal", ...}, ...]}
    /// A `seq` var adds `"elem"` (the element kind) and, when its length is pinned
    /// (`#name = N`), `"len": N`. An `enum` var adds `"variants": ["Red", ...]` so
    /// the renderer can draw a feasibility grid without re-reading the smt2 datatypes.
    /// `role` is "interface" iff the var's prefix is one of the claim's first
    /// `param_count` Membership params, else "internal". Returns (smt2, json).
    pub fn export_claim(&self, name: &str) -> Result<(String, String), RuntimeError> {
        let schema = self.schemas.get(name)
            .ok_or_else(|| RuntimeError::UnknownSchema(name.to_string()))?;
        let empty: HashMap<String, Value> = HashMap::new();
        // `pin_ints = false`: do NOT constant-fold `x = 7` into a literal. Ana's
        // export must DECLARE every named var and keep `x = 7` as `(assert (= x 7))`
        // — her relational model, not z3's simplifier residue `(= 7 7)` (#192).
        let cached = crate::encode::build_cache_opts(
            schema, &self.schemas, self.z3_ctx, &self.datatypes, Some(&self.enums), &empty, 1, false);
        let smt2 = format!("{}", cached.solver);

        // Same interface/internal split as export_transition: the first `param_count`
        // body items are the claim's first-line params (the observable contract).
        let interface: std::collections::HashSet<&str> = schema.body.iter()
            .take(schema.param_count)
            .filter_map(|it| match it {
                crate::core::ast::BodyItem::Membership { name, .. } => Some(name.as_str()),
                _ => None,
            })
            .collect();

        let mut names: Vec<&String> = cached.env.keys().collect();
        names.sort();
        let mut rows: Vec<String> = Vec::new();
        for n in names {
            // At snapshot=1 a claim has no `_prev` twins; skip any stray underscore var.
            if n.starts_with('_') { continue; }
            let role = if interface.contains(n.split('.').next().unwrap_or(n))
                { "interface" } else { "internal" };
            // Scalar (int/real/bool/string) emit `{name,kind,role}`; Seq and enum
            // vars also live in the smt2 (Seq as an Array `name` + `name__len`, enum
            // as a datatype) so the renderer can solve over them — but it needs their
            // shape, which the smt2 doesn't surface conveniently. Emit it here.
            let extra: Option<(&str, String)> = match cached.env.get(n) {
                Some(crate::encode::Var::IntVar(_))    => Some(("int", String::new())),
                Some(crate::encode::Var::RealVar(_))   => Some(("real", String::new())),
                Some(crate::encode::Var::BoolVar(_))   => Some(("bool", String::new())),
                Some(crate::encode::Var::StrVar(_))    => Some(("string", String::new())),
                Some(crate::encode::Var::SeqVar { len, elem, .. }) => {
                    let elem_kind = match elem {
                        crate::core::SeqElem::Int  => "int",
                        crate::core::SeqElem::Bool => "bool",
                        crate::core::SeqElem::Str  => "string",
                    };
                    let mut e = format!(", \"elem\": \"{elem_kind}\"");
                    if let Some(len) = pinned_len(&cached.solver, len) {
                        e.push_str(&format!(", \"len\": {len}"));
                    }
                    Some(("seq", e))
                }
                Some(crate::encode::Var::EnumVar { enum_name, .. }) => {
                    let variants = self.enums.by_name.borrow().get(enum_name)
                        .map(|(_, vs)| vs.iter()
                            .map(|v| format!("\"{}\"", v.name))
                            .collect::<Vec<_>>().join(", "))
                        .unwrap_or_default();
                    Some(("enum", format!(", \"variants\": [{variants}]")))
                }
                _ => None,
            };
            let Some((kind, extra)) = extra else { continue };
            rows.push(format!(
                "    {{\"name\": \"{n}\", \"kind\": \"{kind}\", \"role\": \"{role}\"{extra}}}"));
        }
        let json = format!(
            "{{\n  \"claim\": \"{name}\",\n  \"vars\": [\n{}\n  ]\n}}\n",
            rows.join(",\n"));
        Ok((smt2, json))
    }

}

/// A var "varies" over the transition iff the constraint admits TWO models that
/// disagree on it. A derived var that's a pure function of carried state still
/// varies (different carried states give different values); a global config
/// constant (`red.red = 255`) is pinned to one value and does NOT vary — we skip
/// those so the time_series doesn't gain a flat track per config leaf. Solve once
/// for a witness value, then ask whether `ast != witness` is also satisfiable.
fn varies_in_transition(solver: &Solver, ast: &z3::ast::Dynamic<'static>) -> bool {
    if solver.check() != SatResult::Sat { return false; }
    let Some(model) = solver.get_model() else { return false };
    let Some(v0) = model.eval(ast, true) else { return false };
    solver.push();
    solver.assert(&ast._eq(&v0).not());
    let varies = solver.check() == SatResult::Sat;
    solver.pop(1);
    varies
}

/// The z3 AST of a scalar (int/real/bool/enum) carried var, for pinning. None for
/// composite vars (seq/set/array) — those don't participate in the determinism pin.
fn carried_ast(v: &crate::encode::Var<'static>) -> Option<z3::ast::Dynamic<'static>> {
    match v {
        crate::encode::Var::IntVar(i)  => Some(z3::ast::Dynamic::from_ast(i)),
        crate::encode::Var::RealVar(r) => Some(z3::ast::Dynamic::from_ast(r)),
        crate::encode::Var::BoolVar(b) => Some(z3::ast::Dynamic::from_ast(b)),
        crate::encode::Var::EnumVar { ast, .. } => Some(z3::ast::Dynamic::from_ast(ast)),
        _ => None,
    }
}

/// Is `ast` a pure function of the CURRENT carried state? Solve once for a model, pin
/// every current carried var to its model value, then ask whether `ast` can still take
/// a DIFFERENT value. If not, `ast` is determined by the current carried state — a
/// genuine derived observable (`done = count≥5`). If it can still vary with the current
/// state fixed, it depends on something else (a prev-tick read like `pm = _state.mode`,
/// or an under-determined intermediate) → not a current-state observable; exclude it.
fn determined_by_current_state(
    solver: &Solver,
    ast: &z3::ast::Dynamic<'static>,
    current_carried: &[(String, z3::ast::Dynamic<'static>)],
) -> bool {
    if solver.check() != SatResult::Sat { return false; }
    let Some(model) = solver.get_model() else { return false };
    solver.push();
    for (_, cast) in current_carried {
        if let Some(val) = model.eval(cast, true) {
            solver.assert(&cast._eq(&val));
        }
    }
    let determined = match model.eval(ast, true) {
        Some(v0) => {
            solver.assert(&ast._eq(&v0).not());
            solver.check() == SatResult::Unsat
        }
        None => false,
    };
    solver.pop(1);
    determined
}

/// A Seq's length `#name` is pinned to N iff its min and max over the constraint
/// agree. Optimize both against the solver's assertions; return `Some(N)` only
/// when the length is fully determined, else `None` (renderer omits `"len"`).
fn pinned_len(solver: &Solver, len: &z3::ast::Int<'static>) -> Option<i64> {
    let assertions = solver.get_assertions();
    let bound = |maximize: bool| -> Option<i64> {
        let opt = z3::Optimize::new(len.get_ctx());
        for a in &assertions { opt.assert(a); }
        if maximize { opt.maximize(len); } else { opt.minimize(len); }
        if opt.check(&[]) != SatResult::Sat { return None; }
        opt.get_model()?.eval(len, true)?.as_i64()
    };
    let lo = bound(false)?;
    let hi = bound(true)?;
    (lo == hi).then_some(lo)
}
