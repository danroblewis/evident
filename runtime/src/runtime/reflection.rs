//! Self-hosting reflection: encode the loaded program as a Z3 datatype,
//! and run pass-style queries with the program / per-claim body injected.

use super::desugar::SystemBoundary;
use crate::core::{QueryResult, RuntimeError};
use super::{EvidentRuntime, Value};
use crate::core::ast::Program;
use std::collections::{HashMap, HashSet};

impl EvidentRuntime {
    /// Snapshot the system boundary (stdlib/pass files). Subsequent loads are "user-side";
    /// self-hosted passes then see only user schemas. Idempotent.
    pub fn mark_system_loads_complete(&self) {
        let schemas: HashSet<String> = self.schemas.keys().cloned().collect();
        let enums: HashSet<String> = self.enums.by_name.borrow().keys().cloned().collect();
        *self.system_boundary.borrow_mut() = Some(SystemBoundary { schemas, enums });
    }

    /// User-side schemas/enums only (after system boundary); full program if no boundary set.
    pub(super) fn user_program(&self) -> Program {
        let boundary = self.system_boundary.borrow();
        let Some(b) = boundary.as_ref() else { return self.program.clone() };
        Program {
            schemas: self.program.schemas.iter()
                .filter(|s| !b.schemas.contains(&s.name))
                .cloned().collect(),
            enums: self.program.enums.iter()
                .filter(|e| !b.enums.contains(&e.name))
                .cloned().collect(),
            imports: Vec::new(),
        }
    }

    /// Encode the user program as a Z3 Datatype matching `stdlib/ast.ev`'s `Program` enum.
    /// Requires `stdlib/ast.ev` loaded; returns `EnumNotRegistered` otherwise.
    pub fn encode_program_value(
        &self,
    ) -> std::result::Result<z3::ast::Datatype<'static>,
                              crate::translate::ast_encoder::EncodeError> {
        let prog = self.user_program();
        crate::translate::ast_encoder::encode_program(
            &prog,
            self.z3_ctx,
            &self.enums,
        )
    }

    /// Clone of the user-side Program AST; used by the reflection world-plugin and lints.
    pub fn program_ast(&self) -> Program {
        self.user_program()
    }

    /// Like `query_with_program_and_body` but injects the Nth user claim's body.
    /// Returns `Ok(None)` for out-of-range `claim_idx`.
    pub fn query_with_program_and_nth_claim_body(
        &self,
        claim_name: &str,
        program_var: &str,
        body_var: &str,
        claim_idx: usize,
    ) -> Result<Option<QueryResult>, RuntimeError> {
        let prog_value = self.encode_program_value()
            .map_err(|e| RuntimeError::Parse(format!("encode failed: {e}")))?;
        self.evaluate_with_body(claim_name, program_var, body_var, claim_idx, prog_value)
    }

    /// Like `query_with_program_and_nth_claim_body` but skips the Program injection.
    /// Avoids the expensive deep-datatype equality assertion when the pass only uses `body`.
    pub fn query_with_nth_claim_body_only(
        &self,
        claim_name: &str,
        body_var: &str,
        claim_idx: usize,
    ) -> Result<Option<QueryResult>, RuntimeError> {
        let empty_prog = self.encode_empty_program_value()
            .map_err(|e| RuntimeError::Parse(format!("encode empty program: {e}")))?;
        self.evaluate_with_body(claim_name, "program", body_var, claim_idx, empty_prog)
    }

    /// Build a trivial empty-Program Z3 Datatype (no recursive walk cost).
    pub(super) fn encode_empty_program_value(
        &self,
    ) -> std::result::Result<z3::ast::Datatype<'static>,
                              crate::translate::ast_encoder::EncodeError> {
        let empty = Program::default();
        crate::translate::ast_encoder::encode_program(
            &empty, self.z3_ctx, &self.enums,
        )
    }

    /// Evaluate a pass schema with an encoded Program + Nth claim body. Returns `Ok(None)` OOB.
    fn evaluate_with_body(
        &self,
        claim_name: &str,
        program_var: &str,
        body_var: &str,
        claim_idx: usize,
        program_value: z3::ast::Datatype<'static>,
    ) -> Result<Option<QueryResult>, RuntimeError> {
        let schema = self.schemas.get(claim_name)
            .ok_or_else(|| RuntimeError::UnknownSchema(claim_name.to_string()))?;
        let user = self.user_program();
        let Some(target_claim) = user.schemas.get(claim_idx) else {
            return Ok(None);
        };
        let body_items = &target_claim.body;
        let arith: u32 = std::env::var("EVIDENT_Z3_ARITH_SOLVER").ok()
            .and_then(|s| s.parse().ok()).unwrap_or(2);
        let mut given: HashMap<String, Value> = HashMap::new();
        given.insert("body_len".to_string(), Value::Int(body_items.len() as i64));
        let r = crate::translate::evaluate_with_program_and_body(
            schema, &given, &self.schemas, self.z3_ctx,
            &self.datatypes, &self.enums, arith,
            program_var, program_value,
            body_var, body_items,
        );
        Ok(Some(QueryResult { satisfied: r.satisfied, bindings: r.bindings }))
    }

    /// Like `query_with_nth_claim_body_only` but accepts extra caller-pinned `given` variables.
    pub fn query_with_nth_claim_body_only_given(
        &self,
        claim_name: &str,
        body_var: &str,
        claim_idx: usize,
        extra_given: HashMap<String, Value>,
    ) -> Result<Option<QueryResult>, RuntimeError> {
        let schema = self.schemas.get(claim_name)
            .ok_or_else(|| RuntimeError::UnknownSchema(claim_name.to_string()))?;
        let user = self.user_program();
        let Some(target_claim) = user.schemas.get(claim_idx) else {
            return Ok(None);
        };
        let body_items = &target_claim.body;
        let arith: u32 = std::env::var("EVIDENT_Z3_ARITH_SOLVER").ok()
            .and_then(|s| s.parse().ok()).unwrap_or(2);
        let mut given: HashMap<String, Value> = extra_given;
        given.insert("body_len".to_string(), Value::Int(body_items.len() as i64));
        let empty_prog = self.encode_empty_program_value()
            .map_err(|e| RuntimeError::Parse(format!("encode empty program: {e}")))?;
        let r = crate::translate::evaluate_with_program_and_body(
            schema, &given, &self.schemas, self.z3_ctx,
            &self.datatypes, &self.enums, arith,
            "program", empty_prog,
            body_var, body_items,
        );
        Ok(Some(QueryResult { satisfied: r.satisfied, bindings: r.bindings }))
    }

    pub fn query_with_program_and_body(
        &self,
        claim_name: &str,
        program_var: &str,
        body_var: &str,
    ) -> Result<QueryResult, RuntimeError> {
        let schema = self.schemas.get(claim_name)
            .ok_or_else(|| RuntimeError::UnknownSchema(claim_name.to_string()))?;
        let user = self.user_program();
        let prog_value = crate::translate::ast_encoder::encode_program(
            &user, self.z3_ctx, &self.enums,
        ).map_err(|e| RuntimeError::Parse(format!("encode failed: {e}")))?;
        let body_items: Vec<crate::core::ast::BodyItem> = user.schemas.first()
            .map(|s| s.body.clone())
            .unwrap_or_default();
        let arith: u32 = std::env::var("EVIDENT_Z3_ARITH_SOLVER").ok()
            .and_then(|s| s.parse().ok()).unwrap_or(2);
        let mut given: HashMap<String, Value> = HashMap::new();
        given.insert("body_len".to_string(), Value::Int(body_items.len() as i64));
        let r = crate::translate::evaluate_with_program_and_body(
            schema, &given, &self.schemas, self.z3_ctx,
            &self.datatypes, &self.enums, arith,
            program_var, prog_value,
            body_var, &body_items,
        );
        Ok(QueryResult { satisfied: r.satisfied, bindings: r.bindings })
    }
    /// Evaluate a pass schema with the encoded Program injected as a given for `program_var`.
    pub fn query_with_program(
        &self,
        claim_name: &str,
        program_var: &str,
    ) -> Result<QueryResult, RuntimeError> {
        let prog_value = self.encode_program_value()
            .map_err(|e| RuntimeError::Parse(format!("encode failed: {e}")))?;
        self.query_with_program_value(claim_name, program_var, prog_value)
    }

    /// Like `query_with_program` but accepts a pre-encoded value; encode once, reuse across rules.
    pub fn query_with_program_value(
        &self,
        claim_name: &str,
        program_var: &str,
        program_value: z3::ast::Datatype<'static>,
    ) -> Result<QueryResult, RuntimeError> {
        let schema = self.schemas.get(claim_name)
            .ok_or_else(|| RuntimeError::UnknownSchema(claim_name.to_string()))?;
        let arith: u32 = std::env::var("EVIDENT_Z3_ARITH_SOLVER").ok()
            .and_then(|s| s.parse().ok()).unwrap_or(2);
        let r = crate::translate::evaluate_with_extra_assertion(
            schema,
            &HashMap::new(),
            &self.schemas,
            self.z3_ctx,
            &self.datatypes,
            Some(&self.enums),
            arith,
            program_var,
            program_value,
        );
        Ok(QueryResult { satisfied: r.satisfied, bindings: r.bindings })
    }
}
