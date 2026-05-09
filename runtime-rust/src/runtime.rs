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
use std::time::{Duration, Instant};
use z3::{Config, Context};

pub use crate::translate::Value;

/// Snapshot of "everything loaded so far is the system layer."
/// Schemas / enums loaded after a `mark_system_loads_complete()` call
/// are treated as the user's program for the purposes of AST encoding.
#[derive(Default, Clone)]
pub struct SystemBoundary {
    pub schemas: HashSet<String>,
    pub enums:   HashSet<String>,
}

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

/// Batched build of Z3 DatatypeSorts for every enum declared in
/// `decls`, using `z3::datatype_builder::create_datatypes` so that
/// enums can forward-reference each other or be mutually recursive.
///
/// Three kinds of payload-field references are resolved per variant:
///
///   * Primitive (`Int`/`Nat`/`Pos`/`Real`/`Bool`/`String`) →
///     `DatatypeAccessor::Sort(...)`.
///   * Self-reference or forward-reference to another enum *in this
///     batch* → `DatatypeAccessor::Datatype(name)`. The Z3 multi-
///     builder resolves these during `create_datatypes`.
///   * Reference to an enum already registered in a previous load →
///     `DatatypeAccessor::Sort(prev.sort.clone())`.
///
/// Anything else (unknown type name) errors with a user-readable
/// message naming the offending variant + field.
///
/// Variant names are globally unique across all enums; load fails
/// on collision, both within `decls` and against previously-loaded
/// enums in the registry.
fn register_enums(
    decls: &[crate::ast::EnumDecl],
    ctx: &'static Context,
    registry: &crate::translate::EnumRegistry,
) -> Result<(), RuntimeError> {
    use z3::{DatatypeAccessor, DatatypeBuilder, DatatypeSort, Sort};
    if decls.is_empty() { return Ok(()); }

    // Pre-flight checks: variant uniqueness (across this batch and
    // previously-loaded enums), and enum-name uniqueness (same).
    let batch_names: std::collections::HashSet<&str> =
        decls.iter().map(|d| d.name.as_str()).collect();
    {
        // Same-batch enum-name uniqueness: walk decls once and bail on
        // the first repeat. If batch_names.len() != decls.len() then
        // some name collided; locate it for a useful message.
        if batch_names.len() != decls.len() {
            let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::new();
            for d in decls {
                if !seen.insert(d.name.as_str()) {
                    return Err(RuntimeError::Parse(format!(
                        "enum `{}` declared more than once in the same load",
                        d.name)));
                }
            }
        }
        let existing_by_name = registry.by_name.borrow();
        for d in decls {
            if existing_by_name.contains_key(&d.name) {
                return Err(RuntimeError::Parse(format!(
                    "enum `{}` declared more than once", d.name)));
            }
            if d.variants.is_empty() {
                return Err(RuntimeError::Parse(
                    format!("enum {} has no variants", d.name)));
            }
        }
        let by_variant = registry.by_variant.borrow();
        let mut batch_seen: std::collections::HashMap<String, String> =
            std::collections::HashMap::new();
        for d in decls {
            for v in &d.variants {
                if let Some((existing_enum, _)) = by_variant.get(&v.name) {
                    return Err(RuntimeError::Parse(format!(
                        "enum variant `{}` is declared twice — once in `{}` and once in `{}`",
                        v.name, existing_enum, d.name,
                    )));
                }
                if let Some(prev_in_batch) = batch_seen.get(&v.name) {
                    return Err(RuntimeError::Parse(format!(
                        "enum variant `{}` is declared twice — once in `{}` and once in `{}`",
                        v.name, prev_in_batch, d.name,
                    )));
                }
                batch_seen.insert(v.name.clone(), d.name.clone());
            }
        }
    }

    // Build all DatatypeBuilders. Forward refs (incl. self) use
    // `Datatype(name)`; primitives + previously-registered enums use
    // `Sort(...)`. We need owned String for the variant name passed
    // to `builder.variant(&str, ...)` — they're already owned via
    // `decls[i].variants[j].name`, so we can borrow.
    let mut builders: Vec<DatatypeBuilder<'static>> = Vec::with_capacity(decls.len());
    for d in decls {
        let mut builder = DatatypeBuilder::new(ctx, d.name.as_str());
        for v in &d.variants {
            let mut accessors: Vec<(&str, DatatypeAccessor)> = Vec::new();
            for f in &v.fields {
                let acc = match f.type_name.as_str() {
                    "Int" | "Nat" | "Pos" =>
                        DatatypeAccessor::Sort(Sort::int(ctx)),
                    "Bool"   => DatatypeAccessor::Sort(Sort::bool(ctx)),
                    "Real"   => DatatypeAccessor::Sort(Sort::real(ctx)),
                    "String" => DatatypeAccessor::Sort(Sort::string(ctx)),
                    other if batch_names.contains(other) => {
                        // Self-reference or forward-reference to
                        // another enum in this same batch — Z3 resolves
                        // it during `create_datatypes`.
                        DatatypeAccessor::Datatype(other.into())
                    }
                    other => {
                        // Reference to an enum loaded previously (not
                        // in this batch). Resolve to a concrete sort.
                        if let Some((prev, _)) = registry.by_name.borrow().get(other) {
                            DatatypeAccessor::Sort(prev.sort.clone())
                        } else {
                            return Err(RuntimeError::Parse(format!(
                                "unknown payload type `{}` in variant `{}::{}` \
                                 (must be a primitive or a declared enum)",
                                other, d.name, v.name,
                            )));
                        }
                    }
                };
                accessors.push((f.name.as_str(), acc));
            }
            builder = builder.variant(v.name.as_str(), accessors);
        }
        builders.push(builder);
    }

    let sorts: Vec<DatatypeSort<'static>> =
        z3::datatype_builder::create_datatypes(builders);
    assert_eq!(sorts.len(), decls.len());

    // Stash each built sort + its variant decl list in the registry.
    {
        let mut by_name = registry.by_name.borrow_mut();
        let mut by_variant = registry.by_variant.borrow_mut();
        for (d, dt) in decls.iter().zip(sorts.into_iter()) {
            let leaked: &'static DatatypeSort<'static> = Box::leak(Box::new(dt));
            by_name.insert(d.name.clone(), (leaked, d.variants.clone()));
            for (idx, v) in d.variants.iter().enumerate() {
                by_variant.insert(v.name.clone(), (d.name.clone(), idx));
            }
        }
    }
    Ok(())
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
    /// Z3 datatype + variant info for every `enum` declared in loaded
    /// source. Built eagerly at `load_source_with_base` time (one Z3
    /// `DatatypeBuilder` call per enum, with N nullary variants).
    /// Threaded into the translator alongside `datatypes`.
    enums: crate::translate::EnumRegistry,
    /// Stage 3: schemas + enums loaded BEFORE
    /// `mark_system_loads_complete()` was called. Used by the AST
    /// encoder to filter so a self-hosted pass receives only the
    /// user's program, not the pass + stdlib + ast.ev itself.
    /// `None` means no boundary has been drawn — every schema/enum
    /// is "user" (the default for non-self-hosting use cases like
    /// `evident query`).
    system_boundary: RefCell<Option<SystemBoundary>>,
    /// Canonicalized paths of every file already loaded via `load_file`
    /// (or transitively via `import`). Used for cycle protection so
    /// `A imports B; B imports A` doesn't recurse forever.
    loaded_files: RefCell<HashSet<PathBuf>>,
    /// Per-schema solve-time history + auto-tuner state. Drives the
    /// dynamic `smt.arith.solver` selection. See `SolveHistory` and
    /// `EvidentRuntime::query_cached` for the pricing protocol.
    solve_history: RefCell<HashMap<String, SolveHistory>>,
}

/// Candidate `smt.arith.solver` values the runtime will try when it
/// hasn't yet committed to one. 2 is the older Simplex-based path that
/// wins on Z3 4.8.12 for our workload; 6 is the newer default that
/// wins for newer Z3 versions and on different schemas. The auto-tuner
/// runs each one for a window of frames and locks in the faster one.
///
/// Add another value here (e.g. `12` if Z3 ever ships a useful new one)
/// and pricing will pick it up automatically.
const ARITH_SOLVER_CANDIDATES: &[u32] = &[2, 6];

/// Number of frames each candidate is timed under during pricing.
/// Long enough to swamp Z3's per-build overhead with steady-state
/// per-frame cost; short enough that pricing finishes well within
/// the warmup window of typical executor sessions.
const PRICING_FRAMES_PER_CANDIDATE: u32 = 30;

/// Per-schema history. Drives the auto-tuner. The state machine:
///
///   Pricing { idx } — currently timing candidate ARITH_SOLVER_CANDIDATES[idx].
///                     After PRICING_FRAMES_PER_CANDIDATE frames the runtime
///                     advances `idx` (rebuilding the cache under the next
///                     candidate). After all candidates are timed, transitions
///                     to Locked under the fastest config seen.
///   Locked { config } — pricing complete. All future queries use this config.
///
/// `EVIDENT_Z3_AUTOTUNE=0` skips pricing entirely and locks immediately
/// to the env-specified `EVIDENT_Z3_ARITH_SOLVER` value (default 2).
struct SolveHistory {
    state: TunerState,
    /// Mean ms/iter observed for each candidate fully priced. Used to
    /// pick the winner when pricing completes.
    measured: HashMap<u32, f64>,
    /// Solve times for the *current* candidate's pricing window. Cleared
    /// every time we advance to the next candidate.
    current_window: Vec<Duration>,
}

#[derive(Debug, Clone, Copy)]
enum TunerState {
    /// Currently timing `ARITH_SOLVER_CANDIDATES[idx]`.
    Pricing { idx: usize },
    /// Pricing complete; this is the winner.
    Locked { config: u32 },
}

impl SolveHistory {
    /// Initial state. If autotune is disabled, lock immediately to the
    /// env-specified config (default 2). Otherwise start pricing with
    /// the first candidate.
    fn new() -> Self {
        let autotune = std::env::var("EVIDENT_Z3_AUTOTUNE").as_deref() != Ok("0");
        if !autotune {
            let initial: u32 = std::env::var("EVIDENT_Z3_ARITH_SOLVER").ok()
                .and_then(|s| s.parse().ok()).unwrap_or(2);
            return SolveHistory {
                state: TunerState::Locked { config: initial },
                measured: HashMap::new(),
                current_window: Vec::new(),
            };
        }
        SolveHistory {
            state: TunerState::Pricing { idx: 0 },
            measured: HashMap::new(),
            current_window: Vec::with_capacity(PRICING_FRAMES_PER_CANDIDATE as usize),
        }
    }

    /// The `arith_solver` value the cache should be built under right now.
    fn current_config(&self) -> u32 {
        match self.state {
            TunerState::Pricing { idx }     => ARITH_SOLVER_CANDIDATES[idx],
            TunerState::Locked  { config }  => config,
        }
    }

    /// Record a solve time. Returns `Some(new_config)` if the tuner
    /// decided to swap configs (caller should evict the cache so the
    /// next query rebuilds under the new value), `None` otherwise.
    fn record(&mut self, dt: Duration) -> Option<u32> {
        let TunerState::Pricing { idx } = self.state else { return None; };

        self.current_window.push(dt);
        if self.current_window.len() < PRICING_FRAMES_PER_CANDIDATE as usize {
            return None;
        }

        // Window full — finalize this candidate's measurement.
        let total_ms: f64 = self.current_window.iter()
            .map(|d| d.as_secs_f64() * 1000.0).sum();
        let mean_ms = total_ms / self.current_window.len() as f64;
        let cfg = ARITH_SOLVER_CANDIDATES[idx];
        self.measured.insert(cfg, mean_ms);
        self.current_window.clear();

        let next_idx = idx + 1;
        if next_idx < ARITH_SOLVER_CANDIDATES.len() {
            // More candidates to price.
            self.state = TunerState::Pricing { idx: next_idx };
            let next_cfg = ARITH_SOLVER_CANDIDATES[next_idx];
            if std::env::var("EVIDENT_Z3_AUTOTUNE_LOG").as_deref() == Ok("1") {
                eprintln!("[autotune] arith.solver={cfg} → {mean_ms:.2} ms/iter; \
                           probing arith.solver={next_cfg} next");
            }
            Some(next_cfg)
        } else {
            // All candidates priced. Pick the fastest.
            let winner = self.measured.iter()
                .min_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
                .map(|(c, _)| *c)
                .unwrap_or(2);
            self.state = TunerState::Locked { config: winner };
            if std::env::var("EVIDENT_Z3_AUTOTUNE_LOG").as_deref() == Ok("1") {
                eprintln!("[autotune] pricing complete: {:?}; locking arith.solver={winner}",
                          self.measured);
            }
            // Return Some only if we need to rebuild cache (i.e. we
            // were timing a different config than the winner).
            if winner != cfg { Some(winner) } else { None }
        }
    }
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
            enums: crate::translate::EnumRegistry::new(),
            system_boundary: RefCell::new(None),
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
        // Build all Z3 DatatypeSorts for this batch of enums together
        // via `create_datatypes`. Lets enums forward-reference each
        // other (`Expr` referring to `BinOp` declared later in the
        // file) and be mutually recursive (`A` referring to `B` and
        // vice versa). Variant names must be globally unique across
        // all enums; load fails on collision.
        register_enums(&prog.enums, self.z3_ctx, &self.enums)?;
        self.program.schemas.extend(prog.schemas);
        self.program.traces.extend(prog.traces);
        self.program.shaders.extend(prog.shaders);
        self.program.enums.extend(prog.enums);
        // Loading new schemas invalidates the cache: new schemas might
        // be referenced by ClaimCall / passthrough in old ones. Also
        // reset the auto-tuner — measurements taken under the old
        // schema body don't apply to the new one.
        self.cache.borrow_mut().clear();
        self.solve_history.borrow_mut().clear();
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
        // One-shot query: don't auto-tune (no chance to learn over many
        // calls). Use the env override if set, default 2 (the value
        // that wins on Z3 4.8.12 for our typical workload).
        let arith: u32 = std::env::var("EVIDENT_Z3_ARITH_SOLVER").ok()
            .and_then(|s| s.parse().ok()).unwrap_or(2);
        let r = crate::translate::evaluate(schema, given, &self.schemas, self.z3_ctx, &self.datatypes, Some(&self.enums), arith);
        Ok(QueryResult { satisfied: r.satisfied, bindings: r.bindings })
    }

    /// Like `query`, but on UNSAT also returns the unsat-core: indices
    /// into the schema's `body` for the constraints Z3 identified as
    /// the conflicting subset. Used by `evident test` to highlight
    /// which assertions made a `sat_*` test fail. Givens are not
    /// tracked — the core only includes schema body items.
    pub fn query_with_core(&self, name: &str, given: &HashMap<String, Value>)
        -> Result<(QueryResult, Option<Vec<usize>>), RuntimeError>
    {
        let schema = self.schemas.get(name)
            .ok_or_else(|| RuntimeError::UnknownSchema(name.to_string()))?;
        let arith: u32 = std::env::var("EVIDENT_Z3_ARITH_SOLVER").ok()
            .and_then(|s| s.parse().ok()).unwrap_or(2);
        let r = crate::translate::evaluate_with_core(schema, given, &self.schemas, self.z3_ctx, &self.datatypes, Some(&self.enums), arith);
        let qr = QueryResult { satisfied: r.satisfied, bindings: r.bindings };
        Ok((qr, r.unsat_core_items))
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

    /// Trace declarations parsed from this runtime's loaded files.
    /// Used by `evident test` to drive step-by-step program execution
    /// and check assertions per send line.
    pub fn traces(&self) -> &[crate::ast::TraceDecl] {
        &self.program.traces
    }

    /// Shader declarations parsed from this runtime's loaded files.
    /// Used by `evident transpile-shader` and the future
    /// `SDLShaderPlugin` to look up a shader by name and emit GLSL.
    pub fn shaders(&self) -> &[crate::ast::ShaderDecl] {
        &self.program.shaders
    }

    /// Look up a loaded schema by name. Used by the executor (and other
    /// tooling) to inspect the body of `main` for variable declarations,
    /// passthroughs, and state pairs.
    pub fn get_schema(&self, name: &str) -> Option<&SchemaDecl> {
        self.schemas.get(name)
    }

    /// Inject a `Membership` body item at the head of the named claim.
    /// Used by the `--infer-types` flag pipeline: after running the
    /// self-hosted inference passes against a separate runtime, the
    /// query path calls this to graft the inferred declarations onto
    /// the user's claims before solving.
    ///
    /// Returns `Ok(true)` if a Membership was added, `Ok(false)` if
    /// the variable was already declared in the claim's body (the
    /// idempotent skip lets callers loop over inferences without
    /// double-checking). `Err(UnknownSchema)` if the named claim
    /// doesn't exist.
    ///
    /// Mutates both `self.schemas` (the lookup table) and
    /// `self.program.schemas` (the parsed Program — for encoder
    /// consistency on subsequent calls). Clears the cache so a
    /// re-query rebuilds with the new shape.
    pub fn add_membership_to_claim(
        &mut self,
        claim_name: &str,
        var_name: &str,
        type_name: &str,
    ) -> Result<bool, RuntimeError> {
        use crate::ast::{BodyItem, Pins};
        let already_declared = |body: &[BodyItem]| -> bool {
            body.iter().any(|i| matches!(
                i, BodyItem::Membership { name, .. } if name == var_name
            ))
        };
        let new_item = BodyItem::Membership {
            name: var_name.to_string(),
            type_name: type_name.to_string(),
            pins: Pins::None,
        };
        // Update the lookup table.
        let schema = self.schemas.get_mut(claim_name)
            .ok_or_else(|| RuntimeError::UnknownSchema(claim_name.to_string()))?;
        if already_declared(&schema.body) {
            return Ok(false);
        }
        schema.body.insert(0, new_item.clone());
        // Mirror in self.program.schemas so the encoder sees the same
        // body shape on subsequent queries.
        for s in &mut self.program.schemas {
            if s.name == claim_name && !already_declared(&s.body) {
                s.body.insert(0, new_item.clone());
            }
        }
        // Cached solver still has the old body asserted; flush.
        self.cache.borrow_mut().clear();
        Ok(true)
    }

    /// Stage 3: snapshot everything currently loaded as "system"
    /// (stdlib/ast.ev, the pass file, etc.). Subsequent `load_*`
    /// calls register schemas/enums as user-side. `encode_program_value`
    /// and `query_with_program` then encode only the user's program,
    /// not the system layer — so a self-hosted pass sees exactly what
    /// the user wrote.
    ///
    /// Idempotent: calling twice replaces the boundary with the
    /// current state. (The earlier snapshot is lost, but in practice
    /// you set the boundary once between system and user loads.)
    pub fn mark_system_loads_complete(&self) {
        let schemas: HashSet<String> = self.schemas.keys().cloned().collect();
        let enums: HashSet<String> = self.enums.by_name.borrow().keys().cloned().collect();
        *self.system_boundary.borrow_mut() = Some(SystemBoundary { schemas, enums });
    }

    /// Return a `Program` view containing only schemas/enums loaded
    /// AFTER `mark_system_loads_complete()` was called. If no
    /// boundary has been drawn, returns the full program (no
    /// filtering — matches existing `encode_program_value` semantics).
    fn user_program(&self) -> Program {
        let boundary = self.system_boundary.borrow();
        let Some(b) = boundary.as_ref() else { return self.program.clone() };
        Program {
            schemas: self.program.schemas.iter()
                .filter(|s| !b.schemas.contains(&s.name))
                .cloned().collect(),
            enums: self.program.enums.iter()
                .filter(|e| !b.enums.contains(&e.name))
                .cloned().collect(),
            imports: Vec::new(),
            traces:  Vec::new(),
            shaders: Vec::new(),
        }
    }

    /// Encode this runtime's accumulated `Program` as a Z3 Datatype
    /// value matching `stdlib/ast.ev`'s `Program` enum. Caller is
    /// expected to have loaded `stdlib/ast.ev` first; if any AST
    /// enum is missing from the registry, `encode_program` returns
    /// `EnumNotRegistered`.
    ///
    /// Used by `evident dump-ast` and (in Stage 3) by the CLI hooks
    /// that hand a parsed Program to a self-hosted pass as a `given`.
    pub fn encode_program_value(
        &self,
    ) -> std::result::Result<z3::ast::Datatype<'static>,
                              crate::translate::ast_encoder::EncodeError> {
        let prog = self.user_program();
        crate::translate::ast_encoder::encode_program(
            &prog,
            self.z3_ctx,
            &self.enums,
        )
    }

    /// Stage 5.5 plumbing: like `query_with_program`, but ALSO
    /// injects the user's first claim's body as a `Seq(BodyItem)`
    /// for the named seq variable. Lets a self-hosted pass iterate
    /// over arbitrary-length user programs via `∀ i ∈ {0..#body-1} : …`.
    ///
    /// The user's "first claim" is `user_program().schemas[0]` — the
    /// first user-loaded schema after `mark_system_loads_complete()`.
    /// If the user has no schemas, `body_var` is constrained to
    /// length 0; the pass can detect this via `#body = 0`.
    ///
    /// `program_var` and `body_var` must both be declared in the
    /// pass schema (`program ∈ Program` and `body ∈ Seq(BodyItem)`,
    /// typically). Passes can use either or both — having `body`
    /// makes iteration possible without recursing through the
    /// `BodyItemList` linked-list shape.
    /// Stage 8: like `query_with_program_and_body` but lets the
    /// caller pick which user claim's body to inject. Index is into
    /// `user_program().schemas` (the user-loaded subset). Returns
    /// `None` if `claim_idx` is out of range. Lets the CLI iterate
    /// over every user claim and aggregate per-claim inferences.
    pub fn query_with_program_and_nth_claim_body(
        &self,
        claim_name: &str,
        program_var: &str,
        body_var: &str,
        claim_idx: usize,
    ) -> Result<Option<QueryResult>, RuntimeError> {
        let prog_value = self.encode_program_value()
            .map_err(|e| RuntimeError::Parse(format!("encode failed: {e}")))?;
        self.query_with_program_and_nth_claim_body_value(
            claim_name, program_var, body_var, claim_idx, prog_value,
        )
    }

    /// Variant of `query_with_program_and_nth_claim_body` that skips
    /// the encoded-Program injection. Most iter-style rules
    /// (`iter_types.ev`, `propagation.ev`, `consistency.ev`,
    /// `lint_duplicate_decls.ev`) declare `program ∈ Program` but
    /// never reference it — they only iterate over `body`. Skipping
    /// the encoded-Program assertion eliminates the dominant Z3 cost
    /// (asserting an equality against a deep recursive datatype
    /// value), which on big programs like mario_shader is several
    /// seconds of solver time.
    ///
    /// Returns `Ok(None)` for out-of-range claim_idx, same as the
    /// program+body variant.
    pub fn query_with_nth_claim_body_only(
        &self,
        claim_name: &str,
        body_var: &str,
        claim_idx: usize,
    ) -> Result<Option<QueryResult>, RuntimeError> {
        // Pass an empty Program value as the program injection.
        // Cheap to construct (no recursive walk); the rule's
        // `program ∈ Program` declaration just gets bound to the
        // empty program, which is harmless because the rule never
        // references it.
        let empty_prog = self.encode_empty_program_value()
            .map_err(|e| RuntimeError::Parse(format!("encode empty program: {e}")))?;
        // Reuse the existing implementation with the cheap value.
        // The "program_var" name doesn't have to match a declared var —
        // if it does, it gets bound to empty; if not, the runtime
        // warns and continues.
        self.query_with_program_and_nth_claim_body_value(
            claim_name, "program", body_var, claim_idx, empty_prog,
        )
    }

    /// Build a trivial `MakeProgram(SchLNil, EDLNil)` Z3 Datatype
    /// value. Used by `query_with_nth_claim_body_only` to satisfy
    /// the program-var assertion without paying the recursive-walk
    /// cost on the user's full AST.
    fn encode_empty_program_value(
        &self,
    ) -> std::result::Result<z3::ast::Datatype<'static>,
                              crate::translate::ast_encoder::EncodeError> {
        let empty = Program::default();
        crate::translate::ast_encoder::encode_program(
            &empty, self.z3_ctx, &self.enums,
        )
    }

    /// Same as `query_with_program_and_nth_claim_body` but takes the
    /// encoded `Program` value directly. Pair with
    /// `query_with_program_value` for the inference-pipeline use case
    /// where one encoded value feeds many rule queries.
    pub fn query_with_program_and_nth_claim_body_value(
        &self,
        claim_name: &str,
        program_var: &str,
        body_var: &str,
        claim_idx: usize,
        program_value: z3::ast::Datatype<'static>,
    ) -> Result<Option<QueryResult>, RuntimeError> {
        let schema = self.schemas.get(claim_name)
            .ok_or_else(|| RuntimeError::UnknownSchema(claim_name.to_string()))?;
        let user = self.user_program();
        let Some(target_claim) = user.schemas.get(claim_idx) else {
            return Ok(None);
        };
        let body_items = &target_claim.body;
        let arith: u32 = std::env::var("EVIDENT_Z3_ARITH_SOLVER").ok()
            .and_then(|s| s.parse().ok()).unwrap_or(2);
        let mut given: HashMap<String, Value> = HashMap::new();
        given.insert("body_len".to_string(), Value::Int(body_items.len() as i64));
        let r = crate::translate::evaluate_with_program_and_body(
            schema, &given, &self.schemas, self.z3_ctx,
            &self.datatypes, &self.enums, arith,
            program_var, program_value,
            body_var, body_items,
        );
        Ok(Some(QueryResult { satisfied: r.satisfied, bindings: r.bindings }))
    }

    /// Number of claims the user has loaded (after
    /// `mark_system_loads_complete`). Used by callers iterating over
    /// claims with `query_with_program_and_nth_claim_body`.
    pub fn user_claim_count(&self) -> usize {
        self.user_program().schemas.len()
    }

    /// Name of the n-th user claim, if any. Used by the CLI to
    /// label per-claim inference output.
    pub fn user_claim_name(&self, idx: usize) -> Option<String> {
        self.user_program().schemas.get(idx).map(|s| s.name.clone())
    }

    pub fn query_with_program_and_body(
        &self,
        claim_name: &str,
        program_var: &str,
        body_var: &str,
    ) -> Result<QueryResult, RuntimeError> {
        let schema = self.schemas.get(claim_name)
            .ok_or_else(|| RuntimeError::UnknownSchema(claim_name.to_string()))?;
        let user = self.user_program();
        let prog_value = crate::translate::ast_encoder::encode_program(
            &user, self.z3_ctx, &self.enums,
        ).map_err(|e| RuntimeError::Parse(format!("encode failed: {e}")))?;
        let body_items: Vec<crate::ast::BodyItem> = user.schemas.first()
            .map(|s| s.body.clone())
            .unwrap_or_default();
        let arith: u32 = std::env::var("EVIDENT_Z3_ARITH_SOLVER").ok()
            .and_then(|s| s.parse().ok()).unwrap_or(2);
        // Inject body length as a `given` Int so the literal-int +
        // seq-length pre-passes can pin any `body_len ∈ Nat` /
        // `n = #body` references for quantifier unrolling. The
        // convention: pass `body_len` as the variable name; passes
        // declare it themselves and use it as the upper bound of
        // `∀ i ∈ {0..body_len - 1} : …`.
        let mut given: HashMap<String, Value> = HashMap::new();
        given.insert("body_len".to_string(), Value::Int(body_items.len() as i64));
        let r = crate::translate::evaluate_with_program_and_body(
            schema, &given, &self.schemas, self.z3_ctx,
            &self.datatypes, &self.enums, arith,
            program_var, prog_value,
            body_var, &body_items,
        );
        Ok(QueryResult { satisfied: r.satisfied, bindings: r.bindings })
    }
    /// accumulated `Program` injected as a `given` for one of the
    /// pass's variables.
    ///
    /// Concretely: encode the program as a Z3 Datatype value matching
    /// `stdlib/ast.ev`'s `Program` enum, then evaluate `claim_name`
    /// while asserting that the variable named `program_var` (declared
    /// as `program ∈ Program` in the pass) equals that value. Any
    /// other free variables in the pass behave normally — Z3 picks
    /// values that satisfy the pass's constraints.
    ///
    /// Returns `RuntimeError::Encode` if `stdlib/ast.ev` isn't
    /// loaded; `UnknownSchema` if the named claim doesn't exist.
    pub fn query_with_program(
        &self,
        claim_name: &str,
        program_var: &str,
    ) -> Result<QueryResult, RuntimeError> {
        let prog_value = self.encode_program_value()
            .map_err(|e| RuntimeError::Parse(format!("encode failed: {e}")))?;
        self.query_with_program_value(claim_name, program_var, prog_value)
    }

    /// Same as `query_with_program` but takes the encoded `Program`
    /// value directly. Lets callers running many rules over the same
    /// program (like the inference pipeline) encode once and reuse,
    /// avoiding the recursive-AST walk on every rule. Saves ~70-85%
    /// of the per-rule cost on big programs.
    pub fn query_with_program_value(
        &self,
        claim_name: &str,
        program_var: &str,
        program_value: z3::ast::Datatype<'static>,
    ) -> Result<QueryResult, RuntimeError> {
        let schema = self.schemas.get(claim_name)
            .ok_or_else(|| RuntimeError::UnknownSchema(claim_name.to_string()))?;
        let arith: u32 = std::env::var("EVIDENT_Z3_ARITH_SOLVER").ok()
            .and_then(|s| s.parse().ok()).unwrap_or(2);
        let r = crate::translate::evaluate_with_extra_assertion(
            schema,
            &HashMap::new(),
            &self.schemas,
            self.z3_ctx,
            &self.datatypes,
            Some(&self.enums),
            arith,
            program_var,
            program_value,
        );
        Ok(QueryResult { satisfied: r.satisfied, bindings: r.bindings })
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

        // Auto-tuner: which arith.solver should the cache use right now?
        let arith_solver = {
            let mut hist = self.solve_history.borrow_mut();
            hist.entry(name.to_string()).or_insert_with(SolveHistory::new)
                .current_config()
        };

        let mut cache = self.cache.borrow_mut();
        // Rebuild if (a) no entry, (b) structural signature changed, or
        // (c) cached config doesn't match the auto-tuner's current pick.
        let needs_rebuild = match cache.get(name) {
            Some((cached, cached_sig)) =>
                cached_sig != &cur_sig || cached.arith_solver != arith_solver,
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
                Some(&self.enums), &structural_given, arith_solver);
            cache.insert(name.to_string(), (new_cached, cur_sig));
        }
        let entry = cache.get(name).unwrap();

        // Time the actual solve so the auto-tuner can decide whether to
        // advance to the next pricing window.
        let t0 = Instant::now();
        let r = run_cached(&entry.0, given, self.z3_ctx, Some(&self.enums));
        let dt = t0.elapsed();
        drop(cache);  // release before we may invalidate below

        // Record the timing. If the tuner says to switch configs,
        // evict so the next call rebuilds under the new value.
        if let Some(_new_cfg) = self.solve_history.borrow_mut()
            .get_mut(name).and_then(|h| h.record(dt))
        {
            self.cache.borrow_mut().remove(name);
        }
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
        // Sample uses its own fresh, non-shared cached solver. Two reasons:
        //   1. `arith.solver=2` (the runtime's per-frame default and a
        //      candidate in the auto-tuner) is pathologically slow on
        //      sample_cached_inner's cumulative blocking-clause workload.
        //   2. The blocking clauses asserted inside sample's outer push
        //      shouldn't influence the per-frame solver state that the
        //      auto-tuner is timing.
        // Sample is rare and amortizes the build_cache cost across N
        // models, so the lack of cross-call caching is acceptable.
        let names = crate::translate::structural_names(&schema.body);
        let structural_given: HashMap<String, Value> = given.iter()
            .filter(|(k, _)| names.contains(k.as_str()))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        // Sample's "safe" config: leave Z3 at its default arith path.
        // 0 means "don't call set_params". Empirically this avoids the
        // solver=2 blocking-clause pathology.
        let cached = build_cache(
            &schema, &self.schemas, self.z3_ctx, &self.datatypes,
            Some(&self.enums), &structural_given, 0);
        Ok(sample_cached_inner(&cached, given, n, self.z3_ctx, Some(&self.enums)))
    }
}
