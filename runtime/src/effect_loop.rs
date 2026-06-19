//! The execute stage: discover the single FSM, seed its state, then tick it.
//! Each tick solves the FSM claim, decodes the model's `Effect` values, orders
//! them (declared seq-chains + auto edges, topo-sorted with random tiebreak),
//! dispatches them as real IO, and feeds results / world writes back. Halts when
//! nothing changed or an `Exit` was requested.

use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

use crate::core::ast::{BinOp, BodyItem, Effect, EffectResult, Expr, Pins};
use crate::core::Value;
use crate::effect_dispatch::{dispatch_all, DispatchContext};
use crate::runtime::EvidentRuntime;
use crate::encode::effect_decoder::decode_install_step_list;
use crate::encode::{effect_decoder, Value as TValue};

// ───────────────────────── public entry + loop opts ─────────────────────────

#[derive(Debug, Clone)]
pub struct LoopOpts {

    pub max_steps: usize,
}

impl Default for LoopOpts {
    fn default() -> Self { Self { max_steps: 10_000 } }
}

#[derive(Debug)]
pub struct LoopResult {
    pub steps:       usize,
    pub final_state: Option<Value>,
    pub halted_clean: bool,

    pub exit_code: Option<i32>,
}

pub fn run(rt: &EvidentRuntime, opts: &LoopOpts) -> Result<LoopResult, String> {
    run_with_ctx(rt, opts, &mut DispatchContext::new())
}

pub fn run_with_ctx(
    rt: &EvidentRuntime,
    opts: &LoopOpts,
    ctx: &mut DispatchContext,
) -> Result<LoopResult, String> {
    let fsm = single_fsm(rt)?;

    let mut world_snapshot: HashMap<String, Value> = HashMap::new();
    for (param_name, type_name, pins) in &fsm.install_params {
        let writes = run_declarative_install(
            rt, &fsm.claim_name, param_name, type_name, pins, ctx)?;
        for (k, v) in writes {
            world_snapshot.insert(k, v);
        }
    }

    run_loop(rt, &fsm, opts, ctx, &mut world_snapshot)
}

// ───────────────────────── FSM discovery + shape ─────────────────────────

#[derive(Clone)]
pub struct MainShape {
    pub claim_name:       String,
    pub state_var:        Option<String>,
    pub state_next_var:   Option<String>,
    pub state_type:       Option<String>,
    pub last_results_var: Option<String>,
    pub effects_var:      Option<String>,

    pub world_var:        Option<String>,

    pub world_next_var:   Option<String>,

    pub world_type:       Option<String>,

    pub install_params: Vec<(String, String, Pins)>,
}

pub fn detect_main_shape(rt: &EvidentRuntime) -> Option<MainShape> {
    resolve_fsm(rt, "main")
}

fn has_declarative_install(rt: &EvidentRuntime, type_name: &str) -> bool {
    rt.get_schema(type_name)
        .map(|s| {
            s.body.iter().any(|i| {
                matches!(i, BodyItem::Membership { name, type_name: ty, .. }
                         if name == "install" && ty == "Seq(InstallStep)")
            })
        })
        .unwrap_or(false)
}

pub fn resolve_fsm(rt: &EvidentRuntime, claim_name: &str) -> Option<MainShape> {
    let claim = rt.get_schema(claim_name)?;
    if !matches!(claim.keyword, crate::core::ast::Keyword::Fsm) {
        return None;
    }
    if claim.external {
        return None;
    }
    let mut state_pair: Option<(String, String, String)> = None;
    let mut last_results_var = None;
    let mut effects_var = None;
    let mut world_var: Option<String> = None;
    let mut world_next_var: Option<String> = None;
    let mut world_type: Option<String> = None;
    let mut install_params: Vec<(String, String, Pins)> = Vec::new();

    let mut all_items: Vec<&BodyItem> = Vec::new();
    let mut visited: HashSet<String> = HashSet::new();
    fn collect<'a>(
        items: &'a [BodyItem],
        rt: &'a EvidentRuntime,
        out: &mut Vec<&'a BodyItem>,
        visited: &mut HashSet<String>,
    ) {
        for item in items {
            out.push(item);
            if let BodyItem::Passthrough(name) = item {
                if visited.insert(name.clone()) {
                    if let Some(sub) = rt.get_schema(name) {

                        let body: &'a [BodyItem] = unsafe {
                            std::mem::transmute::<&[BodyItem], &'a [BodyItem]>(&sub.body)
                        };
                        collect(body, rt, out, visited);
                    }
                }
            }
        }
    }
    collect(&claim.body, rt, &mut all_items, &mut visited);
    for item in all_items.iter().copied() {
        if let BodyItem::Membership { name, type_name, pins } = item {
            if type_name == "Seq(Effect)" && name == "effects" && effects_var.is_none() {
                effects_var = Some(name.clone());
            } else if type_name == "Seq(Result)" && name == "last_results"
                   && last_results_var.is_none()
            {
                last_results_var = Some(name.clone());
            } else if name == "world" {
                world_var = Some(name.clone());
                world_type = Some(type_name.clone());
            } else if name == "world_next" {
                world_next_var = Some(name.clone());
                if world_type.is_none() {
                    world_type = Some(type_name.clone());
                }
            } else if has_declarative_install(rt, type_name) {
                install_params.push((name.clone(), type_name.clone(), pins.clone()));
            } else if type_name != "Int" && type_name != "Bool"
                   && type_name != "String" && type_name != "Real"
                   && !type_name.starts_with("Seq(")
                   && !type_name.starts_with("Set(")
            {

                if name.ends_with("_next") {
                    let base = &name[..name.len() - 5];
                    if let Some((b, _, _)) = &state_pair {
                        if b == base { continue; }
                    }
                    state_pair = Some((base.to_string(), name.clone(), type_name.clone()));
                } else if state_pair.is_none()
                       || matches!(&state_pair, Some((b, _, _)) if b != name)
                {
                    let nxt = format!("{}_next", name);
                    if all_items.iter().any(|i| matches!(
                        i, BodyItem::Membership { name: n, type_name: t, .. }
                           if n == &nxt && t == type_name
                    )) {
                        state_pair = Some((name.clone(), nxt, type_name.clone()));
                    }
                }
            }
        }
    }
    let (state_var, state_next_var, state_type) = match state_pair {
        Some((s, sn, st)) => (Some(s), Some(sn), Some(st)),
        None => (None, None, None),
    };
    Some(MainShape {
        claim_name:    claim_name.to_string(),
        state_var,
        state_next_var,
        state_type,
        last_results_var,
        effects_var,
        world_var,
        world_next_var,
        world_type,
        install_params,
    })
}

pub fn single_fsm(rt: &EvidentRuntime) -> Result<MainShape, String> {
    let mut fsms: Vec<MainShape> = rt.schema_names()
        .map(|s| s.to_string())
        .collect::<Vec<_>>()
        .into_iter()
        .filter(|n| !n.starts_with("sat_") && !n.starts_with("unsat_"))
        .filter_map(|n| resolve_fsm(rt, &n))
        .collect();
    match fsms.len() {
        0 => Err("no fsm schemas found (declare one with the `fsm` keyword)".to_string()),
        1 => Ok(fsms.pop().unwrap()),
        n => {
            let names: Vec<&str> = fsms.iter().map(|f| f.claim_name.as_str()).collect();
            Err(format!(
                "{n} fsm-shaped claims found ([{}]) but exactly one is allowed \
                 (one FSM per program)",
                names.join(", ")))
        }
    }
}

// ───────────────────────── declarative install (FTI bridge) ─────────────────────────

fn run_declarative_install(
    rt: &EvidentRuntime,
    claim_name: &str,
    param_name: &str,
    type_name: &str,
    pins: &Pins,
    dispatch_ctx: &mut DispatchContext,
) -> Result<Vec<(String, Value)>, String> {

    let mut given: HashMap<String, Value> = HashMap::new();
    if let Pins::Named(ms) = pins {
        for m in ms {
            let v = match &m.value {
                Expr::Int(n) => Value::Int(*n),
                Expr::Bool(b) => Value::Bool(*b),
                Expr::Str(s) => Value::Str(s.clone()),
                _ => continue,
            };
            given.insert(m.slot.clone(), v);
        }
    }

    let result = rt
        .query_with_pins_and_given(type_name, &[], &given)
        .map_err(|e| format!("declarative install: query {type_name}: {e}"))?;
    if !result.satisfied {
        return Err(format!(
            "declarative install: {type_name} body UNSAT under pins"
        ));
    }
    let install_val = result.bindings.get("install").ok_or_else(|| {
        format!("declarative install: {type_name} has no `install` binding")
    })?;
    let steps = decode_install_step_list(install_val)
        .map_err(|e| format!("declarative install: decode `install`: {e:?}"))?;

    let effects: Vec<_> = steps.iter().map(|s| s.effect.clone()).collect();
    let results = dispatch_all(dispatch_ctx, &effects);

    let mut writes: Vec<(String, Value)> = Vec::new();
    for (step, res) in steps.iter().zip(results.iter()) {
        let Some(field) = &step.field else { continue };
        let key = format!("{claim_name}.{param_name}.{field}");
        let value = match res {
            EffectResult::Int(n) => Value::Int(*n),
            EffectResult::Handle(h) => Value::Int(*h as i64),
            EffectResult::Str(s) => Value::Str(s.clone()),
            EffectResult::Bool(b) => Value::Bool(*b),
            EffectResult::Real(r) => Value::Real(*r),
            EffectResult::Error(e) => {
                return Err(format!(
                    "declarative install: step `Bind({field}, …)` returned Error: {e}"
                ));
            }
            EffectResult::NoResult => continue,
        };
        writes.push((key, value));
    }
    Ok(writes)
}

// ───────────────────────── per-tick scheduler loop ─────────────────────────

fn seed_state(
    rt: &EvidentRuntime,
    s: &MainShape,
) -> (Option<z3::ast::Datatype<'static>>, Option<Value>) {
    let Some(state_type) = s.state_type.as_ref() else { return (None, None); };
    let enums = rt.enums_registry();
    let by_name = enums.by_name.borrow();
    let entry = by_name.get(state_type);
    let dt = entry.and_then(|(sort, _)| {
        let first = sort.variants.first()?;
        if first.constructor.arity() == 0 {
            first.constructor.apply(&[]).as_datatype()
        } else {
            None
        }
    });
    let val = entry.and_then(|(sort, decl_variants)| {
        let first = decl_variants.first()?;
        if sort.variants.first().map(|v| v.constructor.arity()).unwrap_or(0) == 0 {
            Some(Value::Enum {
                enum_name: state_type.clone(),
                variant:   first.name.clone(),
                fields:    Vec::new(),
            })
        } else {
            None
        }
    });
    (dt, val)
}

fn run_loop(
    rt: &EvidentRuntime,
    fsm: &MainShape,
    opts: &LoopOpts,
    ctx: &mut DispatchContext,
    world_snapshot: &mut HashMap<String, Value>,
) -> Result<LoopResult, String> {
    let (mut current_state, mut current_state_v) = seed_state(rt, fsm);
    let mut last_results: Vec<EffectResult> = Vec::new();

    let mut prev_values: HashMap<String, Value> = HashMap::new();

    if let Some(wt) = &fsm.world_type {
        if let Some(world_schema) = rt.get_schema(wt) {
            for item in &world_schema.body {
                if let BodyItem::Membership { name, type_name, .. } = item {
                    let key = format!("world.{name}");
                    if world_snapshot.contains_key(&key) { continue; }
                    let default = match type_name.as_str() {
                        "Int"    => Some(Value::Int(0)),
                        "Bool"   => Some(Value::Bool(false)),
                        "String" => Some(Value::Str(String::new())),
                        "Real"   => Some(Value::Real(0.0)),
                        _        => None,
                    };
                    if let Some(d) = default {
                        world_snapshot.insert(key, d);
                    }
                }
            }
        }
    }

    let mut step_count = 0usize;

    while step_count < opts.max_steps {

        let pins: Vec<(&str, z3::ast::Datatype<'static>)> =
            match (&fsm.state_var, &current_state) {
                (Some(name), Some(s)) => vec![(name.as_str(), s.clone())],
                _ => vec![],
            };

        let mut fsm_view: HashMap<String, Value> = if fsm.install_params.is_empty() {
            world_snapshot.clone()
        } else {
            let mut v = world_snapshot.clone();
            let prefix = format!("{}.", fsm.claim_name);
            for (k, val) in world_snapshot.iter() {
                if let Some(stripped) = k.strip_prefix(&prefix) {
                    v.insert(stripped.to_string(), val.clone());
                }
            }
            v
        };
        if let Some(lr_var) = &fsm.last_results_var {
            let lr = rt.effect_results_to_value(&last_results);
            fsm_view.insert(lr_var.clone(), lr);
        }

        if let Some(claim) = rt.get_schema(&fsm.claim_name) {
            let is_first = prev_values.is_empty();
            let mut sees_underscore = false;
            for item in &claim.body {
                if let BodyItem::Membership { name, .. } = item {
                    if let Some(stripped) = name.strip_prefix('_') {
                        sees_underscore = true;
                        if let Some(prev) = prev_values.get(stripped) {
                            fsm_view.insert(name.clone(), prev.clone());
                        }
                        let prefix = format!("{stripped}.");
                        for (k, v) in &prev_values {
                            if let Some(field) = k.strip_prefix(&prefix) {
                                fsm_view.insert(format!("{name}.{field}"), v.clone());
                            }
                        }
                    }
                }
            }
            if sees_underscore {
                fsm_view.insert("is_first_tick".to_string(), Value::Bool(is_first));
            }
        }
        if let (Some(state_name), Some(state_v)) = (&fsm.state_var, &current_state_v) {
            fsm_view.insert(state_name.clone(), state_v.clone());
        }

        let r = rt
            .query_with_pins_and_given(&fsm.claim_name, &pins, &fsm_view)
            .map_err(|e| format!("FSM `{}` solve step {step_count}: {e}", fsm.claim_name))?;

        if !r.satisfied {
            eprintln!("[loop] FSM `{}` returned UNSAT on tick {step_count}", fsm.claim_name);
            return Ok(LoopResult {
                steps: step_count,
                final_state: current_state_v.clone(),
                halted_clean: false,
                exit_code: ctx.exit_requested,
            });
        }

        let state_next_val: Option<Value> = match &fsm.state_next_var {
            Some(sn) => Some(
                r.bindings.get(sn)
                    .ok_or_else(|| format!(
                        "FSM `{}` step {step_count}: model has no `{}`",
                        fsm.claim_name, sn))?
                    .clone(),
            ),
            None => None,
        };

        let effects = collect_dispatchable_effects(
            rt, &fsm.claim_name, &r.bindings, fsm.effects_var.as_deref());

        let mut any_world_write = false;
        if fsm.world_next_var.is_some() {
            for (k, v) in r.bindings.iter() {
                if let Some(field) = k.strip_prefix("world_next.") {
                    let key = format!("world.{field}");
                    if world_snapshot.get(&key) != Some(v) {
                        any_world_write = true;
                    }
                    world_snapshot.insert(key, v.clone());
                }
            }
        }

        let mut state_changed = false;
        if let Some(snv) = &state_next_val {
            state_changed = current_state_v.as_ref().map(|prev| prev != snv).unwrap_or(true);
            current_state = encode_state_value(rt, snv);
            current_state_v = Some(snv.clone());
        }

        for (k, v) in r.bindings.iter() {
            if k.starts_with('_') { continue; }
            if k == "is_first_tick" { continue; }
            prev_values.insert(k.clone(), v.clone());
        }

        let any_effect = !effects.is_empty();
        last_results = dispatch_all(ctx, &effects);

        step_count += 1;

        if ctx.exit_requested.is_some() {
            return Ok(LoopResult {
                steps: step_count,
                final_state: current_state_v.clone(),
                halted_clean: true,
                exit_code: ctx.exit_requested,
            });
        }

        if !state_changed && !any_effect && !any_world_write {
            return Ok(LoopResult {
                steps: step_count,
                final_state: current_state_v.clone(),
                halted_clean: true,
                exit_code: ctx.exit_requested,
            });
        }
    }

    Ok(LoopResult {
        steps: step_count,
        final_state: current_state_v.clone(),
        halted_clean: false,
        exit_code: ctx.exit_requested,
    })
}

fn encode_state_value(rt: &EvidentRuntime, v: &Value) -> Option<z3::ast::Datatype<'static>> {
    use z3::ast::{Int as Z3Int, Bool as Z3Bool, String as Z3Str, Dynamic, Ast};
    let Value::Enum { enum_name, variant, fields } = v else { return None };
    let enums = rt.enums_registry();
    let by_name = enums.by_name.borrow();
    let (sort, _decl) = by_name.get(enum_name)?;
    let var_idx = sort.variants.iter().position(|v| v.constructor.name() == *variant)?;
    let ctor = &sort.variants[var_idx].constructor;
    if fields.is_empty() {
        return ctor.apply(&[]).as_datatype();
    }

    let ctx = rt.z3_context();
    let owned: Vec<Dynamic<'static>> = fields.iter().filter_map(|f| {
        let dyn_v: Dynamic<'static> = match f {
            Value::Int(n)  => Dynamic::from_ast(&Z3Int::from_i64(ctx, *n)),
            Value::Bool(b) => Dynamic::from_ast(&Z3Bool::from_bool(ctx, *b)),
            Value::Str(s)  => Dynamic::from_ast(&Z3Str::from_str(ctx, s).ok()?),
            Value::Real(r) => {
                let i = (*r * 1_000_000.0) as i64;
                Dynamic::from_ast(&z3::ast::Real::from_real(ctx, i as i32, 1_000_000))
            }
            Value::Enum { .. } => {
                let dt = encode_state_value(rt, f)?;
                Dynamic::from_ast(&dt)
            }
            _ => return None,
        };
        Some(dyn_v)
    }).collect();
    if owned.len() != fields.len() { return None; }
    let refs: Vec<&dyn Ast> = owned.iter().map(|v| v as &dyn Ast).collect();
    ctor.apply(&refs).as_datatype()
}

// ───────────────────────── effect collection + ordering ─────────────────────────

type DispatchKey = (Vec<String>, Vec<(String, String)>);

static DISPATCH_ORDER_CACHE: Mutex<Option<HashMap<DispatchKey, Vec<String>>>>
    = Mutex::new(None);

fn collect_dispatchable_effects(
    rt: &EvidentRuntime,
    claim_name: &str,
    bindings: &HashMap<String, TValue>,
    primary_var: Option<&str>,
) -> Vec<Effect> {

    if let Some(pv) = primary_var {
        if let Some(TValue::SeqEnum(items)) = bindings.get(pv) {
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
            TValue::Enum { enum_name, .. } if enum_name == "Effect" => {
                if let Ok(e) = effect_decoder::decode_effect(v) {
                    node_values.insert(name.clone(), e);
                    all_names.push(name.clone());
                }
            }
            TValue::SeqEnum(items) => {
                let is_effect_seq = !items.is_empty() && items.iter().all(|it|
                    matches!(it, TValue::Enum { enum_name, .. } if enum_name == "Effect")
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

            TValue::SeqComposite(items) => {
                for (i, fields_map) in items.iter().enumerate() {
                    for (fname, fval) in fields_map {
                        let TValue::SeqEnum(inner) = fval else { continue };
                        let is_effect_inner = !inner.is_empty() && inner.iter().all(|it|
                            matches!(it, TValue::Enum { enum_name, .. }
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

fn resolve_synthetic_names_to_effects(
    names: &[String],
    node_values: &HashMap<String, Effect>,
) -> Vec<Effect> {
    names.iter()
        .filter_map(|n| node_values.get(n).cloned())
        .collect()
}

fn topo_sort_with_random_tiebreak(
    nodes: &[String],
    edges: &[(String, String)],
    rng: &mut rand::rngs::StdRng,
) -> Vec<String> {
    use rand::seq::SliceRandom;

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

fn extract_seq_effect_chains(
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
