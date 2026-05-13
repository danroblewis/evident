//! Generic FTI install path driven by the type's `install ∈ Seq(InstallStep)`
//! body member.
//!
//! Replaces the per-type bridge files (sdl_window.rs, gl_program.rs,
//! oneshot_shell.rs) with one mechanism. The type author declares
//! the bridge in Evident:
//!
//! ```evident
//! external type SDL_Window
//!     handle ∈ Int
//!     renderer ∈ Int
//!     install ∈ Seq(InstallStep) = ⟨
//!         Run(LibCall("...", "SDL_Init", "i(i)", ⟨ArgInt(32)⟩)),
//!         Bind("handle", LibCall("...", "SDL_CreateWindow", "p(siiiii)", ⟨…⟩)),
//!         Bind("renderer", LibCall("...", "SDL_CreateRenderer", "p(pii)",
//!                                    ⟨ArgPriorResult(1), ArgInt(-1), ArgInt(0)⟩))
//!     ⟩
//! ```
//!
//! At FTI install time the runtime:
//!   1. Queries the type body with the user's pins, extracts the
//!      `install` Seq value.
//!   2. Decodes it into `Vec<InstallStep>`.
//!   3. Dispatches the contained effects atomically via
//!      `effect_dispatch::dispatch_all` (so `ArgPriorResult(N)`
//!      threading works in-batch).
//!   4. For each `Bind`'d step, writes the result Int/String/Handle
//!      to the world snapshot key `<fsm>.<param>.<field>`.

use std::sync::mpsc::Sender;

use crate::ast::Pins;
use crate::effect_dispatch::{dispatch_all, DispatchContext};
use crate::fti::FtiContext;
use crate::translate::{Value};
use crate::translate::ast_decoder::decode_install_step_list;
use super::{drain, new_write_queue, EventSource, SchedulerEvent, WriteQueue};

/// One-shot install source: dispatches the install Seq at install
/// time, then sits idle (no per-tick wake events). The captured
/// write queue is drained by the scheduler once, at startup.
pub struct DeclarativeInstallSource {
    write_queue: WriteQueue,
}

impl DeclarativeInstallSource {
    pub fn new() -> Self {
        DeclarativeInstallSource { write_queue: new_write_queue() }
    }

    /// Resolve the install Seq for `type_name` with `pins` applied,
    /// dispatch each step, and queue captured results onto the write
    /// queue under `<fsm>.<param>.<field>`.
    pub fn run_install(
        &mut self,
        rt:   &crate::runtime::EvidentRuntime,
        type_name: &str,
        ctx:  &FtiContext,
        pins: &Pins,
        tx:   &Sender<SchedulerEvent>,
    ) -> Result<(), String> {
        // Build given map from pin values. pin_int / pin_str helpers
        // are private to fti.rs, so duplicate the shape here.
        let mut given: std::collections::HashMap<String, Value> =
            std::collections::HashMap::new();
        if let Pins::Named(ms) = pins {
            for m in ms {
                use crate::ast::Expr;
                let v = match &m.value {
                    Expr::Int(n)  => Value::Int(*n),
                    Expr::Bool(b) => Value::Bool(*b),
                    Expr::Str(s)  => Value::Str(s.clone()),
                    _ => continue,
                };
                given.insert(m.slot.clone(), v);
            }
        }

        // Query the type body to get `install`'s Seq value.
        let result = rt.query_with_pins_and_given(type_name, &[], &given)
            .map_err(|e| format!("declarative install: query {type_name}: {e}"))?;
        if !result.satisfied {
            return Err(format!("declarative install: {type_name} body UNSAT under pins"));
        }
        let install_val = result.bindings.get("install").ok_or_else(||
            format!("declarative install: {type_name} has no `install` binding"))?;
        let steps = decode_install_step_list(install_val)
            .map_err(|e| format!("declarative install: decode `install`: {e:?}"))?;

        // Dispatch the install Seq atomically. dispatch_all resolves
        // ArgPriorResult(N) against the running prior vector, so handles
        // created earlier in the Seq feed forward to later steps without
        // any explicit threading from the user.
        let effects: Vec<_> = steps.iter().map(|s| s.effect.clone()).collect();
        let mut dctx = DispatchContext::new();
        let results = dispatch_all(&mut dctx, &effects);

        // Capture Bind'd results into the per-instance world keys.
        let mut q = self.write_queue.lock().unwrap();
        for (step, res) in steps.iter().zip(results.iter()) {
            let Some(field) = &step.field else { continue };
            let key = format!("{}.{}.{}", ctx.claim_name, ctx.param_name, field);
            let value = match res {
                crate::ast::EffectResult::Int(n)    => Value::Int(*n),
                crate::ast::EffectResult::Handle(h) => Value::Int(*h as i64),
                crate::ast::EffectResult::Str(s)    => Value::Str(s.clone()),
                crate::ast::EffectResult::Bool(b)   => Value::Bool(*b),
                crate::ast::EffectResult::Real(r)   => Value::Real(*r),
                crate::ast::EffectResult::Error(e)  => {
                    return Err(format!(
                        "declarative install: step `Bind({field}, …)` returned Error: {e}"));
                }
                crate::ast::EffectResult::NoResult  => continue,
            };
            q.push_back((key, value));
        }
        let _ = tx.send(SchedulerEvent::Tick { name: format!("install:{type_name}") });
        Ok(())
    }
}

impl EventSource for DeclarativeInstallSource {
    fn start(&mut self, _tx: Sender<SchedulerEvent>) -> Result<(), String> {
        // Install is one-shot, invoked via `run_install` synchronously
        // before the scheduler starts. The standard `start` hook is a
        // no-op; we're just here to be drained.
        Ok(())
    }
    fn stop(&mut self) {}
    fn drain_writes(&mut self) -> Vec<(String, Value)> {
        drain(&self.write_queue)
    }
    fn write_fields(&self) -> Vec<String> {
        // The fields written depend on the type's install Seq. Caller
        // (the FTI install glue) reports those — `write_fields` on the
        // source itself is empty.
        Vec::new()
    }
}

impl Default for DeclarativeInstallSource {
    fn default() -> Self { Self::new() }
}
