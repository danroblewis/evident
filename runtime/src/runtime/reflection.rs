//! Self-hosting reflection: encode the loaded program as a Z3 datatype,
//! and run pass-style queries with the program / per-claim body injected.

use super::desugar::SystemBoundary;
use crate::core::{QueryResult, RuntimeError};
use super::{EvidentRuntime, Value};
use crate::core::ast::Program;
use std::collections::{HashMap, HashSet};

impl EvidentRuntime {
    /// Stage 3: snapshot everything currently loaded as "system"
    /// (stdlib/ast.ev, the pass file, etc.). Subsequent `load_*`
    /// calls register schemas/enums as user-side. `encode_program_value`
    /// and `query_with_program` then encode only the user's program,
    /// not the system layer — so a self-hosted pass sees exactly what
    /// the user wrote.
    ///
    /// Idempotent: calling twice replaces the boundary with the
    /// current state. (The earlier snapshot is lost, but in practice
    /// you set the boundary once between system and user loads.)
    pub fn mark_system_loads_complete(&self) {
        let schemas: HashSet<String> = self.schemas.keys().cloned().collect();
        let enums: HashSet<String> = self.enums.by_name.borrow().keys().cloned().collect();
        *self.system_boundary.borrow_mut() = Some(SystemBoundary { schemas, enums });
    }

    /// Return a `Program` view containing only schemas/enums loaded
    /// AFTER `mark_system_loads_complete()` was called. If no
    /// boundary has been drawn, returns the full program (no
    /// filtering — matches existing `encode_program_value` semantics).
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

    /// Encode this runtime's accumulated `Program` as a Z3 Datatype
    /// value matching `stdlib/ast.ev`'s `Program` enum. Caller is
    /// expected to have loaded `stdlib/ast.ev` first; if any AST
    /// enum is missing from the registry, `encode_program` returns
    /// `EnumNotRegistered`.
    ///
    /// Used by `evident dump-ast` and (in Stage 3) by the CLI hooks
    /// that hand a parsed Program to a self-hosted pass as a `given`.
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

    /// Return a clone of the user-side `Program` AST (everything
    /// loaded after `mark_system_loads_complete()`). When the system
    /// boundary hasn't been drawn, returns the full program — same
    /// semantics as `encode_program_value`.
    ///
    /// Used by the reflection world-plugin to build a `Value::Enum`
    /// tree without having to construct Z3 datatype values. Also
    /// useful for any future consumer that wants the raw AST shape
    /// (lints walking the program, custom encoders, etc.).
    pub fn program_ast(&self) -> Program {
        self.user_program()
    }

    /// Stage 5.5 plumbing: like `query_with_program`, but ALSO
    /// injects the user's first claim's body as a `Seq(BodyItem)`
    /// for the named seq variable. Lets a self-hosted pass iterate
    /// over arbitrary-length user programs via `∀ i ∈ {0..#body-1} : …`.
    ///
    /// The user's "first claim" is `user_program().schemas[0]` — the
    /// first user-loaded schema after `mark_system_loads_complete()`.
    /// If the user has no schemas, `body_var` is constrained to
    /// length 0; the pass can detect this via `#body = 0`.
    ///
    /// `program_var` and `body_var` must both be declared in the
    /// pass schema (`program ∈ Program` and `body ∈ Seq(BodyItem)`,
    /// typically). Passes can use either or both — having `body`
    /// makes iteration possible without recursing through the
    /// `BodyItemList` linked-list shape.
    /// Stage 8: like `query_with_program_and_body` but lets the
    /// caller pick which user claim's body to inject. Index is into
    /// `user_program().schemas` (the user-loaded subset). Returns
    /// `None` if `claim_idx` is out of range. Lets the CLI iterate
    /// over every user claim and aggregate per-claim inferences.
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

    /// Variant of `query_with_program_and_nth_claim_body` that skips
    /// the encoded-Program injection. Most iter-style rules
    /// (`iter_types.ev`, `propagation.ev`, `consistency.ev`,
    /// `lint_duplicate_decls.ev`) declare `program ∈ Program` but
    /// never reference it — they only iterate over `body`. Skipping
    /// the encoded-Program assertion eliminates the dominant Z3 cost
    /// (asserting an equality against a deep recursive datatype
    /// value), which on big programs like mario_shader is several
    /// seconds of solver time.
    ///
    /// Returns `Ok(None)` for out-of-range claim_idx, same as the
    /// program+body variant.
    pub fn query_with_nth_claim_body_only(
        &self,
        claim_name: &str,
        body_var: &str,
        claim_idx: usize,
    ) -> Result<Option<QueryResult>, RuntimeError> {
        // Pass an empty Program value as the program injection.
        // Cheap to construct (no recursive walk); the rule's
        // `program ∈ Program` declaration just gets bound to the
        // empty program, which is harmless because the rule never
        // references it.
        let empty_prog = self.encode_empty_program_value()
            .map_err(|e| RuntimeError::Parse(format!("encode empty program: {e}")))?;
        // The "program_var" name doesn't have to match a declared var —
        // if it does, it gets bound to empty; if not, the runtime
        // warns and continues.
        self.evaluate_with_body(claim_name, "program", body_var, claim_idx, empty_prog)
    }

    /// Build a trivial `MakeProgram(SchLNil, EDLNil)` Z3 Datatype
    /// value. Used by `query_with_nth_claim_body_only` to satisfy
    /// the program-var assertion without paying the recursive-walk
    /// cost on the user's full AST.
    pub(super) fn encode_empty_program_value(
        &self,
    ) -> std::result::Result<z3::ast::Datatype<'static>,
                              crate::translate::ast_encoder::EncodeError> {
        let empty = Program::default();
        crate::translate::ast_encoder::encode_program(
            &empty, self.z3_ctx, &self.enums,
        )
    }

    /// Shared helper: evaluate a pass schema with an encoded Program
    /// + the Nth user claim's body injected as a Seq(BodyItem).
    /// Returns `Ok(None)` when `claim_idx` is out of range.
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

    /// Body-only query variant that accepts an extra `given` map for
    /// caller-pinned variables (e.g. `target_idx → 3`). Same cheap
    /// empty-Program injection as `query_with_nth_claim_body_only`.
    /// Used by the desugar pipeline to ask "is body[i] of shape X?"
    /// one index at a time.
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
        // Inject body length as a `given` Int so the literal-int +
        // seq-length pre-passes can pin any `body_len ∈ Nat` /
        // `n = #body` references for quantifier unrolling. The
        // convention: pass `body_len` as the variable name; passes
        // declare it themselves and use it as the upper bound of
        // `∀ i ∈ {0..body_len - 1} : …`.
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
    /// accumulated `Program` injected as a `given` for one of the
    /// pass's variables.
    ///
    /// Concretely: encode the program as a Z3 Datatype value matching
    /// `stdlib/ast.ev`'s `Program` enum, then evaluate `claim_name`
    /// while asserting that the variable named `program_var` (declared
    /// as `program ∈ Program` in the pass) equals that value. Any
    /// other free variables in the pass behave normally — Z3 picks
    /// values that satisfy the pass's constraints.
    ///
    /// Returns `RuntimeError::Encode` if `stdlib/ast.ev` isn't
    /// loaded; `UnknownSchema` if the named claim doesn't exist.
    pub fn query_with_program(
        &self,
        claim_name: &str,
        program_var: &str,
    ) -> Result<QueryResult, RuntimeError> {
        let prog_value = self.encode_program_value()
            .map_err(|e| RuntimeError::Parse(format!("encode failed: {e}")))?;
        self.query_with_program_value(claim_name, program_var, prog_value)
    }

    /// Same as `query_with_program` but takes the encoded `Program`
    /// value directly. Lets callers running many rules over the same
    /// program (like the inference pipeline) encode once and reuse,
    /// avoiding the recursive-AST walk on every rule. Saves ~70-85%
    /// of the per-rule cost on big programs.
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
