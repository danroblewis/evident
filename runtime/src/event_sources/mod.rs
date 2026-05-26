//! Async event sources for the multi-FSM scheduler.
//! Wake channel (one-bit notify) + write queue (world-field writes drained each tick).

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::sync::mpsc::Sender;

use crate::Value;

mod frame_timer;
mod sigint;
mod stdin;
mod file_line_reader;
mod wall_clock;
mod file_watcher;
mod reflection;
mod declarative_install;

// FTI registry in `fti.rs` references these directly; others are registry-only.
pub use frame_timer::FrameTimer;
pub use declarative_install::DeclarativeInstallSource;

/// One event delivered to the scheduler.
#[derive(Debug, Clone)]
pub enum SchedulerEvent {
    /// Periodic tick; `name` identifies the source.
    Tick { name: String },
    /// Source permanently done (e.g. EOF). Dropping sender also works.
    Closed { name: String },
}

/// Anything that can produce `SchedulerEvent`s asynchronously.
pub trait EventSource: Send {
    /// Start background work; source begins pushing events into `tx`.
    fn start(&mut self, tx: Sender<SchedulerEvent>) -> Result<(), String>;
    /// Stop and join; no more events after this returns.
    fn stop(&mut self);
    /// Drain pending `(field, value)` world writes (called each tick).
    fn drain_writes(&mut self) -> Vec<(String, Value)> { Vec::new() }
    /// World fields this source owns (disjoint-write-set check at load).
    fn write_fields(&self) -> Vec<String> { Vec::new() }
}

/// Shared write queue between a source's background thread and the scheduler.
pub type WriteQueue = Arc<Mutex<VecDeque<(String, Value)>>>;

pub fn new_write_queue() -> WriteQueue {
    Arc::new(Mutex::new(VecDeque::new()))
}

pub fn drain(q: &WriteQueue) -> Vec<(String, Value)> {
    let mut g = q.lock().unwrap();
    g.drain(..).collect()
}

// ── World-plugin install registry ────────────────────────────
// Add a bridge: implement EventSource + install_world_plugin + append to WORLD_PLUGIN_INSTALLERS.

/// Read-only context a world-plugin installer inspects to decide whether to install.
pub struct WorldPluginCtx<'a> {
    /// `field_name → type_name` for the user's World type.
    pub world_fields: &'a HashMap<String, String>,
    /// Event-subscription names across all FSMs (e.g. "tick", "signal").
    pub fsm_event_subscriptions: &'a std::collections::HashSet<String>,
    /// `EVIDENT_TICK_MS` snapshot; `Some(_)` opts FrameTimer in even without `tick_count`.
    pub env_tick_ms: Option<u64>,
    /// `EVIDENT_CLOCK_MS` snapshot (default 100ms).
    pub env_clock_ms: u64,
    /// `EVIDENT_FILE_WATCH` snapshot; FileWatcher installs only when set + world has `file_changed`.
    pub env_file_watch: Option<&'a str>,
    /// `EVIDENT_FILE_WATCH_MS` snapshot (default 200ms).
    pub env_file_watch_ms: u64,
    /// `EVIDENT_FILE_INPUT` snapshot; FileLineReader installs only when set + world has `file_line`.
    pub env_file_input: Option<&'a str>,
    /// Returns the FSM name that references `ident`; used by StdinSource to detect ReadLine conflict.
    pub fsm_using_identifier: &'a dyn Fn(&str) -> Option<String>,
    /// Encodes the loaded Program AST as a Value tree (matches `stdlib/ast.ev`).
    /// Returns Err if `stdlib/ast.ev` isn't loaded.
    pub encode_program: &'a dyn Fn() -> Result<Value, String>,
}

impl<'a> WorldPluginCtx<'a> {
    /// True iff World declares field `name` of type `ty`.
    pub fn has_world_field(&self, name: &str, ty: &str) -> bool {
        self.world_fields.get(name).map(|t| t == ty).unwrap_or(false)
    }
}

/// What an installer returns on success: the started bridge, its owned world
/// fields (disjoint-write check), and whether it took exclusive stdin ownership.
pub struct WorldPluginInstall {
    pub source:        Box<dyn EventSource>,
    pub plugin_writes: Vec<String>,
    /// True if the bridge owns fd 0; `Effect::ReadLine` will error rather than race.
    pub owns_stdin: bool,
}

/// `Ok(Some)` = install, `Ok(None)` = decline, `Err` = fatal startup failure.
pub type WorldPluginInstallFn = fn(
    ctx: &WorldPluginCtx,
    event_tx: &Sender<SchedulerEvent>,
) -> Result<Option<WorldPluginInstall>, String>;

/// Ordered registry; scheduler calls each at startup. FrameTimer first for historical order.
pub const WORLD_PLUGIN_INSTALLERS: &[WorldPluginInstallFn] = &[
    frame_timer::install_world_plugin,
    sigint::install_world_plugin,
    stdin::install_world_plugin,
    wall_clock::install_world_plugin,
    file_watcher::install_world_plugin,
    file_line_reader::install_world_plugin,
    reflection::install_world_plugin,
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
        for _ in 0..3 {
            let _ = rx.recv_timeout(Duration::from_millis(500))
                .expect("tick should arrive within 500ms");
        }
        let elapsed = start.elapsed();
        t.stop();
        assert!(elapsed >= Duration::from_millis(60),
            "3 × 20ms ticks should take ≥ 60ms, got {elapsed:?}");
        assert!(elapsed < Duration::from_millis(500),
            "3 × 20ms ticks should be well under 500ms, got {elapsed:?}");
    }

    #[test]
    fn sigint_source_starts_and_stops_without_signal() {
        // Sending SIGINT inside `cargo test` would interrupt the runner; verify lifecycle only.
        let (tx, rx) = mpsc::channel();
        let mut s = SigintSource::new();
        s.start(tx).unwrap();
        assert!(rx.recv_timeout(Duration::from_millis(50)).is_err(),
            "no event should arrive without a signal");
        s.stop();
        assert!(rx.recv().is_err(), "channel should be closed after stop");
    }

    #[test]
    fn frame_timer_stops_cleanly() {
        let (tx, rx) = mpsc::channel();
        let mut t = FrameTimer::new(10, "test");
        t.start(tx).unwrap();
        std::thread::sleep(Duration::from_millis(35));
        t.stop();
        while rx.recv_timeout(Duration::from_millis(50)).is_ok() {}
        assert!(rx.recv_timeout(Duration::from_millis(100)).is_err(),
            "no tick should arrive after stop");
    }
}
