//! Source loading: parse + run all pre-translation passes + cache flush.

use crate::core::RuntimeError;
use super::EvidentRuntime;
use crate::parser;
use std::path::{Path, PathBuf};

impl EvidentRuntime {
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
    pub(super) fn load_source_with_base(&mut self, src: &str, base: Option<&Path>) -> Result<(), RuntimeError> {
        let prog = parser::parse(src).map_err(|e| RuntimeError::Parse(e.to_string()))?;
        // Process imports first so referenced types/claims exist when
        // the importing file's schemas are registered. This ordering
        // doesn't strictly matter for the runtime (schemas resolve
        // lazily by name) but it matches the textual reading order of
        // the file.
        for import_path in &prog.imports {
            // Known-shimmed stdlib paths (registered with the FTI
            // registry) silently no-op when the file isn't found at
            // the expected location — the registry stands in for the
            // file's contents. See `crate::fti::is_shimmed_stdlib`
            // for the policy and the list itself.
            if crate::fti::is_shimmed_stdlib(import_path) {
                // Try a real resolution first; only no-op if it fails.
                if self.resolve_import(import_path, base).is_err() {
                    continue;
                }
            }
            let resolved = self.resolve_import(import_path, base)?;
            self.load_file(&resolved)?;
        }
        for s in &prog.schemas {
            let mut s = s.clone();
            // Unified-state world syntax: rewrite `_world.X`/`world.X`
            // references into the legacy `world.X`/`world_next.X`
            // pattern so the multi-FSM scheduler's writer detection
            // works without changes. No-op for fsms that already
            // declared `world_next` (legacy pattern stays as is).
            super::desugar::unify_world_syntax(&mut s)?;
            // Flatten Seq concatenations (`a ++ b ++ ⟨…⟩`) into a
            // single SeqLit when all operands resolve to literal
            // sequences. The existing `translate_seq_lit_eq` path
            // handles the result. Recurses into subclaims.
            super::desugar::desugar_seq_concat(&mut s);
            super::inject::inject_fsm_params(&mut s)?;
            // lhs-eq inference runs BEFORE prev-tick injection so
            // that inferred memberships (e.g., `frame ∈ Int` from
            // `frame = ternary`) are visible when the prev-tick
            // walker resolves `_frame`'s type. Otherwise `_frame`
            // refers to an undeclared name and never gets injected.
            super::inject::inject_lhs_eq_types(&mut s, &self.schemas, &self.enums);
            super::inject::inject_prev_tick_decls(&mut s)?;
            // Needs the schemas table — runs against already-loaded
            // claims AND siblings in this same prog batch as they get
            // registered below. Self-reference works because we look
            // up the called claim's signature, not the current claim's.
            super::inject::inject_claim_arg_types(&mut s, &self.schemas)?;
            if !self.schemas.contains_key(&s.name) {
                self.schema_order.push(s.name.clone());
            }
            self.schemas.insert(s.name.clone(), s.clone());
            super::validate::register_subclaims(&s.body, &mut self.schemas);
        }
        // Build all Z3 DatatypeSorts for this batch of enums together
        // via `create_datatypes`. Lets enums forward-reference each
        // other (`Expr` referring to `BinOp` declared later in the
        // file) and be mutually recursive (`A` referring to `B` and
        // vice versa). Variant names must be globally unique across
        // all enums; load fails on collision.
        super::register_enums::register_enums(&prog.enums, self.z3_ctx, &self.enums)?;
        self.program.schemas.extend(prog.schemas);
        self.program.enums.extend(prog.enums);

        // Loading new schemas invalidates the cache: new schemas might
        // be referenced by ClaimCall / passthrough in old ones.
        self.cache.borrow_mut().clear();
        self.functionize_z3_cache.borrow_mut().clear();
        self.fn_cache.borrow_mut().clear();
        self.slow_path_cache.borrow_mut().clear();
        // Cross-tick value cache memoizes results keyed by given VALUES;
        // a reload can change a schema body or the functionizer, so any
        // memoized bindings are now potentially stale. Drop them all.
        self.value_cache.borrow_mut().clear();
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
    pub(super) fn resolve_import(&self, import_path: &str, base: Option<&Path>) -> Result<PathBuf, RuntimeError> {
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
        // at each ancestor. This also handles `import "packages/sdl.ev"`
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
}
