//! SIGINT handler bridge.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::thread::JoinHandle;

use crate::Value;
use super::{
    drain, new_write_queue, EventSource, SchedulerEvent, WorldPluginCtx,
    WorldPluginInstall, WriteQueue,
};

/// SIGINT (Ctrl-C) source. Sends `Tick { name: "signal" }` per signal;
/// exits after the first (signal-hook deduplicates). FSMs subscribe via `_ ∈ Signal`.
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

    /// Write SIGINT count into the named World Int field on each fire.
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
        // Closing the handle wakes forever() so the thread exits.
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

/// Installs if World has `signal_received: Int` or any FSM has `_ ∈ Signal`.
/// Without an opt-in, Ctrl-C is not hijacked globally.
pub(super) fn install_world_plugin(
    ctx:      &WorldPluginCtx,
    event_tx: &std::sync::mpsc::Sender<SchedulerEvent>,
) -> Result<Option<WorldPluginInstall>, String> {
    let want = ctx.has_world_field("signal_received", "Int")
        || ctx.fsm_event_subscriptions.contains("signal");
    if !want { return Ok(None); }

    let mut sig = SigintSource::new();
    let mut writes: Vec<String> = Vec::new();
    if ctx.has_world_field("signal_received", "Int") {
        sig = sig.with_count_field("signal_received");
        writes.push("signal_received".to_string());
    }
    sig.start(event_tx.clone())
        .map_err(|e| format!("failed to install SIGINT handler: {e}"))?;
    Ok(Some(WorldPluginInstall {
        source:        Box::new(sig),
        plugin_writes: writes,
        owns_stdin:    false,
    }))
}
