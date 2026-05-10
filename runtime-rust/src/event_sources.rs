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
//! Currently implemented sources:
//!   * `FrameTimer` — periodic ticks; writes a count field if
//!     the user's World declares one.
//!   * `SigintSource` — SIGINT handler; writes a counter field
//!     when triggered.
//!   * `StdinSource` — background line reader; writes each line
//!     to a String field.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::thread::JoinHandle;
use std::time::Duration;

use crate::Value;

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

/// Periodic-tick event source. Spawns a thread that sleeps for the
/// configured interval and sends a `Tick` event, repeatedly, until
/// `stop` is called. If a `count_field` is configured, also queues
/// a world write incrementing the named Int field on each fire —
/// this lets user FSMs subscribe via `world.<count_field>` deltas
/// rather than the marker-type wake mechanism.
pub struct FrameTimer {
    interval:    Duration,
    name:        String,
    count_field: Option<String>,
    write_queue: WriteQueue,
    stop_flag:   Arc<AtomicBool>,
    handle:      Option<JoinHandle<()>>,
}

impl FrameTimer {
    pub fn new(interval_ms: u64, name: impl Into<String>) -> Self {
        FrameTimer {
            interval:    Duration::from_millis(interval_ms),
            name:        name.into(),
            count_field: None,
            write_queue: new_write_queue(),
            stop_flag:   Arc::new(AtomicBool::new(false)),
            handle:      None,
        }
    }

    /// Configure the timer to write its current tick count into
    /// the named world field on each fire. The field must be of
    /// type Int in the user's World type.
    pub fn with_count_field(mut self, field: impl Into<String>) -> Self {
        self.count_field = Some(field.into());
        self
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
        let count_field = self.count_field.clone();
        let write_queue = self.write_queue.clone();
        let handle = std::thread::Builder::new()
            .name(format!("evident-timer-{name}"))
            .spawn(move || {
                let mut count: i64 = 0;
                while !stop.load(Ordering::Relaxed) {
                    std::thread::sleep(interval);
                    if stop.load(Ordering::Relaxed) { break; }
                    count += 1;
                    if let Some(field) = &count_field {
                        let mut q = write_queue.lock().unwrap();
                        q.push_back((field.clone(), Value::Int(count)));
                    }
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

    fn drain_writes(&mut self) -> Vec<(String, Value)> {
        drain(&self.write_queue)
    }

    fn write_fields(&self) -> Vec<String> {
        self.count_field.iter().cloned().collect()
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
    name:        String,
    handle:      Option<JoinHandle<()>>,
    sig_handle:  Option<signal_hook::iterator::Handle>,
    stop_flag:   Arc<AtomicBool>,
    count_field: Option<String>,
    write_queue: WriteQueue,
}

impl SigintSource {
    pub fn new() -> Self {
        SigintSource {
            name:        "signal".to_string(),
            handle:      None,
            sig_handle:  None,
            stop_flag:   Arc::new(AtomicBool::new(false)),
            count_field: None,
            write_queue: new_write_queue(),
        }
    }

    /// Configure to write the SIGINT count into the named world
    /// field (Int) on each fire. User FSMs subscribe via
    /// `world.<count_field>` deltas.
    pub fn with_count_field(mut self, field: impl Into<String>) -> Self {
        self.count_field = Some(field.into());
        self
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
        let count_field = self.count_field.clone();
        let write_queue = self.write_queue.clone();
        let handle = std::thread::Builder::new()
            .name("evident-signal".into())
            .spawn(move || {
                let mut count: i64 = 0;
                for _sig in signals.forever() {
                    if stop.load(Ordering::Relaxed) { break; }
                    count += 1;
                    if let Some(field) = &count_field {
                        let mut q = write_queue.lock().unwrap();
                        q.push_back((field.clone(), Value::Int(count)));
                    }
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

    fn drain_writes(&mut self) -> Vec<(String, Value)> {
        drain(&self.write_queue)
    }

    fn write_fields(&self) -> Vec<String> {
        self.count_field.iter().cloned().collect()
    }
}

impl Drop for SigintSource {
    fn drop(&mut self) { self.stop(); }
}

/// Stdin line reader. Spawns a thread that does blocking
/// `read_line` on stdin; each line is queued as TWO world writes:
///   * `(line_field, Str(line))` — the line text
///   * `(seq_field, Int(seq))`   — incrementing counter (1, 2, 3, …)
/// User FSMs can compare the seq against a value held in their
/// own state to decide "is this a new line I haven't processed?"
/// Without the seq, an FSM whose body emits unconditionally on
/// non-empty `line` would loop forever via effect-feedback after
/// EOF.
pub struct StdinSource {
    name:        String,
    line_field:  String,
    seq_field:   Option<String>,
    write_queue: WriteQueue,
    handle:      Option<JoinHandle<()>>,
}

impl StdinSource {
    /// `line_field` is the world field name to write each received
    /// line into. Must be a String field in the user's World type.
    pub fn new(line_field: impl Into<String>) -> Self {
        StdinSource {
            name:        "stdin".to_string(),
            line_field:  line_field.into(),
            seq_field:   None,
            write_queue: new_write_queue(),
            handle:      None,
        }
    }

    /// Configure to also write an incrementing sequence number
    /// into the named Int field on each line.
    pub fn with_seq_field(mut self, field: impl Into<String>) -> Self {
        self.seq_field = Some(field.into());
        self
    }
}

impl EventSource for StdinSource {
    fn start(&mut self, tx: Sender<SchedulerEvent>) -> Result<(), String> {
        if self.handle.is_some() {
            return Err("StdinSource already started".to_string());
        }
        let name = self.name.clone();
        let line_field = self.line_field.clone();
        let seq_field = self.seq_field.clone();
        let write_queue = self.write_queue.clone();
        let handle = std::thread::Builder::new()
            .name("evident-stdin".into())
            .spawn(move || {
                use std::io::BufRead;
                let stdin = std::io::stdin();
                let mut reader = stdin.lock();
                let mut seq: i64 = 0;
                loop {
                    let mut line = String::new();
                    match reader.read_line(&mut line) {
                        Ok(0) => break,  // EOF
                        Ok(_) => {
                            // Strip trailing newline(s).
                            if line.ends_with('\n') { line.pop(); }
                            if line.ends_with('\r') { line.pop(); }
                            seq += 1;
                            {
                                let mut q = write_queue.lock().unwrap();
                                q.push_back((line_field.clone(), Value::Str(line)));
                                if let Some(sf) = &seq_field {
                                    q.push_back((sf.clone(), Value::Int(seq)));
                                }
                            }
                            if tx.send(SchedulerEvent::Tick { name: name.clone() }).is_err() {
                                break;
                            }
                        }
                        Err(_) => break,
                    }
                }
                // EOF / error → close the channel by dropping tx.
            })
            .map_err(|e| format!("StdinSource spawn: {e}"))?;
        self.handle = Some(handle);
        Ok(())
    }

    fn stop(&mut self) {
        // Stdin's blocking read can't be interrupted portably from
        // another thread. We can't join here without potentially
        // hanging — drop the JoinHandle (the thread will exit on
        // its own when EOF arrives or when the channel closes).
        // The OS reaps the thread at process exit.
        let _ = self.handle.take();
    }

    fn drain_writes(&mut self) -> Vec<(String, Value)> {
        drain(&self.write_queue)
    }

    fn write_fields(&self) -> Vec<String> {
        let mut v = vec![self.line_field.clone()];
        if let Some(s) = &self.seq_field {
            v.push(s.clone());
        }
        v
    }
}

impl Drop for StdinSource {
    fn drop(&mut self) { self.stop(); }
}

/// File line reader. Like StdinSource but reads from a path
/// instead of fd 0. Spawns a thread that opens the file at
/// startup, streams lines until EOF, queues each line as a
/// world write. EOF closes the channel sender (signalling
/// "source dead" to the scheduler).
///
/// This is the smallest concrete step toward Foreign Type
/// Interface — it demonstrates resource lifecycle (open at
/// start, close at EOF/drop) for a non-stdin file descriptor.
/// Configuration is via constructor parameter (the path); a
/// future FTI version would let users declare per-instance
/// resources via type-with-fields.
pub struct FileLineReader {
    name:        String,
    path:        std::path::PathBuf,
    line_field:  String,
    seq_field:   Option<String>,
    eof_field:   Option<String>,
    write_queue: WriteQueue,
    handle:      Option<JoinHandle<()>>,
}

impl FileLineReader {
    pub fn new(path: impl Into<std::path::PathBuf>,
               line_field: impl Into<String>) -> Self {
        FileLineReader {
            name:        "file".to_string(),
            path:        path.into(),
            line_field:  line_field.into(),
            seq_field:   None,
            eof_field:   None,
            write_queue: new_write_queue(),
            handle:      None,
        }
    }

    pub fn with_seq_field(mut self, field: impl Into<String>) -> Self {
        self.seq_field = Some(field.into()); self
    }

    pub fn with_eof_field(mut self, field: impl Into<String>) -> Self {
        self.eof_field = Some(field.into()); self
    }
}

impl EventSource for FileLineReader {
    fn start(&mut self, tx: Sender<SchedulerEvent>) -> Result<(), String> {
        if self.handle.is_some() {
            return Err("FileLineReader already started".to_string());
        }
        let name = self.name.clone();
        let path = self.path.clone();
        let line_field = self.line_field.clone();
        let seq_field = self.seq_field.clone();
        let eof_field = self.eof_field.clone();
        let write_queue = self.write_queue.clone();
        let handle = std::thread::Builder::new()
            .name("evident-file".into())
            .spawn(move || {
                use std::io::{BufRead, BufReader};
                let f = match std::fs::File::open(&path) {
                    Ok(f) => f,
                    Err(_) => {
                        // Path not openable — write eof=true and exit.
                        if let Some(ef) = &eof_field {
                            let mut q = write_queue.lock().unwrap();
                            q.push_back((ef.clone(), Value::Bool(true)));
                        }
                        let _ = tx.send(SchedulerEvent::Tick { name: name.clone() });
                        return;
                    }
                };
                let mut reader = BufReader::new(f);
                let mut seq: i64 = 0;
                loop {
                    let mut line = String::new();
                    match reader.read_line(&mut line) {
                        Ok(0) => break,  // EOF
                        Ok(_) => {
                            if line.ends_with('\n') { line.pop(); }
                            if line.ends_with('\r') { line.pop(); }
                            seq += 1;
                            {
                                let mut q = write_queue.lock().unwrap();
                                q.push_back((line_field.clone(), Value::Str(line)));
                                if let Some(sf) = &seq_field {
                                    q.push_back((sf.clone(), Value::Int(seq)));
                                }
                            }
                            if tx.send(SchedulerEvent::Tick { name: name.clone() }).is_err() {
                                return;
                            }
                        }
                        Err(_) => break,
                    }
                }
                // EOF — set eof flag and send a final wake.
                if let Some(ef) = &eof_field {
                    let mut q = write_queue.lock().unwrap();
                    q.push_back((ef.clone(), Value::Bool(true)));
                }
                let _ = tx.send(SchedulerEvent::Tick { name: name.clone() });
                // Sender drops here when thread exits → channel closed.
            })
            .map_err(|e| format!("FileLineReader spawn: {e}"))?;
        self.handle = Some(handle);
        Ok(())
    }

    fn stop(&mut self) {
        // Like StdinSource: blocking read can't be portably interrupted.
        // Drop the JoinHandle and let the thread finish on its own
        // (it'll exit on EOF or when the sender drops).
        let _ = self.handle.take();
    }

    fn drain_writes(&mut self) -> Vec<(String, Value)> {
        drain(&self.write_queue)
    }

    fn write_fields(&self) -> Vec<String> {
        let mut v = vec![self.line_field.clone()];
        if let Some(s) = &self.seq_field { v.push(s.clone()); }
        if let Some(e) = &self.eof_field { v.push(e.clone()); }
        v
    }
}

impl Drop for FileLineReader {
    fn drop(&mut self) { self.stop(); }
}

/// Wall-clock time source. Spawns a thread that updates the
/// configured world field with current Unix time (ms) at the
/// configured interval. Default interval is 100ms; configurable
/// via constructor. The first write is immediate (don't wait
/// the interval before exposing the initial time).
///
/// Auto-install when World declares `now_ms: Int`. Useful for
/// programs that want to read "what time is it now" without
/// emitting Effect::Time on every tick.
pub struct WallClockSource {
    interval:    Duration,
    field:       String,
    write_queue: WriteQueue,
    stop_flag:   Arc<AtomicBool>,
    handle:      Option<JoinHandle<()>>,
}

impl WallClockSource {
    pub fn new(interval_ms: u64, field: impl Into<String>) -> Self {
        WallClockSource {
            interval:    Duration::from_millis(interval_ms),
            field:       field.into(),
            write_queue: new_write_queue(),
            stop_flag:   Arc::new(AtomicBool::new(false)),
            handle:      None,
        }
    }
}

impl EventSource for WallClockSource {
    fn start(&mut self, tx: Sender<SchedulerEvent>) -> Result<(), String> {
        if self.handle.is_some() {
            return Err("WallClockSource already started".to_string());
        }
        let stop = self.stop_flag.clone();
        let interval = self.interval;
        let field = self.field.clone();
        let write_queue = self.write_queue.clone();
        let handle = std::thread::Builder::new()
            .name("evident-clock".into())
            .spawn(move || {
                let now = || -> i64 {
                    use std::time::{SystemTime, UNIX_EPOCH};
                    SystemTime::now().duration_since(UNIX_EPOCH)
                        .map(|d| d.as_millis() as i64).unwrap_or(0)
                };
                // Initial write — make the time visible without
                // waiting the first interval.
                {
                    let mut q = write_queue.lock().unwrap();
                    q.push_back((field.clone(), Value::Int(now())));
                }
                let _ = tx.send(SchedulerEvent::Tick { name: "clock".to_string() });

                while !stop.load(Ordering::Relaxed) {
                    std::thread::sleep(interval);
                    if stop.load(Ordering::Relaxed) { break; }
                    {
                        let mut q = write_queue.lock().unwrap();
                        q.push_back((field.clone(), Value::Int(now())));
                    }
                    if tx.send(SchedulerEvent::Tick { name: "clock".to_string() }).is_err() {
                        break;
                    }
                }
            })
            .map_err(|e| format!("WallClockSource spawn: {e}"))?;
        self.handle = Some(handle);
        Ok(())
    }

    fn stop(&mut self) {
        self.stop_flag.store(true, Ordering::Relaxed);
        if let Some(h) = self.handle.take() { let _ = h.join(); }
    }

    fn drain_writes(&mut self) -> Vec<(String, Value)> {
        drain(&self.write_queue)
    }

    fn write_fields(&self) -> Vec<String> {
        vec![self.field.clone()]
    }
}

impl Drop for WallClockSource {
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
