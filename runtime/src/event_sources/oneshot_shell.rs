//! One-shot shell command bridge. See `event_sources/mod.rs` for
//! trait and shared helpers.

use std::sync::mpsc::Sender;
use std::thread::JoinHandle;

use crate::Value;
use super::{drain, new_write_queue, EventSource, SchedulerEvent, WriteQueue};

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
