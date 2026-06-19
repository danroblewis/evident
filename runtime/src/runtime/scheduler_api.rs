use crate::core::{QueryResult, RuntimeError};
use super::{EvidentRuntime, Value};
use std::collections::HashMap;

impl EvidentRuntime {

    pub fn query_with_pinned_datatypes(
        &self,
        claim_name: &str,
        pins: &[(&str, z3::ast::Datatype<'static>)],
    ) -> Result<QueryResult, RuntimeError> {
        self.query_with_pins_and_given(claim_name, pins, &HashMap::new())
    }

    pub fn query_with_pins_and_given(
        &self,
        claim_name: &str,
        pins: &[(&str, z3::ast::Datatype<'static>)],
        given: &HashMap<String, Value>,
    ) -> Result<QueryResult, RuntimeError> {
        let schema = self.schemas.get(claim_name)
            .ok_or_else(|| RuntimeError::UnknownSchema(claim_name.to_string()))?;

        if let Some(result) = self.try_functionize_z3(claim_name, schema, given) {
            return Ok(result);
        }

        let arith: u32 = 2;

        let mut given_keys: Vec<String> = given.keys().cloned().collect();
        given_keys.sort();
        let cache_key = (claim_name.to_string(), given_keys);
        if let Some(cached) = self.slow_path_cache.borrow().get(&cache_key).cloned() {
            use z3::ast::Ast;
            cached.solver.push();

            for (var_name, value) in pins {
                if let Some(crate::translate::Var::EnumVar { ast, .. }) = cached.env.get(*var_name) {
                    cached.solver.assert(&ast._eq(value));
                }
            }

            for (name, value) in given {
                if let (Some(crate::translate::Var::EnumVar { ast, .. }), Value::Enum { .. }) =
                    (cached.env.get(name), value)
                {
                    if let Some(dt) = crate::translate::value_enum_to_datatype(
                        value, self.z3_ctx, &self.enums)
                    {
                        cached.solver.assert(&ast._eq(&dt));
                    }
                }
            }
            let r = crate::translate::run_cached(&cached, given, self.z3_ctx, Some(&self.enums));
            cached.solver.pop(1);
            return Ok(QueryResult { satisfied: r.satisfied, bindings: r.bindings });
        }

        let r = crate::translate::evaluate_with_extra_assertions(
            schema,
            given,
            &self.schemas,
            self.z3_ctx,
            &self.datatypes,
            Some(&self.enums),
            arith,
            pins,
        );
        Ok(QueryResult { satisfied: r.satisfied, bindings: r.bindings })
    }
}
