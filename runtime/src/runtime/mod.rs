//! Top-level runtime API: load, query, schema introspection, and execution-layer hooks
//! (`query_with_pins_and_given`, `enums_registry`, `z3_context`) for the multi-FSM scheduler.

mod stats;
pub(crate) mod lenient;
mod autotune;
mod load;
pub(crate) mod desugar;
// pub(crate) so portable::inject can reuse inject_lhs_eq_types / inject_claim_arg_types
// (whole-program-table passes that stay in Rust pending Gap D).
pub(crate) mod inject;
mod validate;
mod register_enums;
mod query;
mod sample;
mod scheduler_api;
mod smtlib_reg;
mod reflection;
mod analysis;
mod introspect;
mod nested;

pub use crate::core::Value;
#[allow(unused_imports)]
pub use crate::core::{QueryResult, RuntimeError};
pub use stats::{FunctionizeStats, PerClaimStats};
pub use desugar::SystemBoundary;
pub use nested::take_percolated_effects;

use crate::core::ast::{Program, SchemaDecl};
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
    /// Per-schema solver cache. Entry pairs (CachedSchema, StructuralSignature); rebuilt
    /// only when structural givens (quantifier bounds) change, not on value-only changes.
    pub(super) cache: RefCell<HashMap<String, (CachedSchema<'static>, StructuralSignature)>>,
    /// Per-(claim, given-keys): extracted Z3Program fed to the Cranelift JIT.
    pub(super) functionize_z3_cache: RefCell<HashMap<(String, Vec<String>),
                                          Option<crate::core::Z3Program<'static>>>>,
    /// Pluggable functionizer (default: Cranelift JIT); swap via `with_functionizer`.
    pub(super) functionizer: Box<dyn crate::core::Functionizer>,
    /// Per-(claim, given-keys): compiled plan (per-component JIT + residual Z3 solver),
    /// or None when the claim can't be functionized.
    pub(super) fn_cache: RefCell<HashMap<(String, Vec<String>),
                              Option<std::rc::Rc<query::ClaimPlan>>>>,
    /// Slow-path cache: CachedSchema reused when JIT refuses; avoids per-tick body rebuild.
    pub(super) slow_path_cache: RefCell<HashMap<(String, Vec<String>),
                                     std::rc::Rc<crate::core::CachedSchema<'static>>>>,
    /// Value cache keyed on given VALUES (not keys); skips JIT call on byte-identical inputs.
    pub(super) value_cache: RefCell<HashMap<String, query::ClaimValueCache>>,
    /// Aggregate functionizer + JIT stats; enable trace via `EVIDENT_FUNCTIONIZE_STATS=1`.
    pub(super) functionize_stats: RefCell<FunctionizeStats>,
    /// Count of cache rebuilds from structural-signature mismatches; useful for perf debug.
    pub(super) cache_rebuilds: RefCell<u64>,
    /// Lazily-built DatatypeSort per `Seq(UserType)` element; Box::leak'd to `'static`.
    /// Shared so Z3 doesn't error on duplicate type names across schemas.
    pub(super) datatypes: DatatypeRegistry,
    /// Enum datatype + variant registry; built eagerly at load time via `create_datatypes`.
    pub(super) enums: crate::core::EnumRegistry,
    /// Schemas/enums loaded before `mark_system_loads_complete()`; used to filter
    /// self-hosted pass input to user-only program (None = no boundary, all schemas are "user").
    pub(super) system_boundary: RefCell<Option<SystemBoundary>>,
    /// Origin file per schema; lets the inference pipeline skip transitively-imported claims.
    pub(super) schema_origins: RefCell<HashMap<String, PathBuf>>,
    /// Canonicalized paths of loaded files; cycle protection for `A imports B; B imports A`.
    pub(super) loaded_files: RefCell<HashSet<PathBuf>>,
    /// Per-schema solve-time history; drives dynamic `smt.arith.solver` selection.
    pub(super) solve_history: RefCell<HashMap<String, autotune::SolveHistory>>,
    /// When true, independent slow components are solved on parallel threads.
    /// Off via `EVIDENT_PARALLEL_SLOW=0` or `set_slow_parallel(false)`.
    pub(super) slow_parallel_enabled: std::cell::Cell<bool>,
    /// SMT-LIB-driven FSMs (strategy 2). When a scheduler per-tick solve targets
    /// a claim in this map, `query_with_pins_and_given` routes to the SMT-LIB
    /// path instead of the Evident-AST evaluator. Empty by default — the
    /// Evident-source path is untouched. See `crate::smtlib_fsm`.
    pub(super) smtlib_fsms: RefCell<HashMap<String, crate::smtlib_fsm::SmtLibFsm>>,
}

impl Default for EvidentRuntime { fn default() -> Self { Self::new() } }

impl EvidentRuntime {
    /// Create a runtime with the default functionizer (Cranelift JIT).
    pub fn new() -> Self {
        Self::with_functionizer(crate::functionize::default())
    }

    /// Create a runtime with a specific functionizer strategy.
    pub fn with_functionizer(functionizer: Box<dyn crate::core::Functionizer>) -> Self {
        // Serialized through the global Z3-setup lock: concurrent first-context
        // creation (libtest launching N runtime-building threads) races Z3's
        // global init → abnormal aborts / silently-wrong answers. See z3_ctx.
        let ctx: &'static Context = crate::z3_ctx::leaked_context();
        EvidentRuntime {
            program: Program::default(),
            schemas: HashMap::new(),
            schema_order: Vec::new(),
            z3_ctx: ctx,
            cache: RefCell::new(HashMap::new()),
            functionize_z3_cache: RefCell::new(HashMap::new()),
            functionizer,
            fn_cache: RefCell::new(HashMap::new()),
            slow_path_cache: RefCell::new(HashMap::new()),
            value_cache: RefCell::new(HashMap::new()),
            functionize_stats: RefCell::new(FunctionizeStats::default()),
            cache_rebuilds: RefCell::new(0),
            datatypes: RefCell::new(HashMap::new()),
            enums: crate::core::EnumRegistry::new(),
            system_boundary: RefCell::new(None),
            schema_origins: RefCell::new(HashMap::new()),
            loaded_files: RefCell::new(HashSet::new()),
            solve_history: RefCell::new(HashMap::new()),
            slow_parallel_enabled: std::cell::Cell::new(
                std::env::var("EVIDENT_PARALLEL_SLOW").map(|s| s != "0").unwrap_or(true)),
            smtlib_fsms: RefCell::new(HashMap::new()),
        }
    }

    /// Enable/disable parallel solving of independent slow components (default: on).
    pub fn set_slow_parallel(&self, on: bool) {
        self.slow_parallel_enabled.set(on);
    }

    /// Structural-signature rebuild count; useful for testing cache invalidation behavior.
    pub fn cache_rebuilds(&self) -> u64 { *self.cache_rebuilds.borrow() }

    /// Iterator over names of all loaded schemas (top-level + lifted subclaims).
    pub fn schema_names(&self) -> impl Iterator<Item = &str> {
        self.schema_order.iter().map(|s| s.as_str())
    }

    /// Look up a loaded schema by name.
    pub fn get_schema(&self, name: &str) -> Option<&SchemaDecl> {
        self.schemas.get(name)
    }

    /// Read-only access to the EnumRegistry (for execution-layer DatatypeSort lookups).
    pub fn enums_registry(&self) -> &crate::core::EnumRegistry {
        &self.enums
    }

    /// The `'static` Z3 context; needed by execution-layer callers constructing Datatype pins.
    pub fn z3_context(&self) -> &'static z3::Context {
        self.z3_ctx
    }

    /// Read-only access to the DatatypeRegistry.
    pub fn datatypes_registry(&self) -> &crate::core::DatatypeRegistry {
        &self.datatypes
    }

    /// Read-only access to the loaded schemas map.
    pub fn schemas_map(&self) -> &HashMap<String, SchemaDecl> {
        &self.schemas
    }

    /// Snapshot of the Z3 functionizer + JIT statistics.
    pub fn functionize_stats(&self) -> FunctionizeStats {
        self.functionize_stats.borrow().clone()
    }

    /// Build a `Value::SeqEnum` of `Result` enums for pinning `last_results` in the scheduler.
    pub fn effect_results_to_value(
        &self,
        items: &[crate::core::ast::EffectResult],
    ) -> crate::core::Value {
        crate::translate::ast_encoder::effect_results_to_value(items)
    }
}
