//! FTI declarative install — the one-shot bridge install path that
//! survives the single-FSM teardown.
//!
//! A typed resource (`win ∈ SDL_Window (title ↦ "X", …)`) declares its
//! C-side lifecycle as an `install ∈ Seq(InstallStep)` member in its
//! `external type` body. At startup, for each such parameter on the
//! main FSM, the runtime:
//!   1. Queries the type body with the user's pins to extract the
//!      `install` Seq value.
//!   2. Decodes it into `Vec<InstallStep>`.
//!   3. Dispatches the contained effects atomically via
//!      `effect_dispatch::dispatch_all` (so `ArgPriorResult(N)`
//!      threading works in-batch — e.g. the renderer handle created
//!      by `SDL_CreateRenderer` feeds later steps).
//!   4. For each `Bind`'d step, writes the result to the world
//!      snapshot key `<fsm>.<param>.<field>`, which the per-tick solve
//!      then exposes to the FSM body as `param.field` (`win.renderer`).
//!
//! CRITICAL: the install MUST run against the same `DispatchContext`
//! the per-tick loop uses, so libffi pointer IDs (window ptr, renderer
//! ptr) registered at install are visible to per-tick `ArgHandle`
//! lookups. A fresh context would orphan them ("renders black").

use std::collections::HashMap;

use crate::core::ast::{Expr, Pins};
use crate::effect_dispatch::{dispatch_all, DispatchContext};
use crate::runtime::EvidentRuntime;
use crate::translate::ast_decoder::decode_install_step_list;
use crate::translate::Value;

/// Run the declarative install for one typed-resource parameter.
/// Returns the captured `<fsm>.<param>.<field>` → Value writes to
/// merge into the world snapshot. Errors propagate as load failures.
pub(super) fn run_declarative_install(
    rt: &EvidentRuntime,
    claim_name: &str,
    param_name: &str,
    type_name: &str,
    pins: &Pins,
    dispatch_ctx: &mut DispatchContext,
) -> Result<Vec<(String, Value)>, String> {
    // Build the given map from pin values.
    let mut given: HashMap<String, Value> = HashMap::new();
    if let Pins::Named(ms) = pins {
        for m in ms {
            let v = match &m.value {
                Expr::Int(n) => Value::Int(*n),
                Expr::Bool(b) => Value::Bool(*b),
                Expr::Str(s) => Value::Str(s.clone()),
                _ => continue,
            };
            given.insert(m.slot.clone(), v);
        }
    }

    // Query the type body to get `install`'s Seq value.
    let result = rt
        .query_with_pins_and_given(type_name, &[], &given)
        .map_err(|e| format!("declarative install: query {type_name}: {e}"))?;
    if !result.satisfied {
        return Err(format!(
            "declarative install: {type_name} body UNSAT under pins"
        ));
    }
    let install_val = result.bindings.get("install").ok_or_else(|| {
        format!("declarative install: {type_name} has no `install` binding")
    })?;
    let steps = decode_install_step_list(install_val)
        .map_err(|e| format!("declarative install: decode `install`: {e:?}"))?;

    // Dispatch the install Seq atomically. dispatch_all resolves
    // ArgPriorResult(N) against the running prior vector, so handles
    // created earlier in the Seq feed forward to later steps.
    let effects: Vec<_> = steps.iter().map(|s| s.effect.clone()).collect();
    let results = dispatch_all(dispatch_ctx, &effects);

    // Capture Bind'd results into the per-instance world keys.
    let mut writes: Vec<(String, Value)> = Vec::new();
    for (step, res) in steps.iter().zip(results.iter()) {
        let Some(field) = &step.field else { continue };
        let key = format!("{claim_name}.{param_name}.{field}");
        let value = match res {
            crate::core::ast::EffectResult::Int(n) => Value::Int(*n),
            crate::core::ast::EffectResult::Handle(h) => Value::Int(*h as i64),
            crate::core::ast::EffectResult::Str(s) => Value::Str(s.clone()),
            crate::core::ast::EffectResult::Bool(b) => Value::Bool(*b),
            crate::core::ast::EffectResult::Real(r) => Value::Real(*r),
            crate::core::ast::EffectResult::Error(e) => {
                return Err(format!(
                    "declarative install: step `Bind({field}, …)` returned Error: {e}"
                ));
            }
            crate::core::ast::EffectResult::NoResult => continue,
        };
        writes.push((key, value));
    }
    Ok(writes)
}
