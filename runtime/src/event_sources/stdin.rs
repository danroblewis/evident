//! Stdin line-reader bridge.

use std::sync::mpsc::Sender;
use std::thread::JoinHandle;

use crate::Value;
use super::{
    drain, new_write_queue, EventSource, SchedulerEvent, WorldPluginCtx,
    WorldPluginInstall, WriteQueue,
};

/// Stdin line reader. Each line queues `(line_field, text)` and `(seq_field, count)`.
/// The seq counter lets FSMs detect "new line not yet processed" without looping at EOF.
pub struct StdinSource {
    name:        String,
    line_field:  String,
    seq_field:   Option<String>,
    write_queue: WriteQueue,
    handle:      Option<JoinHandle<()>>,
}

impl StdinSource {
    pub fn new(line_field: impl Into<String>) -> Self {
        StdinSource {
            name:        "stdin".to_string(),
            line_field:  line_field.into(),
            seq_field:   None,
            write_queue: new_write_queue(),
            handle:      None,
        }
    }

    /// Also write an incrementing seq number into the named Int field on each line.
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
                        Ok(0) => break,
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
                                break;
                            }
                        }
                        Err(_) => break,
                    }
                }
            })
            .map_err(|e| format!("StdinSource spawn: {e}"))?;
        self.handle = Some(handle);
        Ok(())
    }

    fn stop(&mut self) {
        // Blocking read can't be interrupted; drop handle and let thread exit on EOF.
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

/// Installs if World has `stdin_line: String`. Owns fd 0; rejects concurrent
/// `Effect::ReadLine` use. Also increments `stdin_seq: Int` if declared.
pub(super) fn install_world_plugin(
    ctx:      &WorldPluginCtx,
    event_tx: &std::sync::mpsc::Sender<SchedulerEvent>,
) -> Result<Option<WorldPluginInstall>, String> {
    if !ctx.has_world_field("stdin_line", "String") {
        return Ok(None);
    }
    // StdinSource + Effect::ReadLine both want fd 0 — reject the conflict.
    if let Some(claim_name) = (ctx.fsm_using_identifier)("ReadLine") {
        return Err(format!(
            "FSM `{claim_name}` emits Effect::ReadLine, but the program also \
             declares `stdin_line: String` in World which auto-installs \
             StdinSource. Both would race for fd 0. Use either the \
             plugin pattern (subscribe to world.stdin_line) OR remove \
             stdin_line from World and use ReadLine directly."));
    }

    let mut s = StdinSource::new("stdin_line");
    let mut writes: Vec<String> = vec!["stdin_line".to_string()];
    if ctx.has_world_field("stdin_seq", "Int") {
        s = s.with_seq_field("stdin_seq");
        writes.push("stdin_seq".to_string());
    }
    s.start(event_tx.clone())
        .map_err(|e| format!("failed to start stdin reader: {e}"))?;
    Ok(Some(WorldPluginInstall {
        source:        Box::new(s),
        plugin_writes: writes,
        owns_stdin:    true,
    }))
}
