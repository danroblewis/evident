//! Building Z3 Datatypes for user types referenced as `Seq(UserType)`
//! elements. The result is cached in the shared `DatatypeRegistry` so
//! siblings using the same nested type (e.g. `SDLRect.color` and
//! `SDLOutput.bg` both pointing at `Color`) share one Z3 sort.

use std::collections::HashMap;
use z3::{Context, DatatypeAccessor, DatatypeBuilder, DatatypeSort, Sort};

use crate::ast::*;
use super::types::{DatatypeRegistry, FieldKind};

/// Get or build a Z3 `DatatypeSort` for a user type referenced as the
/// element of `Seq(UserType)`. Walks the type's body for `Membership`
/// items, building a parallel `Vec<FieldKind>` and a list of Z3 sorts
/// suitable for `DatatypeBuilder::variant`.
///
/// Recurses for nested user-type fields: a field declared `c ∈ Color`
/// where Color is itself a struct triggers a nested
/// `get_or_build_datatype` call, and the resulting Datatype's sort
/// becomes the field's `DatatypeAccessor::Sort(...)`. Both the outer
/// and inner Datatypes land in the shared `DatatypeRegistry` so
/// siblings using the same nested type (e.g. SDLRect.color and
/// SDLOutput.bg both pointing at Color) share the same Z3 sort.
///
/// v1 limitation: nested fields can only be other user structs (or
/// the same set of leaf primitives — Int/Nat/Pos/Bool/String). Fields
/// of type `Seq(...)` / `Set(...)` are still rejected with a warning
/// (would need different element-array handling that's out of scope
/// for this slice).
///
/// The returned references have a `'static` lifetime: the runtime
/// already leaks its `Context`, so leaking the per-type `DatatypeSort`
/// (which borrows from that Context) is consistent. See
/// `EvidentRuntime::new` for why the Context is leaked.
pub(super) fn get_or_build_datatype(
    type_name: &str,
    ctx: &'static Context,
    schemas: &HashMap<String, SchemaDecl>,
    registry: &DatatypeRegistry,
) -> Option<(&'static DatatypeSort<'static>, Vec<FieldKind>)> {
    // Cache hit: return the previously-built sort + field list.
    if let Some((dt, fields)) = registry.borrow().get(type_name) {
        return Some((*dt, fields.clone()));
    }
    let schema = schemas.get(type_name)?;

    // First pass: walk the type body and resolve each field to either a
    // primitive sort or a recursively-built nested Datatype. We collect
    // both the FieldKind metadata and the parallel `(name, sort)` list
    // for the DatatypeBuilder.
    let mut fields: Vec<FieldKind> = Vec::new();
    let mut field_sorts: Vec<(String, Sort<'static>)> = Vec::new();
    for item in &schema.body {
        if let BodyItem::Membership { name, type_name: ftype, .. } = item {
            match ftype.as_str() {
                "Int" | "Nat" | "Pos" => {
                    fields.push(FieldKind::Primitive {
                        name: name.clone(),
                        prim_type: ftype.clone(),
                    });
                    field_sorts.push((name.clone(), Sort::int(ctx)));
                }
                "Bool" => {
                    fields.push(FieldKind::Primitive {
                        name: name.clone(),
                        prim_type: ftype.clone(),
                    });
                    field_sorts.push((name.clone(), Sort::bool(ctx)));
                }
                "String" => {
                    fields.push(FieldKind::Primitive {
                        name: name.clone(),
                        prim_type: ftype.clone(),
                    });
                    field_sorts.push((name.clone(), Sort::string(ctx)));
                }
                // Nested: recurse if this name is itself a user type.
                user_type if schemas.contains_key(user_type) => {
                    let Some((nested_dt, sub_fields)) =
                        get_or_build_datatype(user_type, ctx, schemas, registry)
                    else {
                        // Inner build failed (warning already logged); abort the
                        // outer build too — we can't include a partial Datatype.
                        return None;
                    };
                    field_sorts.push((name.clone(), nested_dt.sort.clone()));
                    fields.push(FieldKind::Nested {
                        name: name.clone(),
                        type_name: user_type.to_string(),
                        dt: nested_dt,
                        sub_fields,
                    });
                }
                _ => {
                    eprintln!(
                        "warning: unsupported field type {} in Datatype for {}; \
                         only Int/Nat/Pos/Bool/String and other user struct types \
                         are supported in Seq(UserType) elements (v1)",
                        ftype, type_name
                    );
                    return None;
                }
            }
        }
        // Other body items (constraints, passthroughs) don't shape the
        // record. Field invariants from the type body are *not* asserted
        // on Seq elements in v1 — that would require a ∀ i quantifier
        // and is left to a follow-up.
    }
    if fields.is_empty() {
        eprintln!("warning: type {} has no fields; can't build Datatype", type_name);
        return None;
    }

    let ctor_name = format!("mk_{}", type_name);
    let field_refs: Vec<(&str, DatatypeAccessor<'static>)> = field_sorts
        .iter()
        .map(|(n, s)| (n.as_str(), DatatypeAccessor::Sort(s.clone())))
        .collect();
    let dt: DatatypeSort<'static> = DatatypeBuilder::new(ctx, type_name)
        .variant(&ctor_name, field_refs)
        .finish();
    let leaked: &'static DatatypeSort<'static> = Box::leak(Box::new(dt));
    registry.borrow_mut().insert(type_name.to_string(), (leaked, fields.clone()));
    Some((leaked, fields))
}
