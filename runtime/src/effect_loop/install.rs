use std::collections::HashMap;

use crate::core::ast::{Expr, Pins};
use crate::effect_dispatch::{dispatch_all, DispatchContext};
use crate::runtime::EvidentRuntime;
use crate::translate::effect_decoder::decode_install_step_list;
use crate::translate::Value;

pub(super) fn run_declarative_install(
    rt: &EvidentRuntime,
    claim_name: &str,
    param_name: &str,
    type_name: &str,
    pins: &Pins,
    dispatch_ctx: &mut DispatchContext,
) -> Result<Vec<(String, Value)>, String> {

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

    let effects: Vec<_> = steps.iter().map(|s| s.effect.clone()).collect();
    let results = dispatch_all(dispatch_ctx, &effects);

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
