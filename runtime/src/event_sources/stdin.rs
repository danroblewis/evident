//! Stdin line-reader bridge. See `event_sources/mod.rs` for trait
//! and shared helpers.

use std::sync::mpsc::Sender;
use std::thread::JoinHandle;

use crate::Value;
use super::{drain, new_write_queue, EventSource, SchedulerEvent, WriteQueue};

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
