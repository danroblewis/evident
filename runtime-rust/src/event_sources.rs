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
