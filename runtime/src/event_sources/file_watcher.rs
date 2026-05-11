//! File mtime watcher bridge. See `event_sources/mod.rs` for trait
//! and shared helpers.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::thread::JoinHandle;
use std::time::Duration;

use crate::Value;
use super::{drain, new_write_queue, EventSource, SchedulerEvent, WriteQueue};

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
