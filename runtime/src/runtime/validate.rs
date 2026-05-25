//! Schema validation:
//!   * `enforce_external_only` — non-external schemas can't construct FFI effects
//!   * `register_subclaims` — lift nested subclaim decls to the top-level schemas map

use crate::core::RuntimeError;
use crate::core::ast::{BodyItem, SchemaDecl};
use std::collections::HashMap;

/// Reject non-`external` schemas that try to construct FFI effects
/// (`FFICall` / `LibCall` / `FFIOpen` / `FFILookup`). The rule:
/// only `external` schemas (`external type` / `external claim` /
/// `external fsm`) may produce those effect values. Demos and
/// ordinary library code reach C through the `external claim`
/// wrappers in `packages/` and `stdlib/posix.ev`.
///
/// **Self-hosted (session VALIDATE-recursive).** The recursive Expr-tree
/// walk that used to live here (`find_ffi_call`) is deleted; the whole
/// walk now runs in Evident as the `validate_walk` stack-FSM in
/// `stdlib/passes/validate.ev`. This is a thin adapter that delegates to
/// the cached per-thread engine in
/// [`crate::portable::validate::enforce_external_only`] (which checks only
/// `Constraint` body items — subclaim bodies get their own load-pass
/// check, unchanged) and wraps its `String` diagnostic in the load path's
/// `RuntimeError::Parse`. The wording is byte-identical to the old walk,
/// pinned by `runtime/tests/validate_correctness.rs`.
pub(super) fn enforce_external_only(s: &SchemaDecl) -> Result<(), RuntimeError> {
    crate::portable::validate::enforce_external_only(s).map_err(RuntimeError::Parse)
}

/// Walk a schema body and register any nested `subclaim` declarations
/// into `schemas` (recursively, so a subclaim of a subclaim is also
/// reachable).
pub(super) fn register_subclaims(body: &[BodyItem], schemas: &mut HashMap<String, SchemaDecl>) {
    for item in body {
        if let BodyItem::SubclaimDecl(s) = item {
            schemas.insert(s.name.clone(), s.clone());
            register_subclaims(&s.body, schemas);
        }
    }
}
