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

/// File modification watcher. Polls the file's mtime at the
/// configured interval; when it changes, increments the
/// configured counter field. Subscribers see the delta and
/// react. The path is set via the constructor; if the file
/// doesn't exist, the source still polls (it'll fire when the
/// file appears).
///
/// This is the simplest "external state changes" plugin —
/// useful for watching config files, build outputs, etc. More
/// efficient kernel-level mechanisms (inotify on Linux,
/// FSEvents on macOS, kqueue on BSD) are deferred.
pub struct FileWatcherSource {
    interval:    Duration,
    path:        std::path::PathBuf,
    field:       String,
    write_queue: WriteQueue,
    stop_flag:   Arc<AtomicBool>,
    handle:      Option<JoinHandle<()>>,
}

impl FileWatcherSource {
    pub fn new(path: impl Into<std::path::PathBuf>,
               interval_ms: u64,
               field: impl Into<String>) -> Self {
        FileWatcherSource {
            interval:    Duration::from_millis(interval_ms),
            path:        path.into(),
            field:       field.into(),
            write_queue: new_write_queue(),
            stop_flag:   Arc::new(AtomicBool::new(false)),
            handle:      None,
        }
    }
}

impl EventSource for FileWatcherSource {
    fn start(&mut self, tx: Sender<SchedulerEvent>) -> Result<(), String> {
        if self.handle.is_some() {
            return Err("FileWatcherSource already started".to_string());
        }
        let stop = self.stop_flag.clone();
        let interval = self.interval;
        let path = self.path.clone();
        let field = self.field.clone();
        let write_queue = self.write_queue.clone();
        let handle = std::thread::Builder::new()
            .name("evident-fwatch".into())
            .spawn(move || {
                use std::time::SystemTime;
                let mtime_of = |p: &std::path::Path| -> Option<SystemTime> {
                    std::fs::metadata(p).ok().and_then(|m| m.modified().ok())
                };
                let mut last_mtime = mtime_of(&path);
                let mut count: i64 = 0;
                while !stop.load(Ordering::Relaxed) {
                    std::thread::sleep(interval);
                    if stop.load(Ordering::Relaxed) { break; }
                    let cur = mtime_of(&path);
                    if cur != last_mtime {
                        count += 1;
                        last_mtime = cur;
                        {
                            let mut q = write_queue.lock().unwrap();
                            q.push_back((field.clone(), Value::Int(count)));
                        }
                        if tx.send(SchedulerEvent::Tick { name: "fwatch".to_string() }).is_err() {
                            break;
                        }
                    }
                }
            })
            .map_err(|e| format!("FileWatcherSource spawn: {e}"))?;
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

impl Drop for FileWatcherSource {
    fn drop(&mut self) { self.stop(); }
}

/// One-shot string source. Spawns a thread that runs the
/// configured shell command (sh -c) once, captures stdout
/// (trimmed), and writes it to the configured field. After
/// the write, the source is done — drops its sender, channel
/// closes for this source.
///
/// Used by the FTI Hostname bridge: `sh -c "hostname"` →
/// writes `<param>.name`. Generalizes to any one-shot command
/// whose output should be exposed as a typed field.
pub struct OneShotShellSource {
    cmd:         String,
    field:       String,
    write_queue: WriteQueue,
    handle:      Option<JoinHandle<()>>,
}

impl OneShotShellSource {
    pub fn new(cmd: impl Into<String>, field: impl Into<String>) -> Self {
        OneShotShellSource {
            cmd:         cmd.into(),
            field:       field.into(),
            write_queue: new_write_queue(),
            handle:      None,
        }
    }
}

impl EventSource for OneShotShellSource {
    fn start(&mut self, tx: Sender<SchedulerEvent>) -> Result<(), String> {
        if self.handle.is_some() {
            return Err("OneShotShellSource already started".to_string());
        }
        let cmd = self.cmd.clone();
        let field = self.field.clone();
        let write_queue = self.write_queue.clone();
        let handle = std::thread::Builder::new()
            .name("evident-oneshot".into())
            .spawn(move || {
                use std::process::Command;
                let result = Command::new("sh").arg("-c").arg(&cmd).output();
                let value = match result {
                    Ok(out) if out.status.success() => {
                        let mut s = String::from_utf8_lossy(&out.stdout).into_owned();
                        if s.ends_with('\n') { s.pop(); }
                        Value::Str(s)
                    }
                    Ok(_) | Err(_) => Value::Str(String::new()),  // empty on failure
                };
                {
                    let mut q = write_queue.lock().unwrap();
                    q.push_back((field.clone(), value));
                }
                let _ = tx.send(SchedulerEvent::Tick { name: "oneshot".to_string() });
                // Sender drops on thread exit → channel closed for this source.
            })
            .map_err(|e| format!("OneShotShellSource spawn: {e}"))?;
        self.handle = Some(handle);
        Ok(())
    }

    fn stop(&mut self) {
        // The thread exits after writing. Just drop the join handle.
        let _ = self.handle.take();
    }

    fn drain_writes(&mut self) -> Vec<(String, Value)> {
        drain(&self.write_queue)
    }

    fn write_fields(&self) -> Vec<String> {
        vec![self.field.clone()]
    }
}

impl Drop for OneShotShellSource {
    fn drop(&mut self) { self.stop(); }
}

/// SDL_Window resource bridge. On start: load libSDL2, call
/// SDL_Init + SDL_CreateWindow, write the window pointer (as
/// i64) to the configured `handle` field. On stop: call
/// SDL_DestroyWindow + SDL_Quit.
///
/// Lifecycle: the SDL library and window pointer are held by
/// the source; both live until the source is dropped (i.e. the
/// runtime exits or the source is explicitly stopped).
///
/// Caveat: SDL functions must be called from the same thread
/// that initialized SDL on macOS. The current implementation
/// uses a single bridge thread for both init and cleanup,
/// avoiding the cross-thread issue. User FSMs that call SDL
/// functions via Effect::LibCall (for glClear, swap, etc.)
/// will execute on the dispatch thread, which is DIFFERENT —
/// this works because OpenGL doesn't have the same single-
/// thread restriction once a context is current. Window
/// management calls (resize, fullscreen toggle) from user
/// code may behave oddly; for v1 keep window state read-only.
pub struct SdlWindowSource {
    title:           String,
    width:           i32,
    height:          i32,
    handle_field:    String,
    gl_handle_field: Option<String>,
    vao_field:       Option<String>,
    write_queue:     WriteQueue,
    stop_flag:       Arc<AtomicBool>,
    handle:          Option<JoinHandle<()>>,
}

impl SdlWindowSource {
    pub fn new(title: impl Into<String>,
               width: i32,
               height: i32,
               handle_field: impl Into<String>) -> Self {
        SdlWindowSource {
            title:           title.into(),
            width, height,
            handle_field:    handle_field.into(),
            gl_handle_field: None,
            vao_field:       None,
            write_queue:     new_write_queue(),
            stop_flag:       Arc::new(AtomicBool::new(false)),
            handle:          None,
        }
    }

    pub fn with_gl_context_field(mut self, field: impl Into<String>) -> Self {
        self.gl_handle_field = Some(field.into());
        self
    }

    pub fn with_vao_field(mut self, field: impl Into<String>) -> Self {
        self.vao_field = Some(field.into());
        self
    }

    /// Macos-friendly variant: call SDL_Init + SDL_CreateWindow
    /// SYNCHRONOUSLY on the calling thread (which should be the
    /// runtime's main thread). Pushes the resulting handle into
    /// the queue immediately. Skip the background thread; cleanup
    /// happens on Drop. Returns the window pointer (or 0 on failure)
    /// so the caller can also note it for later use.
    pub fn start_inline(&mut self, tx: Sender<SchedulerEvent>) -> Result<i64, String> {
        use libloading::{Library, Symbol};
        use std::ffi::CString;
        use std::os::raw::{c_char, c_int, c_uint, c_void};
        let paths = [
            "/opt/homebrew/lib/libSDL2.dylib",
            "/usr/local/lib/libSDL2.dylib",
            "/usr/lib/x86_64-linux-gnu/libSDL2.so",
            "/usr/lib/libSDL2.so",
        ];
        let lib = paths.iter()
            .find_map(|p| unsafe { Library::new(p) }.ok())
            .ok_or_else(|| "couldn't find libSDL2 in standard paths".to_string())?;

        type SdlInit = unsafe extern "C" fn(u32) -> c_int;
        type SdlCreateWindow = unsafe extern "C" fn(*const c_char, c_int, c_int, c_int, c_int, u32) -> *mut c_void;

        let sdl_init: Symbol<SdlInit> = unsafe { lib.get(b"SDL_Init\0") }
            .map_err(|e| format!("SDL_Init lookup: {e}"))?;
        let sdl_create_window: Symbol<SdlCreateWindow> = unsafe { lib.get(b"SDL_CreateWindow\0") }
            .map_err(|e| format!("SDL_CreateWindow lookup: {e}"))?;

        let init_rc = unsafe { sdl_init(0x20) };
        if init_rc != 0 {
            return Err(format!("SDL_Init returned {init_rc}"));
        }

        // macOS: NSApplicationLoad() bootstraps Cocoa for a
        // command-line tool. SDL's video init does this too,
        // but calling it explicitly + asking the app to be
        // "regular" (not "accessory" or "prohibited") may
        // tighten the GL drawable lifecycle that's blocking
        // dispatch-time renders. Best-effort — ignore if
        // AppKit isn't loadable.
        #[cfg(target_os = "macos")]
        {
            type NsApplicationLoad = unsafe extern "C" fn() -> bool;
            if let Ok(appkit) = unsafe {
                Library::new("/System/Library/Frameworks/AppKit.framework/AppKit")
            } {
                if let Ok(nsapp_load) = unsafe {
                    appkit.get::<Symbol<NsApplicationLoad>>(b"NSApplicationLoad\0")
                } {
                    unsafe { nsapp_load(); }
                }
                let _: &'static Library = Box::leak(Box::new(appkit));
            }
        }

        // GL attributes MUST be set BEFORE SDL_CreateWindow or
        // they're silently ignored. Without these, the context
        // defaults to legacy GL 2.1 on macOS, and #version 330
        // core shaders fail to link / produce nothing visible.
        // Only relevant if the caller wants a GL context — if
        // not, attribute calls are harmless.
        if self.gl_handle_field.is_some() {
            type SdlGlSetAttribute = unsafe extern "C" fn(c_int, c_int) -> c_int;
            if let Ok(set_attr) = unsafe { lib.get::<Symbol<SdlGlSetAttribute>>(b"SDL_GL_SetAttribute\0") } {
                unsafe {
                    set_attr(17, 3);  // CONTEXT_MAJOR_VERSION = 3
                    set_attr(18, 3);  // CONTEXT_MINOR_VERSION = 3
                    set_attr(21, 1);  // CONTEXT_PROFILE_MASK = CORE
                    set_attr(5, 1);   // DOUBLEBUFFER = 1
                }
            }
        }

        let title_c = CString::new(self.title.clone()).unwrap_or_default();
        let win_ptr = unsafe {
            sdl_create_window(
                title_c.as_ptr(),
                0x2FFF0000u32 as i32, 0x2FFF0000u32 as i32,
                self.width, self.height,
                2,  // SDL_WINDOW_OPENGL
            )
        };
        if win_ptr.is_null() {
            return Err("SDL_CreateWindow returned null".to_string());
        }
        // Explicitly show + raise. On macOS, terminal-launched
        // SDL windows can stay hidden behind other apps until
        // the activation policy is set; SDL_RaiseWindow nudges
        // them to the front. Both calls are no-ops if the
        // window is already visible.
        type SdlVoidWin = unsafe extern "C" fn(*mut c_void);
        if let Ok(show) = unsafe { lib.get::<Symbol<SdlVoidWin>>(b"SDL_ShowWindow\0") } {
            unsafe { show(win_ptr); }
        }
        if let Ok(raise) = unsafe { lib.get::<Symbol<SdlVoidWin>>(b"SDL_RaiseWindow\0") } {
            unsafe { raise(win_ptr); }
        }

        // GL context (optional). Attributes were already set
        // above, before SDL_CreateWindow.
        let gl_ptr = if self.gl_handle_field.is_some() {
            type SdlGlCreateContext = unsafe extern "C" fn(*mut c_void) -> *mut c_void;
            type SdlGlMakeCurrent   = unsafe extern "C" fn(*mut c_void, *mut c_void) -> c_int;
            let create_ctx: Symbol<SdlGlCreateContext> =
                unsafe { lib.get(b"SDL_GL_CreateContext\0") }
                    .map_err(|e| format!("SDL_GL_CreateContext lookup: {e}"))?;
            let ctx_ptr = unsafe { create_ctx(win_ptr) };
            if ctx_ptr.is_null() {
                return Err("SDL_GL_CreateContext returned null".to_string());
            }
            if let Ok(make_current) = unsafe { lib.get::<Symbol<SdlGlMakeCurrent>>(b"SDL_GL_MakeCurrent\0") } {
                unsafe { make_current(win_ptr, ctx_ptr) };
            }
            ctx_ptr as i64
        } else {
            0i64
        };

        // Default VAO (optional) + viewport. Core profile draws
        // need a bound VAO; on Apple's GL-on-Metal driver the
        // default viewport is 0×0 until you set it. Both bridge
        // installs reuse the same OpenGL handle so we open it
        // once.
        type GlGenVertexArrays = unsafe extern "C" fn(c_int, *mut c_uint);
        type GlBindVertexArray = unsafe extern "C" fn(c_uint);
        type GlViewport        = unsafe extern "C" fn(c_int, c_int, c_int, c_int);
        let vao_id = if self.vao_field.is_some() {
            // Try OpenGL framework / libGL.
            let gl_paths = [
                "/System/Library/Frameworks/OpenGL.framework/OpenGL",
                "/usr/lib/x86_64-linux-gnu/libGL.so.1",
                "/usr/lib/libGL.so",
            ];
            let gl_lib = gl_paths.iter()
                .find_map(|p| unsafe { Library::new(p) }.ok());
            if let Some(gl_lib) = gl_lib {
                let gen_vao: Result<Symbol<GlGenVertexArrays>, _> =
                    unsafe { gl_lib.get(b"glGenVertexArrays\0") };
                let bind_vao: Result<Symbol<GlBindVertexArray>, _> =
                    unsafe { gl_lib.get(b"glBindVertexArray\0") };
                let viewport: Result<Symbol<GlViewport>, _> =
                    unsafe { gl_lib.get(b"glViewport\0") };
                let id = if let (Ok(gen), Ok(bind)) = (gen_vao, bind_vao) {
                    let mut id: c_uint = 0;
                    unsafe { gen(1, &mut id as *mut c_uint); bind(id); }
                    id as i64
                } else { 0 };
                // Apple's GL-on-Metal default viewport is 0×0; set
                // it explicitly so draws actually rasterize. Width
                // and height come from the SDL_Window FTI pin.
                if let Ok(vp) = viewport {
                    unsafe { vp(0, 0, self.width, self.height); }
                }
                let _: &'static Library = Box::leak(Box::new(gl_lib));
                id
            } else { 0 }
        } else { 0 };

        // Push the handles to the write queue so the runtime
        // applies them to the snapshot via the normal drain path.
        {
            let mut q = self.write_queue.lock().unwrap();
            q.push_back((self.handle_field.clone(), Value::Int(win_ptr as i64)));
            if let Some(gl_field) = &self.gl_handle_field {
                q.push_back((gl_field.clone(), Value::Int(gl_ptr)));
            }
            if let Some(vao_field) = &self.vao_field {
                q.push_back((vao_field.clone(), Value::Int(vao_id)));
            }
        }
        let _ = tx.send(SchedulerEvent::Tick { name: "sdl".into() });

        // Hold the library alive in a long-lived background thread
        // so its Drop doesn't run prematurely. The thread parks
        // until stop_flag is set; cleanup of window pointer is
        // also done from this thread.
        // Hold the library alive in a long-lived background
        // thread that just waits for the stop signal. SDL
        // teardown (DestroyWindow, Quit) intentionally NOT
        // called — on macOS they need the main thread, and the
        // runtime is exiting anyway when the source is dropped.
        // The OS reclaims the window on process exit.
        let stop = self.stop_flag.clone();
        let _ = win_ptr;  // suppress unused (we want to keep the library alive)
        let handle = std::thread::Builder::new()
            .name("evident-sdl-keepalive".into())
            .spawn(move || {
                while !stop.load(Ordering::Relaxed) {
                    std::thread::sleep(Duration::from_millis(100));
                }
                drop(lib);
            })
            .map_err(|e| format!("sdl keepalive spawn: {e}"))?;
        self.handle = Some(handle);

        Ok(win_ptr as i64)
    }
}

impl EventSource for SdlWindowSource {
    fn start(&mut self, tx: Sender<SchedulerEvent>) -> Result<(), String> {
        if self.handle.is_some() {
            return Err("SdlWindowSource already started".to_string());
        }
        let title = self.title.clone();
        let width = self.width;
        let height = self.height;
        let handle_field = self.handle_field.clone();
        let write_queue = self.write_queue.clone();
        let stop_flag = self.stop_flag.clone();
        let handle = std::thread::Builder::new()
            .name("evident-sdl".into())
            .spawn(move || {
                use libloading::{Library, Symbol};
                use std::ffi::CString;
                use std::os::raw::{c_char, c_int, c_void};
                // Try common SDL2 paths.
                let paths = [
                    "/opt/homebrew/lib/libSDL2.dylib",
                    "/usr/local/lib/libSDL2.dylib",
                    "/usr/lib/x86_64-linux-gnu/libSDL2.so",
                    "/usr/lib/libSDL2.so",
                ];
                let lib = paths.iter()
                    .find_map(|p| unsafe { Library::new(p) }.ok());
                let Some(lib) = lib else {
                    eprintln!("[SdlWindowSource] couldn't find libSDL2 \
                               in standard paths; window not created");
                    let _ = tx.send(SchedulerEvent::Tick { name: "sdl".into() });
                    return;
                };

                // SDL_Init(SDL_INIT_VIDEO=0x20)
                type SdlInit  = unsafe extern "C" fn(u32) -> c_int;
                type SdlCreateWindow = unsafe extern "C" fn(*const c_char, c_int, c_int, c_int, c_int, u32) -> *mut c_void;
                type SdlDestroyWindow = unsafe extern "C" fn(*mut c_void);
                type SdlQuit  = unsafe extern "C" fn();

                let sdl_init: Symbol<SdlInit> = match unsafe { lib.get(b"SDL_Init\0") } {
                    Ok(s) => s,
                    Err(e) => { eprintln!("[SdlWindowSource] SDL_Init lookup: {e}"); return; }
                };
                let sdl_create_window: Symbol<SdlCreateWindow> = match unsafe { lib.get(b"SDL_CreateWindow\0") } {
                    Ok(s) => s,
                    Err(e) => { eprintln!("[SdlWindowSource] SDL_CreateWindow lookup: {e}"); return; }
                };
                let sdl_destroy_window: Symbol<SdlDestroyWindow> = match unsafe { lib.get(b"SDL_DestroyWindow\0") } {
                    Ok(s) => s,
                    Err(_) => { eprintln!("[SdlWindowSource] SDL_DestroyWindow lookup failed"); return; }
                };
                let sdl_quit: Symbol<SdlQuit> = match unsafe { lib.get(b"SDL_Quit\0") } {
                    Ok(s) => s,
                    Err(_) => { eprintln!("[SdlWindowSource] SDL_Quit lookup failed"); return; }
                };

                let init_rc = unsafe { sdl_init(0x20) };
                if init_rc != 0 {
                    eprintln!("[SdlWindowSource] SDL_Init returned {init_rc}");
                    return;
                }
                let title_c = CString::new(title.clone()).unwrap_or_default();
                // Position SDL_WINDOWPOS_CENTERED = 0x2FFF0000.
                // Flags 0x2 = SDL_WINDOW_OPENGL.
                let win_ptr = unsafe {
                    sdl_create_window(
                        title_c.as_ptr(),
                        0x2FFF0000u32 as i32, 0x2FFF0000u32 as i32,
                        width, height,
                        2,
                    )
                };
                if win_ptr.is_null() {
                    eprintln!("[SdlWindowSource] SDL_CreateWindow returned null");
                    unsafe { sdl_quit() };
                    return;
                }
                {
                    let mut q = write_queue.lock().unwrap();
                    q.push_back((handle_field.clone(), Value::Int(win_ptr as i64)));
                }
                let _ = tx.send(SchedulerEvent::Tick { name: "sdl".into() });

                // Wait for stop signal.
                while !stop_flag.load(Ordering::Relaxed) {
                    std::thread::sleep(Duration::from_millis(100));
                }

                // Cleanup.
                unsafe { sdl_destroy_window(win_ptr) };
                unsafe { sdl_quit() };
                drop(lib);
            })
            .map_err(|e| format!("SdlWindowSource spawn: {e}"))?;
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
        vec![self.handle_field.clone()]
    }
}

impl Drop for SdlWindowSource {
    fn drop(&mut self) { self.stop(); }
}

/// GL shader program FTI bridge. Synchronous install: compile
/// vertex + fragment shaders, link program, call glUseProgram,
/// write program ID. Requires a current GL context (set by
/// SDL_Window FTI's earlier install in the same FSM).
pub struct GlProgramSource {
    vertex_src:   String,
    fragment_src: String,
    handle_field: String,
    write_queue:  WriteQueue,
}

impl GlProgramSource {
    pub fn new(vertex_src: impl Into<String>,
               fragment_src: impl Into<String>,
               handle_field: impl Into<String>) -> Self {
        GlProgramSource {
            vertex_src:   vertex_src.into(),
            fragment_src: fragment_src.into(),
            handle_field: handle_field.into(),
            write_queue:  new_write_queue(),
        }
    }

    /// Synchronous install: compiles + links on the calling
    /// thread (which has the current GL context). Returns
    /// the program ID (or 0 on failure).
    pub fn start_inline(&mut self, tx: Sender<SchedulerEvent>) -> Result<u32, String> {
        use libloading::{Library, Symbol};
        use std::ffi::CString;
        use std::os::raw::{c_char, c_int, c_uint, c_void};

        // OpenGL framework on macOS; libGL on Linux.
        let paths = [
            "/System/Library/Frameworks/OpenGL.framework/OpenGL",
            "/usr/lib/x86_64-linux-gnu/libGL.so.1",
            "/usr/lib/libGL.so",
        ];
        let lib = paths.iter()
            .find_map(|p| unsafe { Library::new(p) }.ok())
            .ok_or_else(|| "couldn't find OpenGL library".to_string())?;

        type GlCreateShader   = unsafe extern "C" fn(c_uint) -> c_uint;
        type GlShaderSource   = unsafe extern "C" fn(c_uint, c_int, *const *const c_char, *const c_int);
        type GlCompileShader  = unsafe extern "C" fn(c_uint);
        type GlCreateProgram  = unsafe extern "C" fn() -> c_uint;
        type GlAttachShader   = unsafe extern "C" fn(c_uint, c_uint);
        type GlLinkProgram    = unsafe extern "C" fn(c_uint);
        type GlUseProgram     = unsafe extern "C" fn(c_uint);
        type GlDeleteShader   = unsafe extern "C" fn(c_uint);
        type GlGetShaderiv    = unsafe extern "C" fn(c_uint, c_uint, *mut c_int);
        type GlGetShaderInfoLog = unsafe extern "C" fn(c_uint, c_int, *mut c_int, *mut c_char);
        type GlGetProgramiv     = unsafe extern "C" fn(c_uint, c_uint, *mut c_int);
        type GlGetProgramInfoLog = unsafe extern "C" fn(c_uint, c_int, *mut c_int, *mut c_char);

        let create_shader: Symbol<GlCreateShader>   = unsafe { lib.get(b"glCreateShader\0") }
            .map_err(|e| format!("glCreateShader: {e}"))?;
        let shader_source: Symbol<GlShaderSource>   = unsafe { lib.get(b"glShaderSource\0") }
            .map_err(|e| format!("glShaderSource: {e}"))?;
        let compile_shader: Symbol<GlCompileShader> = unsafe { lib.get(b"glCompileShader\0") }
            .map_err(|e| format!("glCompileShader: {e}"))?;
        let create_program: Symbol<GlCreateProgram> = unsafe { lib.get(b"glCreateProgram\0") }
            .map_err(|e| format!("glCreateProgram: {e}"))?;
        let attach_shader: Symbol<GlAttachShader>   = unsafe { lib.get(b"glAttachShader\0") }
            .map_err(|e| format!("glAttachShader: {e}"))?;
        let link_program: Symbol<GlLinkProgram>     = unsafe { lib.get(b"glLinkProgram\0") }
            .map_err(|e| format!("glLinkProgram: {e}"))?;
        let use_program: Symbol<GlUseProgram>       = unsafe { lib.get(b"glUseProgram\0") }
            .map_err(|e| format!("glUseProgram: {e}"))?;
        let delete_shader: Symbol<GlDeleteShader>   = unsafe { lib.get(b"glDeleteShader\0") }
            .map_err(|e| format!("glDeleteShader: {e}"))?;
        let get_shader_iv: Symbol<GlGetShaderiv>    = unsafe { lib.get(b"glGetShaderiv\0") }
            .map_err(|e| format!("glGetShaderiv: {e}"))?;
        let get_shader_log: Symbol<GlGetShaderInfoLog> = unsafe { lib.get(b"glGetShaderInfoLog\0") }
            .map_err(|e| format!("glGetShaderInfoLog: {e}"))?;
        let get_program_iv: Symbol<GlGetProgramiv> = unsafe { lib.get(b"glGetProgramiv\0") }
            .map_err(|e| format!("glGetProgramiv: {e}"))?;
        let get_program_log: Symbol<GlGetProgramInfoLog> = unsafe { lib.get(b"glGetProgramInfoLog\0") }
            .map_err(|e| format!("glGetProgramInfoLog: {e}"))?;

        let compile = |kind: c_uint, src: &str| -> Result<c_uint, String> {
            let id = unsafe { create_shader(kind) };
            if id == 0 { return Err("glCreateShader returned 0".into()); }
            let src_c = CString::new(src).map_err(|_| "shader src has nul")?;
            let src_ptr = src_c.as_ptr();
            unsafe {
                shader_source(id, 1, &src_ptr, std::ptr::null());
                compile_shader(id);
            }
            // Check compile status (GL_COMPILE_STATUS = 0x8B81).
            let mut status: c_int = 0;
            unsafe { get_shader_iv(id, 0x8B81, &mut status); }
            if status == 0 {
                let mut log = vec![0i8; 1024];
                let mut len: c_int = 0;
                unsafe { get_shader_log(id, 1024, &mut len, log.as_mut_ptr() as *mut c_char); }
                let log_str: String = log.iter().take(len as usize)
                    .map(|&b| b as u8 as char).collect();
                return Err(format!("shader compile failed: {log_str}"));
            }
            Ok(id)
        };

        // GL_VERTEX_SHADER=0x8B31, GL_FRAGMENT_SHADER=0x8B30.
        let vs = compile(0x8B31, &self.vertex_src)?;
        let fs = compile(0x8B30, &self.fragment_src)?;
        let prog = unsafe { create_program() };
        if prog == 0 { return Err("glCreateProgram returned 0".into()); }
        unsafe {
            attach_shader(prog, vs);
            attach_shader(prog, fs);
            link_program(prog);
        }
        // Check link status (GL_LINK_STATUS = 0x8B82). Silent
        // link failure is the classic black-screen footgun.
        let mut link_status: c_int = 0;
        unsafe { get_program_iv(prog, 0x8B82, &mut link_status); }
        if link_status == 0 {
            let mut log = vec![0i8; 1024];
            let mut len: c_int = 0;
            unsafe { get_program_log(prog, 1024, &mut len, log.as_mut_ptr() as *mut c_char); }
            let log_str: String = log.iter().take(len as usize)
                .map(|&b| b as u8 as char).collect();
            return Err(format!("program link failed: {log_str}"));
        }
        unsafe {
            use_program(prog);
            delete_shader(vs);
            delete_shader(fs);
        }

        {
            let mut q = self.write_queue.lock().unwrap();
            q.push_back((self.handle_field.clone(), Value::Int(prog as i64)));
        }
        let _ = tx.send(SchedulerEvent::Tick { name: "gl_program".into() });

        // No keepalive thread needed — the lib stays loaded
        // because we leak it (drop suppressed via the
        // Box::leak pattern below would be cleaner, but
        // forgetting the binding works too). For simplicity
        // we just let the borrow extend through `lib` going
        // out of scope; the underlying GL framework remains
        // mapped because SDL_Window's bridge holds it open.
        // Actually we need to either leak this lib or hold
        // onto it. Let's leak it via Box::leak.
        let leaked: &'static Library = Box::leak(Box::new(lib));
        let _ = leaked;
        Ok(prog)
    }
}

impl EventSource for GlProgramSource {
    fn start(&mut self, tx: Sender<SchedulerEvent>) -> Result<(), String> {
        // Always synchronous; uses the current GL context.
        self.start_inline(tx)?;
        Ok(())
    }
    fn stop(&mut self) {}
    fn drain_writes(&mut self) -> Vec<(String, Value)> {
        drain(&self.write_queue)
    }
    fn write_fields(&self) -> Vec<String> {
        vec![self.handle_field.clone()]
    }
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
