//! Generic FTI install driven by `install ∈ Seq(InstallStep)`: query body → decode Seq →
//! dispatch atomically → write `Bind`'d results to `<fsm>.<param>.<field>`.

use std::sync::mpsc::Sender;

use crate::core::ast::Pins;
use crate::effect_dispatch::{dispatch_all, DispatchContext};
use crate::fti::FtiContext;
use crate::translate::{Value};
use crate::translate::ast_decoder::decode_install_step_list;
use super::{drain, new_write_queue, EventSource, SchedulerEvent, WriteQueue};

/// One-shot install source: dispatches the install Seq at startup then sits idle.
pub struct DeclarativeInstallSource {
    write_queue: WriteQueue,
}

impl DeclarativeInstallSource {
    pub fn new() -> Self {
        DeclarativeInstallSource { write_queue: new_write_queue() }
    }

    /// Dispatch the install Seq for `type_name` under `pins`, queue results to write queue.
    /// `dispatch_ctx` MUST be the scheduler's context — a fresh context orphans HandleRegistry IDs.
    pub fn run_install(
        &mut self,
        rt:   &crate::runtime::EvidentRuntime,
        type_name: &str,
        ctx:  &FtiContext,
        pins: &Pins,
        tx:   &Sender<SchedulerEvent>,
        dispatch_ctx: &mut DispatchContext,
    ) -> Result<(), String> {
        let mut given: std::collections::HashMap<String, Value> =
            std::collections::HashMap::new();
        if let Pins::Named(ms) = pins {
            for m in ms {
                use crate::core::ast::Expr;
                let v = match &m.value {
                    Expr::Int(n)  => Value::Int(*n),
                    Expr::Bool(b) => Value::Bool(*b),
                    Expr::Str(s)  => Value::Str(s.clone()),
                    _ => continue,
                };
                given.insert(m.slot.clone(), v);
            }
        }

        let result = rt.query_with_pins_and_given(type_name, &[], &given)
            .map_err(|e| format!("declarative install: query {type_name}: {e}"))?;
        if !result.satisfied {
            return Err(format!("declarative install: {type_name} body UNSAT under pins"));
        }
        let install_val = result.bindings.get("install").ok_or_else(||
            format!("declarative install: {type_name} has no `install` binding"))?;
        let steps = decode_install_step_list(install_val)
            .map_err(|e| format!("declarative install: decode `install`: {e:?}"))?;

        // Dispatch atomically: ArgPriorResult(N) threads earlier handles forward automatically.
        let effects: Vec<_> = steps.iter().map(|s| s.effect.clone()).collect();
        let results = dispatch_all(dispatch_ctx, &effects);

        let mut q = self.write_queue.lock().unwrap();
        for (step, res) in steps.iter().zip(results.iter()) {
            let Some(field) = &step.field else { continue };
            let key = format!("{}.{}.{}", ctx.claim_name, ctx.param_name, field);
            let value = match res {
                crate::core::ast::EffectResult::Int(n)    => Value::Int(*n),
                crate::core::ast::EffectResult::Handle(h) => Value::Int(*h as i64),
                crate::core::ast::EffectResult::Str(s)    => Value::Str(s.clone()),
                crate::core::ast::EffectResult::Bool(b)   => Value::Bool(*b),
                crate::core::ast::EffectResult::Real(r)   => Value::Real(*r),
                crate::core::ast::EffectResult::Error(e)  => {
                    return Err(format!(
                        "declarative install: step `Bind({field}, …)` returned Error: {e}"));
                }
                crate::core::ast::EffectResult::NoResult  => continue,
            };
            q.push_back((key, value));
        }
        let _ = tx.send(SchedulerEvent::Tick { name: format!("install:{type_name}") });
        Ok(())
    }
}

impl EventSource for DeclarativeInstallSource {
    fn start(&mut self, _tx: Sender<SchedulerEvent>) -> Result<(), String> {
        Ok(()) // one-shot; install runs before the scheduler starts
    }
    fn stop(&mut self) {}
    fn drain_writes(&mut self) -> Vec<(String, Value)> {
        drain(&self.write_queue)
    }
    fn write_fields(&self) -> Vec<String> {
        Vec::new() // caller (FTI glue) reports the written fields; source itself is empty
    }
}

impl Default for DeclarativeInstallSource {
    fn default() -> Self { Self::new() }
}
