//! Top-level runtime API: load + query.

mod load;
mod inject;
pub(crate) mod desugar;
mod register_enums;
mod generics;
mod query;

pub use crate::core::Value;
#[allow(unused_imports)]
pub use crate::core::{QueryResult, RuntimeError};

use crate::core::ast::{BodyItem, Program, SchemaDecl};
use crate::translate::{CachedSchema, DatatypeRegistry, StructuralSignature};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use z3::Context;

pub struct EvidentRuntime {
    pub(super) program: Program,
    /// Name→schema map; insertion order tracked separately in `schema_order`.
    pub(super) schemas: HashMap<String, SchemaDecl>,
    /// Declaration order for `schemas`; HashMap order is nondeterministic.
    pub(super) schema_order: Vec<String>,
    /// `'static` Z3 context (Box::leak); one per process is fine for a CLI tool.
    pub(super) z3_ctx: &'static Context,
    /// Per-schema solver cache.
    pub(super) cache: RefCell<HashMap<String, (CachedSchema<'static>, StructuralSignature)>>,
    /// Lazily-built DatatypeSort per `Seq(UserType)` element.
    pub(super) datatypes: DatatypeRegistry,
    /// Enum datatype + variant registry.
    pub(super) enums: crate::core::EnumRegistry,
    /// Canonicalized paths of loaded files; cycle protection.
    pub(super) loaded_files: RefCell<HashSet<PathBuf>>,
}

impl Default for EvidentRuntime { fn default() -> Self { Self::new() } }

impl EvidentRuntime {
    pub fn new() -> Self {
        let ctx: &'static Context = crate::z3_ctx::leaked_context();
        EvidentRuntime {
            program: Program::default(),
            schemas: HashMap::new(),
            schema_order: Vec::new(),
            z3_ctx: ctx,
            cache: RefCell::new(HashMap::new()),
            datatypes: RefCell::new(HashMap::new()),
            enums: crate::core::EnumRegistry::new(),
            loaded_files: RefCell::new(HashSet::new()),
        }
    }

    /// Iterator over names of all loaded schemas.
    pub fn schema_names(&self) -> impl Iterator<Item = &str> {
        self.schema_order.iter().map(|s| s.as_str())
    }

    pub fn get_schema(&self, name: &str) -> Option<&SchemaDecl> {
        self.schemas.get(name)
    }

    pub fn enums_registry(&self) -> &crate::core::EnumRegistry { &self.enums }
    pub fn z3_context(&self) -> &'static z3::Context { self.z3_ctx }
    pub fn datatypes_registry(&self) -> &crate::core::DatatypeRegistry { &self.datatypes }
    pub fn schemas_map(&self) -> &HashMap<String, SchemaDecl> { &self.schemas }
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
