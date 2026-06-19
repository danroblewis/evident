//! Top-level API. Mirrors the Python `EvidentRuntime` for the v0.1 subset.
//!
//! ## Public verbs
//!
//! Most callers (commands/, embedders, tests) use the
//! constraint-solver verbs: `load_file` / `load_source` to load
//! programs, `query` to ask whether
//! claims are satisfiable, `get_schema` / `schema_names` to
//! introspect what's loaded.
//!
//! ## Execution-layer extension surface
//!
//! A small handful of verbs exist explicitly to support the
//! multi-FSM scheduler (`effect_loop.rs`):
//!
//!   * `query_with_pinned_datatypes` / `query_with_pins_and_given`
//!     — pin enum-valued variables (`state`, `last_results`)
//!     across a query so the scheduler can advance an FSM one
//!     tick under known-state.
//!   * `enums_registry` / `z3_context` — read-only access to the
//!     EnumRegistry and `'static` Z3 Context so the scheduler can
//!     re-encode `state_next` as a Datatype value for the next
//!     tick's pin.
//!   * `effect_results_to_value` — build a `Value::SeqEnum` of
//!     Result enums for pinning `last_results ∈ Seq(Result)` via
//!     the multi-FSM scheduler's `given` map.
//!
//! These methods are part of the facade rather than a separate
//! trait because the per-tick query path needs read access to
//! state (registries, context, schemas, cache) that lives behind
//! `&self` and would otherwise need parallel exposure. They
//! intentionally do NOT widen the constraint-side facade — they
//! expose the read-handles necessary for execution-layer
//! callers, nothing more. Callers outside the execution layer
//! should use `query`; if you find yourself
//! reaching for one of these methods from elsewhere, reconsider
//! whether the verb you need exists on the constraint-side
//! facade or whether your concern belongs in the execution
//! layer alongside `effect_loop.rs`.

mod stats;
mod load;
mod desugar;
mod inject;
mod validate;
mod register_enums;
mod query;
mod scheduler_api;
mod analysis;

pub use crate::core::Value;
#[allow(unused_imports)]
pub use crate::core::{QueryResult, RuntimeError};
pub use stats::{FunctionizeStats, PerClaimStats};

use crate::core::ast::{Program, SchemaDecl};
use crate::translate::DatatypeRegistry;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use z3::{Config, Context};

pub struct EvidentRuntime {
    pub(super) program: Program,
    /// Indexed view of program.schemas keyed by name. Mirrors
    /// Python's `EvidentRuntime.schemas`. Used to resolve user-defined
    /// type references during sub-schema expansion.
    pub(super) schemas: HashMap<String, SchemaDecl>,
    /// Insertion order of `schemas` — used by callers (the multi-FSM
    /// scheduler in particular) that need declaration order rather
    /// than HashMap's nondeterministic key order. New names append;
    /// re-loading an existing name doesn't reorder.
    pub(super) schema_order: Vec<String>,
    /// Z3 context shared by all cached evaluations from this runtime.
    /// Leaked via Box::leak so its lifetime is `'static`, which lets
    /// us store cached solvers and env entries that borrow from it
    /// without lifetime gymnastics in the public API. The leak is
    /// intentional — one Context per process is fine for a CLI tool
    /// or a test suite. (For long-running embeddings we'd switch to
    /// a Session<'ctx> design — see PROGRESS.md sketch.)
    pub(super) z3_ctx: &'static Context,
    /// Functionizer strategy. Compiles extracted `Z3Program`s into
    /// callable artifacts. Default is Cranelift JIT; swap via
    /// `EvidentRuntime::with_functionizer`. See `runtime/src/functionize/`.
    pub(super) functionizer: Box<dyn crate::core::Functionizer>,
    /// Compiled-plan cache: per-(claim, given-keys), the per-component
    /// execution plan (`Some`) or `None` when the claim can't be
    /// functionized at all (translator gap / no constrained outputs).
    /// A plan holds one compiled artifact per JIT-able component plus a
    /// single cached Z3 solver covering the components that refused to
    /// compile — so a single problematic sub-expression no longer
    /// blocks the whole claim. See `query::ClaimPlan`.
    pub(super) fn_cache: RefCell<HashMap<(String, Vec<String>),
                              Option<std::rc::Rc<query::ClaimPlan>>>>,
    /// Slow-path schema cache. Populated by `try_functionize_z3`
    /// when extraction or JIT compilation refuses but a CompiledModel
    /// has already been built. Reused by `query_with_pins_and_given`
    /// to skip per-tick body translation. Each tick is then just
    /// push → assert given → check → extract → pop, instead of
    /// rebuilding the body assertions from AST every call.
    pub(super) slow_path_cache: RefCell<HashMap<(String, Vec<String>),
                                     std::rc::Rc<crate::core::CompiledModel<'static>>>>,
    /// Aggregate stats for the Z3 functionizer + JIT pipeline.
    /// Captures per (claim, given-keys) what was absorbed,
    /// what fell back to Z3, what compiled to native, etc.
    /// See `FunctionizeStats` for the fields. Query the
    /// aggregate via `EvidentRuntime::functionize_stats()`.
    pub(super) functionize_stats: RefCell<FunctionizeStats>,
    /// Lazily-built `Z3 DatatypeSort` per user type referenced as the
    /// element of `Seq(UserType)`. Built on first `declare_var`; entries
    /// are `Box::leak`'d to live for `'static` (consistent with the
    /// leaked Context). Shared across queries so a `Seq(Point)` declared
    /// in one schema reuses the same Datatype if another schema
    /// references `Point` again — Z3 would otherwise error on duplicate
    /// type names.
    pub(super) datatypes: DatatypeRegistry,
    /// Z3 datatype + variant info for every `enum` declared in loaded
    /// source. Built eagerly at `load_source_with_base` time (one Z3
    /// `DatatypeBuilder` call per enum, with N nullary variants).
    /// Threaded into the translator alongside `datatypes`.
    pub(super) enums: crate::core::EnumRegistry,
    /// Canonicalized paths of every file already loaded via `load_file`
    /// (or transitively via `import`). Used for cycle protection so
    /// `A imports B; B imports A` doesn't recurse forever.
    pub(super) loaded_files: RefCell<HashSet<PathBuf>>,
}

impl Default for EvidentRuntime { fn default() -> Self { Self::new() } }

impl EvidentRuntime {
    /// Create a runtime with the default functionizer (Cranelift JIT).
    pub fn new() -> Self {
        Self::with_functionizer(crate::functionize::default())
    }

    /// Create a runtime with a specific functionizer strategy.
    /// See `crate::functionize` for the trait and the bundled
    /// `CraneliftFunctionizer` implementation.
    pub fn with_functionizer(functionizer: Box<dyn crate::core::Functionizer>) -> Self {
        let cfg = Config::new();
        let ctx: &'static Context = Box::leak(Box::new(Context::new(&cfg)));
        EvidentRuntime {
            program: Program::default(),
            schemas: HashMap::new(),
            schema_order: Vec::new(),
            z3_ctx: ctx,
            functionizer,
            fn_cache: RefCell::new(HashMap::new()),
            slow_path_cache: RefCell::new(HashMap::new()),
            functionize_stats: RefCell::new(FunctionizeStats::default()),
            datatypes: RefCell::new(HashMap::new()),
            enums: crate::core::EnumRegistry::new(),
            loaded_files: RefCell::new(HashSet::new()),
        }
    }

    /// Iterator over the names of every loaded schema (top-level decls
    /// AND lifted subclaims). Useful for tooling.
    pub fn schema_names(&self) -> impl Iterator<Item = &str> {
        self.schema_order.iter().map(|s| s.as_str())
    }

    /// Look up a loaded schema by name. Used by the executor (and other
    /// tooling) to inspect the body of `main` for variable declarations,
    /// passthroughs, and state pairs.
    pub fn get_schema(&self, name: &str) -> Option<&SchemaDecl> {
        self.schemas.get(name)
    }

    /// Read-only access to the EnumRegistry. Execution-layer
    /// callers use this to look up DatatypeSorts when re-encoding
    /// values for subsequent solves — see the "execution-layer
    /// extension surface" section in the module docs.
    pub fn enums_registry(&self) -> &crate::core::EnumRegistry {
        &self.enums
    }

    /// The `'static` Z3 context this runtime allocates against.
    /// Execution-layer callers need this when constructing
    /// Datatype values (e.g. an enum constructor application)
    /// for subsequent pins — see the "execution-layer extension
    /// surface" section in the module docs.
    pub fn z3_context(&self) -> &'static z3::Context {
        self.z3_ctx
    }

    /// Read-only access to the DatatypeRegistry. Used by the
    /// Z3-AST functionizer pipeline to build cached schemas.
    pub fn datatypes_registry(&self) -> &crate::core::DatatypeRegistry {
        &self.datatypes
    }

    /// Read-only access to the loaded schemas map.
    pub fn schemas_map(&self) -> &HashMap<String, SchemaDecl> {
        &self.schemas
    }

    /// Snapshot of the Z3 functionizer + JIT statistics. Inspect
    /// the per-claim fields directly. See `FunctionizeStats`.
    pub fn functionize_stats(&self) -> FunctionizeStats {
        self.functionize_stats.borrow().clone()
    }

    /// Build a `Value::SeqEnum` of `Result` enums. Used by the
    /// multi-FSM scheduler to pin `last_results ∈ Seq(Result)`
    /// via the `given` map (`assert_seq_given` handles the
    /// `(DatatypeSeqVar, SeqEnum)` pair).
    pub fn effect_results_to_value(
        &self,
        items: &[crate::core::ast::EffectResult],
    ) -> crate::core::Value {
        crate::translate::effect_encoder::effect_results_to_value(items)
    }
}
