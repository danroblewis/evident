//! `MainShape` + the param-resolution walk that turns a `fsm`-keyword'd
//! schema into the slot-resolved record the schedulers consume.
//!
//! The set of FSMs is determined by the `fsm` parse-time keyword, NOT
//! by walking the body looking for "fsm-shaped" structure. The body
//! walk here is for *resolving* which slots an fsm uses (state pair,
//! last_results, effects, world, FTI params), all of which are Option
//! because the unified state model lets authors opt out.

use crate::ast::BodyItem;
use crate::runtime::EvidentRuntime;

/// Resolved param info for one `fsm`-keyword'd schema. The set
/// of FSMs is determined by the `fsm` keyword, NOT by walking
/// the body looking for "fsm-shaped" structure — there's no
/// shape check anywhere; we just read the parse-time tag. The
/// body walk here is for *resolving* which slots an fsm uses
/// (state pair, last_results, effects, world, FTI params), all
/// of which are Option because the unified state model lets
/// authors opt out — a pure-counter fsm has no state pair, no
/// effects, no last_results, just plain variables coordinated
/// via `_var` time-shift.
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
    /// Name of the `world_next` membership; presence makes this FSM
    /// the world WRITER. v1: at most one writer per program.
    pub world_next_var:   Option<String>,
    /// Type name of the world record, if `world_var` or
    /// `world_next_var` is set.
    pub world_type:       Option<String>,
    /// Async event source names this FSM subscribes to. Inferred
    /// from FSM parameters of marker types in `stdlib/runtime.ev`:
    ///   * `_ ∈ FrameTimer` → "tick"
    ///   * `_ ∈ Signal`     → "signal"
    /// If empty AND no other FSM in the program declares any
    /// subscription, the runtime coarsely wakes every FSM on every
    /// event (back-compat for v3-era programs).
    pub event_subscriptions: std::collections::HashSet<String>,
    /// FTI v1+ — typed resource parameters: `(param_name,
    /// type_name, pins)` where `type_name` is a registered FTI
    /// type (currently: `FrameClock`, `Hostname`, `Timer`).
    /// `pins` carries any `(field ↦ value)` configuration the
    /// user supplied (e.g. `t ∈ Timer (interval_ms ↦ 50)`); the
    /// bridge install reads pins at startup for type-specific
    /// configuration. The runtime auto-installs a bridge plugin
    /// per entry that writes the type's fields via per-FSM
    /// `<fsm>.<param>.<field>` pin keys.
    pub fti_params: Vec<(String, String, crate::ast::Pins)>,
}

impl MainShape {
    pub fn is_writer(&self) -> bool { self.world_next_var.is_some() }
}

pub fn detect_main_shape(rt: &EvidentRuntime) -> Option<MainShape> {
    resolve_fsm(rt, "main")
}

/// Resolve a single schema's FSM param info. Returns Some only
/// when the schema is declared with the `fsm` keyword (and isn't
/// `external` — those are Rust-side bridge contracts, not user
/// FSMs). The body walk inside resolves which slots the FSM
/// actually uses (state pair, last_results, effects, world, FTI
/// params); it does NOT decide whether the schema is an FSM.
/// That decision is purely the parse-time keyword tag.
pub fn resolve_fsm(rt: &EvidentRuntime, claim_name: &str) -> Option<MainShape> {
    let claim = rt.get_schema(claim_name)?;
    if !matches!(claim.keyword, crate::ast::Keyword::Fsm) {
        return None;
    }
    // `external fsm` declarations are CONTRACTS for runtime-side
    // bridge FSMs (StdinSource, FrameTimer, EffectDispatcher, …).
    // Their body is implemented in Rust; the Evident declaration
    // names the shared-state slots they read/write so user FSMs
    // can name-match against them. Do NOT auto-instantiate them
    // as user FSMs — the Rust bridges run them.
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
    let mut fti_params:     Vec<(String, String, crate::ast::Pins)> = Vec::new();
    // Walk this claim's body PLUS the bodies of any
    // `..PassthroughClaim` so a declarative library (e.g.
    // packages/sdl/scene.ev's `..SDLScene`) contributes its
    // state-machine vars to the outer claim.
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
                        // SAFETY: lifetime laundering — borrow checker
                        // can't prove the elided lifetime on `sub.body`
                        // is `'a` across the recursive closure call.
                        //
                        // Invariant being upheld: `sub.body` is owned by
                        // `rt.schemas` (a HashMap<String, SchemaDecl>),
                        // and `rt: &'a EvidentRuntime` borrows the
                        // runtime for `'a`. The HashMap is not mutated
                        // through interior mutability anywhere in the
                        // call graph below `resolve_fsm`, so every
                        // `&SchemaDecl` we obtain via `get_schema` lives
                        // exactly as long as `rt` itself — which is `'a`.
                        // The borrow checker can't see that across the
                        // closure-recursion boundary because `get_schema`'s
                        // returned lifetime is elided and tied to its
                        // `&self`, which the inner call site re-borrows.
                        //
                        // What would break this: any future change that
                        // makes `EvidentRuntime::schemas` interior-mutable
                        // (e.g. `RefCell<HashMap>`) so a passthrough
                        // resolution could invalidate previously-handed-out
                        // `&SchemaDecl`s. The fix in that world is a
                        // restructure (clone bodies into an owned Vec,
                        // or take a `&mut` borrow once at the top), not
                        // another transmute.
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
                // State-pair detection (same type, two vars, one
                // ending in `_next`). Excludes world/world_next which
                // matched above.
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

/// Collect every `fsm`-keyword'd schema's resolved param info.
/// Returns the writer FIRST (if any), then readers in declaration
/// order. Multi-FSM execution dispatches in this order.
pub fn all_fsms(rt: &EvidentRuntime) -> Vec<MainShape> {
    let names: Vec<String> = rt.schema_names().map(|s| s.to_string()).collect();
    let mut writers: Vec<MainShape> = Vec::new();
    let mut readers: Vec<MainShape> = Vec::new();
    for name in names {
        if let Some(shape) = resolve_fsm(rt, &name) {
            // Skip claims that carry the `spawnable_only` body marker
            // (one of `crate::ast::BODY_MARKERS`) — they should only
            // run when explicitly spawned via Effect::SpawnFsm, not
            // auto-instantiated at startup.
            if let Some(claim) = rt.get_schema(&name) {
                let is_spawn_only = claim.body.iter().any(|item| {
                    matches!(item,
                        crate::ast::BodyItem::Constraint(crate::ast::Expr::Identifier(s))
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
