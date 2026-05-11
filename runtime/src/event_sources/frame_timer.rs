//! Periodic-tick event source. See `event_sources/mod.rs` for
//! the EventSource trait + queue helpers this file builds on.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::thread::JoinHandle;
use std::time::Duration;

use crate::Value;
use super::{drain, new_write_queue, EventSource, SchedulerEvent, WriteQueue};

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
