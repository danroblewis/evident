//! Async event sources for the multi-FSM scheduler. See
//! `docs/design/schema-interface.md` for the unified model:
//! plugins are first-class schemas that write world fields. The
//! older event-channel mechanism (Phase 4 v3) is kept as a wake
//! channel; the new world-write capability is added on top so
//! plugins behave like writer FSMs.
//!
//! Two-channel design (transitional):
//!   * **Wake channel** (`Sender<SchedulerEvent>`): one-bit "data
//!     arrived" notifications. Used to unblock the scheduler when
//!     no FSM is otherwise ready.
//!   * **Write queue** (`drain_writes()`): per-source queue of
//!     pending world-field writes. Drained at start of each tick
//!     and applied through the same code path as writer-FSM
//!     output (multi-writer disjoint-fields rule applies).
//!
//! v1 sources implement both: wake to unblock, writes to publish
//! data. Future cleanup may collapse the two — every wake is
//! "world changed," nothing else.
//!
//! This file owns the shared abstraction (trait + queue + event
//! enum + helpers); each bridge lives in its own sibling file
//! under `event_sources/<name>.rs` per the per-bridge invariant
//! in `lints/runtime-invariants.md`.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::sync::mpsc::Sender;

use crate::Value;

// ── Per-bridge submodules ────────────────────────────────────
//
// Each file owns the lifecycle of one typed C resource. The
// scheduler interacts with bridges only through the trait
// surface defined below — see lints/runtime-invariants.md for
// the per-bridge file invariant.
mod frame_timer;
mod sigint;
mod stdin;
mod file_line_reader;
mod wall_clock;
mod file_watcher;
mod oneshot_shell;
mod sdl_window;
mod gl_program;
// `gl_context` is NOT a bridge (no struct, no EventSource impl);
// it's a sibling helper for GL-aware bridges so the
// `OpenGL.framework` dlopen doesn't get duplicated across files.
// Keeping it under `event_sources/` (the only role permitted to
// touch library specifics) per AP-001's scope clause.
mod gl_context;

// Bridges referenced by name elsewhere in the runtime (FTI
// registry in `fti.rs` builds them directly): export those.
// The other world-plugin-only bridges live behind the
// WORLD_PLUGIN_INSTALLERS registry and don't need a public
// re-export.
pub use frame_timer::FrameTimer;
pub use oneshot_shell::OneShotShellSource;
pub use sdl_window::SdlWindowSource;
pub use gl_program::GlProgramSource;

// ── Shared abstraction layer ─────────────────────────────────

/// One event delivered to the scheduler. Sources tag their events
/// with a name (currently informational; a future "subscribed-to
/// source X" mechanism can match on it).
#[derive(Debug, Clone)]
pub enum SchedulerEvent {
    /// A periodic tick (e.g. frame timer). `name` identifies the
    /// source ("frame", "1s", etc.).
    Tick { name: String },
    /// Source signaled it's permanently done (e.g. EOF). Currently
    /// dropping the sender works as well — left here for future
    /// "explicit done" plumbing.
    Closed { name: String },
}

/// Anything that can produce `SchedulerEvent`s asynchronously.
pub trait EventSource: Send {
    /// Start the source's background work. After this returns, the
    /// source begins pushing events into its sender.
    fn start(&mut self, tx: Sender<SchedulerEvent>) -> Result<(), String>;
    /// Signal the source's background thread to stop and wait for
    /// it to terminate. After this returns, no more events will
    /// be sent (the source's clone of the sender is dropped).
    fn stop(&mut self);
    /// Drain any pending world-field writes the source has
    /// queued. Each entry is `(field_name, new_value)`. Returns
    /// empty Vec by default (sources that only push wake events
    /// without producing data have no writes). Called by the
    /// runtime at the start of each tick; drained writes are
    /// applied to the world snapshot through the same pathway as
    /// writer-FSM output.
    fn drain_writes(&mut self) -> Vec<(String, Value)> { Vec::new() }
    /// World fields this source declares ownership of (its
    /// write-set, in writer-FSM terms). Used at load time to
    /// participate in the disjoint-write-set check. Returns empty
    /// Vec by default (sources that don't write).
    fn write_fields(&self) -> Vec<String> { Vec::new() }
}

/// A queue of pending writes, shared between a source's
/// background thread (writer) and the scheduler (drainer). Cheap
/// to clone the Arc for sender threads.
pub type WriteQueue = Arc<Mutex<VecDeque<(String, Value)>>>;

pub fn new_write_queue() -> WriteQueue {
    Arc::new(Mutex::new(VecDeque::new()))
}

pub fn drain(q: &WriteQueue) -> Vec<(String, Value)> {
    let mut g = q.lock().unwrap();
    g.drain(..).collect()
}

// ── World-plugin install registry ────────────────────────────
//
// World-plugin bridges are installed by walking a `&'static [...]`
// table of (owned_world_fields, install_fn) entries. Each entry
// declares which world-field names it owns; the scheduler iterates
// the table once at startup and asks each installer whether it
// wants to install given the current world type's fields and a
// few env knobs.
//
// This keeps the scheduler unaware of which specific bridges
// exist. Adding a new world-field-driven bridge means: implement
// the EventSource, write an `install_world_plugin` fn, and append
// one row to `WORLD_PLUGIN_INSTALLERS`. The scheduler does not
// need editing.
//
// The FTI registry (`crate::fti::INSTALLERS`) is the analogous
// shape for typed-resource bridges declared as FSM parameters.
// World plugins and FTI live in separate registries because they
// serve different declaration sites in the user's Evident program.

/// Read-only view of the runtime state a world-plugin installer
/// needs to decide whether (and how) to install. Threaded through
/// once at scheduler startup; installers query the fields and
/// return Some(install) iff the world declares the trigger fields.
pub struct WorldPluginCtx<'a> {
    /// `field_name → type_name` for the user's World type. Empty
    /// if no FSM declares a `world ∈ World`.
    pub world_fields: &'a HashMap<String, String>,
    /// Set of event-subscription names declared across all FSMs
    /// (e.g. "tick" for `_ ∈ FrameTimer`, "signal" for `_ ∈ Signal`).
    /// Used by FrameTimer / Sigint installers as an opt-in path
    /// independent of world fields.
    pub fsm_event_subscriptions: &'a std::collections::HashSet<String>,
    /// `EVIDENT_TICK_MS` snapshot. FrameTimer treats Some(_) as an
    /// explicit opt-in even without a `tick_count` world field.
    pub env_tick_ms: Option<u64>,
    /// `EVIDENT_CLOCK_MS` snapshot (default 100). WallClock uses it.
    pub env_clock_ms: u64,
    /// `EVIDENT_FILE_WATCH` snapshot. FileWatcher only installs
    /// when this is set AND the world declares `file_changed`.
    pub env_file_watch: Option<&'a str>,
    /// `EVIDENT_FILE_WATCH_MS` snapshot (default 200).
    pub env_file_watch_ms: u64,
    /// `EVIDENT_FILE_INPUT` snapshot. FileLineReader only installs
    /// when this is set AND the world declares `file_line`.
    pub env_file_input: Option<&'a str>,
    /// Closure returning the name of the first FSM whose body
    /// references `ident` (e.g. an effect constructor name like
    /// "ReadLine"). Used by the StdinSource installer to detect
    /// the auto-install-vs-Effect::ReadLine race; returns None if
    /// no FSM references the identifier.
    pub fsm_using_identifier: &'a dyn Fn(&str) -> Option<String>,
    /// Closure returning the user-side `Program` AST encoded as a
    /// `Value::Enum` tree matching `stdlib/ast.ev`'s `Program`
    /// shape. Reflection plugin (and any future plugin that needs
    /// to expose the loaded program declaratively) calls this at
    /// install time. Returns `Err` if `stdlib/ast.ev` isn't
    /// loaded — the registry is empty or the `Program` enum is
    /// missing. Other plugins ignore this field.
    pub encode_program: &'a dyn Fn() -> Result<Value, String>,
}

impl<'a> WorldPluginCtx<'a> {
    /// True iff the user's World type declares a field named
    /// `name` of the given Evident `ty`.
    pub fn has_world_field(&self, name: &str, ty: &str) -> bool {
        self.world_fields.get(name).map(|t| t == ty).unwrap_or(false)
    }
}

/// What an installer returns when it decides to install. Carries
/// the started bridge plus the world-field names the bridge now
/// writes (added to the multi-writer disjoint-set check) and any
/// scheduler-level side effects the install requires (e.g. the
/// stdin bridge taking ownership of fd 0).
pub struct WorldPluginInstall {
    /// The started EventSource. Already had `start(tx)` called
    /// successfully — the scheduler just stores it.
    pub source: Box<dyn EventSource>,
    /// World fields the bridge writes. Added to `plugin_writes`
    /// so the disjoint-write-set check rejects user FSMs that
    /// would clobber these fields.
    pub plugin_writes: Vec<String>,
    /// True iff the bridge takes exclusive ownership of stdin
    /// (fd 0). The scheduler propagates this to the dispatch
    /// context so `Effect::ReadLine` errors out instead of
    /// racing with the bridge for bytes.
    pub owns_stdin: bool,
}

/// Signature for a world-plugin install function. Returns
/// `Ok(Some(install))` when the bridge wants to start (the user
/// declared the trigger fields / env opt-in), `Ok(None)` when it
/// declines (no trigger), or `Err` if the bridge tried to start
/// but failed (the scheduler bubbles this up as a load error).
pub type WorldPluginInstallFn = fn(
    ctx: &WorldPluginCtx,
    event_tx: &Sender<SchedulerEvent>,
) -> Result<Option<WorldPluginInstall>, String>;

/// The world-plugin registry. The scheduler iterates this slice
/// once at startup and calls each install fn; each one decides
/// independently whether to install.
///
/// **Order is preserved** (slice, not HashMap). FrameTimer first
/// to mirror the original auto-install order from when these were
/// hardcoded `if has_field(...)` blocks in the scheduler.
pub const WORLD_PLUGIN_INSTALLERS: &[WorldPluginInstallFn] = &[
    frame_timer::install_world_plugin,
    sigint::install_world_plugin,
    stdin::install_world_plugin,
    wall_clock::install_world_plugin,
    file_watcher::install_world_plugin,
    file_line_reader::install_world_plugin,
];

#[cfg(test)]
mod tests {
    use super::*;
    use super::sigint::SigintSource;
    use std::sync::mpsc;
    use std::time::{Duration, Instant};

    #[test]
    fn frame_timer_fires_at_interval() {
        let (tx, rx) = mpsc::channel();
        let mut t = FrameTimer::new(20, "test");
        t.start(tx).unwrap();
        let start = Instant::now();
        // Collect 3 ticks; should take ~60ms (3 × 20ms).
        for _ in 0..3 {
            let _ = rx.recv_timeout(Duration::from_millis(500))
                .expect("tick should arrive within 500ms");
        }
        let elapsed = start.elapsed();
        t.stop();
        // Allow generous slack — sleep precision varies.
        assert!(elapsed >= Duration::from_millis(60),
            "3 × 20ms ticks should take ≥ 60ms, got {elapsed:?}");
        assert!(elapsed < Duration::from_millis(500),
            "3 × 20ms ticks should be well under 500ms, got {elapsed:?}");
    }

    #[test]
    fn sigint_source_starts_and_stops_without_signal() {
        // Just verify lifecycle: install handler, no signal sent,
        // stop cleanly. Sending an actual SIGINT inside `cargo
        // test` would interrupt the test runner — verify the
        // installation path here and rely on integration with the
        // scheduler for end-to-end behavior.
        let (tx, rx) = mpsc::channel();
        let mut s = SigintSource::new();
        s.start(tx).unwrap();
        // No signal — recv should time out.
        assert!(rx.recv_timeout(Duration::from_millis(50)).is_err(),
            "no event should arrive without a signal");
        s.stop();
        // After stop, sender should be dropped → channel closed.
        assert!(rx.recv().is_err(), "channel should be closed after stop");
    }

    #[test]
    fn frame_timer_stops_cleanly() {
        let (tx, rx) = mpsc::channel();
        let mut t = FrameTimer::new(10, "test");
        t.start(tx).unwrap();
        std::thread::sleep(Duration::from_millis(35));
        t.stop();
        // Drain anything in flight, then expect the channel to go
        // quiet within a sensible window.
        while rx.recv_timeout(Duration::from_millis(50)).is_ok() {}
        // After stop, no more ticks within 100ms.
        assert!(rx.recv_timeout(Duration::from_millis(100)).is_err(),
            "no tick should arrive after stop");
    }
}
