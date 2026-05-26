//! Source loading: parse + run all pre-translation passes + cache flush.

use crate::core::RuntimeError;
use super::EvidentRuntime;
use crate::core::ast::BodyItem;
use crate::parser;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

impl EvidentRuntime {
    /// Parse and load Evident source; multiple calls accumulate. Use `load_file`
    /// for file-relative import resolution.
    pub fn load_source(&mut self, src: &str) -> Result<(), RuntimeError> {
        self.load_source_with_base(src, None)
    }

    /// Load Evident source from a file; records the canonical path for import
    /// resolution and cycle detection.
    pub fn load_file(&mut self, path: &Path) -> Result<(), RuntimeError> {
        let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        if !self.loaded_files.borrow_mut().insert(canonical.clone()) {
            return Ok(()); // cycle / duplicate import
        }
        let src = std::fs::read_to_string(path)
            .map_err(|e| RuntimeError::Io(format!("read {}: {e}", path.display())))?;
        self.load_source_with_base(&src, Some(&canonical))
    }

    /// Internal entry: `base` is `None` for raw strings, `Some(path)` when loading from a file.
    pub(super) fn load_source_with_base(&mut self, src: &str, base: Option<&Path>) -> Result<(), RuntimeError> {
        let prog = parser::parse(src).map_err(|e| RuntimeError::Parse(e.to_string()))?;
        for import_path in &prog.imports {
            // Shimmed stdlib paths: the FTI registry stands in for the file; skip if not on disk.
            if crate::fti::is_shimmed_stdlib(import_path) {
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
            // Rewrite `_X`/`X` time-shift for any first-line fsm state var; inert when
            // `X_next` is already declared. Runs before inject so `_X` is consumed first.
            super::desugar::unify_state_syntax(&mut s)?;
            // Flatten `a ++ b ++ ⟨…⟩` into a single SeqLit when all operands are literals.
            super::desugar::desugar_seq_concat(&mut s);
            crate::portable::inject::fsm_params(&mut s);
            // lhs-eq must run before prev_tick so inferred memberships resolve `_frame`'s type.
            super::inject::inject_lhs_eq_types(&mut s, &self.schemas, &self.enums);
            crate::portable::inject::prev_tick(&mut s);
            super::inject::inject_claim_arg_types(&mut s, &self.schemas)?;
            super::validate::enforce_external_only(&s)?;
            if !self.schemas.contains_key(&s.name) {
                self.schema_order.push(s.name.clone());
            }
            self.schemas.insert(s.name.clone(), s.clone());
            super::validate::register_subclaims(&s.body, &mut self.schemas);
            // Track origin file so the inference pipeline can skip imported-helper claims.
            if let Some(path) = base {
                let mut origins = self.schema_origins.borrow_mut();
                origins.insert(s.name.clone(), path.to_path_buf());
                fn record_subclaim_origins(
                    body: &[BodyItem],
                    path: &Path,
                    out: &mut HashMap<String, PathBuf>,
                ) {
                    for item in body {
                        if let BodyItem::SubclaimDecl(s) = item {
                            out.insert(s.name.clone(), path.to_path_buf());
                            record_subclaim_origins(&s.body, path, out);
                        }
                    }
                }
                record_subclaim_origins(&s.body, path, &mut origins);
            }
        }
        // `create_datatypes` handles forward refs and mutual recursion in one pass.
        // Variant names must be globally unique; load fails on collision.
        super::register_enums::register_enums(&prog.enums, self.z3_ctx, &self.enums)?;
        self.program.schemas.extend(prog.schemas);
        self.program.enums.extend(prog.enums);

        // Expand `Edge<Rect>` → concrete schema with `T→Rect`; iterates to fixpoint.
        crate::portable::generics::monomorphize_generics(&mut self.schemas, &mut self.schema_order)?;
        // Validate `run(F,..)` targets after the full batch is registered.
        self.validate_run_targets()?;
        // A reload can change schema bodies or the functionizer; flush all caches.
        self.cache.borrow_mut().clear();
        self.solve_history.borrow_mut().clear();
        self.functionize_z3_cache.borrow_mut().clear();
        self.fn_cache.borrow_mut().clear();
        self.slow_path_cache.borrow_mut().clear();
        self.value_cache.borrow_mut().clear();
        // DatatypeSort entries reference body shape; flush so we rebuild on first use.
        // Note: leaked DatatypeSorts live forever in Z3; re-declaring same name will fail.
        self.datatypes.borrow_mut().clear();
        Ok(())
    }

    /// Resolve an `import "path"` reference: tries verbatim, relative to source file,
    /// cwd, then walks up to 10 ancestor dirs for root-anchored imports.
    pub(super) fn resolve_import(&self, import_path: &str, base: Option<&Path>) -> Result<PathBuf, RuntimeError> {
        let p = Path::new(import_path);
        if p.exists() {
            return Ok(p.to_path_buf());
        }
        if let Some(base) = base {
            if let Some(dir) = base.parent() {
                let candidate = dir.join(p);
                if candidate.exists() {
                    return Ok(candidate);
                }
            }
        }
        if let Ok(cwd) = std::env::current_dir() {
            let candidate = cwd.join(p);
            if candidate.exists() {
                return Ok(candidate);
            }
        }
        // Walk ancestors for root-anchored imports like `import "packages/sdl.ev"`.
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
