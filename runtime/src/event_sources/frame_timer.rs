//! Periodic-tick event source. See `event_sources/mod.rs` for
//! the EventSource trait + queue helpers this file builds on.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::thread::JoinHandle;
use std::time::Duration;

use crate::Value;
use super::{
    drain, new_write_queue, EventSource, SchedulerEvent, WorldPluginCtx,
    WorldPluginInstall, WriteQueue,
};

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

/// World-plugin install fn for FrameTimer. Installs if any of:
///   * `EVIDENT_TICK_MS` env var is set (back-compat: explicit
///     opt-in via env even without a world field)
///   * the user's World declares `tick_count: Int` (new
///     world-write auto-install path)
///   * any FSM has `_ ∈ FrameTimer` parameter (back-compat
///     marker-subscription)
pub(super) fn install_world_plugin(
    ctx:      &WorldPluginCtx,
    event_tx: &std::sync::mpsc::Sender<SchedulerEvent>,
) -> Result<Option<WorldPluginInstall>, String> {
    let want = ctx.env_tick_ms.is_some()
        || ctx.has_world_field("tick_count", "Int")
        || ctx.fsm_event_subscriptions.contains("tick");
    if !want { return Ok(None); }

    let ms = ctx.env_tick_ms.unwrap_or(100);
    let mut timer = FrameTimer::new(ms, "tick");
    let mut writes: Vec<String> = Vec::new();
    if ctx.has_world_field("tick_count", "Int") {
        timer = timer.with_count_field("tick_count");
        writes.push("tick_count".to_string());
    }
    timer.start(event_tx.clone())
        .map_err(|e| format!("failed to start tick timer: {e}"))?;
    Ok(Some(WorldPluginInstall {
        source:        Box::new(timer),
        plugin_writes: writes,
        owns_stdin:    false,
    }))
}
