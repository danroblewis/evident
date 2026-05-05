//! Top-level API. Mirrors the Python `EvidentRuntime` for the v0.1 subset.

use crate::ast::{BodyItem, Program, SchemaDecl};
use crate::parser;
use crate::translate::{build_cache, run_cached, CachedSchema};
use std::cell::RefCell;
use std::collections::HashMap;
use z3::{Config, Context};

pub use crate::translate::Value;

/// Walk a schema body and register any nested `subclaim` declarations
/// into `schemas` (recursively, so a subclaim of a subclaim is also
/// reachable).
fn register_subclaims(body: &[BodyItem], schemas: &mut HashMap<String, SchemaDecl>) {
    for item in body {
        if let BodyItem::SubclaimDecl(s) = item {
            schemas.insert(s.name.clone(), s.clone());
            register_subclaims(&s.body, schemas);
        }
    }
}

pub struct EvidentRuntime {
    program: Program,
    /// Indexed view of program.schemas keyed by name. Mirrors
    /// Python's `EvidentRuntime.schemas`. Used to resolve user-defined
    /// type references during sub-schema expansion.
    schemas: HashMap<String, SchemaDecl>,
    /// Z3 context shared by all cached evaluations from this runtime.
    /// Leaked via Box::leak so its lifetime is `'static`, which lets
    /// us store cached solvers and env entries that borrow from it
    /// without lifetime gymnastics in the public API. The leak is
    /// intentional — one Context per process is fine for a CLI tool
    /// or a test suite. (For long-running embeddings we'd switch to
    /// a Session<'ctx> design — see PROGRESS.md sketch.)
    z3_ctx: &'static Context,
    /// Per-schema cache for `query_cached`. RefCell because we want
    /// `query_cached` to take `&self` (so multiple queries can share
    /// the runtime) while the cache mutates on first access.
    cache: RefCell<HashMap<String, CachedSchema<'static>>>,
}

#[derive(Debug, Clone)]
pub struct QueryResult {
    pub satisfied: bool,
    pub bindings: HashMap<String, Value>,
}

#[derive(Debug)]
pub enum RuntimeError {
    Parse(String),
    UnknownSchema(String),
}

impl std::fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            RuntimeError::Parse(s) => write!(f, "{}", s),
            RuntimeError::UnknownSchema(s) => write!(f, "unknown schema {:?}", s),
        }
    }
}

impl std::error::Error for RuntimeError {}

impl Default for EvidentRuntime { fn default() -> Self { Self::new() } }

impl EvidentRuntime {
    pub fn new() -> Self {
        let cfg = Config::new();
        let ctx: &'static Context = Box::leak(Box::new(Context::new(&cfg)));
        EvidentRuntime {
            program: Program::default(),
            schemas: HashMap::new(),
            z3_ctx: ctx,
            cache: RefCell::new(HashMap::new()),
        }
    }

    /// Parse and load Evident source. Multiple calls accumulate.
    /// Subclaims (defined inside another claim's body) are also lifted
    /// into the runtime's schemas table so other claims can reference
    /// them by name — same convention as the Python runtime.
    pub fn load_source(&mut self, src: &str) -> Result<(), RuntimeError> {
        let prog = parser::parse(src).map_err(|e| RuntimeError::Parse(e.to_string()))?;
        for s in &prog.schemas {
            self.schemas.insert(s.name.clone(), s.clone());
            register_subclaims(&s.body, &mut self.schemas);
        }
        self.program.schemas.extend(prog.schemas);
        // Loading new schemas invalidates the cache: new schemas might
        // be referenced by ClaimCall / passthrough in old ones.
        self.cache.borrow_mut().clear();
        Ok(())
    }

    /// Evaluate the named schema and return whether it's satisfiable
    /// plus a model. `given` pre-binds variables to concrete values
    /// (mirrors the Python `query(schema, given=...)` parameter).
    pub fn query(&self, name: &str, given: &HashMap<String, Value>) -> Result<QueryResult, RuntimeError> {
        let schema = self.schemas.get(name)
            .ok_or_else(|| RuntimeError::UnknownSchema(name.to_string()))?;
        let r = crate::translate::evaluate(schema, given, &self.schemas);
        Ok(QueryResult { satisfied: r.satisfied, bindings: r.bindings })
    }

    /// Convenience: query without any pre-bound values.
    pub fn query_free(&self, name: &str) -> Result<QueryResult, RuntimeError> {
        self.query(name, &HashMap::new())
    }

    /// Iterator over the names of every loaded schema (top-level decls
    /// AND lifted subclaims). Useful for tooling.
    pub fn schema_names(&self) -> impl Iterator<Item = &str> {
        self.schemas.keys().map(|s| s.as_str())
    }

    /// Faster query — translates the schema once on first call and
    /// reuses the resulting Z3 solver across subsequent calls
    /// (push/pop per query). Mirrors Python's `query(name, given,
    /// cached=True)` and the `evaluate_cached` optimization.
    ///
    /// Bindings, satisfaction result, and overall semantics are
    /// identical to `query()`. Faster when called many times against
    /// the same schema with changing `given` values (e.g. an executor
    /// stepping a state machine 60×/sec).
    pub fn query_cached(&self, name: &str, given: &HashMap<String, Value>)
        -> Result<QueryResult, RuntimeError>
    {
        let schema = self.schemas.get(name)
            .ok_or_else(|| RuntimeError::UnknownSchema(name.to_string()))?
            .clone();   // cheap: SchemaDecl is small + Arc-friendly clones
        let mut cache = self.cache.borrow_mut();
        let cached = cache.entry(name.to_string()).or_insert_with(|| {
            build_cache(&schema, &self.schemas, self.z3_ctx)
        });
        let r = run_cached(cached, given, self.z3_ctx);
        Ok(QueryResult { satisfied: r.satisfied, bindings: r.bindings })
    }
}
