//! The trampoline — the program's run loop. There is a state store and a single
//! FSM that bounces on it: each tick reads the previous state, solves the FSM
//! claim for the next state and its effects, and writes the new state back —
//! then bounces again. That's the whole execution model.
//!
//! Per bounce: discover the single FSM (once), then tick it. Each tick solves
//! the FSM claim, decodes the model's `Effect` values, orders them (declared
//! seq-chains + auto edges, topo-sorted with random tiebreak), dispatches them
//! as real IO (via `dispatch`), and rolls the new bindings back into the store —
//! each becomes next tick's `_var`. Halts when nothing carried changed and no
//! effect fired, or when an `Exit` was requested.

use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

use crate::core::ast::{BinOp, BodyItem, Effect, EffectResult, Expr, Pins};
use crate::core::Value;
use crate::ffi::{dispatch_all, DispatchContext};
use crate::session::EvidentRuntime;
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

    // FTI install fields (from declarative `install` bridges) seed the
    // snapshot; they're constant across ticks and merged into each tick's view.
    let mut snapshot: HashMap<String, Value> = HashMap::new();
    for (param_name, type_name, pins) in &fsm.install_params {
        let writes = run_declarative_install(
            rt, &fsm.claim_name, param_name, type_name, pins, ctx)?;
        for (k, v) in writes {
            snapshot.insert(k, v);
        }
    }

    run_loop(rt, &fsm, opts, ctx, &mut snapshot)
}

// ───────────────────────── FSM discovery + shape ─────────────────────────

// The FSM's runtime shape: the two I/O slots it reads/writes each tick
// (`last_results` in, `effects` out) plus any FTI install params. State is
// NOT named here — it carries generically through the `_var` mechanism (any
// `_x` membership reads the previous tick's `x`), so the trampoline needs no
// per-FSM knowledge of which variables are "the state".
#[derive(Clone)]
pub struct MainShape {
    pub claim_name:       String,
    pub last_results_var: Option<String>,
    pub effects_var:      Option<String>,
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
    let mut last_results_var = None;
    let mut effects_var = None;
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
            if let BodyItem::Passthrough { name, .. } = item {
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
            } else if has_declarative_install(rt, type_name) {
                install_params.push((name.clone(), type_name.clone(), pins.clone()));
            }
            // Everything else (record/enum state, scalars, intermediates) is
            // carried generically by the `_var` mechanism — the trampoline
            // needs no per-FSM detection of it.
        }
    }
    Some(MainShape {
        claim_name:    claim_name.to_string(),
        last_results_var,
        effects_var,
        install_params,
    })
}

/// Every fsm-shaped top-level claim, in SOURCE-DECLARATION order (`schema_names()`
/// iterates `schema_order`). `sat_`/`unsat_` test sentinels are skipped.
pub fn all_fsms(rt: &EvidentRuntime) -> Vec<MainShape> {
    rt.schema_names()
        .filter(|n| !n.starts_with("sat_") && !n.starts_with("unsat_"))
        .filter_map(|n| resolve_fsm(rt, n))
        .collect()
}

/// The FSM the trampoline / export should tick: the LAST-DEFINED fsm-shaped claim
/// (multiple are now allowed, so we can test composing programs — #290). Errors
/// only when there is NO fsm at all (the export router keys off `"no fsm"`).
pub fn single_fsm(rt: &EvidentRuntime) -> Result<MainShape, String> {
    let mut fsms = all_fsms(rt);
    match fsms.pop() {
        Some(last) => Ok(last),
        None => Err("no fsm schemas found (declare one with the `fsm` keyword)".to_string()),
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

fn run_loop(
    rt: &EvidentRuntime,
    fsm: &MainShape,
    opts: &LoopOpts,
    ctx: &mut DispatchContext,
    snapshot: &mut HashMap<String, Value>,
) -> Result<LoopResult, String> {
    let mut last_results: Vec<EffectResult> = Vec::new();

    // The carried state: every non-`_` binding from the previous tick,
    // re-exposed this tick as `_<name>` (the `_var` time-shift). One mechanism
    // for all state — scalars, records, and enums carry identically. There is
    // no seeding: the first tick has `is_first_tick = true` and the program
    // initializes explicitly.
    //
    // Two-tick history: `prev_values` is one tick ago (`_var`); `prev2_values`
    // is two ticks ago (`__var`). Each tick they shift forward
    // (prev → prev2, current → prev). `is_second_tick` is true only at tick 1,
    // when `prev_values` exists but `prev2_values` does not — the program uses
    // it to set the second initial condition before any `__var` is referenced.
    let mut prev_values: HashMap<String, Value> = HashMap::new();
    let mut prev2_values: HashMap<String, Value> = HashMap::new();

    let mut step_count = 0usize;

    while step_count < opts.max_steps {
        let mut fsm_view: HashMap<String, Value> = if fsm.install_params.is_empty() {
            snapshot.clone()
        } else {
            let mut v = snapshot.clone();
            let prefix = format!("{}.", fsm.claim_name);
            for (k, val) in snapshot.iter() {
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

        // `_var` / `__var` time-shift: expose the previous tick's value of each
        // `_`-prefixed membership (whole value, plus per-field for records).
        // A membership with `k` leading underscores reads from the k-th history
        // map (1 = previous tick, 2 = two ticks ago). Set `is_first_tick`
        // (tick 0) and `is_second_tick` (tick 1) bootstrap flags.
        if let Some(claim) = rt.get_schema(&fsm.claim_name) {
            let is_first = prev_values.is_empty();
            let is_second = !prev_values.is_empty() && prev2_values.is_empty();
            let mut sees_underscore = false;
            for item in &claim.body {
                if let BodyItem::Membership { name, .. } = item {
                    let depth = name.chars().take_while(|c| *c == '_').count();
                    if depth == 0 { continue; }
                    sees_underscore = true;
                    let base = &name[depth..];
                    let src = if depth >= 2 { &prev2_values } else { &prev_values };
                    if let Some(prev) = src.get(base) {
                        fsm_view.insert(name.clone(), prev.clone());
                    }
                    let prefix = format!("{base}.");
                    for (k, v) in src {
                        if let Some(field) = k.strip_prefix(&prefix) {
                            fsm_view.insert(format!("{name}.{field}"), v.clone());
                        }
                    }
                }
            }
            if sees_underscore {
                fsm_view.insert("is_first_tick".to_string(), Value::Bool(is_first));
                fsm_view.insert("is_second_tick".to_string(), Value::Bool(is_second));
            }
        }

        let r = rt
            .query_with_pins_and_given(&fsm.claim_name, &[], &fsm_view)
            .map_err(|e| format!("FSM `{}` solve step {step_count}: {e}", fsm.claim_name))?;

        if !r.satisfied {
            eprintln!("[loop] FSM `{}` returned UNSAT on tick {step_count}", fsm.claim_name);
            return Ok(LoopResult {
                steps: step_count,
                final_state: None,
                halted_clean: false,
                exit_code: ctx.exit_requested,
            });
        }

        let effects = collect_dispatchable_effects(
            rt, &fsm.claim_name, &r.bindings, fsm.effects_var.as_deref());

        // Roll this tick's bindings into the carried state, noting whether any
        // carried value actually changed — that, together with effects, is the
        // halt signal. `_`-prefixed bindings, `is_first_tick`, and
        // `is_second_tick` are inputs, not carried state.
        //
        // Two-tick shift: this tick's `prev_values` becomes next tick's
        // `prev2_values` (`__var`), and this tick's bindings become next tick's
        // `prev_values` (`_var`). Build the new `prev_values` fresh so the
        // change-check compares against the right (one-tick-ago) snapshot.
        let mut new_prev: HashMap<String, Value> = HashMap::new();
        let mut values_changed = false;
        for (k, v) in r.bindings.iter() {
            if k.starts_with('_') { continue; }
            if k == "is_first_tick" || k == "is_second_tick" { continue; }
            if prev_values.get(k) != Some(v) { values_changed = true; }
            new_prev.insert(k.clone(), v.clone());
        }
        prev2_values = std::mem::replace(&mut prev_values, new_prev);

        let any_effect = !effects.is_empty();
        last_results = dispatch_all(ctx, &effects);

        step_count += 1;

        if ctx.exit_requested.is_some() {
            return Ok(LoopResult {
                steps: step_count,
                final_state: None,
                halted_clean: true,
                exit_code: ctx.exit_requested,
            });
        }

        // Halt when the tick was a fixpoint: nothing carried changed and no
        // effect fired, so no further tick can do anything new.
        if !values_changed && !any_effect {
            return Ok(LoopResult {
                steps: step_count,
                final_state: None,
                halted_clean: true,
                exit_code: ctx.exit_requested,
            });
        }
    }

    Ok(LoopResult {
        steps: step_count,
        final_state: None,
        halted_clean: false,
        exit_code: ctx.exit_requested,
    })
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
