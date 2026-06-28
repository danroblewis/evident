//! UNSAT diagnosis — turn "the FSM/claim is UNSAT" into "*these* constraints conflict".
//!
//! A bare UNSAT verdict is useless for debugging: you can't see WHICH of N constraints made
//! the model unsatisfiable. This does a delta-debug over the schema body — weaken one item and
//! re-solve; an item whose removal flips the query to SAT is a member of the conflict. Two ways
//! to weaken, both chosen to avoid the cascade that would otherwise produce false positives:
//!   * a `Constraint` or `ClaimCall` is REMOVED outright (it asserts a relation, nothing else);
//!   * a pinned `Membership` (`x ∈ T (a ↦ …)`) is DE-PINNED to a bare `x ∈ T` — keeping the
//!     variable defined so other constraints referencing `x` don't silently drop and lie SAT.
//!
//! Cost: O(weakenable items) extra slow-path solves, paid ONLY on failure. The happy path
//! never calls in here, so there's zero cost to a program that solves.

use std::collections::HashMap;

use crate::core::ast::{BodyItem, Pins, SchemaDecl};
use crate::core::Value;

use super::EvidentRuntime;

/// Why a claim/FSM query is UNSAT: the body items that participate in the conflict (each one,
/// weakened, makes the rest satisfiable). An empty `conflicts` (with the schema still UNSAT)
/// means no single weakening resolves it — the conflict needs ≥2 items relaxed together, or
/// lives inside a sub-type's own body (which delta-debug at this level can't pinpoint).
pub(crate) struct UnsatDiagnosis {
    pub conflicts: Vec<String>,
}

impl EvidentRuntime {
    /// Diagnose an UNSAT query by delta-debug. `None` when the schema is unknown or actually
    /// satisfiable (nothing to explain). Re-solves on the slow Z3 oracle — only on failure.
    pub(crate) fn diagnose_unsat(
        &self,
        claim_name: &str,
        given: &HashMap<String, Value>,
    ) -> Option<UnsatDiagnosis> {
        let schema = self.schemas.get(claim_name)?;
        if self.schema_sat(schema, given) {
            return None; // not actually UNSAT on the oracle path — nothing to diagnose
        }
        let mut conflicts = Vec::new();
        for (i, item) in schema.body.iter().enumerate() {
            if let Some(reduced) = weaken(schema, i) {
                if self.schema_sat(&reduced, given) {
                    conflicts.push(describe(item));
                }
            }
        }
        Some(UnsatDiagnosis { conflicts })
    }

    /// Is this schema satisfiable, given these pins, on the slow Z3 oracle path? Bypasses the
    /// JIT/cache so the diagnostic always reflects the constraint semantics, not a compiled plan.
    fn schema_sat(&self, schema: &SchemaDecl, given: &HashMap<String, Value>) -> bool {
        crate::encode::evaluate_with_extra_assertions(
            schema,
            given,
            &self.schemas,
            self.z3_ctx,
            &self.datatypes,
            Some(&self.enums),
            2,
            &[],
        )
        .satisfied
    }
}

/// Produce a copy of `schema` with body item `i` WEAKENED, or `None` if the item carries no
/// standalone assertion to relax (a bare declaration, a subclaim definition, a passthrough).
fn weaken(schema: &SchemaDecl, i: usize) -> Option<SchemaDecl> {
    match &schema.body[i] {
        BodyItem::Constraint(_) | BodyItem::ClaimCall { .. } => {
            let mut s = schema.clone();
            s.body.remove(i);
            Some(s)
        }
        BodyItem::Membership { pins, .. } if !matches!(pins, Pins::None) => {
            let mut s = schema.clone();
            if let BodyItem::Membership { pins, .. } = &mut s.body[i] {
                *pins = Pins::None; // de-pin: keep the variable, drop the pin equalities
            }
            Some(s)
        }
        _ => None,
    }
}

/// A human-readable name for a conflicting body item.
fn describe(item: &BodyItem) -> String {
    match item {
        BodyItem::Membership { name, type_name, .. } => {
            format!("the pins on `{name} ∈ {type_name}`")
        }
        other => format!("{other}"),
    }
}
