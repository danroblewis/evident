//! Reflection world-plugin: writes the encoded `Program` AST to a
//! reserved world field. Lets an in-`effect-run` FSM consume the
//! same encoded form the desugar pass works against (matches
//! `stdlib/ast.ev`'s `Program` enum shape exactly).
//!
//! One-shot. The encoded value is captured at install time and
//! pushed to the write queue once on `start`; after that the
//! source goes silent. The field name is auto-detected by scanning
//! the user's World type for any field whose declared type is
//! `Program` — the user can name it whatever (`program`, `ast`,
//! `loaded_program`, …) as long as the type matches.
//!
//! Foundation for the upcoming GLSL-transpiler FSM, which will
//! pattern-match on the Program tree (walking its `EnumDecl`s and
//! `SchemaDecl`s) to lower a shader DSL into emitted GLSL source.

use std::sync::mpsc::Sender;

use crate::Value;
use super::{
    drain, new_write_queue, EventSource, SchedulerEvent, WorldPluginCtx,
    WorldPluginInstall, WriteQueue,
};

/// One-shot bridge: at `start`, queues a single (field, encoded
/// program) write and a wake event so subscribers see the value
/// on tick 0. No background thread; nothing further to do.
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
        // Only fire once. start() shouldn't normally be called
        // twice (the scheduler installs each source once), but
        // guard so a re-entry can't queue duplicate writes.
        let Some(value) = self.encoded.take() else {
            return Ok(());
        };
        {
            let mut q = self.write_queue.lock().unwrap();
            q.push_back((self.handle_field.clone(), value));
        }
        // Best-effort wake — receiver may already be dropped if
        // the scheduler tore down before we finished install. Not
        // an error; the write queue is still drained at the next
        // tick boundary by the scheduler picking up the source.
        let _ = tx.send(SchedulerEvent::Tick { name: "reflection".to_string() });
        Ok(())
    }

    fn stop(&mut self) {
        // No background work, nothing to tear down.
    }

    fn drain_writes(&mut self) -> Vec<(String, Value)> {
        drain(&self.write_queue)
    }

    fn write_fields(&self) -> Vec<String> {
        vec![self.handle_field.clone()]
    }
}

/// World-plugin install fn for the reflection bridge. Scans the
/// user's World type for a field declared as `∈ Program`. If found,
/// encodes the program (via the ctx closure) and starts the source.
///
/// Returns:
///   * `Ok(None)` — no field of type `Program` declared; nothing
///     to install.
///   * `Ok(Some(install))` — found a Program field; encoded and
///     started successfully.
///   * `Err(msg)` — the user declared a Program field but encoding
///     failed (e.g. `stdlib/ast.ev` not imported), OR there's
///     more than one Program field (ambiguous; the bridge writes
///     one field, the user must consolidate).
pub(super) fn install_world_plugin(
    ctx:      &WorldPluginCtx,
    event_tx: &Sender<SchedulerEvent>,
) -> Result<Option<WorldPluginInstall>, String> {
    // Collect every world field whose declared type is `Program`.
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

    // Encode now (at install time). Failures here propagate to
    // load-time errors — caller sees them before the FSM scheduler
    // starts ticking.
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
