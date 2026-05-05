//! Top-level API. Mirrors the Python `EvidentRuntime` for the v0.1 subset.

use crate::ast::{BodyItem, Program, SchemaDecl};
use crate::parser;
use crate::translate::{
    build_cache, run_cached, sample_cached_inner, structural_signature,
    CachedSchema, DatatypeRegistry, StructuralSignature,
};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
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
    cache: RefCell<HashMap<String, (CachedSchema<'static>, StructuralSignature)>>,
    /// Counter incremented each time a cached entry is rebuilt due
    /// to a structural-signature mismatch. Useful for debugging
    /// performance issues (e.g. "every step is rebuilding — what
    /// structural given is flipping?") and for testing the
    /// invalidation logic.
    cache_rebuilds: RefCell<u64>,
    /// Lazily-built `Z3 DatatypeSort` per user type referenced as the
    /// element of `Seq(UserType)`. Built on first `declare_var`; entries
    /// are `Box::leak`'d to live for `'static` (consistent with the
    /// leaked Context). Shared across `query`, `query_cached`, and
    /// `sample` so a `Seq(Point)` declared in one schema reuses the
    /// same Datatype if another schema references `Point` again — Z3
    /// would otherwise error on duplicate type names.
    datatypes: DatatypeRegistry,
    /// Canonicalized paths of every file already loaded via `load_file`
    /// (or transitively via `import`). Used for cycle protection so
    /// `A imports B; B imports A` doesn't recurse forever.
    loaded_files: RefCell<HashSet<PathBuf>>,
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
    Io(String),
}

impl std::fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            RuntimeError::Parse(s) => write!(f, "{}", s),
            RuntimeError::UnknownSchema(s) => write!(f, "unknown schema {:?}", s),
            RuntimeError::Io(s) => write!(f, "{}", s),
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
            cache_rebuilds: RefCell::new(0),
            datatypes: RefCell::new(HashMap::new()),
            loaded_files: RefCell::new(HashSet::new()),
        }
    }

    /// Number of cache rebuilds triggered by structural-signature
    /// mismatches since this runtime was created. Mostly useful for
    /// tests verifying that a change to a non-structural given does
    /// NOT rebuild, and that a change to a structural given DOES.
    /// Also useful as a perf debugging knob — if this counter climbs
    /// every step, you have an unintended structural dependency.
    pub fn cache_rebuilds(&self) -> u64 { *self.cache_rebuilds.borrow() }

    /// Parse and load Evident source. Multiple calls accumulate.
    /// Subclaims (defined inside another claim's body) are also lifted
    /// into the runtime's schemas table so other claims can reference
    /// them by name — same convention as the Python runtime.
    ///
    /// `import "path"` statements are resolved relative to (1) the
    /// path verbatim, then (2) the current working directory. To get
    /// (3) "relative to the file being loaded" resolution, use
    /// `load_file` instead — it tracks the source path and threads it
    /// through.
    pub fn load_source(&mut self, src: &str) -> Result<(), RuntimeError> {
        self.load_source_with_base(src, None)
    }

    /// Load Evident source from a file. Records the file's canonical
    /// path so subsequent `import` statements can resolve relative to
    /// it (and so cycle protection sees the file as already loaded).
    pub fn load_file(&mut self, path: &Path) -> Result<(), RuntimeError> {
        let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        if !self.loaded_files.borrow_mut().insert(canonical.clone()) {
            // Already loaded — cycle / duplicate import. No-op.
            return Ok(());
        }
        let src = std::fs::read_to_string(path)
            .map_err(|e| RuntimeError::Io(format!("read {}: {e}", path.display())))?;
        self.load_source_with_base(&src, Some(&canonical))
    }

    /// Internal entry point that knows the "current file" so it can
    /// resolve relative imports. `base` is None when loading a raw
    /// source string; `Some(path)` when loading from a file.
    fn load_source_with_base(&mut self, src: &str, base: Option<&Path>) -> Result<(), RuntimeError> {
        let prog = parser::parse(src).map_err(|e| RuntimeError::Parse(e.to_string()))?;
        // Process imports first so referenced types/claims exist when
        // the importing file's schemas are registered. This ordering
        // doesn't strictly matter for the runtime (schemas resolve
        // lazily by name) but it matches the textual reading order of
        // the file.
        for import_path in &prog.imports {
            // Known-stdlib paths whose types are already provided by the
            // embedded stdlibs we auto-load in `cmd_execute` (Stdin/Stdout
            // via `executor::load_io_stdlib`, SDLInput/SDLOutput/etc. via
            // `plugins::sdl::STDLIB_SDL_EV`). Silently no-op these so
            // programs that import them — which is the convention even
            // though our embedded versions cover the same ground — don't
            // fail just because we don't ship the .ev files at the
            // expected path. Users who DO ship a real `stdlib/sdl.ev`
            // alongside their program (via cwd) will still hit it via
            // verbatim resolution above.
            const STDLIB_SHIMS: &[&str] = &[
                "stdlib/sdl.ev",
                "stdlib/io.ev",
            ];
            if STDLIB_SHIMS.contains(&import_path.as_str()) {
                // Try a real resolution first; only no-op if it fails.
                if self.resolve_import(import_path, base).is_err() {
                    continue;
                }
            }
            let resolved = self.resolve_import(import_path, base)?;
            self.load_file(&resolved)?;
        }
        for s in &prog.schemas {
            self.schemas.insert(s.name.clone(), s.clone());
            register_subclaims(&s.body, &mut self.schemas);
        }
        self.program.schemas.extend(prog.schemas);
        // Loading new schemas invalidates the cache: new schemas might
        // be referenced by ClaimCall / passthrough in old ones.
        self.cache.borrow_mut().clear();
        // Datatype registry entries reference the previous schema body
        // shape (field order / types). A new load could redefine a type
        // with a different shape; flush so we rebuild on first reference.
        // (The leaked DatatypeSorts themselves stay alive forever, so
        // re-declaring the same name in Z3 will fail — but we have no
        // tests that re-load with a redefined type, so leaving the leak
        // intentional. PROGRESS.md's gotchas section flags this.)
        self.datatypes.borrow_mut().clear();
        Ok(())
    }

    /// Resolve an `import "path"` reference. Tries, in order:
    ///   1. The path verbatim (absolute, or relative to the process
    ///      working directory).
    ///   2. Relative to the file currently being loaded (if any).
    ///   3. Relative to the current working directory (explicitly).
    ///
    /// Returns the first existing path, or an Io error if none match.
    fn resolve_import(&self, import_path: &str, base: Option<&Path>) -> Result<PathBuf, RuntimeError> {
        let p = Path::new(import_path);
        // (1) verbatim
        if p.exists() {
            return Ok(p.to_path_buf());
        }
        // (2) relative to base file's directory
        if let Some(base) = base {
            if let Some(dir) = base.parent() {
                let candidate = dir.join(p);
                if candidate.exists() {
                    return Ok(candidate);
                }
            }
        }
        // (3) relative to current working directory (already covered by
        // (1) for non-absolute paths, but be explicit in case the cwd
        // differs from where the binary was invoked).
        if let Ok(cwd) = std::env::current_dir() {
            let candidate = cwd.join(p);
            if candidate.exists() {
                return Ok(candidate);
            }
        }
        // (4) project-root-relative: programs/sdl_demo/scatter.ev imports
        // "programs/sdl_demo/game_engine.ev" — that's relative to the
        // project root, not the source file. Walk upward from the source
        // file's directory (capped at 10 levels) and try the import path
        // at each ancestor. This also handles `import "stdlib/sdl.ev"`
        // and similar root-anchored shims when the cwd is somewhere else.
        if let Some(base) = base {
            let mut anc = base.parent();
            for _ in 0..10 {
                let Some(dir) = anc else { break };
                let candidate = dir.join(p);
                if candidate.exists() {
                    return Ok(candidate);
                }
                anc = dir.parent();
            }
        }
        Err(RuntimeError::Io(format!(
            "import not found: {:?} (tried verbatim, relative to source file, cwd, and ancestors of the source file)",
            import_path)))
    }

    /// Evaluate the named schema and return whether it's satisfiable
    /// plus a model. `given` pre-binds variables to concrete values
    /// (mirrors the Python `query(schema, given=...)` parameter).
    pub fn query(&self, name: &str, given: &HashMap<String, Value>) -> Result<QueryResult, RuntimeError> {
        let schema = self.schemas.get(name)
            .ok_or_else(|| RuntimeError::UnknownSchema(name.to_string()))?;
        let r = crate::translate::evaluate(schema, given, &self.schemas, self.z3_ctx, &self.datatypes);
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

    /// Look up a loaded schema by name. Used by the executor (and other
    /// tooling) to inspect the body of `main` for variable declarations,
    /// passthroughs, and state pairs.
    pub fn get_schema(&self, name: &str) -> Option<&SchemaDecl> {
        self.schemas.get(name)
    }

    /// Faster query — translates the schema once on first call and
    /// reuses the resulting Z3 solver across subsequent calls
    /// (push/pop per query). Mirrors Python's `query(name, given,
    /// cached=True)` and the `evaluate_cached` optimization.
    ///
    /// **Structural-signature invalidation.** The cache stores the
    /// subset of the previous `given` keyed on names that appear in
    /// quantifier bounds — the structural signature. If this query's
    /// signature differs (e.g. a config value that drives an unroll
    /// count just changed), the cache is dropped and rebuilt against
    /// the new given. Non-structural changes (player position, etc.)
    /// reuse the cache and just re-assert the new value per-query.
    ///
    /// Bindings, satisfaction result, and overall semantics are
    /// identical to `query()`. Faster when called many times against
    /// the same schema with mostly-stable structural givens (e.g. an
    /// executor stepping a state machine 60×/sec where lengths and
    /// bound names don't change).
    pub fn query_cached(&self, name: &str, given: &HashMap<String, Value>)
        -> Result<QueryResult, RuntimeError>
    {
        let schema = self.schemas.get(name)
            .ok_or_else(|| RuntimeError::UnknownSchema(name.to_string()))?
            .clone();   // cheap: SchemaDecl is small + Arc-friendly clones
        let cur_sig = structural_signature(&schema.body, given);
        let mut cache = self.cache.borrow_mut();
        let needs_rebuild = match cache.get(name) {
            Some((_, cached_sig)) => cached_sig != &cur_sig,
            None => true,
        };
        if needs_rebuild {
            if cache.contains_key(name) {
                *self.cache_rebuilds.borrow_mut() += 1;
            }
            // Build with the full given so the structural values get
            // folded as PinnedInts / literal seq lengths and unrolls
            // fire correctly. Non-structural pinned values also get
            // folded — this is fine when the same `given` is passed
            // to the immediately-following `run_cached`, since the
            // PinnedInt arm matches and is a no-op.
            //
            // Subsequent queries that change ONLY non-structural
            // values would hit PinnedInt-mismatch UNSAT — to dodge
            // that, the runtime could pass a stripped-given to
            // build_cache (only structural keys). But that means
            // re-walking pinned ints and is more code; the cleaner
            // version is to filter `given` here. Doing that:
            let names = crate::translate::structural_names(&schema.body);
            let structural_given: HashMap<String, Value> = given.iter()
                .filter(|(k, _)| names.contains(k.as_str()))
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
            let new_cached = build_cache(
                &schema, &self.schemas, self.z3_ctx, &self.datatypes,
                &structural_given);
            cache.insert(name.to_string(), (new_cached, cur_sig));
        }
        let entry = cache.get(name).unwrap();
        let r = run_cached(&entry.0, given, self.z3_ctx);
        Ok(QueryResult { satisfied: r.satisfied, bindings: r.bindings })
    }

    /// Return up to `n` distinct satisfying models. Uses the cached
    /// solver: one push for the per-query givens, then accumulating
    /// blocking clauses (¬(b1=v1 ∧ … ∧ bn=vn) for each scalar binding)
    /// across iterations until either `n` distinct models or UNSAT.
    /// All blocking clauses + givens are popped before returning so the
    /// cached solver is unchanged from the caller's perspective.
    ///
    /// Limitation (v1): blocking only covers Bool, Int, Str bindings.
    /// Seq/Set values are skipped from the blocking conjunction, so
    /// schemas whose only varying outputs are sequences will return
    /// duplicates. See `sample_cached_inner` in translate.rs.
    pub fn sample(&self, name: &str, given: &HashMap<String, Value>, n: usize)
        -> Result<Vec<HashMap<String, Value>>, RuntimeError>
    {
        let schema = self.schemas.get(name)
            .ok_or_else(|| RuntimeError::UnknownSchema(name.to_string()))?
            .clone();
        let cur_sig = structural_signature(&schema.body, given);
        let mut cache = self.cache.borrow_mut();
        let needs_rebuild = match cache.get(name) {
            Some((_, cached_sig)) => cached_sig != &cur_sig,
            None => true,
        };
        if needs_rebuild {
            if cache.contains_key(name) {
                *self.cache_rebuilds.borrow_mut() += 1;
            }
            let names = crate::translate::structural_names(&schema.body);
            let structural_given: HashMap<String, Value> = given.iter()
                .filter(|(k, _)| names.contains(k.as_str()))
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
            let new_cached = build_cache(
                &schema, &self.schemas, self.z3_ctx, &self.datatypes,
                &structural_given);
            cache.insert(name.to_string(), (new_cached, cur_sig));
        }
        let entry = cache.get(name).unwrap();
        Ok(sample_cached_inner(&entry.0, given, n, self.z3_ctx))
    }
}
