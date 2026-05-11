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

use std::collections::VecDeque;
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

pub use frame_timer::FrameTimer;
pub use sigint::SigintSource;
pub use stdin::StdinSource;
pub use file_line_reader::FileLineReader;
pub use wall_clock::WallClockSource;
pub use file_watcher::FileWatcherSource;
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

#[cfg(test)]
mod tests {
    use super::*;
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
