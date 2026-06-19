mod load;
mod lower;
mod register_enums;
mod query;
mod scheduler_api;
mod union_find;

pub use crate::core::Value;
#[allow(unused_imports)]
pub use crate::core::{QueryResult, RuntimeError};

use crate::core::ast::{Program, SchemaDecl};
use crate::translate::DatatypeRegistry;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use z3::{Config, Context};

pub struct EvidentRuntime {
    pub(super) program: Program,

    pub(super) schemas: HashMap<String, SchemaDecl>,

    pub(super) schema_order: Vec<String>,

    pub(super) z3_ctx: &'static Context,

    pub(super) functionizer: crate::functionize::cranelift::CraneliftFunctionizer,

    pub(super) fn_cache: RefCell<HashMap<(String, Vec<String>),
                              Option<std::rc::Rc<query::ClaimPlan>>>>,

    pub(super) slow_path_cache: RefCell<HashMap<(String, Vec<String>),
                                     std::rc::Rc<crate::core::CompiledModel<'static>>>>,

    pub(super) datatypes: DatatypeRegistry,

    pub(super) enums: crate::core::EnumRegistry,

    pub(super) loaded_files: RefCell<HashSet<PathBuf>>,
}

impl Default for EvidentRuntime { fn default() -> Self { Self::new() } }

impl EvidentRuntime {

    pub fn new() -> Self {
        let cfg = Config::new();
        let ctx: &'static Context = Box::leak(Box::new(Context::new(&cfg)));
        EvidentRuntime {
            program: Program::default(),
            schemas: HashMap::new(),
            schema_order: Vec::new(),
            z3_ctx: ctx,
            functionizer: crate::functionize::cranelift::CraneliftFunctionizer,
            fn_cache: RefCell::new(HashMap::new()),
            slow_path_cache: RefCell::new(HashMap::new()),
            datatypes: RefCell::new(HashMap::new()),
            enums: crate::core::EnumRegistry::new(),
            loaded_files: RefCell::new(HashSet::new()),
        }
    }

    pub fn schema_names(&self) -> impl Iterator<Item = &str> {
        self.schema_order.iter().map(|s| s.as_str())
    }

    pub fn get_schema(&self, name: &str) -> Option<&SchemaDecl> {
        self.schemas.get(name)
    }

    pub fn enums_registry(&self) -> &crate::core::EnumRegistry {
        &self.enums
    }

    pub fn z3_context(&self) -> &'static z3::Context {
        self.z3_ctx
    }

    pub fn datatypes_registry(&self) -> &crate::core::DatatypeRegistry {
        &self.datatypes
    }

    pub fn schemas_map(&self) -> &HashMap<String, SchemaDecl> {
        &self.schemas
    }

    pub fn effect_results_to_value(
        &self,
        items: &[crate::core::ast::EffectResult],
    ) -> crate::core::Value {
        crate::translate::effect_encoder::effect_results_to_value(items)
    }
}
