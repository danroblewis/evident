//! Async event sources for the multi-FSM scheduler. See
//! `docs/design/fsm-subscriptions.md` Phase 4 v3.
//!
//! An `EventSource` runs on a background thread and pushes
//! `SchedulerEvent`s into a shared `mpsc` channel. When the
//! scheduler has no FSM ready to tick, it blocks on the channel
//! (or halts if all senders have been dropped — the "all sources
//! dead" condition). Each event coarsely wakes every FSM, which
//! re-checks its subscription state on the next tick.
//!
//! Currently implemented sources:
//!   * `FrameTimer` — sends `Tick` events at a fixed interval
//!     until stopped.
//!
//! Adding a new source: implement `EventSource` (start spawns a
//! thread that pushes events; stop signals the thread and joins).
//! Wire it up in `effect_loop::run_with_ctx` based on whatever
//! configuration mechanism is appropriate (env var, CLI flag,
//! evident-program declaration).

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::thread::JoinHandle;
use std::time::Duration;

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
}

/// Periodic-tick event source. Spawns a thread that sleeps for the
/// configured interval and sends a `Tick` event, repeatedly, until
/// `stop` is called.
pub struct FrameTimer {
    interval:  Duration,
    name:      String,
    stop_flag: Arc<AtomicBool>,
    handle:    Option<JoinHandle<()>>,
}

impl FrameTimer {
    pub fn new(interval_ms: u64, name: impl Into<String>) -> Self {
        FrameTimer {
            interval:  Duration::from_millis(interval_ms),
            name:      name.into(),
            stop_flag: Arc::new(AtomicBool::new(false)),
            handle:    None,
        }
    }
}

impl EventSource for FrameTimer {
    fn start(&mut self, tx: Sender<SchedulerEvent>) -> Result<(), String> {
        if self.handle.is_some() {
            return Err("FrameTimer already started".to_string());
        }
        let stop = self.stop_flag.clone();
        let interval = self.interval;
        let name = self.name.clone();
        let handle = std::thread::Builder::new()
            .name(format!("evident-timer-{name}"))
            .spawn(move || {
                while !stop.load(Ordering::Relaxed) {
                    std::thread::sleep(interval);
                    if stop.load(Ordering::Relaxed) { break; }
                    // Send-error means the receiver was dropped —
                    // scheduler exited. Exit the thread cleanly.
                    if tx.send(SchedulerEvent::Tick { name: name.clone() }).is_err() {
                        break;
                    }
                }
            })
            .map_err(|e| format!("FrameTimer spawn: {e}"))?;
        self.handle = Some(handle);
        Ok(())
    }

    fn stop(&mut self) {
        self.stop_flag.store(true, Ordering::Relaxed);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

impl Drop for FrameTimer {
    fn drop(&mut self) { self.stop(); }
}

/// SIGINT (Ctrl-C) event source. Installs a signal-hook iterator
/// for SIGINT; on each signal, sends a `Tick { name: "signal" }`
/// event. After the first signal, the source's thread exits and
/// drops its sender — the scheduler sees no more events from this
/// source. (Two consecutive Ctrl-Cs while the runtime is mid-tick
/// will not double-fire; signal-hook deduplicates per iterator
/// `forever()` step.)
///
/// Naming the event "signal" lets FSMs subscribe via `_ ∈ Signal`
/// in their parameter list (see `stdlib/runtime.ev`).
pub struct SigintSource {
    name:      String,
    handle:    Option<JoinHandle<()>>,
    sig_handle: Option<signal_hook::iterator::Handle>,
    stop_flag: Arc<AtomicBool>,
}

impl SigintSource {
    pub fn new() -> Self {
        SigintSource {
            name: "signal".to_string(),
            handle:     None,
            sig_handle: None,
            stop_flag:  Arc::new(AtomicBool::new(false)),
        }
    }
}

impl Default for SigintSource {
    fn default() -> Self { Self::new() }
}

impl EventSource for SigintSource {
    fn start(&mut self, tx: Sender<SchedulerEvent>) -> Result<(), String> {
        if self.handle.is_some() {
            return Err("SigintSource already started".to_string());
        }
        // signal-hook's `Signals` iterator wraps a sigaction handler
        // that writes to a self-pipe. Iteration blocks until a
        // signal arrives. The thread exits cleanly when the
        // iterator is dropped (which happens on stop).
        use signal_hook::iterator::Signals;
        let mut signals = Signals::new([signal_hook::consts::SIGINT])
            .map_err(|e| format!("install SIGINT handler: {e}"))?;
        let sig_handle = signals.handle();
        let stop = self.stop_flag.clone();
        let name = self.name.clone();
        let handle = std::thread::Builder::new()
            .name("evident-signal".into())
            .spawn(move || {
                for _sig in signals.forever() {
                    if stop.load(Ordering::Relaxed) { break; }
                    if tx.send(SchedulerEvent::Tick { name: name.clone() }).is_err() {
                        break;
                    }
                }
            })
            .map_err(|e| format!("SigintSource spawn: {e}"))?;
        self.sig_handle = Some(sig_handle);
        self.handle     = Some(handle);
        Ok(())
    }

    fn stop(&mut self) {
        self.stop_flag.store(true, Ordering::Relaxed);
        // Closing the signal-hook handle wakes the iterator's
        // forever() loop so the thread can exit.
        if let Some(h) = self.sig_handle.take() {
            h.close();
        }
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

impl Drop for SigintSource {
    fn drop(&mut self) { self.stop(); }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::Instant;

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
