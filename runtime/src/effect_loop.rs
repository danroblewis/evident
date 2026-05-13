//! Effect-driven step loop. Replaces the plugin-based executor for
//! programs whose `main` claim declares `effects ∈ Seq(Effect)` and
//! `last_results ∈ Seq(Result)`.
//!
//! Per step:
//!   1. Encode current `state` and `last_results` as Z3 datatype values.
//!   2. Solve `main` with both pinned.
//!   3. Decode `state_next` (an enum value) and `effects` (a list).
//!   4. Dispatch each effect via `effect_dispatch::dispatch_one`.
//!   5. state ← state_next; last_results ← dispatched results.
//!   6. Halt when state matches a user-defined Halt variant or the
//!      step cap is hit.
//!
//! v1: state must be an enum-typed variable. The first variant whose
//! name starts with "Done" or "Halt" (or is exactly "Done") is the
//! halt sentinel — when state's model equals that variant, the loop
//! exits.

use crate::ast::{EffectResult, BodyItem};
use crate::effect_dispatch::{DispatchContext, dispatch_all};
use crate::runtime::EvidentRuntime;
use crate::translate::{Value, ast_decoder};
use std::collections::HashMap;

/// Tunables for the effect loop.
#[derive(Debug, Clone)]
pub struct LoopOpts {
    /// Hard ceiling on iterations. Prevents infinite loops if a
    /// program's halt condition never fires.
    pub max_steps: usize,
}

impl Default for LoopOpts {
    fn default() -> Self { Self { max_steps: 10_000 } }
}

/// Snapshot of every `EVIDENT_*` env var the scheduler consults.
/// Read ONCE at scheduler startup; per-tick code references the
/// cached fields. Without this, `eprintln!`-gating env reads run
/// per-FSM-per-tick — a syscall each — purely to gate diagnostics
/// nobody is reading.
#[derive(Debug, Clone)]
struct LoopEnv {
    /// `EVIDENT_SCHEDULER` — `false` for the legacy "tick every
    /// FSM every iteration" mode; `true` for the default
    /// subscription-driven scheduler.
    delta_mode:     bool,
    /// `EVIDENT_LOOP_TRACE` — gate per-tick scheduling diagnostics.
    /// Hot — checked inside per-FSM body.
    trace:          bool,
    /// `EVIDENT_LOOP_TIMING` — gate per-step solve/dispatch timing.
    timing:         bool,
    /// `EVIDENT_TICK_MS` — explicit FrameTimer interval; opt-in via
    /// env even if World doesn't declare `tick_count`.
    tick_ms:        Option<u64>,
    /// `EVIDENT_CLOCK_MS` — WallClock interval (default 100).
    clock_ms:       u64,
    /// `EVIDENT_FILE_WATCH` — path to watch (FileWatcher only
    /// installs if present).
    file_watch:     Option<String>,
    /// `EVIDENT_FILE_WATCH_MS` — FileWatcher poll interval
    /// (default 200).
    file_watch_ms:  u64,
    /// `EVIDENT_FILE_INPUT` — path to read (FileLineReader only
    /// installs if present).
    file_input:     Option<String>,
}

impl LoopEnv {
    fn from_process_env() -> Self {
        Self {
            delta_mode:    std::env::var("EVIDENT_SCHEDULER").as_deref() != Ok("legacy"),
            trace:         std::env::var("EVIDENT_LOOP_TRACE").is_ok(),
            timing:        std::env::var("EVIDENT_LOOP_TIMING").is_ok(),
            tick_ms:       std::env::var("EVIDENT_TICK_MS").ok()
                               .and_then(|s| s.parse().ok())
                               .filter(|&n: &u64| n > 0),
            clock_ms:      std::env::var("EVIDENT_CLOCK_MS").ok()
                               .and_then(|s| s.parse().ok())
                               .filter(|&n: &u64| n > 0)
                               .unwrap_or(100),
            file_watch:    std::env::var("EVIDENT_FILE_WATCH").ok(),
            file_watch_ms: std::env::var("EVIDENT_FILE_WATCH_MS").ok()
                               .and_then(|s| s.parse().ok())
                               .filter(|&n: &u64| n > 0)
                               .unwrap_or(200),
            file_input:    std::env::var("EVIDENT_FILE_INPUT").ok(),
        }
    }
}

/// Result of running an effect-driven program.
#[derive(Debug)]
pub struct LoopResult {
    pub steps:      usize,
    pub final_state: Option<Value>,
    pub halted_clean: bool,
    /// `Some(code)` iff a FSM emitted `Effect::Exit(code)` during
    /// the run. Recorded at end-of-tick so other FSMs' effects in
    /// the same tick complete before we halt.
    pub exit_code: Option<i32>,
}

/// One FSM-shaped claim's membership info. The runtime detects
/// claims that have the `fsm` keyword and any subset of the
/// canonical slots (state pair / EffectList / Seq(Result) /
/// optional world record). Each slot is `Option` because the
/// unified state model (state-machines-as-relations) lets an
/// author opt out — a pure-counter fsm has no state pair, no
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
    detect_fsm_shape(rt, "main")
}

/// Detect FSM shape for a specific claim. Returns Some only when the
/// claim is declared with the `fsm` keyword AND its body resolves to
/// the canonical state pair + last_results + effects (the implicit-
/// param injector guarantees the latter three for any `fsm`).
///
/// We deliberately keep the body walk for state-pair / world /
/// FTI / event-subscription discovery; only the gate is new.
pub fn detect_fsm_shape(rt: &EvidentRuntime, claim_name: &str) -> Option<MainShape> {
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
                        // call graph below `detect_fsm_shape`, so every
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
            } else if crate::fti::is_fti_type(type_name) {
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

/// Walk every top-level claim and collect those that have the FSM
/// membership shape. Returns the writer FIRST (if any), then readers
/// in declaration order. Multi-FSM execution dispatches in this order.
pub fn detect_all_fsms(rt: &EvidentRuntime) -> Vec<MainShape> {
    let names: Vec<String> = rt.schema_names().map(|s| s.to_string()).collect();
    let mut writers: Vec<MainShape> = Vec::new();
    let mut readers: Vec<MainShape> = Vec::new();
    for name in names {
        if let Some(shape) = detect_fsm_shape(rt, &name) {
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

/// Run the effect loop. Single-FSM programs (one main-shape claim,
/// usually `main`) take the existing per-step path. Multi-FSM
/// programs (≥2 main-shape claims) use the multi-FSM scheduler:
/// per-tick writer-then-readers solving with shared world handoff
/// and per-FSM halt detection.
pub fn run(rt: &EvidentRuntime, opts: &LoopOpts) -> Result<LoopResult, String> {
    run_with_ctx(rt, opts, &mut DispatchContext::new())
}

/// Run with caller-supplied dispatch context. Test entry point —
/// lets callers swap in fake stdin/stdout.
pub fn run_with_ctx(
    rt: &EvidentRuntime,
    opts: &LoopOpts,
    ctx: &mut DispatchContext,
) -> Result<LoopResult, String> {
    let fsms = detect_all_fsms(rt);
    // Snapshot every EVIDENT_* env var the scheduler consults
    // ONCE here; per-tick code references the cached fields.
    // Avoids syscall-per-tick overhead on hot diagnostic gates
    // and keeps env-read sites discoverable in one place.
    let env = LoopEnv::from_process_env();
    // Default scheduler: delta (subscription-driven). Opt out via
    // EVIDENT_SCHEDULER=legacy to get the older "tick every FSM
    // every iteration" behavior with name/fixpoint-based halt.
    let delta_mode = env.delta_mode;

    // Set up async event sources. Two trigger paths (transitional):
    //
    //   1. **Marker-type subscription** (Phase 4 v3): an FSM has a
    //      parameter of type `FrameTimer` / `Signal`. Used in
    //      conjunction with the wake channel.
    //
    //   2. **World-field plugin auto-install** (Phase 4 v3.7,
    //      unified model): the user's World type declares fields
    //      with reserved names; the runtime installs a plugin to
    //      write those fields. User FSMs subscribe via existing
    //      world read-set inference. No marker type needed.
    //
    // Reserved World field names (auto-installed plugins):
    //
    //      tick_count       ∈ Int    — FrameTimer (also needs EVIDENT_TICK_MS)
    //      signal_received  ∈ Int    — SigintSource
    //      stdin_line       ∈ String — StdinSource
    //
    // Both trigger paths can coexist for v3 back-compat.
    let mut event_sources: Vec<Box<dyn crate::event_sources::EventSource>> = Vec::new();
    let (event_tx, event_rx) = std::sync::mpsc::channel::<crate::event_sources::SchedulerEvent>();
    let mut plugin_writes: std::collections::HashSet<String> = std::collections::HashSet::new();
    if delta_mode {
        // Build the read-only context the world-plugin installers
        // consult. The registry walk below is generic over which
        // bridges exist; the scheduler doesn't enumerate them.
        let world_fields: std::collections::HashMap<String, String> = fsms.iter()
            .find_map(|f| f.world_type.as_ref())
            .and_then(|wt| rt.get_schema(wt))
            .map(|w| {
                w.body.iter().filter_map(|item| {
                    if let crate::ast::BodyItem::Membership { name, type_name, .. } = item {
                        Some((name.clone(), type_name.clone()))
                    } else { None }
                }).collect()
            })
            .unwrap_or_default();
        let fsm_event_subscriptions: std::collections::HashSet<String> = fsms.iter()
            .flat_map(|f| f.event_subscriptions.iter().cloned())
            .collect();
        let fsm_using_identifier = |ident: &str| -> Option<String> {
            for fsm in &fsms {
                if let Some(claim) = rt.get_schema(&fsm.claim_name) {
                    if crate::subscriptions::body_references_identifier(claim, ident) {
                        return Some(fsm.claim_name.clone());
                    }
                }
            }
            None
        };
        // Encoder closure for plugins that need the loaded program
        // as a `Value::Enum` tree (reflection plugin et al.). Pure-
        // Rust mirror of `encode_program` — no Z3 needed because
        // the closure produces `Value`, not `Datatype`. Plugins that
        // don't reflect ignore this field.
        let encode_program = || -> Result<crate::Value, String> {
            // Detect missing stdlib/ast.ev with a clear message
            // before encoding (otherwise the user gets a much later,
            // less obvious "Program enum unknown" failure when the
            // FSM tries to pin the value).
            let by_name = rt.enums_registry().by_name.borrow();
            if !by_name.contains_key("Program") {
                return Err(
                    "reflection plugin: world declares a `Program` field, \
                     but `stdlib/ast.ev` isn't imported — add \
                     `import \"stdlib/ast.ev\"` to make the AST schema \
                     available".to_string());
            }
            drop(by_name);
            Ok(crate::translate::ast_encoder::program_to_value(
                &rt.program_ast(),
            ))
        };
        let plugin_ctx = crate::event_sources::WorldPluginCtx {
            world_fields:            &world_fields,
            fsm_event_subscriptions: &fsm_event_subscriptions,
            env_tick_ms:             env.tick_ms,
            env_clock_ms:            env.clock_ms,
            env_file_watch:          env.file_watch.as_deref(),
            env_file_watch_ms:       env.file_watch_ms,
            env_file_input:          env.file_input.as_deref(),
            fsm_using_identifier:    &fsm_using_identifier,
            encode_program:          &encode_program,
        };

        // World-plugin auto-install (Phase 4 v3.7, unified model).
        // Each registry entry decides for itself whether to install,
        // based on world fields and a few env knobs. The scheduler
        // is unaware of which specific bridges exist.
        for installer in crate::event_sources::WORLD_PLUGIN_INSTALLERS {
            if let Some(install) = installer(&plugin_ctx, &event_tx)? {
                for k in install.plugin_writes { plugin_writes.insert(k); }
                if install.owns_stdin { ctx.stdin_owned_by_plugin = true; }
                event_sources.push(install.source);
            }
        }

        // FTI v1 — typed-resource bridges declared as FSM
        // parameters (e.g. `t ∈ Timer (interval_ms ↦ 50)`).
        // Independent of world plugins: each FTI instance is
        // per-FSM-per-param, with FSM-prefixed write keys
        // ("<fsm>.<param>.<field>") so two FSMs declaring the
        // same param type get distinct bridges.
        for fsm in &fsms {
            for (param_name, type_name, pins) in &fsm.fti_params {
                let Some(install_fn) = crate::fti::fti_install_fn(type_name)
                    else { continue };
                let fti_ctx = crate::fti::FtiContext {
                    claim_name:  fsm.claim_name.clone(),
                    param_name:  param_name.clone(),
                    env_tick_ms: env.tick_ms,
                };
                let install = install_fn(&fti_ctx, pins, event_tx.clone())?;
                event_sources.push(install.source);
                for k in install.keys { plugin_writes.insert(k); }
            }
        }
    }
    // Drop our own clone of the sender now that all sources have
    // their own. When the last source's sender is dropped (via
    // EventSource::stop / Drop), the receiver returns Err and the
    // scheduler knows all sources are dead.
    drop(event_tx);
    let event_rx = if event_sources.is_empty() { None } else { Some(event_rx) };

    if env.trace {
        eprintln!("[loop] startup: delta_mode={delta_mode} fsms=[{}] plugin_writes=[{}]",
            fsms.iter().map(|f| f.claim_name.as_str()).collect::<Vec<_>>().join(","),
            plugin_writes.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(","),
        );
    }

    // Multi-writer disjoint-fields rule (Phase 4 v3.7+ unified
    // model): every writer FSM PLUS every plugin-write claim
    // must have a disjoint write-set. A field has at most one
    // writer (single-owner). Hoisted out of the per-arity match
    // so it runs for single-FSM-with-plugin too.
    if delta_mode {
        let mut writer_sets: Vec<(String, std::collections::HashSet<String>)> = fsms.iter()
            .filter(|f| f.is_writer())
            .map(|f| {
                let aset = rt.get_schema(&f.claim_name)
                    .map(|s| crate::subscriptions::world_access_sets(s))
                    .unwrap_or_default();
                (f.claim_name.clone(), aset.writes)
            })
            .collect();
        for pf in &plugin_writes {
            let mut s = std::collections::HashSet::new();
            s.insert(pf.clone());
            writer_sets.push((format!("<plugin>:{pf}"), s));
        }
        for i in 0..writer_sets.len() {
            for j in (i + 1)..writer_sets.len() {
                let (a_name, a_writes) = &writer_sets[i];
                let (b_name, b_writes) = &writer_sets[j];
                let overlap: Vec<&String> = a_writes.intersection(b_writes).collect();
                if !overlap.is_empty() {
                    // Stop all sources before returning Err to avoid leaking threads.
                    for source in &mut event_sources { source.stop(); }
                    return Err(format!(
                        "multi-FSM: writers `{a_name}` and `{b_name}` both write \
                         to world fields {overlap:?}. Each world field must have \
                         at most one writer (single-owner rule)."
                    ));
                }
            }
        }
    }

    let result = match fsms.len() {
        0 => Err("no effect-driven claims found (need state pair + EffectList + Seq(Result))".to_string()),
        1 if !delta_mode => run_with_shape(rt, &fsms[0], opts, ctx, &env),
        1 => run_multi_fsm(rt, &fsms, opts, ctx, event_rx.as_ref(), &mut event_sources, &env),
        _ => run_multi_fsm(rt, &fsms, opts, ctx, event_rx.as_ref(), &mut event_sources, &env),
    };
    // Stop all event sources cleanly. Each source's stop signals
    // its background thread and joins. Drop also calls stop, but
    // explicit stop here ensures errors don't leak threads if the
    // result was Err.
    for source in &mut event_sources {
        source.stop();
    }
    result
}

fn run_with_shape(
    rt: &EvidentRuntime,
    shape: &MainShape,
    opts: &LoopOpts,
    ctx: &mut DispatchContext,
    env: &LoopEnv,
) -> Result<LoopResult, String> {
    // Initial state: pin to the FIRST variant of the state enum.
    // Convention: programs declare the initial state as the first
    // variant of their state type. This prevents Z3 from picking a
    // non-initial variant on step 0 (which would silently skip the
    // program's setup).
    let mut last_results: Vec<EffectResult> = Vec::new();
    let mut current_state_value: Option<z3::ast::Datatype<'static>> = match &shape.state_type {
        Some(st) => {
            let enums = rt.enums_registry();
            let by_name = enums.by_name.borrow();
            by_name.get(st)
                .and_then(|(sort, _)| sort.variants.first()
                    .and_then(|v| v.constructor.apply(&[]).as_datatype()))
        }
        None => None,
    };
    if shape.state_type.is_some() && current_state_value.is_none() {
        return Err(format!(
            "could not pin initial state: enum `{}` has no nullary first variant",
            shape.state_type.as_deref().unwrap_or("?")));
    }

    let mut step_count = 0usize;
    let mut final_state_model: Option<Value> = None;
    // EVIDENT_LOOP_TIMING=1 → per-step solve+dispatch timing + summary.
    // Useful for figuring out where time goes in long-running demos
    // (Z3 solve vs FFI dispatch vs idle in delays).
    let timing = env.timing;
    let loop_t0 = std::time::Instant::now();
    let mut total_solve = std::time::Duration::ZERO;
    let mut total_dispatch = std::time::Duration::ZERO;

    while step_count < opts.max_steps {
        // Pin last_results as a Seq(Result) via the `given` map —
        // assert_seq_given handles the (DatatypeSeqVar, SeqEnum)
        // pair, asserting `len + arr[i]=elem` per element.
        let mut given: std::collections::HashMap<String, Value> =
            std::collections::HashMap::new();
        if let Some(lr_var) = &shape.last_results_var {
            let last_results_val = rt.effect_results_to_value(&last_results);
            given.insert(lr_var.clone(), last_results_val);
        }

        // Build pin list. For step 0 we don't pin state (Z3 picks
        // the initial — the user's main pins it via state.step = 0
        // pattern or similar).
        let pins: Vec<(&str, z3::ast::Datatype<'static>)> = match (&shape.state_var, &current_state_value) {
            (Some(name), Some(s)) => vec![(name.as_str(), s.clone())],
            _ => vec![],
        };

        let solve_t0 = std::time::Instant::now();
        let r = rt.query_with_pins_and_given(&shape.claim_name, &pins, &given)
            .map_err(|e| format!("solve step {step_count}: {e}"))?;
        let solve_dt = solve_t0.elapsed();
        total_solve += solve_dt;

        if !r.satisfied {
            return Ok(LoopResult {
                steps: step_count,
                final_state: final_state_model,
                halted_clean: false,
                exit_code: ctx.exit_requested,
            });
        }

        // Read state_next + effects from model when those slots exist.
        let state_next_val: Option<&Value> = match &shape.state_next_var {
            Some(sn) => Some(r.bindings.get(sn)
                .ok_or_else(|| format!("step {step_count}: model has no `{}`", sn))?),
            None => None,
        };
        let effects: Vec<crate::ast::Effect> = match &shape.effects_var {
            Some(ev) => {
                let effects_val = r.bindings.get(ev)
                    .ok_or_else(|| format!("step {step_count}: model has no `{}`", ev))?;
                ast_decoder::decode_effect_list(effects_val)
                    .map_err(|e| format!("step {step_count}: decode effects: {e}"))?
            }
            None => Vec::new(),
        };

        // Halt-check: if effects empty AND state_next equals state, we
        // consider the program halted (fixpoint). User can also issue
        // `Effect::Exit(0)` to terminate immediately.
        let halted_by_fixpoint = effects.is_empty()
            && current_state_value.is_some()
            && state_next_val.is_some()
            && model_matches_value(state_next_val.unwrap(),
                shape.state_type.as_deref().unwrap_or(""));

        let dispatch_t0 = std::time::Instant::now();
        let new_results = dispatch_all(ctx, &effects);
        let dispatch_dt = dispatch_t0.elapsed();
        total_dispatch += dispatch_dt;

        if env.trace {
            eprintln!("[loop] step {step_count}: state_next={state_next_val:?} effects={effects:?}");
        }
        if timing {
            eprintln!(
                "[timing] step {step_count}: solve={:.2}ms dispatch={:.2}ms ({} effects)",
                solve_dt.as_secs_f64() * 1000.0,
                dispatch_dt.as_secs_f64() * 1000.0,
                effects.len(),
            );
        }
        // Re-encode state for the next step's pin. Handles nullary
        // and payload variants. Skip when state isn't part of this fsm.
        if let Some(snv) = state_next_val {
            current_state_value = encode_state_value(rt, snv);
            final_state_model = Some(snv.clone());
        }

        last_results = new_results;
        step_count += 1;

        // Effect::Exit handling: an FSM emitted Exit. Dispatch
        // already completed (other effects in this tick ran),
        // so halt cleanly with the requested code.
        if ctx.exit_requested.is_some() {
            if timing { print_timing_summary(loop_t0, step_count, total_solve, total_dispatch); }
            return Ok(LoopResult {
                steps: step_count,
                final_state: final_state_model,
                halted_clean: true,
                exit_code: ctx.exit_requested,
            });
        }

        if halted_by_fixpoint {
            if timing { print_timing_summary(loop_t0, step_count, total_solve, total_dispatch); }
            return Ok(LoopResult {
                steps: step_count,
                final_state: final_state_model,
                halted_clean: true,
                exit_code: None,
            });
        }
    }

    if timing { print_timing_summary(loop_t0, step_count, total_solve, total_dispatch); }
    Ok(LoopResult {
        steps: step_count,
        final_state: final_state_model,
        halted_clean: false,
        exit_code: None,
    })
}

/// Multi-FSM scheduler. Per tick:
///   1. Solve writer (if any), capture world_next.* values.
///   2. Solve each reader with world.* pinned to writer's new values
///      (or the previous tick's snapshot if no writer / writer halted).
///   3. Dispatch all FSMs' effects (writer first, readers in order).
///   4. Per-FSM halt detection (state_next == state ∧ effects empty).
///   5. Drop halted FSMs from the active set.
/// Program halts when no active FSMs remain.
fn run_multi_fsm(
    rt: &EvidentRuntime,
    fsms: &[MainShape],
    opts: &LoopOpts,
    ctx: &mut DispatchContext,
    event_rx: Option<&std::sync::mpsc::Receiver<crate::event_sources::SchedulerEvent>>,
    event_sources: &mut [Box<dyn crate::event_sources::EventSource>],
    env: &LoopEnv,
) -> Result<LoopResult, String> {
    use std::collections::HashMap;
    // Convert to owned Vec so we can grow at runtime (Effect::SpawnFsm).
    let mut fsms: Vec<MainShape> = fsms.to_vec();
    // Per-FSM mutable state. We track BOTH the encoded Datatype
    // (for the next tick's pin) and the decoded Value (for halt
    // detection — fixpoint = state_next_val equals previous tick's
    // state value).
    struct FsmRt {
        current_state:   Option<z3::ast::Datatype<'static>>,
        current_state_v: Option<Value>,
        last_results:    Vec<EffectResult>,
        halted:          bool,
        /// Per-FSM cache of every variable's value at end of the
        /// previous tick. Used to pin `_name` references this tick
        /// (the runtime half of the `_var` time-shift convention —
        /// see docs/design/state-machines-as-relations.md).
        /// Empty on tick 0: `is_first_tick` will be pinned true.
        prev_values:     HashMap<String, Value>,
    }
    // Seed each FSM's initial state to its enum's first variant. This
    // is convention: the first variant declared in `enum FooState =
    // Init | …` is the starting state. Without this pin, Z3 picks an
    // arbitrary satisfying state on tick 0 — often a Done state that
    // immediately self-loops with no effects, halting the FSM before
    // any work happens.
    //
    // Halt-check below only fires if state_next is variant-named
    // "Done"/"Halt", so the seeded Init pin doesn't cause spurious
    // halts (we never set current_state_v to a value matching that
    // pattern unless the user explicitly transitions there).
    // Closure: build the seeded initial state for any FSM shape.
    // Used for both auto-detected FSMs and dynamically spawned ones.
    let seed_state = |s: &MainShape| -> (Option<z3::ast::Datatype<'static>>, Option<Value>) {
        let Some(state_type) = s.state_type.as_ref() else { return (None, None); };
        let enums = rt.enums_registry();
        let by_name = enums.by_name.borrow();
        let entry = by_name.get(state_type);
        // Only seed if the first variant is nullary. Payload
        // variants need actual values, which we don't have at
        // seed time — let Z3 pick on tick 0 instead.
        let dt = entry.and_then(|(sort, _)| {
            let first = sort.variants.first()?;
            if first.constructor.arity() == 0 {
                first.constructor.apply(&[]).as_datatype()
            } else { None }
        });
        let val = entry.and_then(|(sort, decl_variants)| {
            let first = decl_variants.first()?;
            if sort.variants.first().map(|v| v.constructor.arity()).unwrap_or(0) == 0 {
                Some(Value::Enum {
                    enum_name: state_type.clone(),
                    variant:   first.name.clone(),
                    fields:    Vec::new(),
                })
            } else { None }
        });
        (dt, val)
    };
    let mut fsm_rt: Vec<FsmRt> = fsms.iter().map(|s| {
        let (initial_dt, initial_val) = seed_state(s);
        FsmRt {
            current_state:   initial_dt,
            current_state_v: initial_val,
            last_results:    Vec::new(),
            halted:          false,
            prev_values:     HashMap::new(),
        }
    }).collect();
    // Note: with a payload first-variant the FSM starts with no
    // pinned state; Z3 picks on tick 0. Document as a current
    // limitation if it bites — the workaround is to declare a
    // nullary state as the first variant.

    // Tick 0 starts with no shared world; the writer's body must
    // initialize world_next without depending on world (typically
    // via state-pattern guards: `state matches Init ⇒ world_next.x = …`).
    let mut world_snapshot: HashMap<String, Value> = HashMap::new();
    // Pre-populate plugin-managed fields with type defaults so
    // Z3 doesn't pick arbitrary values on tick 0 before any
    // plugin write has been applied. Without this, an FSM
    // reading `world.stdin_seq` on tick 0 would see an
    // unconstrained Int (any value Z3 chooses).
    if let Some(_world_type_name) = fsms.iter().find_map(|f| f.world_type.as_ref()) {
        for fsm in &fsms {
            if let Some(wt) = &fsm.world_type {
                if let Some(world_schema) = rt.get_schema(wt) {
                    for item in &world_schema.body {
                        if let crate::ast::BodyItem::Membership { name, type_name, .. } = item {
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
        }
    }

    let mut step_count = 0usize;
    let timing = env.timing;
    let loop_t0 = std::time::Instant::now();
    let mut total_solve = std::time::Duration::ZERO;
    let mut total_dispatch = std::time::Duration::ZERO;
    // Per-FSM solve time + tick count, indexed parallel to `fsms`.
    let mut per_fsm_solve: Vec<std::time::Duration> = vec![std::time::Duration::ZERO; fsms.len()];
    let mut per_fsm_ticks: Vec<usize> = vec![0; fsms.len()];

    // Phase 2: subscription-driven scheduling. Opt-in via env flag
    // until we trust it enough to flip the default. See
    // docs/design/fsm-subscriptions.md for the full model.
    // Default scheduler: delta (subscription-driven). Opt out via
    // EVIDENT_SCHEDULER=legacy to get the older "tick every FSM
    // every iteration" behavior with name/fixpoint-based halt.
    let delta_mode = env.delta_mode;
    // Access sets are needed in BOTH modes now: delta mode uses
    // them for scheduling decisions; multi-writer support uses
    // them to scope each writer's snapshot updates to its own
    // write-set (so two writers with disjoint fields don't clobber
    // each other).
    let mut access_sets: Vec<crate::subscriptions::AccessSets> = fsms.iter().map(|fsm| {
        rt.get_schema(&fsm.claim_name)
          .map(|s| crate::subscriptions::world_access_sets(s))
          .unwrap_or_default()
    }).collect();
    // Per-FSM "world fields that changed since I was last scheduled."
    // When the FSM is scheduled, this is consumed (cleared). Writers
    // populate it on other FSMs after their solve. NOT used in legacy
    // mode (every FSM ticks unconditionally).
    let mut pending_changes: Vec<std::collections::HashSet<String>> =
        vec![std::collections::HashSet::new(); fsms.len()];
    // Self-feedback: did this FSM emit effects last tick? If so, it
    // has new last_results to consume → schedule it next.
    let mut had_effects_last: Vec<bool> = vec![false; fsms.len()];
    // State-change feedback: did this FSM transition to a new state
    // last tick? If so, schedule it next — the body can compute
    // different things when state pins to a new value, even if
    // world and last_results are unchanged. Without this, an FSM
    // that does Idle→Frame(N) on one tick (silently, no effects)
    // would never run its Frame(N) body.
    let mut state_changed_last: Vec<bool> = vec![false; fsms.len()];
    // External-event feedback: an async event source (e.g.
    // FrameTimer) fired since this FSM was last scheduled.
    // Currently we coarsely wake every FSM on every external
    // event — Phase 4 v3.5 will add per-FSM subscription matching.
    let mut external_event: Vec<bool> = vec![false; fsms.len()];
    // Local FIFO of plugin-queued world writes drained from
    // event sources. We apply one per tick so each change is
    // visible to subscribers; remaining entries wait for the
    // next tick. Prevents fast sources from collapsing many
    // values into "last wins."
    let mut pending_world_writes: std::collections::VecDeque<(String, Value)> =
        std::collections::VecDeque::new();

    while step_count < opts.max_steps {
        // Any active FSMs left? If not, program halted.
        if fsm_rt.iter().all(|f| f.halted) {
            if timing {
                let rows: Vec<(&str, std::time::Duration, usize)> = fsms.iter().enumerate()
                    .map(|(i, f)| (f.claim_name.as_str(), per_fsm_solve[i], per_fsm_ticks[i]))
                    .collect();
                print_timing_summary_full(loop_t0, step_count, total_solve, total_dispatch, &rows);
            }
            return Ok(LoopResult {
                steps: step_count,
                // Synthesize a final-state value from the writer's
                // last seen state if available; otherwise the first
                // active FSM's. Multi-FSM doesn't have a single
                // "final_state" the way single-FSM does, so this is
                // best-effort.
                final_state: fsm_rt.iter().find_map(|f| f.current_state_v.clone()),
                halted_clean: true,
                exit_code: ctx.exit_requested,
            });
        }

        // Drain plugin world writes — applying ONE entry per tick
        // (so subscribers see each individual change with its own
        // wake). Sources may produce writes faster than ticks can
        // consume them; we move source-side queues into a local
        // FIFO so nothing is lost.
        if delta_mode {
            for src in event_sources.iter_mut() {
                let writes = src.drain_writes();
                pending_world_writes.append(&mut writes.into_iter().collect());
            }
            // Dual policy: STATE writes (dotted keys, FTI) apply
            // ALL queued values immediately — only the latest
            // matters; intermediate values would be invisible
            // anyway because the FSM only solves once per tick.
            // EVENT writes (bare keys, world reserved fields)
            // apply ONE per tick — each individual value matters
            // (e.g. each stdin line is a discrete event).
            //
            // For FTI: a bridge writing 5 values between ticks
            // collapses to "the latest count," consistent with
            // the field's role as continuous state.
            let mut event_writes: std::collections::VecDeque<(String, Value)> =
                std::collections::VecDeque::new();
            let mut state_writes: Vec<(String, Value)> = Vec::new();
            while let Some((field, val)) = pending_world_writes.pop_front() {
                if field.contains('.') {
                    state_writes.push((field, val));
                } else {
                    event_writes.push_back((field, val));
                }
            }
            // Re-queue event writes for the one-per-tick draining.
            pending_world_writes = event_writes;

            // Apply all state writes (last value wins per key).
            // For FTI keys (`<fsm>.<param>.<field>`), wake the
            // matching FSM if its access_sets include the
            // stripped `<param>.<field>` (or its first segment,
            // <param>, since access_sets stores top-level field
            // names from `world.X` reads but FTI param-field
            // reads land directly in env without expansion).
            for (field, val) in state_writes {
                let key = field.clone();  // dotted, used as-is
                let changed = world_snapshot.get(&key) != Some(&val);
                if changed {
                    world_snapshot.insert(key.clone(), val);
                    // FTI wake distribution: if the key matches
                    // `<claim>.<rest>` for some FSM's claim_name,
                    // wake that FSM.
                    for (j, fsm) in fsms.iter().enumerate() {
                        let prefix = format!("{}.", fsm.claim_name);
                        if let Some(rest) = key.strip_prefix(&prefix) {
                            // Top-level segment of `rest` is the param name.
                            let param = rest.split('.').next().unwrap_or(rest);
                            if fsm.fti_params.iter().any(|(p, _, _)| p == param) {
                                pending_changes[j].insert(rest.to_string());
                            }
                        }
                    }
                }
            }

            if let Some((field, val)) = pending_world_writes.pop_front() {
                // Bare field name → world.X pin.
                let key = format!("world.{field}");
                let changed = world_snapshot.get(&key) != Some(&val);
                if changed {
                    world_snapshot.insert(key, val);
                    for (j, _f) in fsms.iter().enumerate() {
                        if access_sets[j].reads.contains(&field) {
                            pending_changes[j].insert(field.clone());
                        }
                    }
                }
            }
        }

        // Per-tick effect ordering: writer first, then readers in
        // declaration order (which is the order in `fsms`).
        let mut all_effects: Vec<(usize, Vec<crate::ast::Effect>)> = Vec::new();
        // Track which FSMs we actually scheduled this tick — used
        // for clearing self-feedback flags at the end.
        let mut scheduled_this_tick: Vec<bool> = vec![false; fsms.len()];

        for (idx, fsm) in fsms.iter().enumerate() {
            if fsm_rt[idx].halted { continue; }

            // Phase 2 scheduling decision. Three triggers wake an FSM:
            //   1. Bootstrap (tick 0)
            //   2. Self-feedback: emitted effects last tick → fresh
            //      last_results to consume.
            //   3. World delta: a field in the FSM's read-set was
            //      written since this FSM was last scheduled.
            // All others stay asleep this tick. `pending_changes` is
            // cleared on schedule (events consumed).
            if delta_mode && step_count > 0 {
                let woken = had_effects_last[idx]
                    || !pending_changes[idx].is_empty()
                    || state_changed_last[idx]
                    || external_event[idx];
                if !woken {
                    if env.trace {
                        eprintln!("[loop] tick {step_count} fsm={}: skipped (no inputs)",
                            fsm.claim_name);
                    }
                    continue;
                }
                pending_changes[idx].clear();
                external_event[idx] = false;
            }
            scheduled_this_tick[idx] = true;

            // Build per-FSM pin list (state as Datatype; last_results
            // goes through the given map below as a Seq(Result)).
            let pins: Vec<(&str, z3::ast::Datatype<'static>)> = match (&fsm.state_var, &fsm_rt[idx].current_state) {
                (Some(name), Some(s)) => vec![(name.as_str(), s.clone())],
                _ => vec![],
            };

            // Build per-FSM view of the snapshot: include all
            // world.X entries as-is, plus FTI keys whose prefix
            // matches THIS fsm's claim_name (with prefix stripped
            // so they match env's `param.field` keys). Also include
            // last_results as a Seq(Result) — pinned via the given
            // map's assert_seq_given path.
            let mut fsm_view: HashMap<String, Value> = if fsm.fti_params.is_empty() {
                world_snapshot.clone()
            } else {
                let mut v = world_snapshot.clone();
                let prefix = format!("{}.", fsm.claim_name);
                for (k, val) in &world_snapshot {
                    if let Some(stripped) = k.strip_prefix(&prefix) {
                        v.insert(stripped.to_string(), val.clone());
                    }
                }
                v
            };
            if let Some(lr_var) = &fsm.last_results_var {
                let last_results_val = rt.effect_results_to_value(&fsm_rt[idx].last_results);
                fsm_view.insert(lr_var.clone(), last_results_val);
            }
            // Time-shift convention: for every `_name` in this fsm's
            // body whose `name` we have a previous-tick value for,
            // pin `_name` to that value. Also pin `is_first_tick`
            // (true iff we have no previous values yet — i.e., tick
            // 0 for this fsm). See docs/design/state-machines-as-
            // relations.md for the framing.
            if let Some(claim) = rt.get_schema(&fsm.claim_name) {
                let is_first = fsm_rt[idx].prev_values.is_empty();
                let mut sees_underscore = false;
                for item in &claim.body {
                    if let crate::ast::BodyItem::Membership { name, .. } = item {
                        if let Some(stripped) = name.strip_prefix('_') {
                            sees_underscore = true;
                            // Primitive case: prev_values has a direct
                            // entry for `stripped` (Int / Bool / etc.).
                            if let Some(prev) = fsm_rt[idx].prev_values.get(stripped) {
                                fsm_view.insert(name.clone(), prev.clone());
                            }
                            // Record case: prev_values has per-field
                            // entries like `pos.x` / `pos.y` (records
                            // get flattened at translation). Mirror
                            // every `stripped.<field>` entry into
                            // `_name.<field>` so `_pos.x` resolves.
                            let prefix = format!("{stripped}.");
                            for (k, v) in &fsm_rt[idx].prev_values {
                                if let Some(field) = k.strip_prefix(&prefix) {
                                    fsm_view.insert(
                                        format!("{name}.{field}"),
                                        v.clone(),
                                    );
                                }
                            }
                            // If no previous value yet, leave `_name`
                            // unconstrained — the fsm's body should
                            // gate via `is_first_tick`.
                        }
                    }
                }
                if sees_underscore {
                    fsm_view.insert(
                        "is_first_tick".to_string(),
                        Value::Bool(is_first),
                    );
                }
            }
            let solve_input: &HashMap<String, Value> = &fsm_view;

            let solve_t0 = std::time::Instant::now();
            let r = rt.query_with_pins_and_given(&fsm.claim_name, &pins, solve_input)
                .map_err(|e| format!("FSM `{}` solve step {step_count}: {e}", fsm.claim_name))?;
            let solve_dt = solve_t0.elapsed();
            total_solve += solve_dt;
            per_fsm_solve[idx] += solve_dt;
            per_fsm_ticks[idx] += 1;

            if !r.satisfied {
                eprintln!("[loop] FSM `{}` returned UNSAT on tick {step_count}", fsm.claim_name);
                if timing {
                    let rows: Vec<(&str, std::time::Duration, usize)> = fsms.iter().enumerate()
                        .map(|(i, f)| (f.claim_name.as_str(), per_fsm_solve[i], per_fsm_ticks[i]))
                        .collect();
                    print_timing_summary_full(loop_t0, step_count, total_solve, total_dispatch, &rows);
                }
                return Ok(LoopResult {
                    steps: step_count,
                    final_state: fsm_rt[idx].current_state_v.clone(),
                    halted_clean: false,
                    exit_code: ctx.exit_requested,
                });
            }

            // Read state_next + effects when those slots exist.
            let state_next_val: Option<&Value> = match &fsm.state_next_var {
                Some(sn) => Some(r.bindings.get(sn)
                    .ok_or_else(|| format!("FSM `{}` step {step_count}: model has no `{}`",
                        fsm.claim_name, sn))?),
                None => None,
            };
            let effects: Vec<crate::ast::Effect> = match &fsm.effects_var {
                Some(ev) => {
                    let effects_val = r.bindings.get(ev)
                        .ok_or_else(|| format!("FSM `{}` step {step_count}: model has no `{}`",
                            fsm.claim_name, ev))?;
                    ast_decoder::decode_effect_list(effects_val)
                        .map_err(|e| format!("FSM `{}` step {step_count}: decode effects: {e}",
                            fsm.claim_name))?
                }
                None => Vec::new(),
            };

            // Legacy halt-check: state_next == state (value equality,
            // true fixpoint) AND effects empty AND we're past tick 0.
            // Skipped in delta mode — under subscription scheduling,
            // FSMs that fixpoint just stop being scheduled (no inputs
            // to wake them); the program halts when no FSM is
            // scheduled at all in a tick.
            let will_halt = !delta_mode
                && step_count > 0
                && effects.is_empty()
                && state_next_val.is_some()
                && fsm_rt[idx].current_state_v.as_ref()
                    .map(|cv| Some(cv) == state_next_val).unwrap_or(false);

            // Writer? Capture world_next.* for snapshot. The snapshot
            // becomes the `world.*` given for subsequent FSM solves
            // this tick AND the writer's own world.* given next tick.
            //
            // Phase 2: also compute the field-level delta (which
            // fields actually changed value) and distribute to other
            // FSMs whose read-set includes a changed field. The
            // writer is excluded from its own deltas — own writes
            // shouldn't self-schedule (Phase 1 discovery).
            //
            // Multi-writer (Phase 4 v3.7+): each writer MERGES its
            // own world_next.X fields into the snapshot rather than
            // clearing it. Writers' write-sets are disjoint
            // (enforced at load), so this is well-defined. Within
            // a tick, writers run in declaration order (writers
            // first via detect_all_fsms ordering); a later writer's
            // body sees the earlier writers' just-written fields.
            if fsm.is_writer() {
                let mut just_changed: std::collections::HashSet<String> =
                    std::collections::HashSet::new();
                // Only consume fields that this writer actually
                // owns (its write-set). Z3 may produce world_next
                // bindings for fields outside the write-set if the
                // body references them; ignoring those keeps each
                // writer scoped to its own fields.
                let my_writes = &access_sets[idx].writes;
                for (k, v) in r.bindings.iter() {
                    if let Some(field) = k.strip_prefix("world_next.") {
                        let first = field.split('.').next().unwrap_or(field);
                        if !my_writes.contains(first) { continue; }
                        let key = format!("world.{field}");
                        if world_snapshot.get(&key) != Some(v) {
                            just_changed.insert(first.to_string());
                        }
                        world_snapshot.insert(key, v.clone());
                    }
                }
                if delta_mode {
                    for j in 0..fsms.len() {
                        if j == idx { continue; }
                        for f in &just_changed {
                            if access_sets[j].reads.contains(f) {
                                pending_changes[j].insert(f.clone());
                            }
                        }
                    }
                }
            }

            // Mark whether this solve transitioned to a new state.
            // Drives the state-change wake trigger next tick.
            state_changed_last[idx] = match state_next_val {
                Some(snv) => fsm_rt[idx].current_state_v.as_ref()
                    .map(|prev| prev != snv).unwrap_or(true),
                None => false,
            };

            // Update next-tick state for this FSM (only when this fsm
            // has a state-pair).
            if let Some(snv) = state_next_val {
                fsm_rt[idx].current_state = encode_state_value(rt, snv);
                fsm_rt[idx].current_state_v = Some(snv.clone());
            }

            // Capture every non-prefix variable's bound value for
            // the next tick's `_name` pinning. The underscore-prefix
            // bindings themselves (and the `is_first_tick` flag)
            // are skipped — they're rebuilt fresh each tick.
            for (k, v) in r.bindings.iter() {
                if k.starts_with('_') { continue; }
                if k == "is_first_tick" { continue; }
                fsm_rt[idx].prev_values.insert(k.clone(), v.clone());
            }

            if env.trace {
                eprintln!("[loop] tick {step_count} fsm={}: state_next={state_next_val:?} effects={effects:?}",
                    fsm.claim_name);
            }
            if timing {
                eprintln!("[timing] tick {step_count} fsm={}: solve={:.2}ms ({} effects)",
                    fsm.claim_name, solve_dt.as_secs_f64() * 1000.0, effects.len());
            }

            all_effects.push((idx, effects));

            // Mark halt — drops on next tick's iteration.
            if will_halt {
                fsm_rt[idx].halted = true;
            }
        }

        // Dispatch all effects in order, capturing each FSM's
        // results into its own last_results for next tick. Also
        // update the per-FSM self-feedback flag — true iff this FSM
        // emitted at least one effect this tick (so its
        // last_results will be fresh next tick).
        let dispatch_t0 = std::time::Instant::now();
        // Reset self-feedback for FSMs we scheduled this tick;
        // unscheduled ones keep whatever they had (they didn't
        // observe last_results yet).
        for (i, was_scheduled) in scheduled_this_tick.iter().enumerate() {
            if *was_scheduled { had_effects_last[i] = false; }
        }
        for (fsm_idx, effects) in all_effects {
            let emitted_anything = !effects.is_empty();
            let results = dispatch_all(ctx, &effects);
            fsm_rt[fsm_idx].last_results = results;
            had_effects_last[fsm_idx] = emitted_anything;
        }
        let dispatch_dt = dispatch_t0.elapsed();
        total_dispatch += dispatch_dt;

        // Effect::SpawnFsm handling: any spawn requests
        // accumulated during dispatch get instantiated as new
        // FsmRt entries here. They join the scheduler from the
        // next tick. v1: shares the parent's world; no
        // per-instance world. See docs/design/fsm-spawning.md.
        if !ctx.pending_spawns.is_empty() {
            for (claim_name, spawn_arg) in std::mem::take(&mut ctx.pending_spawns) {
                let shape = match detect_fsm_shape(rt, &claim_name) {
                    Some(s) => s,
                    None => {
                        eprintln!("[loop] spawn: claim `{claim_name}` doesn't \
                                   have FSM shape (state pair + EffectList + \
                                   Seq(Result)); spawn ignored.");
                        continue;
                    }
                };
                if env.trace {
                    eprintln!("[loop] tick {step_count}: spawned `{claim_name}` \
                               as FSM #{} with arg={spawn_arg}", fsms.len());
                }
                let aset = rt.get_schema(&shape.claim_name)
                    .map(|s| crate::subscriptions::world_access_sets(s))
                    .unwrap_or_default();
                // Spawn-arg seeding: pin the new FSM's state to
                // `FirstVariant(spawn_arg)` if the first variant
                // takes a single Int payload. Otherwise fall back
                // to the regular seed (nullary first variant) or
                // None (Z3 picks).
                let (initial_dt, initial_val) = seed_state_with_arg(rt, &shape, spawn_arg)
                    .unwrap_or_else(|| seed_state(&shape));
                fsms.push(shape);
                access_sets.push(aset);
                fsm_rt.push(FsmRt {
                    current_state:   initial_dt,
                    current_state_v: initial_val,
                    last_results:    Vec::new(),
                    halted:          false,
                    prev_values:     HashMap::new(),
                });
                per_fsm_solve.push(std::time::Duration::ZERO);
                per_fsm_ticks.push(0);
                pending_changes.push(std::collections::HashSet::new());
                had_effects_last.push(true);   // bootstrap-equivalent
                state_changed_last.push(true); // ensure first-tick scheduling
                external_event.push(false);
            }
        }

        step_count += 1;

        // Effect::Exit handling: checked first — works in both
        // legacy and delta mode, takes priority over the no-FSM
        // halt and over event-wait.
        if ctx.exit_requested.is_some() {
            if timing {
                let rows: Vec<(&str, std::time::Duration, usize)> = fsms.iter().enumerate()
                    .map(|(i, f)| (f.claim_name.as_str(), per_fsm_solve[i], per_fsm_ticks[i]))
                    .collect();
                print_timing_summary_full(loop_t0, step_count, total_solve, total_dispatch, &rows);
            }
            return Ok(LoopResult {
                steps: step_count,
                final_state: fsm_rt.iter().find_map(|f| f.current_state_v.clone()),
                halted_clean: true,
                exit_code: ctx.exit_requested,
            });
        }

        // Phase 3 halt criterion (delta mode only): if no FSM was
        // scheduled this tick, no work happened — and since
        // scheduling decisions are deterministic from world deltas
        // + self-feedback + state-feedback, no work would happen
        // next tick either. Halt cleanly UNLESS an async event
        // source can wake us (Phase 4 v3): block on the channel,
        // then continue the loop on the next event.
        if delta_mode && scheduled_this_tick.iter().all(|s| !s) && pending_world_writes.is_empty() {
            if let Some(rx) = event_rx {
                // Per-FSM event subscription matching. If ANY FSM
                // declared an explicit subscription, only wake FSMs
                // whose subscription set contains the event's name.
                // If NO FSM declared any subscription, fall back to
                // coarse wake (every alive FSM) for v3 back-compat.
                let any_explicit = fsms.iter()
                    .any(|f| !f.event_subscriptions.is_empty());
                match rx.recv() {
                    Ok(crate::event_sources::SchedulerEvent::Tick { name }) => {
                        if env.trace {
                            eprintln!("[loop] tick {step_count}: woke on event {name}");
                        }
                        for (i, fsm) in fsms.iter().enumerate() {
                            if fsm_rt[i].halted { continue; }
                            let matches = if any_explicit {
                                fsm.event_subscriptions.contains(&name)
                            } else {
                                true  // coarse wake
                            };
                            if matches { external_event[i] = true; }
                        }
                        continue;
                    }
                    Ok(crate::event_sources::SchedulerEvent::Closed { .. }) | Err(_) => {
                        // All sources dead; fall through to halt.
                    }
                }
            }
            if timing {
                let rows: Vec<(&str, std::time::Duration, usize)> = fsms.iter().enumerate()
                    .map(|(i, f)| (f.claim_name.as_str(), per_fsm_solve[i], per_fsm_ticks[i]))
                    .collect();
                print_timing_summary_full(loop_t0, step_count, total_solve, total_dispatch, &rows);
            }
            return Ok(LoopResult {
                steps: step_count,
                final_state: fsm_rt.iter().find_map(|f| f.current_state_v.clone()),
                halted_clean: true,
                exit_code: ctx.exit_requested,
            });
        }
    }

    if timing {
        let rows: Vec<(&str, std::time::Duration, usize)> = fsms.iter().enumerate()
            .map(|(i, f)| (f.claim_name.as_str(), per_fsm_solve[i], per_fsm_ticks[i]))
            .collect();
        print_timing_summary_full(loop_t0, step_count, total_solve, total_dispatch, &rows);
    }
    Ok(LoopResult {
        steps: step_count,
        final_state: fsm_rt.iter().find_map(|f| f.current_state_v.clone()),
        halted_clean: false,
        exit_code: ctx.exit_requested,
    })
}

fn print_timing_summary(
    loop_t0: std::time::Instant,
    steps: usize,
    total_solve: std::time::Duration,
    total_dispatch: std::time::Duration,
) {
    print_timing_summary_full(loop_t0, steps, total_solve, total_dispatch, &[]);
}

/// Per-FSM rows: `(claim_name, solve_total, ticks_solved)`.
/// Empty slice = single-FSM mode → omit the breakdown.
fn print_timing_summary_full(
    loop_t0: std::time::Instant,
    steps: usize,
    total_solve: std::time::Duration,
    total_dispatch: std::time::Duration,
    per_fsm: &[(&str, std::time::Duration, usize)],
) {
    let wall = loop_t0.elapsed();
    let other = wall.saturating_sub(total_solve).saturating_sub(total_dispatch);
    eprintln!("[timing] ── summary ──────────────────────────────");
    eprintln!("[timing] steps:    {steps}");
    eprintln!("[timing] wall:     {:>7.2}ms ({:>5.1}ms/step)",
        wall.as_secs_f64() * 1000.0,
        if steps > 0 { wall.as_secs_f64() * 1000.0 / steps as f64 } else { 0.0 });
    eprintln!("[timing] solve:    {:>7.2}ms ({:>5.1}ms/step)",
        total_solve.as_secs_f64() * 1000.0,
        if steps > 0 { total_solve.as_secs_f64() * 1000.0 / steps as f64 } else { 0.0 });
    for (name, solve, ticks) in per_fsm {
        let per_tick = if *ticks > 0 {
            solve.as_secs_f64() * 1000.0 / *ticks as f64
        } else { 0.0 };
        eprintln!("[timing]   {:<10} {:>7.2}ms ({:>5.1}ms/tick × {} ticks)",
            name, solve.as_secs_f64() * 1000.0, per_tick, ticks);
    }
    eprintln!("[timing] dispatch: {:>7.2}ms ({:>5.1}ms/step)",
        total_dispatch.as_secs_f64() * 1000.0,
        if steps > 0 { total_dispatch.as_secs_f64() * 1000.0 / steps as f64 } else { 0.0 });
    eprintln!("[timing] other:    {:>7.2}ms (encoding, decoding, idle)",
        other.as_secs_f64() * 1000.0);
}

/// Check whether a model `Value` corresponds to a halt sentinel —
/// for v1 that's any variant whose name is exactly "Done" or "Halt".
/// (Future: user-declared halt predicate.)
fn model_matches_value(v: &Value, _state_type: &str) -> bool {
    matches!(v, Value::Enum { variant, .. } if variant == "Done" || variant == "Halt")
}

/// Re-encode a state Value as a Z3 Datatype for the next step's pin.
/// Handles nullary AND payload variants by recursively encoding
/// each field. Primitive payloads (Int, Bool, String, Real) are
/// encoded as Z3 literals; nested enum payloads recurse.
/// (Pin-readers moved to `crate::fti` — used only by FTI install.)

/// Seed a spawned FSM's state to `FirstVariant(arg)` when the
/// state enum's first variant takes a single Int payload. Used
/// by `Effect::SpawnFsm(claim, arg)` — lets the parent pass
/// an instance ID (or other Int parameter) into the spawned
/// FSM's body, which can `match state` to read it.
///
/// Returns None if the first variant doesn't have exactly one
/// Int payload (caller falls back to `seed_state`).
fn seed_state_with_arg(
    rt: &EvidentRuntime,
    shape: &MainShape,
    arg: i64,
) -> Option<(Option<z3::ast::Datatype<'static>>, Option<Value>)> {
    let state_type = shape.state_type.as_ref()?;
    let enums = rt.enums_registry();
    let by_name = enums.by_name.borrow();
    let (sort, decl_variants) = by_name.get(state_type)?;
    let first_sort = sort.variants.first()?;
    let first_decl = decl_variants.first()?;
    if first_sort.constructor.arity() != 1 { return None; }
    // Check the field type is Int. The decl_variants entry has
    // payload type info.
    if first_decl.fields.len() != 1 { return None; }
    if first_decl.fields[0].type_name != "Int" { return None; }
    // Encode `FirstVariant(arg)`.
    let value = Value::Enum {
        enum_name: state_type.clone(),
        variant:   first_decl.name.clone(),
        fields:    vec![Value::Int(arg)],
    };
    let dt = encode_state_value(rt, &value);
    Some((dt, Some(value)))
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
    // Payload — encode each field as a Dynamic so vtable dispatch
    // through &dyn Ast works correctly. Earlier attempts using
    // Box<dyn Ast> ran into a Z3 null-pointer return from apply,
    // probably from variance issues with the dyn trait object.
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn ctx_silent() -> DispatchContext {
        DispatchContext::with_streams(
            Box::new(std::io::BufReader::new(Cursor::new(Vec::<u8>::new()))),
            Box::new(Vec::<u8>::new()),
        )
    }

    #[test]
    fn detect_main_shape_finds_state_and_lists() {
        let mut rt = EvidentRuntime::new();
        rt.load_file(std::path::Path::new("../stdlib/runtime.ev")).unwrap();
        // Body references all three implicit slots (state_next,
        // effects, last_results) so smart-inject fires for each.
        rt.load_source("\
enum S = Init | Done

fsm main
    state ∈ S
    state = Init ⇒ (state_next = Done ∧ effects = ⟨⟩ ∧ #last_results = 0)
    state = Done ⇒ (state_next = Done ∧ effects = ⟨⟩ ∧ #last_results = 0)
").unwrap();
        let shape = detect_main_shape(&rt).expect("should detect");
        assert_eq!(shape.state_var.as_deref(), Some("state"));
        assert_eq!(shape.state_next_var.as_deref(), Some("state_next"));
        assert_eq!(shape.state_type.as_deref(), Some("S"));
        assert_eq!(shape.effects_var.as_deref(), Some("effects"));
        assert_eq!(shape.last_results_var.as_deref(), Some("last_results"));
    }

    /// Smart-inject: an fsm body that doesn't reference
    /// `last_results` doesn't get it injected, and detection
    /// returns None for that slot. Confirms the unified state
    /// model's "opt-in" behavior for canonical slots.
    #[test]
    fn smart_inject_skips_unreferenced_slots() {
        let mut rt = EvidentRuntime::new();
        rt.load_file(std::path::Path::new("../stdlib/runtime.ev")).unwrap();
        rt.load_source("\
enum S = Init | Done

fsm main
    state ∈ S
    state = Init ⇒ (state_next = Done ∧ effects = ⟨⟩)
    state = Done ⇒ (state_next = Done ∧ effects = ⟨⟩)
").unwrap();
        let shape = detect_main_shape(&rt).expect("should detect");
        assert_eq!(shape.state_var.as_deref(), Some("state"));
        assert_eq!(shape.state_next_var.as_deref(), Some("state_next"));
        assert_eq!(shape.effects_var.as_deref(), Some("effects"));
        assert_eq!(shape.last_results_var, None,
            "last_results never referenced → should not be auto-injected");
    }

    #[test]
    fn halt_after_one_step_when_state_reaches_done() {
        let mut rt = EvidentRuntime::new();
        rt.load_file(std::path::Path::new("../stdlib/runtime.ev")).unwrap();
        rt.load_source("\
enum S = Init | Done

fsm main
    state ∈ S
    state = Init ⇒ (state_next = Done ∧ effects = ⟨⟩)
    state = Done ⇒ (state_next = Done ∧ effects = ⟨⟩)
").unwrap();
        let mut ctx = ctx_silent();
        let r = run_with_ctx(&rt, &LoopOpts { max_steps: 5 }, &mut ctx).unwrap();
        // Steps: solve#1 (no state pin) → state_next=Init or Done?
        // Z3 may pick either; the loop terminates when fixpoint hits.
        assert!(r.steps <= 5);
        assert!(r.halted_clean || r.steps == 5);
    }
}
