//! Schema validation: `enforce_external_only` + `register_subclaims`.

use crate::core::RuntimeError;
use crate::core::ast::{BodyItem, SchemaDecl};
use std::collections::HashMap;

/// Reject non-`external` schemas that construct FFI effects (`FFICall`/`LibCall`/`FFIOpen`/`FFILookup`).
/// Walk is self-hosted via `stdlib/passes/validate.ev`; this is a thin adapter wrapping its diagnostic.
pub(super) fn enforce_external_only(s: &SchemaDecl) -> Result<(), RuntimeError> {
    crate::portable::validate::enforce_external_only(s).map_err(RuntimeError::Parse)
}

/// Recursively register nested `subclaim` declarations into `schemas`.
pub(super) fn register_subclaims(body: &[BodyItem], schemas: &mut HashMap<String, SchemaDecl>) {
    for item in body {
        if let BodyItem::SubclaimDecl(s) = item {
            schemas.insert(s.name.clone(), s.clone());
            register_subclaims(&s.body, schemas);
        }
    }
}
