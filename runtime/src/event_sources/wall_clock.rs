//! Wall-clock event source. See `event_sources/mod.rs` for trait
//! and shared helpers.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::thread::JoinHandle;
use std::time::Duration;

use crate::Value;
use super::{drain, new_write_queue, EventSource, SchedulerEvent, WriteQueue};

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
