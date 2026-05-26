//! `MainShape` + param-resolution walk: `fsm`-keyword schemas → slot-resolved scheduler records.
//! FSM identity comes from the parse-time keyword, not body-shape detection.

use crate::core::ast::BodyItem;
use crate::runtime::EvidentRuntime;

/// Resolved param info for one `fsm` schema. All slots are Option — authors can opt out.
#[derive(Clone)]
pub struct MainShape {
    pub claim_name:       String,
    pub state_var:        Option<String>,
    pub state_next_var:   Option<String>,
    pub state_type:       Option<String>,
    pub last_results_var: Option<String>,
    pub effects_var:      Option<String>,
    /// Name of the `world` membership, if this FSM reads world.
    pub world_var:        Option<String>,
    /// Presence makes this FSM the world WRITER. At most one writer per program.
    pub world_next_var:   Option<String>,
    pub world_type:       Option<String>,
    /// Inferred from marker-type params (`FrameTimer`→"tick", `Signal`→"signal").
    /// Empty + no other FSM subscribed → coarse wake for back-compat.
    pub event_subscriptions: std::collections::HashSet<String>,
    /// FTI typed resource params: `(param_name, type_name, pins)`.
    /// Runtime auto-installs a bridge plugin per entry; pins configure it at startup.
    pub fti_params: Vec<(String, String, crate::core::ast::Pins)>,
}

impl MainShape {
    pub fn is_writer(&self) -> bool { self.world_next_var.is_some() }
}

pub fn detect_main_shape(rt: &EvidentRuntime) -> Option<MainShape> {
    resolve_fsm(rt, "main")
}

/// Resolve FSM param info for `claim_name`. Returns None if not `fsm`-tagged or is `external`.
pub fn resolve_fsm(rt: &EvidentRuntime, claim_name: &str) -> Option<MainShape> {
    let claim = rt.get_schema(claim_name)?;
    if !matches!(claim.keyword, crate::core::ast::Keyword::Fsm) {
        return None;
    }
    // `external fsm` = Rust-side bridge contract (StdinSource, FrameTimer, …). Don't instantiate.
    if claim.external {
        return None;
    }
    let mut state_pair: Option<(String, String, String)> = None;
    let mut last_results_var = None;
    let mut effects_var = None;
    let mut world_var:      Option<String> = None;
    let mut world_next_var: Option<String> = None;
    let mut world_type:     Option<String> = None;
    let mut event_subs:     std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut fti_params:     Vec<(String, String, crate::core::ast::Pins)> = Vec::new();
    // Walk body + transitive `..Passthrough` bodies so library claims contribute their slots.
    let mut all_items: Vec<&BodyItem> = Vec::new();
    let mut visited: std::collections::HashSet<String> = std::collections::HashSet::new();
    fn collect<'a>(
        items: &'a [BodyItem],
        rt: &'a EvidentRuntime,
        out: &mut Vec<&'a BodyItem>,
        visited: &mut std::collections::HashSet<String>,
    ) {
        for item in items {
            out.push(item);
            if let BodyItem::Passthrough(name) = item {
                if visited.insert(name.clone()) {
                    if let Some(sub) = rt.get_schema(name) {
                        // SAFETY: `sub.body` lives for `'a` but borrow checker loses it across
                        // the closure boundary. Breaks if `schemas` becomes interior-mutable.
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
            } else if type_name == "FrameTimer" {
                event_subs.insert("tick".to_string());
            } else if type_name == "Signal" {
                event_subs.insert("signal".to_string());
            } else if crate::fti::is_fti_type(type_name)
                   || rt.get_schema(type_name).map(|s| s.body.iter().any(|i|
                          matches!(i, BodyItem::Membership { name, type_name: ty, .. }
                                   if name == "install" && ty == "Seq(InstallStep)"))
                      ).unwrap_or(false)
            {
                fti_params.push((name.clone(), type_name.clone(), pins.clone()));
            } else if type_name != "Int" && type_name != "Bool"
                   && type_name != "String" && type_name != "Real"
                   && !type_name.starts_with("Seq(")
                   && !type_name.starts_with("Set(")
            {
                // State-pair: same type, two vars, one ending in `_next`. world/world_next excluded above.
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
        claim_name:       claim_name.to_string(),
        state_var,
        state_next_var,
        state_type,
        last_results_var,
        effects_var,
        world_var,
        world_next_var,
        world_type,
        event_subscriptions: event_subs,
        fti_params,
    })
}

/// All `fsm` schemas resolved: writer first, then readers in declaration order.
pub fn all_fsms(rt: &EvidentRuntime) -> Vec<MainShape> {
    let names: Vec<String> = rt.schema_names().map(|s| s.to_string()).collect();
    // FSMs referenced by `run(...)`/`halts_within(...)` are embedded-only — skip them
    // so `claim`→`fsm` relabeling doesn't accidentally add them as top-level FSMs.
    let embedded = rt.embedded_fsm_targets();
    let mut writers: Vec<MainShape> = Vec::new();
    let mut readers: Vec<MainShape> = Vec::new();
    for name in names {
        if embedded.contains(&name) { continue; }
        if let Some(shape) = resolve_fsm(rt, &name) {
            // Skip `spawnable_only` FSMs — run only when explicitly spawned via Effect::SpawnFsm.
            if let Some(claim) = rt.get_schema(&name) {
                let is_spawn_only = claim.body.iter().any(|item| {
                    matches!(item,
                        crate::core::ast::BodyItem::Constraint(crate::core::ast::Expr::Identifier(s))
                        if s == "spawnable_only")
                });
                if is_spawn_only { continue; }
            }
            if shape.is_writer() { writers.push(shape) } else { readers.push(shape) }
        }
    }
    let mut all = writers;
    all.extend(readers);
    all
}

/// World read/write sets, resolving `..Passthrough` transitively. Without this, passthrough-writers
/// get empty sets, silently bypassing the disjoint-owner check and scheduler scoping.
pub fn full_world_access(
    rt: &EvidentRuntime,
    claim_name: &str,
) -> crate::subscriptions::AccessSets {
    fn rec(
        rt: &EvidentRuntime,
        name: &str,
        acc: &mut crate::subscriptions::AccessSets,
        visited: &mut std::collections::HashSet<String>,
    ) {
        if !visited.insert(name.to_string()) {
            return;
        }
        let Some(s) = rt.get_schema(name) else { return };
        let local = crate::portable::subscriptions::access_sets(s);
        acc.reads.extend(local.reads);
        acc.writes.extend(local.writes);
        for item in &s.body {
            if let BodyItem::Passthrough(p) = item {
                rec(rt, p, acc, visited);
            }
        }
    }
    let mut acc = crate::subscriptions::AccessSets::default();
    let mut visited = std::collections::HashSet::new();
    rec(rt, claim_name, &mut acc, &mut visited);
    acc
}
