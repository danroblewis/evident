//! Reflection world-plugin: writes the encoded `Program` AST (matching `stdlib/ast.ev`)
//! to a World field declared as `∈ Program`. One-shot; fires once on tick 0.

use std::sync::mpsc::Sender;

use crate::Value;
use super::{
    drain, new_write_queue, EventSource, SchedulerEvent, WorldPluginCtx,
    WorldPluginInstall, WriteQueue,
};

/// One-shot bridge: queues the encoded program write + wake at `start`, then goes silent.
pub struct ReflectionSource {
    handle_field: String,
    encoded:      Option<Value>,   // taken at start; None thereafter
    write_queue:  WriteQueue,
}

impl ReflectionSource {
    pub fn new(handle_field: String, encoded: Value) -> Self {
        ReflectionSource {
            handle_field,
            encoded:     Some(encoded),
            write_queue: new_write_queue(),
        }
    }
}

impl EventSource for ReflectionSource {
    fn start(&mut self, tx: Sender<SchedulerEvent>) -> Result<(), String> {
        // Guard against double-start; encoded is taken once.
        let Some(value) = self.encoded.take() else {
            return Ok(());
        };
        {
            let mut q = self.write_queue.lock().unwrap();
            q.push_back((self.handle_field.clone(), value));
        }
        let _ = tx.send(SchedulerEvent::Tick { name: "reflection".to_string() }); // best-effort
        Ok(())
    }

    fn stop(&mut self) {}

    fn drain_writes(&mut self) -> Vec<(String, Value)> {
        drain(&self.write_queue)
    }

    fn write_fields(&self) -> Vec<String> {
        vec![self.handle_field.clone()]
    }
}

/// Installs if World has exactly one field typed `Program`. Multiple Program fields → Err.
/// Encoding fails if `stdlib/ast.ev` isn't imported.
pub(super) fn install_world_plugin(
    ctx:      &WorldPluginCtx,
    event_tx: &Sender<SchedulerEvent>,
) -> Result<Option<WorldPluginInstall>, String> {
    let candidates: Vec<&String> = ctx.world_fields.iter()
        .filter_map(|(name, ty)| if ty == "Program" { Some(name) } else { None })
        .collect();

    if candidates.is_empty() {
        return Ok(None);
    }
    if candidates.len() > 1 {
        let names = candidates.iter()
            .map(|s| s.as_str()).collect::<Vec<_>>().join(", ");
        return Err(format!(
            "reflection plugin: world declares multiple `Program` fields \
             ({names}); only one Program field per world is supported. \
             Drop the redundant fields or rename one to a different type."
        ));
    }
    let field = candidates[0].clone();
    let encoded = (ctx.encode_program)()?;

    let mut src = ReflectionSource::new(field.clone(), encoded);
    src.start(event_tx.clone())
        .map_err(|e| format!("reflection plugin: failed to start: {e}"))?;
    Ok(Some(WorldPluginInstall {
        source:        Box::new(src),
        plugin_writes: vec![field],
        owns_stdin:    false,
    }))
}
