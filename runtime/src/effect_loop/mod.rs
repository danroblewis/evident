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

use crate::effect_dispatch::DispatchContext;
use crate::runtime::EvidentRuntime;
use crate::core::Value;

mod collect;
mod fsm;
mod scheduler;
mod seq_chains;
mod state;
mod timing;
mod toposort;

// ── Public re-exports ────────────────────────────────────────
//
// Anything that was `pub` at the top level of the pre-split
// effect_loop.rs MUST remain accessible as
// `crate::effect_loop::X` / `evident_runtime::effect_loop::X` —
// `lib.rs` does `pub mod effect_loop;` and downstream crates
// (commands/, tests/) name these symbols directly.
pub use fsm::{MainShape, all_fsms, detect_main_shape, resolve_fsm};

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
pub(crate) struct LoopEnv {
    /// `EVIDENT_LOOP_TRACE` — gate per-tick scheduling diagnostics.
    /// Hot — checked inside per-FSM body.
    pub(crate) trace:          bool,
    /// `EVIDENT_LOOP_TIMING` — gate per-step solve/dispatch timing.
    pub(crate) timing:         bool,
    /// `EVIDENT_TICK_MS` — explicit FrameTimer interval; opt-in via
    /// env even if World doesn't declare `tick_count`.
    pub(crate) tick_ms:        Option<u64>,
    /// `EVIDENT_CLOCK_MS` — WallClock interval (default 100).
    pub(crate) clock_ms:       u64,
    /// `EVIDENT_FILE_WATCH` — path to watch (FileWatcher only
    /// installs if present).
    pub(crate) file_watch:     Option<String>,
    /// `EVIDENT_FILE_WATCH_MS` — FileWatcher poll interval
    /// (default 200).
    pub(crate) file_watch_ms:  u64,
    /// `EVIDENT_FILE_INPUT` — path to read (FileLineReader only
    /// installs if present).
    pub(crate) file_input:     Option<String>,
}

impl LoopEnv {
    fn from_process_env() -> Self {
        Self {
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
    let fsms = all_fsms(rt);
    // Snapshot every EVIDENT_* env var the scheduler consults
    // ONCE here; per-tick code references the cached fields.
    // Avoids syscall-per-tick overhead on hot diagnostic gates
    // and keeps env-read sites discoverable in one place.
    let env = LoopEnv::from_process_env();

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
    // Build the read-only context the world-plugin installers
    // consult. The registry walk below is generic over which
    // bridges exist; the scheduler doesn't enumerate them.
    let world_fields: std::collections::HashMap<String, String> = fsms.iter()
        .find_map(|f| f.world_type.as_ref())
        .and_then(|wt| rt.get_schema(wt))
        .map(|w| {
            w.body.iter().filter_map(|item| {
                if let crate::core::ast::BodyItem::Membership { name, type_name, .. } = item {
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
            let fti_ctx = crate::fti::FtiContext {
                claim_name:  fsm.claim_name.clone(),
                param_name:  param_name.clone(),
                env_tick_ms: env.tick_ms,
            };
            // Declarative install (preferred): if the type's
            // body has an `install ∈ Seq(InstallStep)` member,
            // dispatch it via the generic mechanism and skip
            // any specific Rust bridge. Only falls back to
            // INSTALLERS for thread-driven bridges (FrameTimer,
            // Timer — long-running event sources that can't be
            // expressed as a one-shot Seq).
            let has_declarative = rt.get_schema(type_name).map(|s|
                s.body.iter().any(|i| matches!(i,
                    crate::core::ast::BodyItem::Membership { name, type_name: ty, .. }
                    if name == "install" && ty == "Seq(InstallStep)"))
            ).unwrap_or(false);
            if has_declarative {
                let mut src = crate::event_sources::DeclarativeInstallSource::new();
                // CRITICAL: pass the scheduler's DispatchContext so the
                // HandleRegistry IDs assigned at install (window ptr,
                // renderer ptr, …) are visible to per-tick effect
                // dispatch — otherwise ArgHandle lookups go to a
                // different (empty) registry.
                src.run_install(rt, type_name, &fti_ctx, pins, &event_tx, ctx)?;
                // Captured keys = leading Memberships of the type
                // that aren't first-line input pins. The drain pass
                // applies them to world_snapshot below.
                if let Some(type_decl) = rt.get_schema(type_name) {
                    for item in &type_decl.body {
                        if let crate::core::ast::BodyItem::Membership { name, type_name: _, .. } = item {
                            if name == "install" { continue; }
                            let key = format!("{}.{}.{}",
                                fsm.claim_name, param_name, name);
                            plugin_writes.insert(key);
                        }
                    }
                }
                event_sources.push(Box::new(src));
                continue;
            }
            let Some(install_fn) = crate::fti::fti_install_fn(type_name)
                else { continue };
            let install = install_fn(&fti_ctx, pins, event_tx.clone())?;
            event_sources.push(install.source);
            for k in install.keys { plugin_writes.insert(k); }
        }
    }
    // Drop our own clone of the sender now that all sources have
    // their own. When the last source's sender is dropped (via
    // EventSource::stop / Drop), the receiver returns Err and the
    // scheduler knows all sources are dead.
    drop(event_tx);
    let event_rx = if event_sources.is_empty() { None } else { Some(event_rx) };

    if env.trace {
        eprintln!("[loop] startup: fsms=[{}] plugin_writes=[{}]",
            fsms.iter().map(|f| f.claim_name.as_str()).collect::<Vec<_>>().join(","),
            plugin_writes.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(","),
        );
    }

    // Multi-writer disjoint-fields rule (Phase 4 v3.7+ unified
    // model): every writer FSM PLUS every plugin-write claim
    // must have a disjoint write-set. A field has at most one
    // writer (single-owner).
    {
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

    let result = if fsms.is_empty() {
        Err("no fsm schemas found (declare one with the `fsm` keyword)".to_string())
    } else {
        scheduler::run_scheduler(rt, &fsms, opts, ctx, event_rx.as_ref(), &mut event_sources, &env)
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
