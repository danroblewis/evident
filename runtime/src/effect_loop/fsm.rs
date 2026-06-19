//! `MainShape` + the param-resolution walk that turns a `fsm`-keyword'd
//! schema into the slot-resolved record the loop consumes.
//!
//! The set of FSMs is determined by the `fsm` parse-time keyword, NOT
//! by walking the body looking for "fsm-shaped" structure. The body
//! walk here is for *resolving* which slots an fsm uses (state pair,
//! last_results, effects, world, install params), all of which are
//! Option because the unified state model lets authors opt out.

use crate::core::ast::BodyItem;
use crate::runtime::EvidentRuntime;

/// Resolved param info for one `fsm`-keyword'd schema. The body walk
/// resolves which slots an fsm uses (state pair, last_results,
/// effects, world, install params); these are Option because the
/// unified state model lets authors opt out — a pure-counter fsm has
/// no state pair, no effects, no last_results, just plain variables
/// coordinated via `_var` time-shift.
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
    /// Name of the `world_next` membership, if this FSM writes world.
    /// After the `_world`/`world` time-shift desugar (`unify_world_syntax`),
    /// a world-touching FSM has both: `world.X` reads the previous
    /// tick's snapshot, `world_next.X` is its write for this tick.
    pub world_next_var:   Option<String>,
    /// Type name of the world record, if `world_var` is set.
    pub world_type:       Option<String>,
    /// Typed-resource parameters with a declarative `install ∈
    /// Seq(InstallStep)` body member: `(param_name, type_name, pins)`.
    /// The runtime runs the install Seq once at startup and exposes
    /// the bound fields via per-FSM `<fsm>.<param>.<field>` keys.
    pub install_params: Vec<(String, String, crate::core::ast::Pins)>,
}

pub fn detect_main_shape(rt: &EvidentRuntime) -> Option<MainShape> {
    resolve_fsm(rt, "main")
}

/// True iff `type_name`'s body declares a declarative install Seq.
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

/// Resolve a single schema's FSM param info. Returns Some only when
/// the schema is declared with the `fsm` keyword (and isn't
/// `external` — those are Rust-side bridge contracts, not user FSMs).
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
    let mut install_params: Vec<(String, String, crate::core::ast::Pins)> = Vec::new();
    // Walk this claim's body PLUS the bodies of any `..PassthroughClaim`
    // so a declarative library (e.g. packages/sdl/scene.ev's
    // `..SDLScene`) contributes its state-machine vars to the outer
    // claim.
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
                        // SAFETY: lifetime laundering — `sub.body` is owned
                        // by `rt.schemas` and `rt` is borrowed for `'a`; the
                        // map isn't mutated through interior mutability in
                        // this call graph, so the `&SchemaDecl` lives as long
                        // as `rt`. (Same invariant the prior multi-FSM code
                        // relied on.)
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
                // State-pair detection (same type, two vars, one
                // ending in `_next`). Excludes world which matched above.
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

/// Collect every `fsm`-keyword'd schema's resolved param info, in
/// declaration order (the order the loop dispatches effects each tick).
/// Claims named `sat_*` / `unsat_*` are static assertions, not FSMs,
/// and are skipped.
pub fn all_fsms(rt: &EvidentRuntime) -> Vec<MainShape> {
    rt.schema_names()
        .map(|s| s.to_string())
        .collect::<Vec<_>>()
        .into_iter()
        .filter(|n| !n.starts_with("sat_") && !n.starts_with("unsat_"))
        .filter_map(|n| resolve_fsm(rt, &n))
        .collect()
}
