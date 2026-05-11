//! SIGINT handler bridge. See `event_sources/mod.rs` for trait
//! and shared helpers.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::thread::JoinHandle;

use crate::Value;
use super::{drain, new_write_queue, EventSource, SchedulerEvent, WriteQueue};

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
