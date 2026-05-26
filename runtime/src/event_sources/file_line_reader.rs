//! File line-reader bridge.

use std::sync::mpsc::Sender;
use std::thread::JoinHandle;

use crate::Value;
use super::{
    drain, new_write_queue, EventSource, SchedulerEvent, WorldPluginCtx,
    WorldPluginInstall, WriteQueue,
};

/// Like StdinSource but reads from a path instead of fd 0.
/// EOF closes the channel sender ("source dead" to the scheduler).
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
                                return;
                            }
                        }
                        Err(_) => break,
                    }
                }
                // EOF — set eof flag and send a final wake; sender drops here.
                if let Some(ef) = &eof_field {
                    let mut q = write_queue.lock().unwrap();
                    q.push_back((ef.clone(), Value::Bool(true)));
                }
                let _ = tx.send(SchedulerEvent::Tick { name: name.clone() });
            })
            .map_err(|e| format!("FileLineReader spawn: {e}"))?;
        self.handle = Some(handle);
        Ok(())
    }

    fn stop(&mut self) {
        // Blocking read can't be interrupted portably; drop handle and let thread finish.
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

/// Installs if World has `file_line: String` and `EVIDENT_FILE_INPUT` is set.
/// Optional: `file_seq: Int` (sequence counter), `file_eof: Bool` (set at EOF).
pub(super) fn install_world_plugin(
    ctx:      &WorldPluginCtx,
    event_tx: &std::sync::mpsc::Sender<SchedulerEvent>,
) -> Result<Option<WorldPluginInstall>, String> {
    if !ctx.has_world_field("file_line", "String") {
        return Ok(None);
    }
    let Some(path) = ctx.env_file_input else { return Ok(None); };

    let mut f = FileLineReader::new(path, "file_line");
    let mut writes: Vec<String> = vec!["file_line".to_string()];
    if ctx.has_world_field("file_seq", "Int") {
        f = f.with_seq_field("file_seq");
        writes.push("file_seq".to_string());
    }
    if ctx.has_world_field("file_eof", "Bool") {
        f = f.with_eof_field("file_eof");
        writes.push("file_eof".to_string());
    }
    f.start(event_tx.clone())
        .map_err(|e| format!("failed to start file reader for {path:?}: {e}"))?;
    Ok(Some(WorldPluginInstall {
        source:        Box::new(f),
        plugin_writes: writes,
        owns_stdin:    false,
    }))
}
