//! Top-level API. Mirrors the Python `EvidentRuntime` for the v0.1 subset.
//!
//! ## Public verbs
//!
//! Most callers (commands/, embedders, tests) use the
//! constraint-solver verbs: `load_file` / `load_source` to load
//! programs, `query` / `query_cached` / `sample` to ask whether
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
//! should use `query` / `query_cached`; if you find yourself
//! reaching for one of these methods from elsewhere, reconsider
//! whether the verb you need exists on the constraint-side
//! facade or whether your concern belongs in the execution
//! layer alongside `effect_loop.rs`.

mod stats;
mod lenient;
mod autotune;
mod load;
mod generics;
mod desugar;
mod inject;
mod validate;
mod register_enums;
mod query;
mod sample;
mod scheduler_api;
mod reflection;
mod analysis;
mod introspect;
mod profile;

pub use crate::core::Value;
#[allow(unused_imports)]
pub use crate::core::{QueryResult, RuntimeError};
pub use stats::{FunctionizeStats, PerClaimStats};
pub use desugar::SystemBoundary;
pub use profile::BottleneckEntry;

use crate::core::ast::{Program, SchemaDecl};
use crate::translate::{CachedSchema, DatatypeRegistry, StructuralSignature};
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
    /// Per-schema cache for `query_cached`. RefCell because we want
    /// `query_cached` to take `&self` (so multiple queries can share
    /// the runtime) while the cache mutates on first access.
    ///
    /// Each entry pairs the cached solver+env with the structural
    /// signature it was built with — the subset of the previous
    /// `given` keyed on names that appear in quantifier bounds. On
    /// the next query, if the signature would be different (i.e. a
    /// structural given changed), we drop the entry and rebuild
    /// against the new given. Non-structural givens (e.g. a player
    /// position used in body arithmetic but not as an unroll bound)
    /// don't trigger a rebuild — `run_cached` just asserts the new
    /// value per-query and Z3 solves with the existing constraint
    /// shape.
    pub(super) cache: RefCell<HashMap<String, (CachedSchema<'static>, StructuralSignature)>>,
    /// Z3-AST functionizer cache: per-(claim, given-keys), the
    /// extracted Z3Program (or None if extraction failed). The
    /// extracted program is the input to the Cranelift JIT — when
    /// JIT compilation succeeds, the cached program is what the
    /// JIT ran on (kept here for inspection / re-compile).
    pub(super) functionize_z3_cache: RefCell<HashMap<(String, Vec<String>),
                                          Option<crate::core::Z3Program<'static>>>>,
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
    /// when extraction or JIT compilation refuses but a CachedSchema
    /// has already been built. Reused by `query_with_pins_and_given`
    /// to skip per-tick body translation. Each tick is then just
    /// push → assert given → check → extract → pop, instead of
    /// rebuilding the body assertions from AST every call.
    pub(super) slow_path_cache: RefCell<HashMap<(String, Vec<String>),
                                     std::rc::Rc<crate::core::CachedSchema<'static>>>>,
    /// Cross-tick value cache: per claim, a bounded map from
    /// `hash(given-values)` to the bindings `try_functionize_z3`
    /// produced for those exact inputs. Where `fn_cache` is keyed on
    /// the given KEYS (the compiled plan is generic over input values),
    /// this is keyed on the given VALUES — so an FSM fed byte-identical
    /// inputs across ticks (an idle Mario player) skips the
    /// compiled-function call entirely and returns the prior result.
    /// Cleared on reload alongside the other caches. See
    /// `query::ClaimValueCache`.
    pub(super) value_cache: RefCell<HashMap<String, query::ClaimValueCache>>,
    /// Aggregate stats for the Z3 functionizer + JIT pipeline.
    /// Captures per (claim, given-keys) what was absorbed,
    /// what fell back to Z3, what compiled to native, etc.
    /// See `FunctionizeStats` for the fields. Enable per-call
    /// trace via `EVIDENT_FUNCTIONIZE_STATS=1`; query the
    /// aggregate via `EvidentRuntime::functionize_stats()`.
    pub(super) functionize_stats: RefCell<FunctionizeStats>,
    /// Counter incremented each time a cached entry is rebuilt due
    /// to a structural-signature mismatch. Useful for debugging
    /// performance issues (e.g. "every step is rebuilding — what
    /// structural given is flipping?") and for testing the
    /// invalidation logic.
    pub(super) cache_rebuilds: RefCell<u64>,
    /// Lazily-built `Z3 DatatypeSort` per user type referenced as the
    /// element of `Seq(UserType)`. Built on first `declare_var`; entries
    /// are `Box::leak`'d to live for `'static` (consistent with the
    /// leaked Context). Shared across `query`, `query_cached`, and
    /// `sample` so a `Seq(Point)` declared in one schema reuses the
    /// same Datatype if another schema references `Point` again — Z3
    /// would otherwise error on duplicate type names.
    pub(super) datatypes: DatatypeRegistry,
    /// Z3 datatype + variant info for every `enum` declared in loaded
    /// source. Built eagerly at `load_source_with_base` time (one Z3
    /// `DatatypeBuilder` call per enum, with N nullary variants).
    /// Threaded into the translator alongside `datatypes`.
    pub(super) enums: crate::core::EnumRegistry,
    /// Stage 3: schemas + enums loaded BEFORE
    /// `mark_system_loads_complete()` was called. Used by the AST
    /// encoder to filter so a self-hosted pass receives only the
    /// user's program, not the pass + stdlib + ast.ev itself.
    /// `None` means no boundary has been drawn — every schema/enum
    /// is "user" (the default for non-self-hosting use cases like
    /// `evident query`).
    pub(super) system_boundary: RefCell<Option<SystemBoundary>>,
    /// Per-schema source-file tracking: which file each top-level
    /// schema was directly defined in. Schemas pulled in via
    /// `import` chains get the importer's path. Lets the inference
    /// pipeline restrict iteration to "claims defined in the user's
    /// directly-specified file" rather than every transitively
    /// loaded schema — saves substantial time when the user's file
    /// imports a big helper library (mario_shader.ev → engine.ev's
    /// 20+ helper claims).
    pub(super) schema_origins: RefCell<HashMap<String, PathBuf>>,
    /// Canonicalized paths of every file already loaded via `load_file`
    /// (or transitively via `import`). Used for cycle protection so
    /// `A imports B; B imports A` doesn't recurse forever.
    pub(super) loaded_files: RefCell<HashSet<PathBuf>>,
    /// Per-schema solve-time history + auto-tuner state. Drives the
    /// dynamic `smt.arith.solver` selection. See `SolveHistory` and
    /// `EvidentRuntime::query_cached` for the pricing protocol.
    pub(super) solve_history: RefCell<HashMap<String, autotune::SolveHistory>>,
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
        }
    }

    /// Number of cache rebuilds triggered by structural-signature
    /// mismatches since this runtime was created. Mostly useful for
    /// tests verifying that a change to a non-structural given does
    /// NOT rebuild, and that a change to a structural given DOES.
    /// Also useful as a perf debugging knob — if this counter climbs
    /// every step, you have an unintended structural dependency.
    pub fn cache_rebuilds(&self) -> u64 { *self.cache_rebuilds.borrow() }

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

    /// Snapshot of the Z3 functionizer + JIT statistics. Print
    /// the summary with `stats.print_summary()` or inspect the
    /// per-claim fields directly. See `FunctionizeStats`.
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
        crate::translate::ast_encoder::effect_results_to_value(items)
    }
}
